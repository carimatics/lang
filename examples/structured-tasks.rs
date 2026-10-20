//! Runnable host-driver fixture for RFC 0021, before source coroutine adapters.
//! `cargo run --example structured-tasks` prints 42 and cleanup order 2,1,0.

use lang_runtime::task::{Dispatch, Limits, Outcome, Scheduler, TaskId};

enum Frame {
    Root { child: Option<TaskId> },
    Worker { value: i64, yield_first: bool },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut scheduler = Scheduler::new(Limits {
        live_tasks: 3,
        total_tasks: 3,
    })?;
    let root = scheduler.start(None)?;
    let mut frames = vec![Some(Frame::Root { child: None })];
    let mut results = [None; 3];
    let mut cleaned = Vec::new();
    while let Some(dispatch) = scheduler.dispatch()? {
        match dispatch {
            Dispatch::Resume(task) => match frames[task.index()].as_mut().unwrap() {
                Frame::Root { child } if child.is_none() => {
                    let left = scheduler.spawn(&task, None)?;
                    let right = scheduler.spawn(&task, None)?;
                    *child = Some(left.clone());
                    assert_eq!((left.index(), right.index()), (1, 2));
                    frames.push(Some(Frame::Worker {
                        value: 20,
                        yield_first: true,
                    }));
                    frames.push(Some(Frame::Worker {
                        value: 22,
                        yield_first: false,
                    }));
                    scheduler.join(&task, &left)?;
                }
                Frame::Root { .. } => {
                    results[0] = Some(results[1].take().unwrap() + results[2].take().unwrap());
                    scheduler.finish(&task, Outcome::Success)?;
                }
                Frame::Worker { yield_first, .. } if *yield_first => {
                    *yield_first = false;
                    scheduler.yield_now(&task)?;
                }
                Frame::Worker { value, .. } => {
                    results[task.index()] = Some(*value);
                    scheduler.finish(&task, Outcome::Success)?;
                }
            },
            Dispatch::Cleanup { task, outcome } => {
                assert_eq!(outcome, Outcome::Success);
                // Real adapter-owned storage retires before acknowledgement.
                let frame = frames[task.index()].take().unwrap();
                drop(frame);
                cleaned.push(task.index());
                scheduler.finish_cleanup(&task, None)?;
            }
        }
    }
    assert!(scheduler.is_finished());
    assert_eq!(results[0], Some(42));
    assert_eq!(cleaned, [2, 1, 0]);
    assert!(frames.iter().all(Option::is_none));
    println!("result={} cleanup={cleaned:?}", results[0].unwrap());
    println!(
        "{} tasks admitted; {} live",
        scheduler.usage().admitted,
        scheduler.usage().live
    );
    assert_eq!(scheduler.parent(&root)?, None);
    cancelled_tree()?;
    Ok(())
}

fn cancelled_tree() -> Result<(), Box<dyn std::error::Error>> {
    let mut scheduler = Scheduler::new(Limits {
        live_tasks: 3,
        total_tasks: 3,
    })?;
    let root = scheduler.start(None)?;
    let mut frames = vec![Some(vec![0_i64])];
    assert_eq!(scheduler.dispatch()?, Some(Dispatch::Resume(root.clone())));
    let child = scheduler.spawn(&root, None)?;
    frames.push(Some(vec![1_i64]));
    scheduler.join(&root, &child)?;
    assert_eq!(scheduler.dispatch()?, Some(Dispatch::Resume(child.clone())));
    let leaf = scheduler.spawn(&child, None)?;
    assert_eq!(leaf.index(), 2);
    frames.push(Some(vec![2_i64]));
    scheduler.yield_now(&child)?;
    scheduler.request_cancel(&root)?;
    let mut cleaned = Vec::new();
    while let Some(dispatch) = scheduler.dispatch()? {
        let Dispatch::Cleanup { task, outcome } = dispatch else {
            panic!("cancelled suspended frames must clean without resuming user code");
        };
        assert_eq!(outcome, Outcome::Cancelled);
        drop(frames[task.index()].take().unwrap());
        cleaned.push(task.index());
        scheduler.finish_cleanup(&task, None)?;
    }
    assert_eq!(cleaned, [2, 1, 0]);
    assert!(frames.iter().all(Option::is_none));
    assert!(scheduler.is_finished());
    println!(
        "cancelled cleanup={cleaned:?} live={}",
        scheduler.usage().live
    );
    Ok(())
}
