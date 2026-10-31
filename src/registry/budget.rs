use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

pub(crate) struct Budget {
    maximum: usize,
    used: AtomicUsize,
}
impl Budget {
    pub(crate) fn new(maximum: usize) -> Arc<Self> {
        Arc::new(Self {
            maximum,
            used: AtomicUsize::new(0),
        })
    }
    pub(crate) fn reserve(self: &Arc<Self>, bytes: usize) -> Result<Reservation, Exhausted> {
        self.used
            .try_update(Ordering::AcqRel, Ordering::Acquire, |used| {
                used.checked_add(bytes).filter(|next| *next <= self.maximum)
            })
            .map_err(|_| Exhausted)?;
        Ok(Reservation {
            budget: self.clone(),
            bytes,
        })
    }
    pub(crate) fn maximum(&self) -> usize {
        self.maximum
    }
    #[cfg(test)]
    pub(crate) fn used(&self) -> usize {
        self.used.load(Ordering::Acquire)
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Exhausted;

pub(crate) struct Reservation {
    budget: Arc<Budget>,
    bytes: usize,
}
impl Reservation {
    pub(crate) fn covers(&self, budget: &Arc<Budget>, bytes: usize) -> bool {
        Arc::ptr_eq(&self.budget, budget) && self.bytes >= bytes
    }

    pub(crate) fn grow(&mut self, bytes: usize) -> Result<(), Exhausted> {
        let additional = bytes.checked_sub(self.bytes).ok_or(Exhausted)?;
        self.budget
            .used
            .try_update(Ordering::AcqRel, Ordering::Acquire, |used| {
                used.checked_add(additional)
                    .filter(|next| *next <= self.budget.maximum)
            })
            .map_err(|_| Exhausted)?;
        self.bytes = bytes;
        Ok(())
    }
    pub(crate) fn shrink(&mut self, bytes: usize) -> Result<(), Exhausted> {
        let released = self.bytes.checked_sub(bytes).ok_or(Exhausted)?;
        self.bytes = bytes;
        self.budget.used.fetch_sub(released, Ordering::AcqRel);
        Ok(())
    }
}
impl Drop for Reservation {
    fn drop(&mut self) {
        self.budget.used.fetch_sub(self.bytes, Ordering::AcqRel);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reservation_growth_is_atomic_and_failed_growth_preserves_the_lease() {
        let budget = Budget::new(8);
        let barrier = Arc::new(std::sync::Barrier::new(9));
        let workers: Vec<_> = (0..8)
            .map(|_| {
                let budget = budget.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    let mut lease = budget.reserve(1).unwrap();
                    barrier.wait();
                    let refused = lease.grow(2).is_err();
                    barrier.wait();
                    assert!(refused);
                })
            })
            .collect();
        barrier.wait();
        assert_eq!(budget.used(), 8);
        assert!(budget.reserve(1).is_err());
        barrier.wait();
        for worker in workers {
            worker.join().unwrap();
        }
        assert_eq!(budget.used(), 0);
        let mut lease = budget.reserve(2).unwrap();
        lease.grow(8).unwrap();
        assert!(lease.grow(usize::MAX).is_err());
        assert_eq!(budget.used(), 8);
        assert!(lease.shrink(9).is_err());
        assert_eq!(budget.used(), 8);
        lease.shrink(3).unwrap();
        assert_eq!(budget.used(), 3);
        drop(lease);
        assert_eq!(budget.used(), 0);
    }
}
