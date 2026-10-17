//! Type-only body analysis. Success is not move/loan/region validation and
//! cannot construct the executable `TypedProgram` consumed by MIR lowering.

use super::{
    CheckError, CheckInputErrorKind, CheckLimits, Context, FunctionChecker, FunctionFacts, input,
};
use crate::{core, resolved as r, types};
use lang_diagnostics::{Diagnostic, SourceMap, Span};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BodyTypes {
    source: r::Program,
    types: Vec<r::ConstructedType>,
    functions: Vec<FunctionFacts>,
    definitions: Vec<types::Properties>,
    constructed: Vec<types::Properties>,
    type_work: usize,
    type_allocations: usize,
}
impl BodyTypes {
    pub fn source(&self) -> &r::Program {
        &self.source
    }
    /// Typed facts refer to this arena. The resolved source arena remains its
    /// original prefix; compiler-created provisional loan sums live only here.
    pub fn types(&self) -> &[r::ConstructedType] {
        &self.types
    }
    pub fn functions(&self) -> &[FunctionFacts] {
        &self.functions
    }
    pub fn properties(&self, ty: r::Type) -> Option<types::Properties> {
        match ty {
            r::Type::I64 | r::Type::Bool => Some(scalar()),
            r::Type::Nominal(id) => self.definitions.get(id.0).copied(),
            r::Type::Constructed(id) => self.constructed.get(id.0).copied(),
        }
    }
    pub const fn type_work_used(&self) -> usize {
        self.type_work
    }
    pub const fn requested_type_bytes(&self) -> usize {
        self.type_allocations
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BodyAnalysis {
    pub bodies: Option<BodyTypes>,
    pub diagnostics: Vec<Diagnostic>,
}

pub fn analyze_bodies(
    source: r::Program,
    sources: &SourceMap,
    limits: CheckLimits,
) -> Result<BodyAnalysis, CheckError> {
    let analysis = types::analyze(&source, sources, limits.types).map_err(CheckError::Types)?;
    let Some(properties) = analysis.facts else {
        return Ok(BodyAnalysis {
            bodies: None,
            diagnostics: analysis.diagnostics,
        });
    };
    let root = source.modules[0].span;
    let store = TypeStore::new(&source, properties, limits.types, root)?;
    let mut context = Context {
        source: &source,
        sources,
        limits,
        work: 0,
        coverage_work: 0,
        diagnostics: Vec::new(),
        errors: 0,
        types: Some(store),
    };
    context.metadata()?;
    let mut functions = Vec::new();
    for (index, function) in source.functions.iter().enumerate() {
        functions.push(
            FunctionChecker::new(&mut context, function, false, r::FunctionId(index)).check()?,
        );
    }
    if context.errors != 0 {
        return Ok(BodyAnalysis {
            bodies: None,
            diagnostics: context.diagnostics,
        });
    }
    let functions = functions
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| input(CheckInputErrorKind::IncompleteTyping, None))?;
    let store = context.types.take().unwrap();
    let diagnostics = context.diagnostics;
    Ok(BodyAnalysis {
        bodies: Some(BodyTypes {
            source,
            types: store.nodes,
            functions,
            definitions: store.definitions,
            constructed: store.properties,
            type_work: store.budget.work,
            type_allocations: store.budget.allocations,
        }),
        diagnostics,
    })
}

fn scalar() -> types::Properties {
    types::Properties {
        is_copy: true,
        requires_drop: false,
        contains_loan: false,
        contains_arena: false,
        contains_allocator: false,
    }
}

pub(super) struct TypeStore {
    pub(super) nodes: Vec<r::ConstructedType>,
    definitions: Vec<types::Properties>,
    properties: Vec<types::Properties>,
    depths: Vec<usize>,
    budget: types::Budget,
}
impl TypeStore {
    fn new(
        source: &r::Program,
        facts: types::Facts,
        limits: types::Limits,
        span: Span,
    ) -> Result<Self, CheckError> {
        let (definitions, properties, work, allocations) = facts.into_parts();
        let mut budget = types::Budget {
            limits,
            work,
            allocations,
        };
        let mut nodes = budget
            .vector(span, source.types.len())
            .map_err(CheckError::Types)?;
        let mut depths: Vec<usize> = budget
            .vector(span, source.types.len())
            .map_err(CheckError::Types)?;
        for node in &source.types {
            budget.charge(node.span, 1).map_err(CheckError::Types)?;
            nodes.push(*node);
            let mut depth = 1;
            for argument in node.constructor.arguments() {
                budget.charge(node.span, 1).map_err(CheckError::Types)?;
                let child = match argument {
                    r::Type::Constructed(id) => depths[id.0],
                    _ => 1,
                };
                depth = depth.max(child.saturating_add(1));
            }
            depths.push(depth);
        }
        Ok(Self {
            nodes,
            definitions,
            properties,
            depths,
            budget,
        })
    }
    pub(super) fn constructor(&self, ty: r::Type) -> Option<r::TypeConstructor> {
        if let r::Type::Constructed(id) = ty {
            self.nodes.get(id.0).map(|n| n.constructor)
        } else {
            None
        }
    }
    pub(super) fn properties(
        &self,
        ty: r::Type,
        span: Span,
    ) -> Result<types::Properties, CheckError> {
        match ty {
            r::Type::I64 | r::Type::Bool => Ok(scalar()),
            r::Type::Nominal(id) => self
                .definitions
                .get(id.0)
                .copied()
                .ok_or_else(|| input(CheckInputErrorKind::InvalidDefinition(id.0), Some(span))),
            r::Type::Constructed(id) => self
                .properties
                .get(id.0)
                .copied()
                .ok_or_else(|| input(CheckInputErrorKind::InvalidConstructed(id.0), Some(span))),
        }
    }
    pub(super) fn intern(
        &mut self,
        constructor: r::TypeConstructor,
        span: Span,
    ) -> Result<r::Type, CheckError> {
        for (index, node) in self.nodes.iter().enumerate() {
            self.budget.charge(span, 1).map_err(CheckError::Types)?;
            if node.constructor == constructor {
                return Ok(r::Type::Constructed(r::ConstructedTypeId(index)));
            }
        }
        if self.nodes.len() >= self.budget.limits.constructed {
            return Err(CheckError::Types(
                self.budget.limit(span, "body type instance limit exceeded"),
            ));
        }
        let mut depth = 1;
        let mut properties = scalar();
        for argument in constructor.arguments() {
            self.budget.charge(span, 1).map_err(CheckError::Types)?;
            let child = self.properties(argument, span)?;
            properties.is_copy &= child.is_copy;
            properties.requires_drop |= child.requires_drop;
            properties.contains_loan |= child.contains_loan;
            properties.contains_arena |= child.contains_arena;
            properties.contains_allocator |= child.contains_allocator;
            let child_depth = match argument {
                r::Type::Constructed(id) => self.depths[id.0],
                _ => 1,
            };
            depth = depth.max(child_depth.saturating_add(1));
        }
        if depth > self.budget.limits.depth {
            return Err(CheckError::Types(
                self.budget.limit(span, "body type depth limit exceeded"),
            ));
        }
        match constructor {
            r::TypeConstructor::ArenaOwner(_) => {
                properties.is_copy = false;
                properties.requires_drop = true;
                properties.contains_arena = true;
            }
            r::TypeConstructor::Builtin(r::BuiltinType::Text | r::BuiltinType::Bytes)
            | r::TypeConstructor::Vec(_)
            | r::TypeConstructor::Map(..)
            | r::TypeConstructor::Shared(_)
            | r::TypeConstructor::Weak(_) => {
                properties.is_copy = false;
                properties.requires_drop = true;
            }
            r::TypeConstructor::Builtin(r::BuiltinType::Allocator) => {
                properties.is_copy = false;
                properties.contains_allocator = true;
            }
            r::TypeConstructor::Loan(..) => {
                properties.is_copy = false;
                properties.requires_drop = false;
                properties.contains_loan = true;
            }
            r::TypeConstructor::Arena(_) => {
                properties.is_copy = true;
                properties.requires_drop = false;
                properties.contains_arena = true;
            }
            _ => {}
        }
        let id = r::ConstructedTypeId(self.nodes.len());
        self.budget
            .push(
                span,
                &mut self.nodes,
                r::ConstructedType { constructor, span },
            )
            .map_err(CheckError::Types)?;
        self.budget
            .push(span, &mut self.properties, properties)
            .map_err(CheckError::Types)?;
        self.budget
            .push(span, &mut self.depths, depth)
            .map_err(CheckError::Types)?;
        Ok(r::Type::Constructed(id))
    }
    pub(super) fn instantiate(
        &mut self,
        pattern: core::TypePattern,
        arguments: &[r::Type],
        span: Span,
        depth: usize,
    ) -> Result<r::Type, CheckError> {
        self.instantiate_with_owner(pattern, arguments, None, span, depth)
    }
    pub(super) fn instantiate_with_owner(
        &mut self,
        pattern: core::TypePattern,
        arguments: &[r::Type],
        owner: Option<r::Type>,
        span: Span,
        depth: usize,
    ) -> Result<r::Type, CheckError> {
        self.budget.charge(span, 1).map_err(CheckError::Types)?;
        if depth >= self.budget.limits.depth.min(64) {
            return Err(CheckError::Types(
                self.budget
                    .limit(span, "core signature depth limit exceeded"),
            ));
        }
        use core::TypePattern as P;
        let constructor = match pattern {
            P::I64 => return Ok(r::Type::I64),
            P::Bool => return Ok(r::Type::Bool),
            P::Argument(index) => {
                return arguments
                    .get(usize::from(index))
                    .copied()
                    .ok_or_else(|| input(CheckInputErrorKind::InvalidSignature, Some(span)));
            }
            P::ArenaOwner => {
                return owner
                    .ok_or_else(|| input(CheckInputErrorKind::InvalidSignature, Some(span)));
            }
            P::Builtin(b) => r::TypeConstructor::Builtin(b),
            P::Option(t) => r::TypeConstructor::Option(self.instantiate_with_owner(
                *t,
                arguments,
                owner,
                span,
                depth + 1,
            )?),
            P::Result(t, e) => {
                let t = self.instantiate_with_owner(*t, arguments, owner, span, depth + 1)?;
                let e = self.instantiate_with_owner(*e, arguments, owner, span, depth + 1)?;
                r::TypeConstructor::Result(t, e)
            }
            P::Vec(t) => r::TypeConstructor::Vec(self.instantiate_with_owner(
                *t,
                arguments,
                owner,
                span,
                depth + 1,
            )?),
            P::Map(k, v) => {
                let k = self.instantiate_with_owner(*k, arguments, owner, span, depth + 1)?;
                let v = self.instantiate_with_owner(*v, arguments, owner, span, depth + 1)?;
                r::TypeConstructor::Map(k, v)
            }
            P::Shared(t) => r::TypeConstructor::Shared(self.instantiate_with_owner(
                *t,
                arguments,
                owner,
                span,
                depth + 1,
            )?),
            P::Weak(t) => r::TypeConstructor::Weak(self.instantiate_with_owner(
                *t,
                arguments,
                owner,
                span,
                depth + 1,
            )?),
            P::Arena(t) => r::TypeConstructor::Arena(self.instantiate_with_owner(
                *t,
                arguments,
                owner,
                span,
                depth + 1,
            )?),
            P::Loan(kind, t) => r::TypeConstructor::Loan(
                kind,
                self.instantiate_with_owner(*t, arguments, owner, span, depth + 1)?,
            ),
        };
        self.intern(constructor, span)
    }
    pub(super) fn family(&self, ty: r::Type) -> Option<(core::Family, [r::Type; 2])> {
        use core::Family as F;
        use r::BuiltinType as B;
        Some(match self.constructor(ty)? {
            r::TypeConstructor::Option(t) => (F::Option, [t, r::Type::I64]),
            r::TypeConstructor::Result(t, e) => (F::Result, [t, e]),
            r::TypeConstructor::Builtin(b) => (
                match b {
                    B::AllocError => F::AllocError,
                    B::BoundsError => F::BoundsError,
                    B::BufferError => F::BufferError,
                    B::Utf8Error => F::Utf8Error,
                    B::OwnershipError => F::OwnershipError,
                    B::ArenaError => F::ArenaError,
                    _ => return None,
                },
                [r::Type::I64; 2],
            ),
            _ => return None,
        })
    }
}
