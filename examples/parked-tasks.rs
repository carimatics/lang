//! Trusted host readiness fixture for RFCs 0021/0025; not a channel adapter.
//! `cargo run --example parked-tasks` returns 42 and proves actual frame cleanup.

use lang_runtime::task::{Dispatch, Error, Limits, Outcome, Park, Scheduler, TaskId, WaitId};
use std::cell::RefCell;
use std::rc::Rc;

struct Frame {
    values: Vec<i64>,
    index: usize,
    retired: Rc<RefCell<Vec<usize>>>,
}
impl Frame {
    fn new(index: usize, value: i64, retired: &Rc<RefCell<Vec<usize>>>) -> Self {
        Self {
            values: vec![value],
            index,
            retired: Rc::clone(retired),
        }
    }
}
impl Drop for Frame {
    fn drop(&mut self) {
        self.retired.borrow_mut().push(self.index);
    }
}

fn park(scheduler: &mut Scheduler, task: &TaskId) -> Result<WaitId, Error> {
    scheduler.check_park(task)?;
    let Park::Waiting(wait) = scheduler.park(task)? else {
        panic!("normal readiness fixture must not close before registration");
    };
    Ok(wait)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut scheduler = Scheduler::new(Limits {
        live_tasks: 2,
        total_tasks: 2,
    })?;
    let root = scheduler.start(None)?;
    let retired = Rc::new(RefCell::new(Vec::new()));
    let mut frames = vec![Some(Frame::new(0, 0, &retired))];
    let mut stage = [0; 2];
    let mut root_wait = None;
    let mut readiness = None;
    let mut result = None;
    let mut parked_events = 0;
    loop {
        let Some(dispatch) = scheduler.dispatch()? else {
            if scheduler.is_finished() {
                break;
            }
            // This trusted driver explicitly supplies one readiness event. A
            // channel driver must instead prove readiness from its queue state.
            let wait = readiness.take().expect("one registered host event");
            assert_eq!(scheduler.usage().live, 2);
            assert!(retired.borrow().is_empty());
            scheduler.wake(&wait)?;
            assert_eq!(scheduler.wake(&wait), Err(Error::StaleWait));
            parked_events += 1;
            continue;
        };
        match dispatch {
            Dispatch::Resume(task) => {
                let index = task.index();
                match (index, stage[index]) {
                    (0, 0) => {
                        let child = scheduler.spawn(&root, None)?;
                        assert_eq!(child.index(), 1);
                        frames.push(Some(Frame::new(1, 42, &retired)));
                        root_wait = Some(park(&mut scheduler, &root)?);
                    }
                    (1, 0) => readiness = Some(park(&mut scheduler, &task)?),
                    (1, 1) => {
                        result = Some(frames[1].as_ref().unwrap().values[0]);
                        // Waking the root cannot steal this active dispatch.
                        scheduler.wake(&root_wait.take().unwrap())?;
                        assert_eq!(scheduler.dispatch(), Err(Error::Busy));
                        scheduler.finish(&task, Outcome::Success)?;
                    }
                    (0, 1) => {
                        assert_eq!(result, Some(42));
                        scheduler.finish(&root, Outcome::Success)?;
                    }
                    _ => unreachable!("each frame resumes exactly twice"),
                }
                stage[index] += 1;
            }
            Dispatch::Cleanup { task, outcome } => {
                assert_eq!(outcome, Outcome::Success);
                // Drop real host-owned vector storage before acknowledgement.
                drop(frames[task.index()].take().unwrap());
                scheduler.finish_cleanup(&task, None)?;
            }
        }
    }
    assert_eq!(stage, [2, 2]);
    assert_eq!(parked_events, 1);
    assert_eq!(*retired.borrow(), [1, 0]);
    assert!(frames.iter().all(Option::is_none));
    assert_eq!(scheduler.usage().admitted, 2);
    assert_eq!(scheduler.usage().live, 0);
    println!(
        "result={} cleanup={:?} live=0",
        result.unwrap(),
        retired.borrow()
    );
    cancelled_tree()?;
    Ok(())
}

fn cancelled_tree() -> Result<(), Box<dyn std::error::Error>> {
    let mut scheduler = Scheduler::new(Limits {
        live_tasks: 3,
        total_tasks: 3,
    })?;
    let retired = Rc::new(RefCell::new(Vec::new()));
    let root = scheduler.start(None)?;
    let mut frames = vec![Some(Frame::new(0, 0, &retired))];
    let mut waits = Vec::new();
    for index in 0..3 {
        let Some(Dispatch::Resume(task)) = scheduler.dispatch()? else {
            panic!("expected running tree frame");
        };
        assert_eq!(task.index(), index);
        if index < 2 {
            let child = scheduler.spawn(&task, None)?;
            frames.push(Some(Frame::new(
                child.index(),
                child.index() as i64,
                &retired,
            )));
        }
        waits.push(park(&mut scheduler, &task)?);
    }
    assert_eq!(scheduler.dispatch()?, None);
    assert!(!scheduler.is_finished());
    assert_eq!(scheduler.usage().live, 3);
    scheduler.request_cancel(&root)?;
    assert!(retired.borrow().is_empty());
    for wait in &waits {
        assert_eq!(scheduler.wake(wait), Err(Error::StaleWait));
    }
    while let Some(dispatch) = scheduler.dispatch()? {
        let Dispatch::Cleanup { task, outcome } = dispatch else {
            panic!("a cancelled waiter cannot resume guest execution");
        };
        assert_eq!(outcome, Outcome::Cancelled);
        drop(frames[task.index()].take().unwrap());
        scheduler.finish_cleanup(&task, None)?;
    }
    assert_eq!(*retired.borrow(), [2, 1, 0]);
    assert!(frames.iter().all(Option::is_none));
    assert!(scheduler.is_finished());
    assert_eq!(scheduler.usage().live, 0);
    println!("cancelled cleanup={:?} live=0", retired.borrow());
    Ok(())
}
