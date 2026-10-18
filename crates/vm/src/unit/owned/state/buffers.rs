use super::*;

impl<P: a::Provider> State<'_, '_, '_, P> {
    pub(super) fn token(&self, target: Target) -> Step<[u8; 32]> {
        if let Target::Map { read, location } = target {
            return self
                .map_view(read)?
                .read_token(location, self.allocator)
                .map(|t| t.bytes)
                .map_err(invariant);
        }
        if let Target::Vector { read, location } = target {
            return self
                .vector_view(read)?
                .read_token(location, self.allocator)
                .map(|t| t.bytes)
                .map_err(invariant);
        }
        if let Target::Shared { read, ty, location } = target {
            return self
                .shared_view(read, ty)?
                .read_token(location, self.allocator)
                .map(|t| t.bytes)
                .map_err(invariant);
        }
        if let Target::Arena {
            owner,
            read,
            location,
        } = target
        {
            let view = self.arena_view(owner, read)?;
            let start = location.offset() as usize;
            return view
                .data(self.allocator)
                .map_err(invariant)?
                .get(
                    start
                        ..start
                            .checked_add(32)
                            .ok_or(c::ExecutionFailure::Invariant)?,
                )
                .ok_or(c::ExecutionFailure::Invariant)?
                .try_into()
                .map_err(invariant);
        }
        let Target::Data { cell, location } = target else {
            return Err(c::ExecutionFailure::Invariant);
        };
        let start = location.offset() as usize;
        self.storage
            .as_ref()
            .ok_or(c::ExecutionFailure::Invariant)?
            .cell(cell)
            .map_err(invariant)?
            .data(self.allocator)
            .map_err(invariant)?
            .get(
                start
                    ..start
                        .checked_add(32)
                        .ok_or(c::ExecutionFailure::Invariant)?,
            )
            .ok_or(c::ExecutionFailure::Invariant)?
            .try_into()
            .map_err(invariant)
    }
    fn store_token(&mut self, target: Target, token: [u8; 32]) -> Step<()> {
        if let Target::Vector { read, location } = target {
            return self.loans[read]
                .vector_read
                .as_ref()
                .ok_or(c::ExecutionFailure::Invariant)?
                .write_buffer_token(self.layouts, location, token, self.allocator)
                .map_err(invariant);
        }
        let Target::Data { cell, location } = target else {
            return Err(c::ExecutionFailure::Invariant);
        };
        let start = location.offset() as usize;
        self.storage
            .as_ref()
            .ok_or(c::ExecutionFailure::Invariant)?
            .cell(cell)
            .map_err(invariant)?
            .data_mut(self.allocator)
            .map_err(invariant)?
            .get_mut(
                start
                    ..start
                        .checked_add(32)
                        .ok_or(c::ExecutionFailure::Invariant)?,
            )
            .ok_or(c::ExecutionFailure::Invariant)?
            .copy_from_slice(&token);
        Ok(())
    }
    fn borrowed_bytes(&self, reference: usize) -> Step<(Target, buffer::Bytes)> {
        let target = self.reference(self.root(reference)?)?;
        let bytes = buffer::Bytes::from_token(&mut self.token(target)?, self.allocator)
            .map_err(|_| c::ExecutionFailure::Fault)?;
        Ok((target, bytes))
    }
    pub(super) fn immediate_result(
        &mut self,
        destination: usize,
        outcome: Result<l::Immediate, l::Immediate>,
        span: m::Span,
    ) -> Step<()> {
        if !self.is_copy(destination) {
            return Err(c::ExecutionFailure::Invariant);
        }
        let (case, payload) = match outcome {
            Ok(value) => (0, value),
            Err(error) => (1, error),
        };
        let cell = self
            .storage
            .as_ref()
            .ok_or(c::ExecutionFailure::Invariant)?
            .cell(self.cells[destination].physical)
            .map_err(invariant)?;
        let root = cell.root();
        cell.prepare_case(root, case, self.allocator)
            .map_err(invariant)?;
        let at = cell.child(root, 0, self.allocator).map_err(invariant)?;
        cell.write_immediate(at, payload, self.allocator)
            .map_err(invariant)?;
        cell.initialize_order(root, [(0, span)].into_iter(), self.allocator)
            .map_err(invariant)?;
        self.cells[destination].live = true;
        Ok(())
    }
    pub fn bytes_get(
        &mut self,
        reference: usize,
        index: i64,
        destination: usize,
        span: m::Span,
    ) -> Step<()> {
        let (_, bytes) = self.borrowed_bytes(reference)?;
        let raw = {
            let (allocator, mut work) = self.buffer_service();
            bytes.get_metered(allocator, index, &mut work)
        };
        let outcome = match raw {
            Ok(value) => Ok(l::Immediate::I64(value)),
            Err(buffer::Failure::Error(error)) => Err(l::Immediate::Bounds(error)),
            Err(buffer::Failure::Fault(error)) => return Err(buffer_fault(error)),
        };
        self.immediate_result(destination, outcome, span)
    }
    fn finish_write(
        &mut self,
        target: Target,
        mut bytes: buffer::Bytes,
        outcome: buffer::Outcome<(), buffer::BufferError>,
        destination: usize,
        span: m::Span,
    ) -> Step<()> {
        // Growth may replace the heap handle. Restore the actual token to the
        // loan's owner before any guest failure/control boundary, including Err.
        let token = bytes
            .take_token(self.allocator)
            .map_err(|_| c::ExecutionFailure::Fault)?;
        self.store_token(target, token)?;
        let outcome = match outcome {
            Ok(()) => Ok(l::Immediate::Unit),
            Err(buffer::Failure::Error(error)) => Err(l::Immediate::Buffer(error)),
            Err(buffer::Failure::Fault(error)) => return Err(buffer_fault(error)),
        };
        self.immediate_result(destination, outcome, span)
    }
    pub fn bytes_push(
        &mut self,
        reference: usize,
        value: i64,
        destination: usize,
        span: m::Span,
    ) -> Step<()> {
        let (target, mut bytes) = self.borrowed_bytes(reference)?;
        let outcome = {
            let (allocator, mut work) = self.buffer_service();
            bytes.push_metered(allocator, value, &mut work)
        };
        self.finish_write(target, bytes, outcome, destination, span)
    }
    pub fn bytes_set(
        &mut self,
        reference: usize,
        index: i64,
        value: i64,
        destination: usize,
        span: m::Span,
    ) -> Step<()> {
        let (target, mut bytes) = self.borrowed_bytes(reference)?;
        let outcome = {
            let (allocator, mut work) = self.buffer_service();
            bytes.set_metered(allocator, index, value, &mut work)
        };
        self.finish_write(target, bytes, outcome, destination, span)
    }
    fn borrowed_text(&mut self, reference: usize) -> Step<buffer::Text> {
        let target = self.reference(self.root(reference)?)?;
        let mut token = self.token(target)?;
        let (allocator, mut work) = self.buffer_service();
        buffer::Text::from_token_metered(&mut token, allocator, &mut work).map_err(buffer_fault)
    }
    pub fn require_allocator(&self, reference: usize) -> Step<()> {
        match self.reference(self.root(reference)?)? {
            Target::Allocator => Ok(()),
            _ => Err(c::ExecutionFailure::Invariant),
        }
    }
    pub fn text_len(&mut self, reference: usize) -> Step<i64> {
        let text = self.borrowed_text(reference)?;
        text.len(self.allocator)
            .map_err(|_| c::ExecutionFailure::Fault)
    }
    pub fn text_equal(&mut self, left: usize, right: usize) -> Step<bool> {
        let left = self.borrowed_text(left)?;
        let right = self.borrowed_text(right)?;
        let (allocator, mut work) = self.buffer_service();
        left.equal_metered(&right, allocator, &mut work)
            .map_err(buffer_fault)
    }
    pub fn allocate_buffer(
        &mut self,
        operation: o::core::Operation,
        reference: usize,
        other: Option<usize>,
        destination: usize,
        span: m::Span,
    ) -> Step<()> {
        if self.cells[destination].live {
            return Err(c::ExecutionFailure::Invariant);
        }
        let outcome = match operation {
            o::core::Operation::BytesClone => {
                let (_, bytes) = self.borrowed_bytes(reference)?;
                {
                    let (allocator, mut work) = self.buffer_service();
                    bytes
                        .try_clone_metered(allocator, &mut work)
                        .and_then(|mut value| {
                            value.take_token(allocator).map_err(buffer::Failure::Fault)
                        })
                }
            }
            o::core::Operation::TextClone => {
                let text = self.borrowed_text(reference)?;
                {
                    let (allocator, mut work) = self.buffer_service();
                    text.try_clone_metered(allocator, &mut work)
                        .and_then(|mut value| {
                            value.take_token(allocator).map_err(buffer::Failure::Fault)
                        })
                }
            }
            o::core::Operation::TextBytes => {
                let text = self.borrowed_text(reference)?;
                {
                    let (allocator, mut work) = self.buffer_service();
                    text.to_bytes_metered(allocator, &mut work)
                        .and_then(|mut value| {
                            value.take_token(allocator).map_err(buffer::Failure::Fault)
                        })
                }
            }
            o::core::Operation::TextConcat => {
                let left = self.borrowed_text(reference)?;
                let right = self.borrowed_text(other.ok_or(c::ExecutionFailure::Invariant)?)?;
                {
                    let (allocator, mut work) = self.buffer_service();
                    left.concat_metered(&right, allocator, &mut work)
                        .and_then(|mut value| {
                            value.take_token(allocator).map_err(buffer::Failure::Fault)
                        })
                }
            }
            _ => return Err(c::ExecutionFailure::Invariant),
        };
        let case = match &outcome {
            Ok(_) => 0,
            Err(buffer::Failure::Error(_)) => 1,
            Err(buffer::Failure::Fault(error)) => return Err(buffer_fault(*error)),
        };
        let cell = self
            .storage
            .as_ref()
            .ok_or(c::ExecutionFailure::Invariant)?
            .cell(self.cells[destination].physical)
            .map_err(invariant)?;
        let root = cell.root();
        cell.prepare_case(root, case, self.allocator)
            .map_err(invariant)?;
        let payload = cell.child(root, 0, self.allocator).map_err(invariant)?;
        match outcome {
            Ok(token) => {
                let start = payload.offset() as usize;
                cell.data_mut(self.allocator)
                    .map_err(invariant)?
                    .get_mut(
                        start
                            ..start
                                .checked_add(32)
                                .ok_or(c::ExecutionFailure::Invariant)?,
                    )
                    .ok_or(c::ExecutionFailure::Invariant)?
                    .copy_from_slice(&token);
            }
            Err(buffer::Failure::Error(error)) => cell
                .write_immediate(payload, l::Immediate::Allocation(error), self.allocator)
                .map_err(invariant)?,
            Err(buffer::Failure::Fault(_)) => unreachable!("handled before publication"),
        }
        cell.initialize_order(root, [(0, span)].into_iter(), self.allocator)
            .map_err(invariant)?;
        self.cells[destination].live = true;
        Ok(())
    }
    pub fn text_from_utf8(&mut self, source: usize, destination: usize, span: m::Span) -> Step<()> {
        if self.cells[destination].live {
            return Err(c::ExecutionFailure::Invariant);
        }
        let input = self.root(source)?;
        let mut bytes = buffer::Bytes::from_token(&mut self.token(input)?, self.allocator)
            .map_err(|_| c::ExecutionFailure::Fault)?;
        let outcome = {
            let (allocator, mut work) = self.buffer_service();
            bytes.take_text_metered(allocator, &mut work)
        };
        let cell = self
            .storage
            .as_ref()
            .ok_or(c::ExecutionFailure::Invariant)?
            .cell(self.cells[destination].physical)
            .map_err(invariant)?;
        let root = cell.root();
        match outcome {
            Ok(mut text) => {
                cell.prepare_case(root, 0, self.allocator)
                    .map_err(invariant)?;
                let at = cell.child(root, 0, self.allocator).map_err(invariant)?;
                let token = text
                    .take_token(self.allocator)
                    .map_err(|_| c::ExecutionFailure::Fault)?;
                let target = Target::Data {
                    cell: self.cells[destination].physical,
                    location: at,
                };
                self.store_token(target, token)?;
            }
            Err(buffer::Failure::Error(error)) => {
                cell.prepare_case(root, 1, self.allocator)
                    .map_err(invariant)?;
                let at = cell.child(root, 0, self.allocator).map_err(invariant)?;
                cell.write_immediate(at, l::Immediate::Utf8(error), self.allocator)
                    .map_err(invariant)?;
            }
            // Buffer preflight faults preserve the source owner for cleanup.
            Err(buffer::Failure::Fault(error)) => return Err(buffer_fault(error)),
        }
        let cell = self
            .storage
            .as_ref()
            .unwrap()
            .cell(self.cells[destination].physical)
            .map_err(invariant)?;
        cell.initialize_order(root, [(0, span)].into_iter(), self.allocator)
            .map_err(invariant)?;
        // Bytes is a flat token with no construction metadata. Both ordinary
        // outcomes consume it; the result or buffer service now owns retirement.
        self.store_token(input, [0; 32])?;
        self.cells[source].live = false;
        self.cells[destination].live = true;
        Ok(())
    }
}
