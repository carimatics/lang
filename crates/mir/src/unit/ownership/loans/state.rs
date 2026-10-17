use super::*;
use std::ops::Range;

pub(super) fn words(bits: usize) -> Result<usize, m::Error> {
    Ok(add(bits, 63)? / 64)
}
pub(super) fn has(row: &[u64], range: Range<usize>, id: usize) -> bool {
    row[range.start + id / 64] & (1u64 << (id % 64)) != 0
}
pub(super) fn set(row: &mut [u64], range: Range<usize>, id: usize, value: bool) {
    let bit = 1u64 << (id % 64);
    let word = &mut row[range.start + id / 64];
    if value {
        *word |= bit;
    } else {
        *word &= !bit;
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Layout {
    pub words: usize,
    pub loans: usize,
    pub nodes: usize,
    pub loan_words: usize,
    pub node_words: usize,
    pub may: Range<usize>,
    pub must: Range<usize>,
    refs: usize,
    pub dead: Range<usize>,
    targets: usize,
    dependencies: usize,
    captures: usize,
    capture_deps: usize,
    pub capture_dead: Range<usize>,
}
impl Layout {
    pub fn new(topology: &topology::Topology, prepared: usize) -> Result<Self, m::Error> {
        let (loans, nodes) = (topology.kinds.len(), topology.cells.len());
        let (wl, wn) = (words(loans)?, words(nodes)?);
        let may = 0..wl;
        let must = wl..mul(2, wl)?;
        let refs = must.end;
        let dead = add(refs, mul(topology.leaves, wl)?)?
            ..add(
                add(refs, mul(topology.leaves, wl)?)?,
                words(topology.leaves)?,
            )?;
        let targets = dead.end;
        let dependencies = add(targets, mul(loans, wn)?)?;
        let captures = add(dependencies, mul(loans, wl)?)?;
        let capture_deps = add(captures, mul(prepared, wn)?)?;
        let first_dead = add(capture_deps, mul(prepared, wl)?)?;
        let capture_dead = first_dead..add(first_dead, words(prepared)?)?;
        Ok(Self {
            words: capture_dead.end,
            loans,
            nodes,
            loan_words: wl,
            node_words: wn,
            may,
            must,
            refs,
            dead,
            targets,
            dependencies,
            captures,
            capture_deps,
            capture_dead,
        })
    }
    pub fn refs(&self, leaf: usize) -> Range<usize> {
        let start = self.refs + leaf * self.loan_words;
        start..start + self.loan_words
    }
    pub fn targets(&self, loan: usize) -> Range<usize> {
        let start = self.targets + loan * self.node_words;
        start..start + self.node_words
    }
    pub fn deps(&self, loan: usize) -> Range<usize> {
        let start = self.dependencies + loan * self.loan_words;
        start..start + self.loan_words
    }
    pub fn capture(&self, id: usize) -> Range<usize> {
        let start = self.captures + id * self.node_words;
        start..start + self.node_words
    }
    pub fn capture_deps(&self, id: usize) -> Range<usize> {
        let start = self.capture_deps + id * self.loan_words;
        start..start + self.loan_words
    }
    pub fn has_ref(&self, row: &[u64], leaf: usize, id: usize) -> bool {
        has(row, self.refs(leaf), id)
    }
    pub fn active(&self, row: &[u64], id: usize) -> bool {
        has(row, self.may.clone(), id)
    }
    pub fn activity(&self, row: &mut [u64], id: usize, value: bool) {
        set(row, self.may.clone(), id, value);
        set(row, self.must.clone(), id, value);
    }
    pub fn merge(
        &self,
        budget: &mut Budget,
        destination: &mut [u64],
        source: &[u64],
    ) -> Result<bool, m::Error> {
        let mut changed = false;
        for (i, (a, b)) in destination.iter_mut().zip(source).enumerate() {
            budget.charge(1)?;
            let next = if self.must.contains(&i) {
                *a & *b
            } else {
                *a | *b
            };
            changed |= next != *a;
            *a = next;
        }
        Ok(changed)
    }
}
pub(super) fn copy(
    budget: &mut Budget,
    destination: &mut [u64],
    source: &[u64],
) -> Result<(), m::Error> {
    budget.charge(source.len())?;
    destination.copy_from_slice(source);
    Ok(())
}
pub(super) fn ids(
    budget: &mut Budget,
    row: &[u64],
    range: Range<usize>,
    count: usize,
) -> Result<Vec<usize>, m::Error> {
    let mut values = budget.vector(count)?;
    for id in 0..count {
        budget.charge(1)?;
        if has(row, range.clone(), id) {
            values.push(id);
        }
    }
    Ok(values)
}
