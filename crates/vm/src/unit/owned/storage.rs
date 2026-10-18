use super::*;

/// Global physical cell addresses keep caller referents stable across calls.
pub(super) struct StackStorage<'l, 't> {
    pub frames: Vec<l::FrameStorage<'l, 't>>,
    pub active: Vec<bool>,
    pub stride: usize,
}
impl<'l, 't> StackStorage<'l, 't> {
    pub fn cell(&self, index: usize) -> Result<&l::Reservation<'l, 't>, l::Error> {
        self.frames
            .get(index / self.stride)
            .ok_or(l::Error::InvalidCell(index))?
            .cell(index % self.stride)
    }
    pub fn cell_mut(&mut self, index: usize) -> Result<&mut l::Reservation<'l, 't>, l::Error> {
        self.frames
            .get_mut(index / self.stride)
            .ok_or(l::Error::InvalidCell(index))?
            .cell_mut(index % self.stride)
    }
    pub fn two_cells_mut(
        &mut self,
        first: usize,
        second: usize,
    ) -> Result<(&mut l::Reservation<'l, 't>, &mut l::Reservation<'l, 't>), l::Error> {
        let a = first / self.stride;
        let b = second / self.stride;
        if a == b {
            return self
                .frames
                .get_mut(a)
                .ok_or(l::Error::InvalidCell(first))?
                .two_cells_mut(first % self.stride, second % self.stride);
        }
        if a >= self.frames.len() || b >= self.frames.len() {
            return Err(l::Error::InvalidCell(first.max(second)));
        }
        if a < b {
            let (left, right) = self.frames.split_at_mut(b);
            Ok((
                left[a].cell_mut(first % self.stride)?,
                right[0].cell_mut(second % self.stride)?,
            ))
        } else {
            let (left, right) = self.frames.split_at_mut(a);
            Ok((
                right[0].cell_mut(first % self.stride)?,
                left[b].cell_mut(second % self.stride)?,
            ))
        }
    }
    pub fn admit<P: a::Provider>(
        &mut self,
        depth: usize,
        plan: &l::FramePlan<'l, 't>,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), l::Error> {
        self.frames
            .get_mut(depth)
            .ok_or(l::Error::State)?
            .admit(plan, allocator)?;
        self.active[depth] = true;
        Ok(())
    }
    pub fn three_cells_mut(
        &mut self,
        ids: [usize; 3],
    ) -> Result<[&mut l::Reservation<'l, 't>; 3], l::Error> {
        let [a, b, c] = ids.map(|id| id / self.stride);
        for (id, frame) in ids.into_iter().zip([a, b, c]) {
            if frame >= self.frames.len() {
                return Err(l::Error::InvalidCell(id));
            }
        }
        let [x, y, z] = ids.map(|id| id % self.stride);
        if a == b && b == c {
            return self.frames[a].three_cells_mut([x, y, z]);
        }
        if a == b {
            let [ab, c] = self
                .frames
                .get_disjoint_mut([a, c])
                .map_err(|_| l::Error::State)?;
            let (x, y) = ab.two_cells_mut(x, y)?;
            return Ok([x, y, c.cell_mut(z)?]);
        }
        if a == c {
            let [ac, b] = self
                .frames
                .get_disjoint_mut([a, b])
                .map_err(|_| l::Error::State)?;
            let (x, z) = ac.two_cells_mut(x, z)?;
            return Ok([x, b.cell_mut(y)?, z]);
        }
        if b == c {
            let [a, bc] = self
                .frames
                .get_disjoint_mut([a, b])
                .map_err(|_| l::Error::State)?;
            let (y, z) = bc.two_cells_mut(y, z)?;
            return Ok([a.cell_mut(x)?, y, z]);
        }
        let [a, b, c] = self
            .frames
            .get_disjoint_mut([a, b, c])
            .map_err(|_| l::Error::State)?;
        Ok([a.cell_mut(x)?, b.cell_mut(y)?, c.cell_mut(z)?])
    }
    pub fn release_frame<P: a::Provider>(
        &mut self,
        depth: usize,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), l::Error> {
        self.frames
            .get_mut(depth)
            .ok_or(l::Error::State)?
            .release(allocator)?;
        self.active[depth] = false;
        Ok(())
    }
    pub fn release<P: a::Provider>(
        &mut self,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), l::Error> {
        for depth in (0..self.frames.len()).rev() {
            if self.active[depth] {
                self.release_frame(depth, allocator)?;
            }
        }
        Ok(())
    }
}
