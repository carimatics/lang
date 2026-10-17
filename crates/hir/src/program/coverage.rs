//! Constructor specialization with an explicit search stack. Record columns
//! can expand to thousands of cells without recursive Rust calls.

use std::collections::{BTreeMap, BTreeSet};

use lang_diagnostics::{Diagnostic, Label, Severity, Span, codes};

use super::{CheckError, Context};
use crate::core;
use crate::resolved as r;

#[derive(Clone, Copy)]
enum Cell {
    Wild,
    Pattern(r::PatternId),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Constructor {
    Integer(i64),
    Boolean(bool),
    Record(r::DefinitionId),
    Case(r::DefinitionId, usize),
    CoreUnit,
    CoreCase(core::Case, r::Type),
    Opaque(r::Type),
}
struct State {
    rows: Vec<Vec<Cell>>,
    vector: Vec<Cell>,
    types: Vec<r::Type>,
    history: Vec<Constructor>,
}

struct Search<'a, 'b> {
    context: &'b mut Context<'a>,
    function: &'a r::Function,
    fields: &'b BTreeMap<r::PatternId, Vec<r::PatternId>>,
    span: Span,
}

pub(super) fn check_match<'a>(
    context: &mut Context<'a>,
    function: &'a r::Function,
    fields: &BTreeMap<r::PatternId, Vec<r::PatternId>>,
    ty: r::Type,
    arms: &[r::Arm],
    span: Span,
) -> Result<bool, CheckError> {
    let mut search = Search {
        context,
        function,
        fields,
        span,
    };
    let mut previous = Vec::new();
    for (index, arm) in arms.iter().enumerate() {
        if search
            .useful(&previous, Cell::Pattern(arm.pattern), ty)?
            .is_none()
        {
            let mut diagnostic = Diagnostic::new(
                codes::UNREACHABLE_ARM,
                Severity::Warning,
                "match arm is covered by earlier arms",
                Label::new(arm.span, "this arm cannot be selected"),
            );
            for earlier in &arms[..index] {
                search.charge()?;
                diagnostic.secondary.push(Label::new(
                    earlier.span,
                    "earlier arm contributes to coverage",
                ));
            }
            search.context.report(diagnostic)?;
        }
        search.charge()?;
        previous.push(vec![Cell::Pattern(arm.pattern)]);
    }
    if let Some(witness) = search.useful(&previous, Cell::Wild, ty)? {
        let rendered = search.render(&witness)?;
        let mut diagnostic = Diagnostic::new(
            codes::NON_EXHAUSTIVE_MATCH,
            Severity::Error,
            "match does not cover every value",
            Label::new(span, "non-exhaustive match"),
        );
        diagnostic
            .notes
            .push(format!("missing pattern: {rendered}"));
        search.context.report(diagnostic)?;
        Ok(false)
    } else {
        Ok(true)
    }
}

impl<'a> Search<'a, '_> {
    fn nominal_name(&self, id: r::DefinitionId) -> (&'a str, bool) {
        let source: &'a r::Program = self.context.source;
        let name = source.definitions[id.0].qualified_name.as_str();
        let origin = source.modules[self.function.module.0]
            .qualified_name
            .as_str();
        if origin.is_empty() {
            return (name, false);
        }
        if let Some(relative) = name
            .strip_prefix(origin)
            .and_then(|name| name.strip_prefix("::"))
        {
            (relative, false)
        } else {
            (name, true)
        }
    }
    fn charge(&mut self) -> Result<(), CheckError> {
        if self.context.coverage_work >= self.context.limits.exhaustiveness {
            return Err(self
                .context
                .limit(self.span, "exhaustiveness work limit exceeded"));
        }
        self.context.coverage_work += 1;
        Ok(())
    }
    fn copy<T: Copy>(&mut self, input: &[T]) -> Result<Vec<T>, CheckError> {
        let mut result = Vec::new();
        for item in input {
            self.charge()?;
            result.push(*item);
        }
        Ok(result)
    }
    fn wildcard(&mut self, cell: Cell) -> Result<bool, CheckError> {
        self.charge()?;
        Ok(match cell {
            Cell::Wild => true,
            Cell::Pattern(id) => matches!(
                self.function.patterns[id.0].kind,
                r::PatternKind::Wildcard | r::PatternKind::Binding(_)
            ),
        })
    }
    fn constructor(&mut self, cell: Cell, ty: r::Type) -> Result<Option<Constructor>, CheckError> {
        if self.wildcard(cell)? {
            return Ok(None);
        }
        let Cell::Pattern(id) = cell else {
            unreachable!()
        };
        Ok(Some(match self.function.patterns[id.0].kind {
            r::PatternKind::CoreUnit => Constructor::CoreUnit,
            r::PatternKind::CoreCase { case, .. } => {
                Constructor::CoreCase(case, self.context.loan(ty).map_or(ty, |(_, target)| target))
            }
            r::PatternKind::Integer(value) => Constructor::Integer(value),
            r::PatternKind::Boolean(value) => Constructor::Boolean(value),
            r::PatternKind::Record { definition, .. } => Constructor::Record(definition),
            r::PatternKind::Case {
                definition, index, ..
            } => Constructor::Case(definition, index),
            _ => unreachable!("wildcard handled above"),
        }))
    }
    fn constructors(&mut self, state: &State) -> Result<Vec<Constructor>, CheckError> {
        if let Some(constructor) = self.constructor(state.vector[0], state.types[0])? {
            return Ok(vec![constructor]);
        }
        let ty = self
            .context
            .loan(state.types[0])
            .map_or(state.types[0], |(_, target)| target);
        match ty {
            r::Type::Constructed(_) => {
                if self.context.constructor(ty)
                    == Some(r::TypeConstructor::Builtin(r::BuiltinType::Unit))
                {
                    return Ok(vec![Constructor::CoreUnit]);
                }
                if let Some((family, _)) = self
                    .context
                    .types
                    .as_ref()
                    .and_then(|types| types.family(ty))
                {
                    let mut result = Vec::new();
                    for case in family.cases() {
                        self.charge()?;
                        result.push(Constructor::CoreCase(*case, ty));
                    }
                    Ok(result)
                } else {
                    Ok(vec![Constructor::Opaque(ty)])
                }
            }
            r::Type::Bool => Ok(vec![
                Constructor::Boolean(false),
                Constructor::Boolean(true),
            ]),
            r::Type::I64 => {
                let mut literals = BTreeSet::new();
                for row in &state.rows {
                    if let Some(Constructor::Integer(value)) = self.constructor(row[0], ty)? {
                        literals.insert(value);
                    }
                }
                let mut complement = 0_i64;
                loop {
                    self.charge()?;
                    if !literals.contains(&complement) {
                        break;
                    }
                    complement = complement.wrapping_add(1);
                }
                let mut result = Vec::new();
                for value in literals {
                    self.charge()?;
                    result.push(Constructor::Integer(value));
                }
                result.push(Constructor::Integer(complement));
                Ok(result)
            }
            r::Type::Nominal(id) => match &self.context.source.definitions[id.0].kind {
                r::DefinitionKind::Record(_) => Ok(vec![Constructor::Record(id)]),
                r::DefinitionKind::Enum(cases) => {
                    let mut result = Vec::new();
                    for index in 0..cases.len() {
                        self.charge()?;
                        result.push(Constructor::Case(id, index));
                    }
                    Ok(result)
                }
            },
        }
    }
    fn payload_types(&mut self, constructor: Constructor) -> Result<Vec<r::Type>, CheckError> {
        let mut result = Vec::new();
        match constructor {
            Constructor::CoreCase(case, ty) => {
                result = self
                    .context
                    .core_payload(case, Some(ty), self.span)?
                    .ok_or_else(|| {
                        super::input(
                            super::CheckInputErrorKind::IncompleteTyping,
                            Some(self.span),
                        )
                    })?;
            }
            Constructor::Record(id) => {
                let r::DefinitionKind::Record(fields) = &self.context.source.definitions[id.0].kind
                else {
                    unreachable!()
                };
                for field in fields {
                    self.charge()?;
                    result.push(field.value_type.value_type);
                }
            }
            Constructor::Case(id, index) => {
                let r::DefinitionKind::Enum(cases) = &self.context.source.definitions[id.0].kind
                else {
                    unreachable!()
                };
                for ty in &cases[index].payload {
                    self.charge()?;
                    result.push(ty.value_type);
                }
            }
            _ => {}
        }
        Ok(result)
    }
    fn specialize(
        &mut self,
        cell: Cell,
        ty: r::Type,
        constructor: Constructor,
        arity: usize,
    ) -> Result<Option<Vec<Cell>>, CheckError> {
        self.charge()?;
        if self.wildcard(cell)? {
            let mut result = Vec::new();
            for _ in 0..arity {
                self.charge()?;
                result.push(Cell::Wild);
            }
            return Ok(Some(result));
        }
        if self.constructor(cell, ty)? != Some(constructor) {
            return Ok(None);
        }
        let Cell::Pattern(id) = cell else {
            unreachable!()
        };
        let payload: &[r::PatternId] = match &self.function.patterns[id.0].kind {
            r::PatternKind::Record { .. } => &self.fields[&id],
            r::PatternKind::Case { payload, .. } | r::PatternKind::CoreCase { payload, .. } => {
                payload
            }
            _ => &[],
        };
        let mut result = Vec::new();
        for id in payload {
            self.charge()?;
            result.push(Cell::Pattern(*id));
        }
        Ok(Some(result))
    }
    fn useful(
        &mut self,
        rows: &[Vec<Cell>],
        pattern: Cell,
        ty: r::Type,
    ) -> Result<Option<Vec<Constructor>>, CheckError> {
        let mut initial = Vec::new();
        for row in rows {
            self.charge()?;
            initial.push(self.copy(row)?);
        }
        let mut pending = vec![State {
            rows: initial,
            vector: vec![pattern],
            types: vec![ty],
            history: Vec::new(),
        }];
        while let Some(state) = pending.pop() {
            self.charge()?;
            if state.vector.is_empty() {
                if state.rows.is_empty() {
                    return Ok(Some(state.history));
                }
                continue;
            }
            // An all-wildcard row covers every remaining column. This avoids
            // enumerating 2^N boolean products for a single wildcard product.
            let mut covered = false;
            for row in &state.rows {
                self.charge()?;
                let mut all_wild = true;
                for cell in row {
                    if !self.wildcard(*cell)? {
                        all_wild = false;
                        break;
                    }
                }
                if all_wild {
                    covered = true;
                    break;
                }
            }
            if covered {
                continue;
            }
            for constructor in self.constructors(&state)?.into_iter().rev() {
                self.charge()?;
                let mut payload_types = self.payload_types(constructor)?;
                let Some(mut vector) = self.specialize(
                    state.vector[0],
                    state.types[0],
                    constructor,
                    payload_types.len(),
                )?
                else {
                    continue;
                };
                vector.extend(self.copy(&state.vector[1..])?);
                let mut specialized = Vec::new();
                for row in &state.rows {
                    self.charge()?;
                    if let Some(mut payload) =
                        self.specialize(row[0], state.types[0], constructor, payload_types.len())?
                    {
                        payload.extend(self.copy(&row[1..])?);
                        specialized.push(payload);
                    }
                }
                payload_types.extend(self.copy(&state.types[1..])?);
                let mut history = self.copy(&state.history)?;
                history.push(constructor);
                pending.push(State {
                    rows: specialized,
                    vector,
                    types: payload_types,
                    history,
                });
            }
        }
        Ok(None)
    }
    fn render(&mut self, constructors: &[Constructor]) -> Result<String, CheckError> {
        enum Piece<'a> {
            Text(&'a str),
            Value,
        }
        let mut pieces = vec![Piece::Value];
        let mut next = constructors.iter();
        let mut text = String::new();
        while let Some(piece) = pieces.pop() {
            self.charge()?;
            match piece {
                Piece::Text(fragment) => {
                    if fragment.len() > self.context.limits.witness_bytes.saturating_sub(text.len())
                    {
                        return Err(self
                            .context
                            .limit(self.span, "exhaustiveness witness byte limit exceeded"));
                    }
                    text.push_str(fragment);
                }
                Piece::Value => match next
                    .next()
                    .expect("witness constructors are a complete preorder")
                {
                    Constructor::CoreUnit => pieces.push(Piece::Text("core::Unit {}")),
                    Constructor::Opaque(_) => pieces.push(Piece::Text("_")),
                    Constructor::CoreCase(case, _) => {
                        if !case.payload().is_empty() {
                            pieces.push(Piece::Text(")"));
                            for position in (0..case.payload().len()).rev() {
                                self.charge()?;
                                pieces.push(Piece::Value);
                                if position != 0 {
                                    pieces.push(Piece::Text(", "));
                                }
                            }
                            pieces.push(Piece::Text("("));
                        }
                        pieces.push(Piece::Text(case.name()));
                        pieces.push(Piece::Text("::"));
                        pieces.push(Piece::Text(case.family().name()));
                        pieces.push(Piece::Text("core::"));
                    }
                    Constructor::Integer(value) => {
                        let value = value.to_string();
                        if value.len()
                            > self.context.limits.witness_bytes.saturating_sub(text.len())
                        {
                            return Err(self
                                .context
                                .limit(self.span, "exhaustiveness witness byte limit exceeded"));
                        }
                        text.push_str(&value);
                    }
                    Constructor::Boolean(value) => {
                        pieces.push(Piece::Text(if *value { "true" } else { "false" }))
                    }
                    Constructor::Record(id) => {
                        let definition = &self.context.source.definitions[id.0];
                        let r::DefinitionKind::Record(fields) = &definition.kind else {
                            unreachable!()
                        };
                        pieces.push(Piece::Text(if fields.is_empty() { "}" } else { " }" }));
                        for (index, field) in fields.iter().enumerate().rev() {
                            self.charge()?;
                            pieces.push(Piece::Value);
                            pieces.push(Piece::Text(": "));
                            pieces.push(Piece::Text(&field.name.text));
                            pieces.push(Piece::Text(if index == 0 { " " } else { ", " }));
                        }
                        pieces.push(Piece::Text(" {"));
                        let (name, root) = self.nominal_name(*id);
                        pieces.push(Piece::Text(name));
                        if root {
                            pieces.push(Piece::Text("root::"));
                        }
                    }
                    Constructor::Case(id, index) => {
                        let definition = &self.context.source.definitions[id.0];
                        let r::DefinitionKind::Enum(cases) = &definition.kind else {
                            unreachable!()
                        };
                        let case = &cases[*index];
                        if !case.payload.is_empty() {
                            pieces.push(Piece::Text(")"));
                            for position in (0..case.payload.len()).rev() {
                                self.charge()?;
                                pieces.push(Piece::Value);
                                if position != 0 {
                                    pieces.push(Piece::Text(", "));
                                }
                            }
                            pieces.push(Piece::Text("("));
                        }
                        pieces.push(Piece::Text(&case.name.text));
                        pieces.push(Piece::Text("::"));
                        let (name, root) = self.nominal_name(*id);
                        pieces.push(Piece::Text(name));
                        if root {
                            pieces.push(Piece::Text("root::"));
                        }
                    }
                },
            }
        }
        debug_assert!(next.next().is_none());
        Ok(text)
    }
}
