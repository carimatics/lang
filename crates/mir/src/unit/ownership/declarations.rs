//! Type-only function/local declarations. No instruction, initialization,
//! loan/region flow, cleanup, or runtime authority follows from this wrapper.

use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LocalId(pub u32);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocalKind {
    Parameter,
    Mutable,
    Pattern,
    ArenaOwner,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Local {
    pub name: String,
    pub span: Span,
    pub kind: LocalKind,
    pub ty: TypeUse,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ImportAbi {
    #[default]
    Host,
    C,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum FunctionAbi {
    #[default]
    Guest,
    CExport,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum FunctionEffect {
    #[default]
    Synchronous,
    Asynchronous {
        span: Span,
    },
}
impl FunctionEffect {
    pub const fn is_async(self) -> bool {
        matches!(self, Self::Asynchronous { .. })
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Function {
    pub effect: FunctionEffect,
    pub abi: FunctionAbi,
    pub name: String,
    pub public: bool,
    pub span: Span,
    pub signature_span: Span,
    pub parameters: Vec<LocalId>,
    pub result: TypeUse,
    pub locals: Vec<Local>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Import {
    pub abi: ImportAbi,
    pub name: String,
    pub span: Span,
    pub parameters: Vec<TypeUse>,
    pub result: TypeUse,
}
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Declarations {
    pub types: TypeTable,
    pub imports: Vec<Import>,
    pub functions: Vec<Function>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedDeclarations {
    types: VerifiedTypes,
    imports: Vec<Import>,
    functions: Vec<Function>,
    work: usize,
    bytes: usize,
    elements: usize,
}
impl VerifiedDeclarations {
    pub fn task_feature_span(&self) -> Option<Span> {
        self.types.task_feature_span().or_else(|| {
            self.functions.iter().find_map(|f| match f.effect {
                FunctionEffect::Asynchronous { span } => Some(span),
                FunctionEffect::Synchronous => None,
            })
        })
    }
    pub fn types(&self) -> &VerifiedTypes {
        &self.types
    }
    pub fn imports(&self) -> &[Import] {
        &self.imports
    }
    pub fn functions(&self) -> &[Function] {
        &self.functions
    }
    /// A C interface remains distinct even when unused or scalar-shaped.
    pub fn foreign_interface_span(&self) -> Option<Span> {
        self.imports
            .iter()
            .find(|import| import.abi == ImportAbi::C)
            .map(|import| import.span)
            .or_else(|| {
                self.functions
                    .iter()
                    .find(|function| function.abi == FunctionAbi::CExport)
                    .map(|function| function.signature_span)
            })
    }
    /// Check explicit materialized argument types. This proves no move, loan,
    /// liveness, region, capability, or execution transition and infers no borrow.
    pub fn check_call(
        &self,
        function: FunctionId,
        arguments: &[Type],
        span: Span,
        budget: &mut Budget,
    ) -> Result<Type, Error> {
        at(
            validation::source_span(&self.types.table.sources, Some(span), budget),
            span,
        )?;
        let f = self
            .functions
            .get(function.0 as usize)
            .ok_or_else(|| Error::new(ErrorKind::InvalidFunction(function), Some(span)))?;
        if f.effect.is_async() {
            return Err(Error::new(ErrorKind::UnsupportedTaskProfile, Some(span)));
        }
        self.check_arguments(
            f.parameters.iter().map(|p| f.locals[p.0 as usize].ty.ty),
            arguments,
            span,
            budget,
        )?;
        Ok(f.result.ty)
    }
    pub fn check_import(
        &self,
        import: super::super::ImportId,
        arguments: &[Type],
        span: Span,
        budget: &mut Budget,
    ) -> Result<Type, Error> {
        at(
            validation::source_span(&self.types.table.sources, Some(span), budget),
            span,
        )?;
        let f = self
            .imports
            .get(import.0 as usize)
            .ok_or_else(|| Error::new(ErrorKind::InvalidImport(import), Some(span)))?;
        self.check_arguments(f.parameters.iter().map(|p| p.ty), arguments, span, budget)?;
        Ok(f.result.ty)
    }
    fn check_arguments(
        &self,
        expected: impl ExactSizeIterator<Item = Type>,
        arguments: &[Type],
        span: Span,
        budget: &mut Budget,
    ) -> Result<(), Error> {
        at(budget.charge(1), span)?;
        if expected.len() != arguments.len() {
            return Err(Error::new(
                ErrorKind::CallArity {
                    expected: expected.len(),
                    actual: arguments.len(),
                },
                Some(span),
            ));
        }
        for (expected, &actual) in expected.zip(arguments) {
            at(budget.charge(1), span)?;
            match self.types.equivalent(expected, actual) {
                Some(true) => {}
                Some(false) => {
                    return Err(Error::new(
                        ErrorKind::CallType { expected, actual },
                        Some(span),
                    ));
                }
                None => return Err(Error::new(ErrorKind::InvalidType(actual), Some(span))),
            }
        }
        Ok(())
    }
    pub const fn work_used(&self) -> usize {
        self.work
    }
    pub const fn requested_bytes(&self) -> usize {
        self.bytes
    }
    pub const fn elements_used(&self) -> usize {
        self.elements
    }
    pub fn into_declarations(self) -> Declarations {
        Declarations {
            types: self.types.into_table(),
            imports: self.imports,
            functions: self.functions,
        }
    }
}
impl Declarations {
    pub fn verify(self, limits: Limits) -> Result<VerifiedDeclarations, Error> {
        self.verify_with_budget(&mut Budget::new(limits.unit), limits.constructed)
    }
    pub fn verify_with_budget(
        self,
        budget: &mut Budget,
        constructed_limit: usize,
    ) -> Result<VerifiedDeclarations, Error> {
        cap(
            self.functions.len(),
            budget.limits().functions,
            super::super::LimitKind::Functions,
        )?;
        cap(
            self.imports.len(),
            budget.limits().imports,
            super::super::LimitKind::Imports,
        )?;
        u32::try_from(self.functions.len())
            .map_err(|_| super::super::Error::new(super::super::ErrorKind::ArithmeticOverflow))?;
        let types = self.types.verify_with_budget(budget, constructed_limit)?;
        validate(&types, &self.imports, &self.functions, budget)?;
        Ok(VerifiedDeclarations {
            types,
            imports: self.imports,
            functions: self.functions,
            work: budget.work_used(),
            bytes: budget.allocation_bytes(),
            elements: budget.elements_used(),
        })
    }
}

struct Check<'a> {
    types: &'a VerifiedTypes,
    budget: &'a mut Budget,
}
impl Check<'_> {
    fn c_limit(
        &self,
        requested: usize,
        limit: usize,
        kind: CProfileLimit,
        span: Span,
    ) -> Result<(), Error> {
        if requested > limit {
            Err(Error::new(
                ErrorKind::CProfileLimit {
                    kind,
                    requested,
                    limit,
                },
                Some(span),
            ))
        } else {
            Ok(())
        }
    }
    fn c_type(&mut self, ty: Type, root: Span, depth: usize) -> Result<(), Error> {
        at(self.budget.charge(1), root)?;
        self.c_limit(depth, 64, CProfileLimit::Depth, root)?;
        let invalid = || Error::new(ErrorKind::InvalidCSignatureType(ty), Some(root));
        let properties = self.types.properties(ty).ok_or_else(invalid)?;
        if !properties.is_copy
            || properties.contains_loan
            || properties.contains_arena
            || properties.contains_allocator
            || properties.contains_owner
        {
            return Err(invalid());
        }
        // The independently verified type graph rules out inline recursion;
        // this walk adds the C profile's hard depth/width bounds and whitelist.
        let types = self.types;
        match ty {
            Type::I64 | Type::Bool => {}
            Type::Nominal(id) => match &types.table.definitions[id.0 as usize].kind {
                DefinitionKind::Record(fields) => {
                    self.c_limit(fields.len(), 256, CProfileLimit::Fields, root)?;
                    for field in fields {
                        self.c_type(field.ty.ty, root, depth + 1)?;
                    }
                }
                DefinitionKind::Enum(cases) => {
                    self.c_limit(cases.len(), 256, CProfileLimit::Cases, root)?;
                    for case in cases {
                        self.c_limit(case.payload.len(), 256, CProfileLimit::Payloads, root)?;
                        for field in &case.payload {
                            self.c_type(field.ty, root, depth + 1)?;
                        }
                    }
                }
            },
            Type::Constructed(_) => match self.constructor(ty).ok_or_else(invalid)? {
                Constructor::Builtin(
                    Builtin::Unit | Builtin::AllocError | Builtin::BoundsError,
                ) => {}
                Constructor::Builtin(
                    Builtin::BufferError
                    | Builtin::Utf8Error
                    | Builtin::OwnershipError
                    | Builtin::ArenaError,
                ) => {
                    // Account for the enum's implicit AllocError/i64 payload.
                    at(self.budget.charge(1), root)?;
                    self.c_limit(depth + 1, 64, CProfileLimit::Depth, root)?;
                }
                Constructor::Option(value) => self.c_type(value, root, depth + 1)?,
                Constructor::Result(ok, error) => {
                    self.c_type(ok, root, depth + 1)?;
                    self.c_type(error, root, depth + 1)?;
                }
                _ => return Err(invalid()),
            },
        }
        Ok(())
    }
    fn within(&mut self, owner: Span, span: Span) -> Result<(), Error> {
        at(
            validation::source_span(&self.types.table.sources, Some(span), self.budget),
            span,
        )?;
        if owner.source != span.source || span.start < owner.start || span.end > owner.end {
            Err(Error::new(ErrorKind::SpanOutsideOwner, Some(span)))
        } else {
            Ok(())
        }
    }
    fn span(&mut self, span: Span) -> Result<(), Error> {
        self.within(span, span)
    }
    fn type_use(&mut self, owner: Span, ty: TypeUse) -> Result<Properties, Error> {
        self.within(owner, ty.span)?;
        self.budget.charge(1)?;
        self.types
            .properties(ty.ty)
            .ok_or_else(|| Error::new(ErrorKind::InvalidType(ty.ty), Some(ty.span)))
    }
    fn constructor(&self, ty: Type) -> Option<Constructor> {
        if let Type::Constructed(id) = ty {
            self.types
                .table
                .constructed
                .get(id.0 as usize)
                .map(|t| t.constructor)
        } else {
            None
        }
    }
    fn signature(
        &mut self,
        owner: Span,
        ty: TypeUse,
        result: bool,
        public: bool,
    ) -> Result<(), Error> {
        let p = self.type_use(owner, ty)?;
        if p.contains_task
            || p.contains_owner
            || (result && (p.contains_loan || p.contains_arena || p.contains_allocator))
        {
            return Err(Error::new(
                ErrorKind::InvalidSignatureType(ty.ty),
                Some(ty.span),
            ));
        }
        let mut referent = ty.ty;
        let mut borrowed = false;
        while let Some(Constructor::Loan(_, t)) = self.constructor(referent) {
            self.budget.charge(1)?;
            referent = t;
            borrowed = true;
        }
        // Source functions may receive nested ordinary loans, but cannot name
        // the compiler-known loan-bearing stack sums used by core operations.
        if (!borrowed && p.contains_allocator)
            || self.types.properties(referent).unwrap().contains_loan
        {
            return Err(Error::new(
                ErrorKind::InvalidSignatureType(ty.ty),
                Some(ty.span),
            ));
        }
        if public && self.types.bits[node(&self.types.table, ty.ty)?] & PRIVATE != 0 {
            return Err(Error::new(
                ErrorKind::PrivateSignatureType(ty.ty),
                Some(ty.span),
            ));
        }
        Ok(())
    }
}

fn validate(
    types: &VerifiedTypes,
    imports: &[Import],
    functions: &[Function],
    budget: &mut Budget,
) -> Result<(), Error> {
    let mut c = Check { types, budget };
    let total_names = types
        .table
        .definitions
        .len()
        .checked_add(imports.len())
        .and_then(|n| n.checked_add(functions.len()))
        .ok_or_else(|| super::super::Error::new(super::super::ErrorKind::ArithmeticOverflow))?;
    let mut names = c.budget.vector(total_names)?;
    for d in &types.table.definitions {
        c.budget.charge(1)?;
        names.push(d.name.as_str());
    }
    for import in imports {
        c.span(import.span)?;
        at(validation::name(&import.name, true, c.budget), import.span)?;
        names.push(import.name.as_str());
        cap(
            import.parameters.len(),
            c.budget.limits().fields,
            super::super::LimitKind::Fields,
        )?;
        for t in &import.parameters {
            c.type_use(import.span, *t)?;
        }
        c.type_use(import.span, import.result)?;
        if import.abi == ImportAbi::C {
            c.c_limit(
                import.parameters.len(),
                256,
                CProfileLimit::Parameters,
                import.span,
            )?;
            for parameter in &import.parameters {
                c.c_type(parameter.ty, parameter.span, 1)?;
            }
            c.c_type(import.result.ty, import.result.span, 1)?;
        } else if import.parameters.len() != 1
            || import.parameters[0].ty != Type::I64
            || import.result.ty != Type::I64
        {
            return Err(Error::new(
                ErrorKind::InvalidHostSignature,
                Some(import.span),
            ));
        }
    }
    let mut local_count = 0usize;
    for (index, f) in functions.iter().enumerate() {
        let function = FunctionId(index as u32);
        c.span(f.span)?;
        c.within(f.span, f.signature_span)?;
        if let FunctionEffect::Asynchronous { span } = f.effect {
            c.within(f.signature_span, span)?;
            let text = &types.table.sources[span.source.0 as usize].text
                [span.start as usize..span.end as usize];
            if text != "async" || f.abi == FunctionAbi::CExport {
                return Err(Error::new(
                    ErrorKind::InvalidAsyncEffect(function),
                    Some(span),
                ));
            }
        }
        at(validation::name(&f.name, true, c.budget), f.span)?;
        names.push(f.name.as_str());
        if f.abi == FunctionAbi::CExport {
            if !f.public
                || f.name.is_empty()
                || f.name.len() > 128
                || !f.name.bytes().enumerate().all(|(index, b)| {
                    b == b'_' || b.is_ascii_alphabetic() || (index > 0 && b.is_ascii_digit())
                })
            {
                return Err(Error::new(
                    ErrorKind::InvalidCExport(function),
                    Some(f.signature_span),
                ));
            }
            c.c_limit(
                f.parameters.len(),
                256,
                CProfileLimit::Parameters,
                f.signature_span,
            )?;
            c.within(f.signature_span, f.result.span)?;
            c.c_type(f.result.ty, f.result.span, 1)?;
        }
        c.signature(f.signature_span, f.result, true, f.public)?;
        cap(
            f.parameters.len(),
            c.budget.limits().fields,
            super::super::LimitKind::Fields,
        )?;
        local_count = local_count
            .checked_add(f.locals.len())
            .ok_or_else(|| super::super::Error::new(super::super::ErrorKind::ArithmeticOverflow))?;
        cap(
            local_count,
            c.budget.limits().registers,
            super::super::LimitKind::Registers,
        )?;
        u32::try_from(f.locals.len())
            .map_err(|_| super::super::Error::new(super::super::ErrorKind::ArithmeticOverflow))?;
        let mut seen = filled(c.budget, f.locals.len(), false)?;
        let mut local_names = c.budget.vector(f.locals.len())?;
        for id in &f.parameters {
            c.budget.charge(1)?;
            let local = f.locals.get(id.0 as usize).ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidLocal {
                        function,
                        local: id.0,
                    },
                    Some(f.signature_span),
                )
            })?;
            if local.kind != LocalKind::Parameter || seen[id.0 as usize] {
                return Err(Error::new(
                    ErrorKind::InvalidParameterList(function),
                    Some(local.span),
                ));
            }
            seen[id.0 as usize] = true;
            c.within(f.signature_span, local.span)?;
            if f.abi == FunctionAbi::CExport {
                c.within(f.signature_span, local.ty.span)?;
                c.c_type(local.ty.ty, local.ty.span, 1)?;
            }
            c.signature(f.signature_span, local.ty, false, f.public)?;
        }
        for (i, local) in f.locals.iter().enumerate() {
            c.within(f.span, local.span)?;
            at(validation::name(&local.name, false, c.budget), local.span)?;
            local_names.push(local.name.as_str());
            let p = c.type_use(f.span, local.ty)?;
            if p.contains_task
                && (!f.effect.is_async()
                    || local.kind != LocalKind::Mutable
                    || !matches!(c.constructor(local.ty.ty), Some(Constructor::Task(_))))
            {
                return Err(Error::new(
                    ErrorKind::InvalidLocalType(local.ty.ty),
                    Some(local.ty.span),
                ));
            }
            if local.kind == LocalKind::Parameter && !seen[i] {
                return Err(Error::new(
                    ErrorKind::InvalidParameterList(function),
                    Some(local.span),
                ));
            }
            if local.kind == LocalKind::ArenaOwner {
                let expected = ArenaOwner {
                    function,
                    local: i as u32,
                };
                if c.constructor(local.ty.ty) != Some(Constructor::ArenaOwner(expected)) {
                    return Err(Error::new(
                        ErrorKind::InvalidOwner(expected),
                        Some(local.ty.span),
                    ));
                }
            } else if p.contains_owner
                || (p.contains_allocator
                    && !matches!(c.constructor(local.ty.ty), Some(Constructor::Loan(..))))
            {
                return Err(Error::new(
                    ErrorKind::InvalidLocalType(local.ty.ty),
                    Some(local.ty.span),
                ));
            }
        }
        at(validation::unique(&mut local_names, c.budget), f.span)?;
    }
    validation::unique(&mut names, c.budget)?;
    for t in &types.table.constructed {
        c.budget.charge(1)?;
        if let Constructor::ArenaOwner(owner) = t.constructor {
            let f = functions.get(owner.function.0 as usize).ok_or_else(|| {
                Error::new(ErrorKind::InvalidFunction(owner.function), Some(t.span))
            })?;
            c.within(f.span, t.span)?;
            let local = f.locals.get(owner.local as usize).ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidLocal {
                        function: owner.function,
                        local: owner.local,
                    },
                    Some(t.span),
                )
            })?;
            if local.kind != LocalKind::ArenaOwner
                || c.constructor(local.ty.ty) != Some(Constructor::ArenaOwner(owner))
            {
                return Err(Error::new(ErrorKind::InvalidOwner(owner), Some(t.span)));
            }
        }
    }
    Ok(())
}
