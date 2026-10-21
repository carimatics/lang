//! Common checked arena publication over inline and indirect owned values.
use super::*;
pub fn inspect<P: a::Provider>(
    layouts: &l::Layouts<'_>,
    inspector: &mut i::Inspector<'_, '_>,
    token: &l::Token,
    arena: &Arena<'_, '_>,
    allocator: &a::Allocator<P>,
    budget: &mut PublicationBudget,
) -> Result<(), Error> {
    let mut read = inspector.token(token, allocator)?;
    while let Some(observed) = budget.inspect_next(&mut read)? {
        let mut bytes = observed.bytes;
        let Some(l::Layout {
            kind: l::Kind::Token(constructor),
            ..
        }) = layouts.get(observed.ty)
        else {
            return Err(Error {
                kind: Kind::State,
                span: observed.span,
            });
        };
        match constructor {
            o::Constructor::Arena(ty) => {
                arena.resolve(Reference::from_token(bytes)?, *ty, allocator)?
            }
            o::Constructor::Shared(_) | o::Constructor::Weak(_) => {
                return Err(Error {
                    kind: Kind::Arena(l::ArenaError::CrossRegion),
                    span: observed.span,
                });
            }
            o::Constructor::Builtin(o::Builtin::Bytes) => {
                buffer::Bytes::from_token(&mut bytes, allocator)
                    .and_then(|v| v.len(allocator))
                    .map_err(|e| Error {
                        kind: Kind::Buffer(e),
                        span: observed.span,
                    })?;
            }
            o::Constructor::Builtin(o::Builtin::Text) => {
                let text = buffer::Text::from_token_metered(&mut bytes, allocator, &mut |n| {
                    budget.service(n)
                })
                .map_err(|e| Error {
                    kind: Kind::Buffer(e),
                    span: observed.span,
                })?;
                let length = usize::try_from(text.len(allocator).map_err(|e| Error {
                    kind: Kind::Buffer(e),
                    span: observed.span,
                })?)
                .map_err(|_| Error {
                    kind: Kind::State,
                    span: observed.span,
                })?;
                budget.observe(length)?;
                text.as_str(allocator).map_err(|e| Error {
                    kind: Kind::Buffer(e),
                    span: observed.span,
                })?;
            }
            _ => {
                return Err(Error {
                    kind: Kind::Unsupported(observed.ty),
                    span: observed.span,
                });
            }
        }
    }
    Ok(())
}
