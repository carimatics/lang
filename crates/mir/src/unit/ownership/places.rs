//! Typed local/value roots, explicit ordinary-loan dereferences, and record
//! fields. Static access checks do not prove initialization, loan provenance,
//! liveness, region flow, dominance, cleanup, or executable instruction validity.

use super::super::Register;
use super::{declarations as d, *};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlaceId(pub u32);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Root {
    Local(d::LocalId),
    Value(Register),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectionKind {
    Dereference,
    Field(u32),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Projection {
    pub kind: ProjectionKind,
    pub span: Span,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Place {
    /// Type/source slot for the projected place, not an executed load.
    pub origin: Register,
    pub root: Root,
    pub projections: Vec<Projection>,
    pub span: Span,
}
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FunctionPlaces {
    /// Typed value descriptors. Definitions/evaluation require the later CFG.
    pub values: Vec<TypeUse>,
    pub places: Vec<Place>,
}
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PlaceTable {
    pub declarations: d::Declarations,
    pub functions: Vec<FunctionPlaces>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Limits {
    pub places: usize,
    pub ownership: super::Limits,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            places: 1_000_000,
            ownership: super::Limits::default(),
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Facts {
    pub value_type: Type,
    pub root: Root,
    pub through_loan: bool,
    pub whole_value: bool,
    /// Potential permission assuming every loan value was validly produced.
    /// Active loans and their transitive cell dependencies remain unverified.
    pub exclusive_borrow: bool,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Access {
    Read,
    Copy,
    Move,
    Replace(Type),
    Borrow(LoanKind),
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedPlaces {
    declarations: d::VerifiedDeclarations,
    functions: Vec<FunctionPlaces>,
    facts: Vec<Vec<Facts>>,
    work: usize,
    bytes: usize,
    elements: usize,
}
impl VerifiedPlaces {
    pub fn declarations(&self) -> &d::VerifiedDeclarations {
        &self.declarations
    }
    pub fn functions(&self) -> &[FunctionPlaces] {
        &self.functions
    }
    pub fn facts(&self, function: FunctionId, place: PlaceId) -> Option<&Facts> {
        self.facts.get(function.0 as usize)?.get(place.0 as usize)
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
    pub fn into_table(self) -> PlaceTable {
        PlaceTable {
            declarations: self.declarations.into_declarations(),
            functions: self.functions,
        }
    }
    /// Check static type/permission constraints only. No borrow is created and
    /// no value is read, moved, replaced, or initialized by this query.
    pub fn check_access(
        &self,
        function: FunctionId,
        place: PlaceId,
        access: Access,
        span: Span,
        budget: &mut Budget,
    ) -> Result<(), Error> {
        at(
            validation::source_span(&self.declarations.types().table.sources, Some(span), budget),
            span,
        )?;
        at(budget.charge(1), span)?;
        let facts = self
            .facts
            .get(function.0 as usize)
            .ok_or_else(|| Error::new(ErrorKind::InvalidFunction(function), Some(span)))?
            .get(place.0 as usize)
            .ok_or_else(|| Error::new(ErrorKind::InvalidPlace(place.0), Some(span)))?;
        let types = self.declarations.types();
        let p = types
            .properties(facts.value_type)
            .expect("verified place type");
        let authority = matches!(
            constructor(types, facts.value_type),
            Some(Constructor::ArenaOwner(_) | Constructor::Builtin(Builtin::Allocator))
        );
        let kind = match access {
            Access::Read | Access::Borrow(LoanKind::Shared) => None,
            Access::Copy if p.is_copy => None,
            Access::Copy => Some(ErrorKind::NonCopyPlace(facts.value_type)),
            Access::Move if authority => Some(ErrorKind::MoveAuthority(facts.value_type)),
            Access::Move if p.is_copy || facts.whole_value => None,
            Access::Move if facts.through_loan => {
                Some(ErrorKind::MoveThroughLoan(facts.value_type))
            }
            Access::Move => Some(ErrorKind::PartialMove(facts.value_type)),
            Access::Borrow(LoanKind::Exclusive) if facts.exclusive_borrow => None,
            Access::Borrow(LoanKind::Exclusive) => Some(ErrorKind::ImmutablePlace),
            Access::Replace(_) if authority => Some(ErrorKind::ReplaceAuthority(facts.value_type)),
            Access::Replace(_) if !facts.exclusive_borrow => Some(ErrorKind::ImmutablePlace),
            Access::Replace(actual) => match types.equivalent(facts.value_type, actual) {
                Some(true) => None,
                Some(false) => Some(ErrorKind::PlaceType {
                    expected: facts.value_type,
                    actual,
                }),
                None => Some(ErrorKind::InvalidType(actual)),
            },
        };
        if let Some(kind) = kind {
            Err(Error::new(kind, Some(span)))
        } else {
            Ok(())
        }
    }
}
impl PlaceTable {
    pub fn verify(self, limits: Limits) -> Result<VerifiedPlaces, Error> {
        self.verify_with_budget(
            &mut Budget::new(limits.ownership.unit),
            limits.ownership.constructed,
            limits.places,
        )
    }
    pub fn verify_with_budget(
        self,
        budget: &mut Budget,
        constructed_limit: usize,
        place_limit: usize,
    ) -> Result<VerifiedPlaces, Error> {
        self.verify_profile(
            budget,
            constructed_limit,
            place_limit,
            super::bodies::Profile::Synchronous,
        )
    }
    pub(crate) fn verify_profile(
        self,
        budget: &mut Budget,
        constructed_limit: usize,
        place_limit: usize,
        profile: super::bodies::Profile,
    ) -> Result<VerifiedPlaces, Error> {
        if self.functions.len() != self.declarations.functions.len() {
            return Err(Error::new(
                ErrorKind::FunctionBodyCount {
                    expected: self.declarations.functions.len(),
                    actual: self.functions.len(),
                },
                None,
            ));
        }
        let declarations = self
            .declarations
            .verify_with_budget(budget, constructed_limit)?;
        budget.charge(
            declarations
                .types()
                .table()
                .constructed
                .len()
                .saturating_add(declarations.functions().len()),
        )?;
        if let Some(span) = declarations.task_feature_span()
            && profile == super::bodies::Profile::Synchronous
        {
            return Err(Error::new(ErrorKind::UnsupportedTaskProfile, Some(span)));
        }
        let facts = validate(&declarations, &self.functions, budget, place_limit)?;
        Ok(VerifiedPlaces {
            declarations,
            functions: self.functions,
            facts,
            work: budget.work_used(),
            bytes: budget.allocation_bytes(),
            elements: budget.elements_used(),
        })
    }
}

fn constructor(types: &VerifiedTypes, ty: Type) -> Option<Constructor> {
    if let Type::Constructed(id) = ty {
        types
            .table
            .constructed
            .get(id.0 as usize)
            .map(|t| t.constructor)
    } else {
        None
    }
}
fn within(
    types: &VerifiedTypes,
    owner: Span,
    span: Span,
    budget: &mut Budget,
) -> Result<(), Error> {
    at(
        validation::source_span(&types.table.sources, Some(span), budget),
        span,
    )?;
    if owner.source != span.source || span.start < owner.start || span.end > owner.end {
        Err(Error::new(ErrorKind::SpanOutsideOwner, Some(span)))
    } else {
        Ok(())
    }
}
fn value(values: &[TypeUse], id: Register, span: Span) -> Result<Type, Error> {
    values
        .get(id.0 as usize)
        .map(|t| t.ty)
        .ok_or_else(|| Error::new(ErrorKind::InvalidValue(id), Some(span)))
}
fn validate(
    declarations: &d::VerifiedDeclarations,
    functions: &[FunctionPlaces],
    budget: &mut Budget,
    limit: usize,
) -> Result<Vec<Vec<Facts>>, Error> {
    let types = declarations.types();
    let mut all = budget.vector(functions.len())?;
    let mut registers = 0usize;
    let mut places = 0usize;
    for (i, input) in functions.iter().enumerate() {
        let declaration = &declarations.functions()[i];
        registers = registers
            .checked_add(declaration.locals.len())
            .and_then(|n| n.checked_add(input.values.len()))
            .ok_or_else(|| super::super::Error::new(super::super::ErrorKind::ArithmeticOverflow))?;
        cap(
            registers,
            budget.limits().registers,
            super::super::LimitKind::Registers,
        )?;
        u32::try_from(input.values.len())
            .map_err(|_| super::super::Error::new(super::super::ErrorKind::ArithmeticOverflow))?;
        places = places
            .checked_add(input.places.len())
            .ok_or_else(|| super::super::Error::new(super::super::ErrorKind::ArithmeticOverflow))?;
        if places > limit {
            return Err(Error::new(
                ErrorKind::PlaceLimit {
                    requested: places,
                    limit,
                },
                Some(declaration.span),
            ));
        }
        u32::try_from(input.places.len())
            .map_err(|_| super::super::Error::new(super::super::ErrorKind::ArithmeticOverflow))?;
        for t in &input.values {
            within(types, declaration.span, t.span, budget)?;
            at(budget.charge(1), t.span)?;
            if types.properties(t.ty).is_none() {
                return Err(Error::new(ErrorKind::InvalidType(t.ty), Some(t.span)));
            }
        }
        let mut seen = filled(budget, input.values.len(), false)?;
        let mut facts = budget.vector(input.places.len())?;
        for place in &input.places {
            within(types, declaration.span, place.span, budget)?;
            let expected = value(&input.values, place.origin, place.span)?;
            if input.values[place.origin.0 as usize].span != place.span {
                return Err(Error::new(ErrorKind::SpanOutsideOwner, Some(place.span)));
            }
            if seen[place.origin.0 as usize] {
                return Err(Error::new(
                    ErrorKind::DuplicatePlace(place.origin),
                    Some(place.span),
                ));
            }
            seen[place.origin.0 as usize] = true;
            cap(
                place.projections.len(),
                budget.limits().depth,
                super::super::LimitKind::Depth,
            )?;
            let (mut current, mut mutable) = match place.root {
                Root::Local(local) => {
                    let l = declaration.locals.get(local.0 as usize).ok_or_else(|| {
                        Error::new(
                            ErrorKind::InvalidLocal {
                                function: FunctionId(i as u32),
                                local: local.0,
                            },
                            Some(place.span),
                        )
                    })?;
                    (
                        l.ty.ty,
                        matches!(l.kind, d::LocalKind::Mutable | d::LocalKind::ArenaOwner),
                    )
                }
                Root::Value(v) => (value(&input.values, v, place.span)?, false),
            };
            let mut through_loan = false;
            let mut shared_path = false;
            for projection in &place.projections {
                within(types, place.span, projection.span, budget)?;
                at(budget.charge(1), projection.span)?;
                current = match projection.kind {
                    ProjectionKind::Dereference => {
                        let Some(Constructor::Loan(kind, referent)) = constructor(types, current)
                        else {
                            return Err(Error::new(
                                ErrorKind::InvalidDereference(current),
                                Some(projection.span),
                            ));
                        };
                        through_loan = true;
                        shared_path |= kind == LoanKind::Shared;
                        mutable = !shared_path;
                        referent
                    }
                    ProjectionKind::Field(field) => {
                        let Type::Nominal(id) = current else {
                            return Err(Error::new(
                                ErrorKind::ExpectedRecord(current),
                                Some(projection.span),
                            ));
                        };
                        let DefinitionKind::Record(fields) =
                            &types.table.definitions[id.0 as usize].kind
                        else {
                            return Err(Error::new(
                                ErrorKind::ExpectedRecord(current),
                                Some(projection.span),
                            ));
                        };
                        fields
                            .get(field as usize)
                            .ok_or_else(|| {
                                Error::new(
                                    ErrorKind::InvalidField {
                                        definition: id,
                                        field,
                                    },
                                    Some(projection.span),
                                )
                            })?
                            .ty
                            .ty
                    }
                };
            }
            if types.equivalent(current, expected) != Some(true) {
                return Err(Error::new(
                    ErrorKind::PlaceType {
                        expected,
                        actual: current,
                    },
                    Some(place.span),
                ));
            }
            facts.push(Facts {
                value_type: current,
                root: place.root,
                through_loan,
                whole_value: place.projections.is_empty(),
                exclusive_borrow: mutable,
            });
        }
        all.push(facts);
    }
    Ok(all)
}
