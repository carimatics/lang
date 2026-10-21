//! Typed managed services, shared by interpreted and generated task bodies.
use super::*;
use crate::buffer;
impl Access<'_, '_, '_> {
    pub fn unpack_case<P: a::Provider>(
        &mut self,
        from: usize,
        case: u32,
        mut destinations: impl ExactSizeIterator<Item = usize>,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Reason> {
        self.available(from)?;
        let root = self.root(from)?.at;
        if self.data()?.cell(from)?.case(root, allocator)? != case {
            return Err(Reason::Invariant);
        }
        if !self.is_copy(from)? {
            return match destinations.len() {
                0 => self.cleanup_slot(from, crate::cleanup::Reason::ScopeExit, allocator),
                1 => self.extract(
                    from,
                    destinations.next().ok_or(Reason::Invariant)?,
                    case,
                    0,
                    allocator,
                ),
                _ => Err(Reason::Unsupported),
            };
        }
        for (field, to) in destinations.enumerate() {
            let cell = self.data()?.cell(from)?;
            if matches!(
                cell.layouts().get(root.ty()).ok_or(Reason::Invariant)?.kind,
                l::Kind::Error { .. }
            ) {
                if field != 0 {
                    return Err(Reason::Invariant);
                }
                let payload = cell
                    .read_immediate(root, allocator)?
                    .payload()
                    .ok_or(Reason::Invariant)?;
                self.write(to, payload, allocator)?;
            } else {
                let at = cell.child(root, field as u32, allocator)?;
                self.copy_location(
                    Location {
                        activation: self.activation,
                        slot: from,
                        at,
                    },
                    self.root(to)?,
                    allocator,
                )?;
            }
        }
        Ok(())
    }
    pub fn token<P: a::Provider>(
        &self,
        location: impl Into<Target>,
        allocator: &a::Allocator<P>,
    ) -> Result<[u8; 32], Reason> {
        let location = match location.into() {
            Target::Cell(location) => location,
            target => return self.observed_token(target, allocator),
        };
        self.owner(location.activation)?.available(location.slot)?;
        let start = location.at.offset() as usize;
        self.cell(location)?
            .data(allocator)?
            .get(start..start.checked_add(32).ok_or(Reason::Invariant)?)
            .ok_or(Reason::Invariant)?
            .try_into()
            .map_err(|error| Reason::service(&error))
    }
    pub fn store_token<P: a::Provider>(
        &self,
        location: impl Into<Target>,
        token: [u8; 32],
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Reason> {
        let location = match location.into() {
            Target::Cell(location) => location,
            Target::Vector { origin, at } => {
                return self
                    .lease(origin)?
                    .vector
                    .as_ref()
                    .ok_or(Reason::Invariant)?
                    .write_buffer_token(self.services.layouts, at, token, allocator)
                    .map_err(|error| Reason::service(&error));
            }
            _ => return Err(Reason::Invariant),
        };
        let start = location.at.offset() as usize;
        self.cell(location)?
            .data_mut(allocator)?
            .get_mut(start..start.checked_add(32).ok_or(Reason::Invariant)?)
            .ok_or(Reason::Invariant)?
            .copy_from_slice(&token);
        Ok(())
    }
    pub fn extract<P: a::Provider>(
        &mut self,
        from: usize,
        to: usize,
        case: u32,
        panic_code: i64,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Reason> {
        self.available(from)?;
        let source = self.data()?.cell(from)?;
        let root = source.root();
        if source.case(root, allocator)? != case {
            return Err(Reason::Panic(Some(panic_code)));
        }
        if self.is_reference(to)? {
            let managed = self.frame.managed.as_mut().ok_or(Reason::Invariant)?;
            managed.references[to] = Some(managed.references[from].ok_or(Reason::Invariant)?);
            self.forget(from);
            let span = self.origin;
            return self.mark(to, span);
        }
        if self.is_copy(from)? {
            let at = source.child(root, 0, allocator)?;
            self.copy_location(
                Location {
                    activation: self.activation,
                    slot: from,
                    at,
                },
                self.root(to)?,
                allocator,
            )
        } else {
            let span = source
                .initialized_field(root, 0, allocator)?
                .1
                .or_else(|| self.managed.as_ref().and_then(|m| m.spans[from]))
                .unwrap_or(self.origin);
            let (source, target) = self
                .storage
                .as_mut()
                .ok_or(Reason::Invariant)?
                .two_cells_mut(from, to)?;
            source.extract_case_into(case, target, allocator)?;
            source.recycle(allocator)?;
            self.forget(from);
            self.mark(to, span)
        }
    }
    pub(super) fn immediate_result<P: a::Provider>(
        &mut self,
        to: usize,
        outcome: Result<l::Immediate, l::Immediate>,
        span: m::Span,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Reason> {
        let (case, payload) = match outcome {
            Ok(v) => (0, v),
            Err(e) => (1, e),
        };
        let cell = self.data()?.cell(to)?;
        let root = cell.root();
        cell.prepare_case(root, case, allocator)?;
        let payload_at = cell.child(root, 0, allocator)?;
        cell.write_immediate(payload_at, payload, allocator)?;
        cell.initialize_order(root, [(0, span)].into_iter(), allocator)?;
        self.mark(to, span)
    }
    pub(super) fn token_result<P: a::Provider>(
        &mut self,
        to: usize,
        outcome: Result<[u8; 32], l::Immediate>,
        span: m::Span,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Reason> {
        let cell = self.data()?.cell(to)?;
        let root = cell.root();
        match outcome {
            Ok(token) => {
                cell.prepare_case(root, 0, allocator)?;
                self.store_token(
                    Location {
                        activation: self.activation,
                        slot: to,
                        at: cell.child(root, 0, allocator)?,
                    },
                    token,
                    allocator,
                )?;
            }
            Err(error) => {
                cell.prepare_case(root, 1, allocator)?;
                cell.write_immediate(cell.child(root, 0, allocator)?, error, allocator)?;
            }
        }
        cell.initialize_order(root, [(0, span)].into_iter(), allocator)?;
        self.mark(to, span)
    }
    pub(super) fn referenced_cell(&self, slot: usize) -> Result<Target, Reason> {
        self.reference(slot)
    }
    pub(super) fn loan_sum<P: a::Provider>(
        &mut self,
        to: usize,
        case: u32,
        present: bool,
        span: m::Span,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Reason> {
        let cell = self.data()?.cell(to)?;
        let root = cell.root();
        cell.prepare_case(root, case, allocator)?;
        cell.initialize_order(root, present.then_some((0, span)).into_iter(), allocator)?;
        if !present {
            self.frame
                .managed
                .as_mut()
                .ok_or(Reason::Invariant)?
                .references[to] = None;
        }
        self.mark(to, span)
    }
    pub(super) fn retire_token<P: a::Provider>(
        &mut self,
        mut token: l::Token,
        slot: usize,
        span: m::Span,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Reason> {
        let meter = self.services.work.clone();
        meter.unmetered(|| {
            let types = self.frame.types;
            let services = &mut self.services;
            let managed = self.frame.managed.as_mut().ok_or(Reason::Invariant)?;
            let mut cleaner = c::Contextual::new(|_: &mut (), _, context: &mut c::Context<'_>| {
                services.retire(types, &mut token, allocator, context)
            });
            managed
                .session
                .retire(
                    c::Slot(slot),
                    (),
                    Some(span),
                    c::Reason::Replacement,
                    &mut cleaner,
                )
                .map_err(|_| Reason::Cleanup)
        })
    }
    #[allow(
        clippy::too_many_arguments,
        reason = "Keep verified operands, publication context and allocator explicit at the shared backend boundary."
    )]
    pub fn core<P: a::Provider>(
        &mut self,
        operation: o::core::Operation,
        mut type_arguments: impl ExactSizeIterator<Item = o::Type>,
        result_loan: Option<b::LoanId>,
        mut arguments: impl ExactSizeIterator<Item = usize>,
        to: usize,
        span: m::Span,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Reason> {
        use o::core::Operation as O;
        if operation == O::Allocator {
            if arguments.len() != 0 {
                return Err(Reason::Invariant);
            }
            return self.borrow_target(
                result_loan.ok_or(Reason::Invariant)?,
                Target::Allocator,
                to,
            );
        }
        let first = arguments.next().ok_or(Reason::Invariant)?;
        if operation.is_service() {
            return self.service_core(
                operation,
                result_loan,
                first,
                arguments,
                to,
                span,
                allocator,
            );
        }
        if operation.is_channel() {
            return self.channel_core(operation, first, arguments, to, span, allocator);
        }
        if matches!(operation, O::ArenaNew | O::ArenaRead | O::ArenaReplace) {
            return self.arena_core(
                operation,
                type_arguments.next().ok_or(Reason::Invariant)?,
                result_loan,
                first,
                arguments,
                to,
                span,
                allocator,
            );
        }
        if let Some(contract) = operation.extraction() {
            return self.extract(first, to, contract.case, contract.panic_code, allocator);
        }
        if matches!(
            operation,
            O::VecNew | O::VecLen | O::VecPush | O::VecPop | O::VecGet | O::VecGetMut
        ) {
            return self.vector_core(
                operation,
                type_arguments.next().ok_or(Reason::Invariant)?,
                result_loan,
                first,
                arguments,
                to,
                span,
                allocator,
            );
        }
        if matches!(
            operation,
            O::MapNew | O::MapLen | O::MapInsert | O::MapGet | O::MapRemove
        ) {
            let key = type_arguments.next().ok_or(Reason::Invariant)?;
            let value = type_arguments.next().ok_or(Reason::Invariant)?;
            return self.map_core(
                operation,
                key,
                value,
                result_loan,
                first,
                arguments,
                to,
                span,
                allocator,
            );
        }
        if super::super::services::shared(operation) {
            return self.shared_core(
                operation,
                type_arguments.next().ok_or(Reason::Invariant)?,
                result_loan,
                first,
                arguments,
                to,
                span,
                allocator,
            );
        }
        match operation {
            O::BytesNew => {
                if !matches!(self.reference(first)?, Target::Allocator) {
                    return Err(Reason::Invariant);
                }
                let outcome = buffer::Bytes::new(allocator);
                let cell = self.data()?.cell(to)?;
                let root = cell.root();
                match outcome {
                    Ok(mut bytes) => {
                        cell.prepare_case(root, 0, allocator)?;
                        let at = cell.child(root, 0, allocator)?;
                        let token = bytes
                            .take_token(allocator)
                            .map_err(|error| Reason::service(&error))?;
                        self.store_token(
                            Location {
                                activation: self.activation,
                                slot: to,
                                at,
                            },
                            token,
                            allocator,
                        )?;
                    }
                    Err(buffer::Failure::Error(error)) => {
                        cell.prepare_case(root, 1, allocator)?;
                        let at = cell.child(root, 0, allocator)?;
                        cell.write_immediate(at, l::Immediate::Allocation(error), allocator)?;
                    }
                    Err(buffer::Failure::Fault(error)) => return Err(Reason::service(&error)),
                }
                cell.initialize_order(root, [(0, span)].into_iter(), allocator)?;
                self.mark(to, span)
            }
            O::BytesLen => {
                let target = self.reference(first)?;
                let mut token = self.token(target, allocator)?;
                let bytes = buffer::Bytes::from_token(&mut token, allocator)
                    .map_err(|error| Reason::service(&error))?;
                let length = bytes
                    .len(allocator)
                    .map_err(|error| Reason::service(&error))?;
                self.write(to, l::Immediate::I64(length), allocator)
            }
            O::BytesPush => {
                let second = arguments.next().ok_or(Reason::Invariant)?;
                let l::Immediate::I64(value) = self.read(second, allocator)? else {
                    return Err(Reason::Invariant);
                };
                let target = self.reference(first)?;
                let mut token = self.token(target, allocator)?;
                let mut bytes = buffer::Bytes::from_token(&mut token, allocator)
                    .map_err(|error| Reason::service(&error))?;
                let meter = self.services.work.clone();
                let outcome =
                    bytes.push_metered(allocator, value, &mut |amount| meter.charge(amount));
                let token = bytes
                    .take_token(allocator)
                    .map_err(|error| Reason::service(&error))?;
                self.store_token(target, token, allocator)?;
                let outcome = match outcome {
                    Ok(()) => Ok(l::Immediate::Unit),
                    Err(buffer::Failure::Error(error)) => Err(l::Immediate::Buffer(error)),
                    Err(buffer::Failure::Fault(error)) => return Err(Reason::service(&error)),
                };
                self.immediate_result(to, outcome, span, allocator)
            }
            _ => self.buffer_core(operation, first, arguments, to, span, allocator),
        }
    }
}
