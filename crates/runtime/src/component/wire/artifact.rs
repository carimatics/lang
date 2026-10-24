//! RFC 0028 complete LANGBC Component metadata suffix. No guest source is
//! parsed here; the shared wire session accounts every traversal and buffer.
use super::super::{bindings as b, idl};
use super::*;
use idl::{CheckedDocument, DefinitionKind, Document, Spanned, TypeKind};
mod read;
mod write;

pub struct Metadata {
    pub document: CheckedDocument,
    pub mappings: b::Map,
}

fn valid_interval(
    min: &super::super::negotiation::SemanticVersion,
    max: &super::super::negotiation::SemanticVersion,
    session: &mut Session,
    at: usize,
) -> Result<(), Error> {
    use super::super::negotiation::PrereleaseIdentifier;
    for version in [min, max] {
        session.charge(1, at)?;
        for part in version.prerelease() {
            let (PrereleaseIdentifier::Numeric(text) | PrereleaseIdentifier::AlphaNumeric(text)) =
                part;
            session.charge(Session::add(text.len(), 1, at)?, at)?;
        }
    }
    if min.precedence_cmp(max).is_lt() {
        Ok(())
    } else {
        Err(session.error(at, ErrorKind::InvalidSchema))
    }
}

pub fn prepare(
    document: Document,
    mappings: b::Map,
    sources: &[m::Source],
    session: &mut Session,
    binding_limit: usize,
) -> Result<Metadata, Error> {
    session.metadata(|session, limits| {
        let limits = idl::Limits {
            depth: limits.depth.min(session.limits.depth),
            ..limits
        };
        let counts = write::measure(&document, &mappings, sources, session, binding_limit)?;
        let budget = m::Budget::with_usage(session.budget.limits(), session.preparation_usage())
            .map_err(|e| Session::budget_error(0, e))?;
        let (result, budget) = idl::check_decoded(
            document,
            limits,
            budget,
            counts.declarations,
            counts.elements,
        );
        session.budget = budget;
        let document = result.map_err(|e| session.error(0, ErrorKind::Declaration(e)))?;
        Ok(Metadata { document, mappings })
    })
}
pub fn encode(
    document: &CheckedDocument,
    mappings: &b::Map,
    sources: &[m::Source],
    session: &mut Session,
    binding_limit: usize,
) -> Result<Vec<u8>, Error> {
    session.metadata(|session, limits| {
        let limits = idl::Limits {
            depth: limits.depth.min(session.limits.depth),
            ..limits
        };
        super::contract::proof_limits(document.usage(), limits, session)?;
        write::encode(
            document.document(),
            mappings,
            sources,
            session,
            binding_limit,
        )
    })
}
pub fn decode(
    bytes: &[u8],
    sources: &[m::Source],
    session: &mut Session,
    binding_limit: usize,
) -> Result<Metadata, Error> {
    session.metadata(|session, _| {
        let (document, mappings) = read::decode(bytes, sources, session, binding_limit)?;
        prepare(document, mappings, sources, session, binding_limit)
    })
}
/// Derive the source contract from decoded data under a bounded session. This
/// preserves all original origins/type IDs for exact Unit/Contract pairing.
pub fn source_contract(
    unit: &b::Unit,
    session: &mut Session,
    binding_limit: usize,
) -> Result<Contract, Error> {
    let sources = &unit.code().declarations().types().table().sources;
    let bytes = encode(
        unit.document(),
        unit.mappings(),
        sources,
        session,
        binding_limit,
    )?;
    let metadata = decode(&bytes, sources, session, binding_limit)?;
    Contract::new(metadata.document, Vec::new(), session)
}
pub(super) fn valid_span(span: m::Span, source: m::SourceId, text: &str) -> bool {
    span.source == source
        && span.start <= span.end
        && (span.end as usize) <= text.len()
        && text.is_char_boundary(span.start as usize)
        && text.is_char_boundary(span.end as usize)
}
fn spans(
    document: &mut Document,
    mut f: impl FnMut(&mut m::Span) -> Result<(), Error>,
) -> Result<(), Error> {
    f(&mut document.package.span)?;
    f(&mut document.version.span)?;
    for i in &mut document.interfaces {
        f(&mut i.key.span)?;
        f(&mut i.span)?;
        for r in &mut i.resources {
            f(&mut r.span)?;
        }
        for d in &mut i.definitions {
            f(&mut d.key.span)?;
            f(&mut d.span)?;
            match &mut d.kind {
                DefinitionKind::Record(fields) | DefinitionKind::Variant(fields) => {
                    for field in fields {
                        f(&mut field.name.span)?;
                    }
                }
                DefinitionKind::Alias(_) => {}
            }
        }
        for fun in &mut i.functions {
            f(&mut fun.name.span)?;
            f(&mut fun.effect.span)?;
            f(&mut fun.capability.span)?;
            f(&mut fun.span)?;
            for param in &mut fun.params {
                f(&mut param.name.span)?;
            }
        }
    }
    for i in &mut document.imports {
        f(&mut i.interface.span)?;
        f(&mut i.requirement.span)?;
        f(&mut i.span)?;
        for member in &mut i.members {
            f(&mut member.span)?;
        }
    }
    for e in &mut document.exports {
        f(&mut e.span)?;
    }
    for node in &mut document.type_nodes {
        f(&mut node.span)?;
        match &mut node.kind {
            TypeKind::Named(key) => f(&mut key.span)?,
            TypeKind::Own(key) | TypeKind::Borrow { resource: key, .. } => f(&mut key.span)?,
            _ => {}
        }
    }
    Ok(())
}
/// Localize a compiler/global IDL origin without changing offsets or inventing
/// source entries. The caller supplies the actual owning snapshot explicitly.
pub fn relocate(
    document: &mut Document,
    sources: &mut Vec<m::Source>,
    snapshot: m::Source,
    source_limit: usize,
    session: &mut Session,
) -> Result<(), Error> {
    session.cap(
        snapshot.text.len(),
        session.limits.metadata.source_bytes,
        LimitKind::Bytes,
        0,
    )?;
    if snapshot.text.len() != document.source_bytes as usize {
        return Err(session.error(0, ErrorKind::InvalidSchema));
    }
    let old = document.source;
    spans(document, |span| {
        session.charge(1, 0)?;
        if !valid_span(*span, old, &snapshot.text) {
            return Err(session.error(0, ErrorKind::InvalidSchema));
        }
        Ok(())
    })?;
    let mut found = None;
    for (index, source) in sources.iter().enumerate() {
        session.charge(Session::add(source.path.len(), snapshot.path.len(), 0)?, 0)?;
        if source.path == snapshot.path {
            session.charge(Session::add(source.text.len(), snapshot.text.len(), 0)?, 0)?;
            if source.text == snapshot.text {
                found = Some(index);
                break;
            }
        }
    }
    let index = if let Some(index) = found {
        index
    } else {
        let index = sources.len();
        session.cap(
            Session::add(index, 1, 0)?,
            source_limit,
            LimitKind::Declarations,
            0,
        )?;
        session.push(sources, snapshot, 0)?;
        index
    };
    let source =
        m::SourceId(u32::try_from(index).map_err(|_| session.error(0, ErrorKind::InvalidSchema))?);
    spans(document, |span| {
        session.charge(1, 0)?;
        span.source = source;
        Ok(())
    })?;
    document.source = source;
    Ok(())
}
