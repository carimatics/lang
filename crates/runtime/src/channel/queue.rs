use std::collections::VecDeque;

/// A committed entry owns its value; removing it transfers that obligation.
pub(super) struct Entry<T> {
    pub value: T,
    pub position: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Refusal {
    Full,
    PositionExhausted,
}

pub(super) struct Queue<T> {
    values: VecDeque<Entry<T>>,
    capacity: usize,
    last_position: Option<u64>,
}
impl<T> Queue<T> {
    pub fn prepare(capacity: usize) -> Result<Self, ()> {
        if capacity == 0 {
            return Err(());
        }
        let mut values = VecDeque::new();
        values.try_reserve_exact(capacity).map_err(|_| ())?;
        Ok(Self {
            values,
            capacity,
            last_position: None,
        })
    }

    pub fn has_space(&self) -> bool {
        self.values.len() < self.capacity
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    pub fn push(&mut self, value: T) -> Result<u64, (Refusal, T)> {
        if !self.has_space() {
            return Err((Refusal::Full, value));
        }
        let position = match self.last_position {
            None => 0,
            Some(previous) => match previous.checked_add(1) {
                Some(position) => position,
                None => return Err((Refusal::PositionExhausted, value)),
            },
        };
        self.values.push_back(Entry { value, position });
        self.last_position = Some(position);
        Ok(position)
    }

    /// Unlink the owner before any adapter cleanup callback is entered.
    pub fn pop(&mut self) -> Option<Entry<T>> {
        self.values.pop_front()
    }

    pub fn entries(&self) -> impl Iterator<Item = &Entry<T>> {
        self.values.iter()
    }

    pub fn release(&mut self) {
        assert!(
            self.values.is_empty(),
            "release only after actual owner retirement"
        );
        self.values = VecDeque::new();
        self.capacity = 0;
    }

    #[cfg(test)]
    pub fn exhaust_positions(&mut self) {
        self.last_position = Some(u64::MAX);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::{Dispatch, Limits, Outcome, Park, Scheduler};

    #[test]
    fn owned_queue_and_real_parked_driver_transfer_42_without_copying_payloads() {
        let mut scheduler = Scheduler::new(Limits {
            live_tasks: 2,
            total_tasks: 2,
        })
        .unwrap();
        let root = scheduler.start(None).unwrap();
        assert_eq!(
            scheduler.dispatch().unwrap(),
            Some(Dispatch::Resume(root.clone()))
        );
        let producer = scheduler.spawn(&root, None).unwrap();
        let mut queue = Queue::prepare(1).unwrap();
        assert!(queue.is_empty());
        let Park::Waiting(receiver) = scheduler.park(&root).unwrap() else {
            panic!("receiver should park");
        };
        assert_eq!(
            scheduler.dispatch().unwrap(),
            Some(Dispatch::Resume(producer.clone()))
        );
        let payload = vec![42_i64];
        let allocation = payload.as_ptr();
        assert_eq!(queue.push(payload).unwrap(), 0);
        assert_eq!(queue.entries().count(), 1);
        scheduler.wake(&receiver).unwrap();
        scheduler.finish(&producer, Outcome::Success).unwrap();
        assert_eq!(
            scheduler.dispatch().unwrap(),
            Some(Dispatch::Resume(root.clone()))
        );
        let entry = queue.pop().unwrap();
        assert_eq!(entry.position, 0);
        assert_eq!(entry.value.as_ptr(), allocation);
        assert_eq!(entry.value[0], 42);
        assert!(queue.is_empty());
        drop(entry);
        scheduler.finish(&root, Outcome::Success).unwrap();
        for expected in [&producer, &root] {
            let Some(Dispatch::Cleanup { task, outcome }) = scheduler.dispatch().unwrap() else {
                panic!("expected actual cleanup acknowledgement");
            };
            assert_eq!(&task, expected);
            assert_eq!(outcome, Outcome::Success);
            scheduler.finish_cleanup(&task, None).unwrap();
        }
        assert!(scheduler.is_finished());
        assert_eq!(scheduler.usage().live, 0);
    }
}
