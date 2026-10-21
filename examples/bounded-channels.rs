//! Trusted host fixture for RFC 0025. Source/MIR/VM/native channel adapters
//! remain separate; this executes the shared runtime service, not a model.

use lang_runtime::{channel as h, cleanup as c, execution as e, task as t};
use std::{cell::RefCell, num::NonZeroUsize, rc::Rc};

#[derive(Debug)]
struct Packet {
    bytes: Vec<u8>,
    retired: Rc<RefCell<Vec<u8>>>,
}
impl Packet {
    fn new(value: u8, retired: &Rc<RefCell<Vec<u8>>>) -> Self {
        Self {
            bytes: vec![value],
            retired: Rc::clone(retired),
        }
    }
}
impl h::Payload for Packet {
    fn receivers(
        &self,
        _: &mut dyn FnMut(&h::Receiver) -> Result<(), h::Error>,
    ) -> Result<(), h::Error> {
        Ok(())
    }
}
impl Drop for Packet {
    fn drop(&mut self) {
        self.retired.borrow_mut().push(self.bytes[0]);
    }
}
fn session() -> c::Session {
    c::Session::prepare(
        NonZeroUsize::new(4).unwrap(),
        &mut lang_mir::unit::Budget::new(Default::default()),
    )
    .unwrap()
}
fn main() {
    normal();
    cancelled();
    deadlock();
}

fn normal() {
    let mut scheduler = t::Scheduler::new(t::Limits {
        live_tasks: 2,
        total_tasks: 2,
    })
    .unwrap();
    let root = scheduler.start(None).unwrap();
    let retired = Rc::new(RefCell::new(Vec::new()));
    let mut hub = h::Hub::<Packet>::prepare(
        &scheduler,
        &root,
        h::Limits::default(),
        e::ServiceWork::new(Some(64)),
    )
    .unwrap();
    let setup = hub.create(1).unwrap();
    let sender = hub.sender(&setup).unwrap();
    let mut receiver = hub.receiver(setup).unwrap();
    let mut frames = vec![Some(vec![0_i64])];
    let mut stage = [0; 2];
    let mut receive = None;
    let mut send = None;
    let mut result = 0;
    let mut cleaned = Vec::new();
    while let Some(dispatch) = scheduler.dispatch().unwrap() {
        match dispatch {
            t::Dispatch::Resume(task) => {
                let index = task.index();
                match (index, stage[index]) {
                    (0, 0) => {
                        let producer = scheduler.spawn(&root, None).unwrap();
                        assert_eq!(producer.index(), 1);
                        frames.push(Some(vec![1]));
                        let h::Receive::Waiting(operation) = hub
                            .receive(&mut scheduler, &root, &mut receiver, None)
                            .unwrap()
                        else {
                            panic!("empty receive must wait")
                        };
                        receive = Some(operation);
                    }
                    (1, 0) => {
                        assert!(matches!(
                            hub.send(
                                &mut scheduler,
                                &task,
                                &sender,
                                Packet::new(20, &retired),
                                None
                            )
                            .unwrap(),
                            h::Send::Sent { position: 0 }
                        ));
                        let h::Send::Waiting(operation) = hub
                            .send(
                                &mut scheduler,
                                &task,
                                &sender,
                                Packet::new(22, &retired),
                                None,
                            )
                            .unwrap()
                        else {
                            panic!("capacity-one backpressure")
                        };
                        send = Some(operation);
                    }
                    (0, 1) => {
                        let h::Receive::Value { value, position: 0 } = hub
                            .resume_receive(&mut scheduler, &receive.take().unwrap())
                            .unwrap()
                        else {
                            panic!("first value")
                        };
                        result += value.bytes[0] as i64;
                        drop(value);
                        let h::Receive::Waiting(operation) = hub
                            .receive(&mut scheduler, &root, &mut receiver, None)
                            .unwrap()
                        else {
                            panic!("producer's reserved turn has not committed yet")
                        };
                        receive = Some(operation);
                    }
                    (1, 1) => {
                        assert!(matches!(
                            hub.resume_send(&mut scheduler, &send.take().unwrap())
                                .unwrap(),
                            h::Send::Sent { position: 1 }
                        ));
                        hub.release_sender(&mut scheduler, &sender).unwrap();
                        scheduler.finish(&task, t::Outcome::Success).unwrap();
                    }
                    (0, 2) => {
                        let h::Receive::Value { value, position: 1 } = hub
                            .resume_receive(&mut scheduler, &receive.take().unwrap())
                            .unwrap()
                        else {
                            panic!("second value")
                        };
                        result += value.bytes[0] as i64;
                        drop(value);
                        assert!(matches!(
                            hub.receive(&mut scheduler, &root, &mut receiver, None)
                                .unwrap(),
                            h::Receive::Closed
                        ));
                        hub.abort_receiver(
                            &mut scheduler,
                            &receiver,
                            &mut session(),
                            c::Reason::ScopeExit,
                            &mut |_: &mut Packet, _| Ok(()),
                        )
                        .unwrap();
                        scheduler.finish(&task, t::Outcome::Success).unwrap();
                    }
                    _ => unreachable!("bounded fixture stage"),
                }
                stage[index] += 1;
            }
            t::Dispatch::Cleanup { task, outcome } => {
                assert_eq!(outcome, t::Outcome::Success);
                drop(frames[task.index()].take().unwrap());
                cleaned.push(task.index());
                scheduler.finish_cleanup(&task, None).unwrap();
            }
        }
    }
    assert!(scheduler.is_finished());
    assert_eq!(result, 42);
    assert_eq!(*retired.borrow(), [20, 22]);
    assert_eq!(cleaned, [1, 0]);
    assert!(frames.iter().all(Option::is_none));
    assert_eq!(scheduler.usage().live, 0);
    assert_eq!(
        (
            hub.usage().queued,
            hub.usage().pending,
            hub.usage().reserved_slots,
            hub.usage().handles
        ),
        (0, 0, 0, 0)
    );
    println!(
        "result={result} payloads={:?} frames={cleaned:?} live=0",
        retired.borrow()
    );
}

fn cancelled() {
    let mut scheduler = t::Scheduler::new(t::Limits {
        live_tasks: 2,
        total_tasks: 2,
    })
    .unwrap();
    let root = scheduler.start(None).unwrap();
    assert_eq!(
        scheduler.dispatch().unwrap(),
        Some(t::Dispatch::Resume(root.clone()))
    );
    let retired = Rc::new(RefCell::new(Vec::new()));
    let mut hub = h::Hub::<Packet>::prepare(
        &scheduler,
        &root,
        h::Limits::default(),
        e::ServiceWork::new(None),
    )
    .unwrap();
    let setup = hub.create(1).unwrap();
    let sender = hub.sender(&setup).unwrap();
    let receiver = hub.receiver(setup).unwrap();
    assert!(matches!(
        hub.send(
            &mut scheduler,
            &root,
            &sender,
            Packet::new(7, &retired),
            None
        )
        .unwrap(),
        h::Send::Sent { .. }
    ));
    let child = scheduler.spawn(&root, None).unwrap();
    scheduler.yield_now(&root).unwrap();
    assert_eq!(
        scheduler.dispatch().unwrap(),
        Some(t::Dispatch::Resume(child.clone()))
    );
    let h::Send::Waiting(operation) = hub
        .send(
            &mut scheduler,
            &child,
            &sender,
            Packet::new(9, &retired),
            None,
        )
        .unwrap()
    else {
        panic!("full send")
    };
    assert_eq!(
        scheduler.dispatch().unwrap(),
        Some(t::Dispatch::Resume(root.clone()))
    );
    let h::Cancelled::Send(value) = hub.cancel(&mut scheduler, &operation).unwrap() else {
        panic!("cancelled sending owner")
    };
    let mut report = session();
    report
        .retire(
            c::Slot(0),
            value,
            None,
            c::Reason::Failure,
            &mut |_: &mut Packet, _| Ok(()),
        )
        .unwrap();
    hub.abort_receiver(
        &mut scheduler,
        &receiver,
        &mut report,
        c::Reason::Failure,
        &mut |_: &mut Packet, _| Ok(()),
    )
    .unwrap();
    hub.release_sender(&mut scheduler, &sender).unwrap();
    scheduler.finish(&root, t::Outcome::Success).unwrap();
    while let Some(dispatch) = scheduler.dispatch().unwrap() {
        let t::Dispatch::Cleanup { task, outcome } = dispatch else {
            panic!("cancelled waiter cannot resume")
        };
        assert_eq!(
            outcome,
            if task == child {
                t::Outcome::Cancelled
            } else {
                t::Outcome::Success
            }
        );
        scheduler.finish_cleanup(&task, None).unwrap();
    }
    assert_eq!(*retired.borrow(), [9, 7]);
    assert!(!report.is_poisoned());
    assert_eq!(
        (
            hub.usage().pending,
            hub.usage().queued,
            hub.usage().reserved_slots
        ),
        (0, 0, 0)
    );
    assert_eq!(scheduler.usage().live, 0);
    println!("cancelled payloads={:?} live=0", retired.borrow());
}

fn deadlock() {
    let mut scheduler = t::Scheduler::new(t::Limits {
        live_tasks: 1,
        total_tasks: 1,
    })
    .unwrap();
    let root = scheduler.start(None).unwrap();
    scheduler.dispatch().unwrap();
    let mut hub = h::Hub::<i64>::prepare(
        &scheduler,
        &root,
        h::Limits::default(),
        e::ServiceWork::new(None),
    )
    .unwrap();
    let setup = hub.create(1).unwrap();
    let sender = hub.sender(&setup).unwrap();
    let mut receiver = hub.receiver(setup).unwrap();
    let h::Receive::Waiting(operation) = hub
        .receive(&mut scheduler, &root, &mut receiver, None)
        .unwrap()
    else {
        panic!("open empty receive")
    };
    assert_eq!(scheduler.dispatch().unwrap(), None);
    // Record the typed closed-invocation failure before mandatory cancellation.
    let failure = hub
        .deadlock(&scheduler)
        .unwrap()
        .expect("oldest pending origin");
    assert_eq!(failure.operation, operation);
    assert!(matches!(
        hub.cancel(&mut scheduler, &operation).unwrap(),
        h::Cancelled::Receive
    ));
    hub.abort_receiver(
        &mut scheduler,
        &receiver,
        &mut session(),
        c::Reason::Failure,
        &mut |_: &mut i64, _| Ok(()),
    )
    .unwrap();
    hub.release_sender(&mut scheduler, &sender).unwrap();
    let Some(t::Dispatch::Cleanup { task, outcome }) = scheduler.dispatch().unwrap() else {
        panic!("deadlocked root cleanup")
    };
    assert_eq!(outcome, t::Outcome::Cancelled);
    scheduler.finish_cleanup(&task, None).unwrap();
    assert_eq!(scheduler.usage().live, 0);
    println!(
        "ChannelDeadlock ordinal={} live=0",
        failure.operation.ordinal()
    );
}
