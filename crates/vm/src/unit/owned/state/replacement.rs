use super::*;

impl<P: a::Provider> State<'_, '_, '_, P> {
    pub fn capture(&self, place: &Place, locals: usize) -> Step<Capture> {
        if place.projections.is_empty() {
            Ok(Capture::Root(
                self.current.cell
                    + match place.root {
                        Root::Local(id) => id.0 as usize,
                        Root::Value(id) => locals + id.0 as usize,
                    },
            ))
        } else {
            Ok(Capture::Path(self.place(place, locals)?))
        }
    }
    pub fn captured(&self, capture: Capture) -> Step<(usize, Target, bool)> {
        let target = match capture {
            Capture::Root(index) => {
                let physical = self.cells[index].physical;
                let location = self
                    .storage
                    .as_ref()
                    .unwrap()
                    .cell(physical)
                    .map_err(invariant)?
                    .root();
                Target::Data {
                    cell: physical,
                    location,
                }
            }
            Capture::Path(target) => target,
        };
        let Target::Data { cell, location } = target else {
            return Err(c::ExecutionFailure::Invariant);
        };
        let logical = self.inverse[cell];
        let root = self
            .storage
            .as_ref()
            .unwrap()
            .cell(cell)
            .map_err(invariant)?
            .root()
            == location;
        if !self.cells[logical].live && !(root && matches!(capture, Capture::Root(_))) {
            return Err(c::ExecutionFailure::Invariant);
        }
        Ok((logical, target, root))
    }
    pub fn exchange(
        &mut self,
        target: Target,
        source: usize,
        span: m::Span,
    ) -> Step<Option<m::Span>> {
        self.root(source)?;
        let Target::Data { cell, location } = target else {
            return Err(c::ExecutionFailure::Invariant);
        };
        let physical = self.cells[source].physical;
        let (target_cell, incoming) = self
            .storage
            .as_mut()
            .unwrap()
            .two_cells_mut(cell, physical)
            .map_err(invariant)?;
        target_cell
            .exchange_root_at(location, incoming, span, self.allocator)
            .map_err(invariant)
    }
}
