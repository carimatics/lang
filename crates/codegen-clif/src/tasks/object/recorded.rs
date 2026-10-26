use super::*;
use crate::{
    native_record::{self, Kind, Record},
    program::FunctionCode,
};

/// Passive exact-build object data without borrows of the producer program.
pub struct RecordedObject {
    record: Record,
}
impl ObjectArtifact<'_> {
    pub fn into_recorded(self, limits: m::Limits) -> Result<RecordedObject, Error> {
        let mut budget = m::Budget::new(limits);
        let mut functions = budget.vector(self.image.functions.len())?;
        for function in self.image.functions.into_iter().flatten() {
            budget.charge(1)?;
            functions.push(function.metadata);
        }
        let kind = if self.image.component.is_some() {
            Kind::Component(self.image.entry)
        } else {
            Kind::Task(self.image.entry)
        };
        Ok(RecordedObject {
            record: Record::new(crate::Triple::host(), kind, self.bytes, functions),
        })
    }
}
impl RecordedObject {
    pub fn into_record(self) -> Record {
        self.record
    }
    pub fn from_record(record: Record) -> Result<Self, native_record::Error> {
        if !matches!(record.kind(), Kind::Task(_) | Kind::Component(_)) {
            return Err(native_record::Error {
                offset: 17,
                kind: native_record::ErrorKind::Profile,
            });
        }
        Ok(Self { record })
    }
    /// Rebuild checked native nodes without ISA lookup, code emission or JIT.
    /// The proof must match the exact semantic bytes paired with this record.
    pub fn prepare(
        self,
        proof: &o::tasks::VerifiedTasks,
        limits: m::Limits,
    ) -> Result<ObjectArtifact<'_>, Error> {
        let Kind::Task(entry) = self.record.kind() else {
            return Err(Error::Profile);
        };
        self.prepare_profile(proof, entry, None, limits)
    }
    /// Retain the original explicitly bound component and its export authority.
    pub fn prepare_component<'p>(
        self,
        program: &'p lang_runtime::component::program::Program<'p>,
        limits: m::Limits,
    ) -> Result<ObjectArtifact<'p>, Error> {
        let Kind::Component(entry) = self.record.kind() else {
            return Err(Error::Profile);
        };
        if !program.is_export(entry) {
            return Err(Error::Entry);
        }
        self.prepare_profile(program.proof(), entry, Some(program), limits)
    }
    fn prepare_profile<'p>(
        self,
        proof: &'p o::tasks::VerifiedTasks,
        entry: m::FunctionId,
        component: Option<&'p lang_runtime::component::program::Program<'p>>,
        limits: m::Limits,
    ) -> Result<ObjectArtifact<'p>, Error> {
        let record = self.record;
        let expected = crate::Triple::host();
        if record.target != expected {
            return Err(Error::Target {
                expected,
                recorded: record.target,
            });
        }
        let mut records = record.functions;
        let mut prefix = None;
        let (functions, _, budget) = prepare_program_with::<()>(
            proof,
            entry,
            limits,
            component.is_some(),
            component,
            |function, _, budget| {
                let fail = |kind| Error::Metadata { function, kind };
                if prefix.is_none() {
                    budget.allocate::<FunctionCode>(records.capacity())?;
                    for source in &proof
                        .typed()
                        .places()
                        .declarations()
                        .types()
                        .table()
                        .sources
                    {
                        budget.charge(source.text.len() / 1024 + 1)?;
                    }
                    let value = object_prefix(proof, entry, component.is_some());
                    budget.allocate::<u8>(value.capacity())?;
                    prefix = Some(value);
                }
                let mut steps = 0;
                let index = records.binary_search_by(|record| {
                    steps += 1;
                    record.function.0.cmp(&function.0)
                });
                budget.charge(steps)?;
                let metadata = &mut records[index.map_err(|_| fail(MetadataError::FunctionSet))?];
                budget.allocate::<u8>(metadata.symbol.capacity())?;
                budget
                    .allocate::<crate::program::SourceRange>(metadata.source_ranges.capacity())?;
                if metadata.symbol != format!("{}_f{}", prefix.as_ref().unwrap(), function.0) {
                    return Err(fail(MetadataError::Symbol));
                }
                if metadata.code_size == 0 {
                    return Err(fail(MetadataError::CodeRange));
                }
                budget.charge(metadata.source_ranges.len())?;
                for range in &metadata.source_ranges {
                    if range.start >= range.end || range.end as usize > metadata.code_size {
                        return Err(fail(MetadataError::CodeRange));
                    }
                    let valid = proof
                        .typed()
                        .places()
                        .declarations()
                        .types()
                        .table()
                        .sources
                        .get(range.span.source.0 as usize)
                        .is_some_and(|source| {
                            let start = range.span.start as usize;
                            let end = range.span.end as usize;
                            start <= end
                                && end <= source.text.len()
                                && source.text.is_char_boundary(start)
                                && source.text.is_char_boundary(end)
                        });
                    if !valid {
                        return Err(fail(MetadataError::SourceSpan));
                    }
                }
                Ok((
                    (),
                    FunctionCode {
                        function,
                        symbol: std::mem::take(&mut metadata.symbol),
                        code_size: metadata.code_size,
                        source_ranges: std::mem::take(&mut metadata.source_ranges),
                    },
                ))
            },
        )?;
        if let Some(extra) = records.iter().find(|record| !record.symbol.is_empty()) {
            return Err(Error::Metadata {
                function: extra.function,
                kind: MetadataError::FunctionSet,
            });
        }
        Ok(ObjectArtifact {
            bytes: record.bytes,
            image: Image {
                component,
                proof,
                entry,
                functions,
                usage: usage(&budget),
            },
        })
    }
}
