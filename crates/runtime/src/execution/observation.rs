//! Host-only bounded observations; failures never replace guest outcomes.
use std::sync::Mutex;

pub const MAX_SAMPLES: usize = 65_536;
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Point {
    pub source: u32,
    pub start: u32,
    pub end: u32,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObservationError {
    InvalidLimit,
    Storage,
    Capacity,
    CounterOverflow,
    Poisoned,
}
impl std::fmt::Display for ObservationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "execution observation failed: {self:?}")
    }
}
impl std::error::Error for ObservationError {}
struct State {
    total: u64,
    current: Option<Point>,
    samples: Vec<Option<Point>>,
    failure: Option<ObservationError>,
}
/// Create a fresh recorder for one original invocation. No callback is invoked
/// and no guest capability is created. Recording uses only reserved storage.
/// Only instrumented engine charge points are observed; callers must reconcile
/// the snapshot count with the original execution usage before publication.
pub struct PointRecorder {
    limit: usize,
    sample_every: u32,
    state: Mutex<State>,
}
impl std::fmt::Debug for PointRecorder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PointRecorder")
            .field("limit", &self.limit)
            .field("sample_every", &self.sample_every)
            .finish_non_exhaustive()
    }
}
#[derive(Debug)]
pub struct PointSnapshot {
    pub total: u64,
    pub sample_every: u32,
    pub samples: Vec<Option<Point>>,
}
impl PointRecorder {
    pub fn new(limit: usize, sample_every: u32) -> Result<Self, ObservationError> {
        if limit == 0
            || limit > MAX_SAMPLES
            || sample_every == 0
            || sample_every > MAX_SAMPLES as u32
        {
            return Err(ObservationError::InvalidLimit);
        }
        let mut samples = Vec::new();
        samples
            .try_reserve_exact(limit)
            .map_err(|_| ObservationError::Storage)?;
        Ok(Self {
            limit,
            sample_every,
            state: Mutex::new(State {
                total: 0,
                current: None,
                samples,
                failure: None,
            }),
        })
    }
    pub(super) fn record(&self, point: Option<Point>) {
        let mut state = match self.state.lock() {
            Ok(state) => state,
            Err(error) => {
                error
                    .into_inner()
                    .failure
                    .get_or_insert(ObservationError::Poisoned);
                return;
            }
        };
        state.current = point;
        let Some(total) = state.total.checked_add(1) else {
            state
                .failure
                .get_or_insert(ObservationError::CounterOverflow);
            return;
        };
        state.total = total;
        if state.failure.is_some() || total % u64::from(self.sample_every) != 0 {
            return;
        }
        if state.samples.len() == self.limit {
            state.failure = Some(ObservationError::Capacity);
            return;
        }
        state.samples.push(point);
    }
    pub fn snapshot(&self) -> Result<PointSnapshot, ObservationError> {
        let state = self.state.lock().map_err(|_| ObservationError::Poisoned)?;
        if let Some(error) = state.failure {
            return Err(error);
        }
        let mut samples = Vec::new();
        samples
            .try_reserve_exact(state.samples.len())
            .map_err(|_| ObservationError::Storage)?;
        samples.extend_from_slice(&state.samples);
        Ok(PointSnapshot {
            total: state.total,
            sample_every: self.sample_every,
            samples,
        })
    }
    /// The last successful charge, which is an observation rather than a
    /// claim about an allocation's declaration or a cleanup source position.
    pub fn current(&self) -> Result<Option<Point>, ObservationError> {
        let state = self.state.lock().map_err(|_| ObservationError::Poisoned)?;
        if let Some(error) = state.failure {
            return Err(error);
        }
        Ok(state.current)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution::{Control, InvocationAccounting, Usage};

    #[test]
    fn sampled_missing_spans_and_overflow_preserve_original_charge_counts() {
        fn send_sync<T: Send + Sync>() {}
        send_sync::<Control<'static>>();
        let recorder = PointRecorder::new(3, 2).unwrap();
        let control = Control::default().and_observations(&recorder);
        let mut usage = Usage::default();
        for start in 0..6 {
            control.charge(&mut usage).unwrap();
            control.observe((start != 3).then_some(Point {
                source: 0,
                start,
                end: start + 1,
            }));
        }
        let snapshot = recorder.snapshot().unwrap();
        assert_eq!(snapshot.total, usage.fuel);
        assert_eq!(
            snapshot.samples,
            vec![
                Some(Point {
                    source: 0,
                    start: 1,
                    end: 2
                }),
                None,
                Some(Point {
                    source: 0,
                    start: 5,
                    end: 6
                })
            ]
        );
        for _ in 0..2 {
            control.charge(&mut usage).unwrap();
            control.observe(None);
        }
        assert_eq!(usage.fuel, 8);
        assert!(matches!(
            recorder.snapshot(),
            Err(ObservationError::Capacity)
        ));
    }
    #[test]
    fn accounting_keeps_recorder_identity_and_rejects_invalid_bounds() {
        let a = PointRecorder::new(1, 1).unwrap();
        let b = PointRecorder::new(1, 1).unwrap();
        let control = Control::default().and_observations(&a);
        let accounting = InvocationAccounting::prepare(
            control,
            &mut lang_mir::unit::Budget::new(Default::default()),
        )
        .unwrap();
        assert!(accounting.matches(control));
        assert!(!accounting.matches(Control::default().and_observations(&b)));
        assert!(!control.same_configuration(Control::default()));
        assert!(PointRecorder::new(0, 1).is_err());
        assert!(PointRecorder::new(MAX_SAMPLES + 1, 1).is_err());
        assert!(PointRecorder::new(1, 0).is_err());
    }
}
