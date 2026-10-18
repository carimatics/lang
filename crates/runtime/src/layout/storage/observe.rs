//! Copyable, private read coordinates. No mutation, release or transfer surface
//! escapes through this type. An inspection session also borrows the allocator.
use super::*;

#[derive(Clone, Copy)]
pub(crate) struct ReadView<'l, 't> {
    layouts: &'l Layouts<'t>,
    ty: o::Type,
    data: Handle,
    metadata: Handle,
    slice: Option<Slice>,
}
impl<'l, 't> Reservation<'l, 't> {
    pub(crate) fn observe(&self) -> Result<ReadView<'l, 't>, Error> {
        if self.state != State::Prepared {
            return Err(Error::State);
        }
        Ok(ReadView {
            layouts: self.layouts,
            ty: self.ty,
            data: self.data,
            metadata: self.metadata,
            slice: self.slice,
        })
    }
}
impl ReadView<'_, '_> {
    fn value(&self) -> Reservation<'_, '_> {
        Reservation {
            layouts: self.layouts,
            ty: self.ty,
            data: self.data,
            metadata: self.metadata,
            top: 0,
            outstanding: false,
            state: State::Prepared,
            slice: self.slice,
        }
    }
    pub(crate) fn root(&self) -> Location {
        self.value().root()
    }
    pub(crate) fn child<P: Provider>(
        &self,
        at: Location,
        field: u32,
        allocator: &Allocator<P>,
    ) -> Result<Location, Error> {
        self.value().child(at, field, allocator)
    }
    pub(crate) fn case<P: Provider>(
        &self,
        at: Location,
        allocator: &Allocator<P>,
    ) -> Result<u32, Error> {
        self.value().case(at, allocator)
    }
    pub(crate) fn immediate<P: Provider>(
        &self,
        at: Location,
        allocator: &Allocator<P>,
    ) -> Result<Immediate, Error> {
        self.value().read_immediate(at, allocator)
    }
    pub(crate) fn token<P: Provider>(
        &self,
        at: Location,
        allocator: &Allocator<P>,
    ) -> Result<Token, Error> {
        self.value().read_token(at, allocator)
    }
    pub(crate) fn initialized<P: Provider>(
        &self,
        at: Location,
        rank: u32,
        allocator: &Allocator<P>,
    ) -> Result<(u32, Option<m::Span>), Error> {
        self.value().initialized_field(at, rank, allocator)
    }
    pub(crate) fn initialized_aggregate<P: Provider>(
        &self,
        at: Location,
        allocator: &Allocator<P>,
    ) -> Result<(), Error> {
        let value = self.value();
        let layout = value.validate_location(at)?;
        if layout.metadata_bytes != 0 {
            let case = if matches!(layout.kind, Kind::Enum { .. }) {
                u64::from(value.case(at, allocator)?)
            } else {
                0
            };
            if read_u64(allocator.bytes(self.metadata)?, at.metadata as usize)? != (case << 32) | 1
            {
                return Err(Error::Uninitialized);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inspection as i;

    #[test]
    fn inspection_rejects_duplicate_initializer_entries_before_exposing_a_child() {
        let span = m::Span {
            source: m::SourceId(0),
            start: 0,
            end: 1,
        };
        let ty = o::Type::Nominal(m::DefinitionId(0));
        let bytes = o::Type::Constructed(o::ConstructedId(0));
        let types = o::TypeTable {
            sources: vec![m::Source {
                path: "order.lang".into(),
                text: "x".into(),
            }],
            constructed: vec![o::Constructed {
                constructor: o::Constructor::Builtin(o::Builtin::Bytes),
                span,
            }],
            definitions: vec![o::Definition {
                name: "Pair".into(),
                public: false,
                span,
                kind: o::DefinitionKind::Record(
                    ["a", "b"]
                        .into_iter()
                        .map(|name| o::Field {
                            name: name.into(),
                            span,
                            ty: o::TypeUse { ty: bytes, span },
                        })
                        .collect(),
                ),
            }],
        }
        .verify(Default::default())
        .unwrap();
        let layouts = Layouts::compile(&types, Default::default()).unwrap();
        let mut allocator = Allocator::new(allocation::System, Default::default()).unwrap();
        let mut value = Reservation::allocate(&layouts, ty, &mut allocator).unwrap();
        value
            .initialize_order(
                value.root(),
                [(0, span), (1, span)].into_iter(),
                &mut allocator,
            )
            .unwrap();
        let before = allocator.bytes(value.metadata).unwrap().to_vec();
        let second = (ORDER_HEADER + ORDER_ENTRY) as usize;
        allocator.bytes_mut(value.metadata).unwrap()[second..second + 4]
            .copy_from_slice(&0u32.to_le_bytes());
        let invalid = allocator.bytes(value.metadata).unwrap().to_vec();
        let mut inspector =
            i::Inspector::prepare(&layouts, 4, Default::default(), layouts.usage()).unwrap();
        let mut read = inspector.read(&value, None, &allocator).unwrap();
        assert_eq!(
            read.next(&mut i::Work::new(10)),
            Err(i::Error::Layout(Error::InvalidOrder))
        );
        assert_eq!(allocator.bytes(value.metadata).unwrap(), invalid);
        allocator
            .bytes_mut(value.metadata)
            .unwrap()
            .copy_from_slice(&before);
        // No actual buffer owners were installed in this forged-input probe.
        value.release_storage(&mut allocator).unwrap();
        assert_eq!(allocator.stats().live_allocations, 0);
    }
}
