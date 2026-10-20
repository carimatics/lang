use lang_mir::unit::{
    self as m,
    ownership::{bodies as b, declarations as d},
};
use lang_runtime::{allocation as a, execution as e, layout as l};
use std::{cell::RefCell, path::Path, rc::Rc};
fn prepare(text: &str) -> Box<lang_semantics::analysis::TaskProgram> {
    let analyzed = lang::frontend::analyze_virtual(
        lang_diagnostics::SourceMap::new(),
        Path::new("retirement.lang"),
        text.as_bytes(),
        None,
        Default::default(),
        None,
    );
    assert!(
        analyzed.diagnostics.is_empty(),
        "{:?}",
        analyzed.diagnostics
    );
    let lang_semantics::analysis::Program::Tasks(p) = analyzed.result.unwrap() else {
        panic!("task profile")
    };
    p
}
struct Region {
    id: usize,
    storage: a::Storage,
}
impl a::Region for Region {
    fn bytes(&self) -> &[u8] {
        self.storage.bytes()
    }
    fn bytes_mut(&mut self) -> &mut [u8] {
        self.storage.bytes_mut()
    }
}
struct Provider {
    next: usize,
    retired: Rc<RefCell<Vec<usize>>>,
}
impl a::Provider for Provider {
    type Region = Region;
    fn allocate(&mut self, request: a::Request) -> Option<Region> {
        self.next += 1;
        Some(Region {
            id: self.next,
            storage: a::Storage::try_new(request).ok()?,
        })
    }
    fn deallocate(&mut self, region: Region, _: a::Request) {
        self.retired.borrow_mut().push(region.id);
    }
}
#[test]
fn unclaimed_task_results_retire_after_all_joins_in_reverse_publication_order() {
    for reverse in [false, true] {
        let original = include_str!("../examples/scripts/task_result_retirement.lang");
        let source = if reverse {
            original.replace("value < 22", "21 < value")
        } else {
            original.to_owned()
        };
        let program = prepare(&source);
        let proof = program.verified();
        let layouts = l::Layouts::compile_tasks(proof, Default::default()).unwrap();
        let functions = proof.typed().places().declarations().functions();
        let main = m::FunctionId(functions.iter().position(|f| f.name == "main").unwrap() as u32);
        let worker =
            m::FunctionId(functions.iter().position(|f| f.name == "worker").unwrap() as u32);
        let root =
            l::FramePlan::compile_tasks(&layouts, proof, main, Default::default(), usize::MAX)
                .unwrap();
        let child =
            l::FramePlan::compile_tasks(&layouts, proof, worker, Default::default(), usize::MAX)
                .unwrap();
        let root_requests = root.requests().iter().filter(|r| r.bytes() != 0).count();
        let child_requests = child.requests().iter().filter(|r| r.bytes() != 0).count();
        let result_offset = child.requests()[..child.return_cell() * 2]
            .iter()
            .filter(|r| r.bytes() != 0)
            .count()
            + 1;
        let first = root_requests + result_offset;
        let second = first + child_requests;
        let native =
            lang_codegen_clif::tasks::CompiledProgram::compile(proof, Default::default()).unwrap();
        for compiled in [false, true] {
            let retired = Rc::new(RefCell::new(Vec::new()));
            let mut allocator = a::Allocator::new(
                Provider {
                    next: 0,
                    retired: retired.clone(),
                },
                Default::default(),
            )
            .unwrap();
            let limits = lang_vm::unit::tasks::Limits {
                trace_events: 128,
                ..Default::default()
            };
            let control = e::Control::with_limits(Default::default());
            let result = if compiled {
                native.execute(&mut allocator, control, limits)
            } else {
                lang_vm::unit::tasks::execute_entry(proof, &mut allocator, control, limits)
            }
            .unwrap();
            assert_eq!(result.value, 42);
            assert_eq!(result.tasks.live, 0);
            assert_eq!(allocator.stats().live_allocations, 0);
            let publication: Vec<_> = result
                .events
                .iter()
                .filter(|e| {
                    e.kind
                        == lang_vm::unit::tasks::EventKind::Complete(
                            lang_runtime::task::Outcome::Success,
                        )
                        && e.task != 0
                })
                .map(|e| e.task)
                .collect();
            assert_eq!(publication, if reverse { vec![2, 1] } else { vec![1, 2] });
            let actual: Vec<_> = retired
                .borrow()
                .iter()
                .copied()
                .filter(|id| *id == first || *id == second)
                .collect();
            assert_eq!(
                actual,
                if reverse {
                    vec![first, second]
                } else {
                    vec![second, first]
                },
                "compiled={compiled}: retained results must retire after all joins in reverse publication order"
            );
            let events = retired.borrow();
            let slow_argument = root_requests + if reverse { 0 } else { child_requests } + 1;
            let fast_result = if reverse { second } else { first };
            let argument_retired = events.iter().position(|id| *id == slow_argument).unwrap();
            assert!(
                events.iter().position(|id| *id == fast_result).unwrap() > argument_retired,
                "a completed sibling result retired before the slow child finished cleanup"
            );
        }
    }
}
#[test]
fn source_task_return_ends_loans_then_joins_before_ordinary_owned_cleanup() {
    let program = prepare(include_str!("fixtures/task_owned.lang"));
    let proof = program.verified();
    let typed = proof.typed();
    let declarations = typed.places().declarations();
    for (index, function) in declarations.functions().iter().enumerate() {
        if function.name != "worker" {
            continue;
        }
        let ops: Vec<_> = typed.functions()[index]
            .blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .collect();
        let join = ops
            .iter()
            .rposition(|i| matches!(i.op, b::Op::TaskScopeExit(_)))
            .unwrap();
        let saved = ops
            .iter()
            .position(|i| matches!(i.op, b::Op::TaskSaveReturn(_)))
            .unwrap();
        let mut owning = 0;
        for (at, instruction) in ops.iter().enumerate().skip(saved + 1) {
            if let b::Op::Cleanup(cleanup) = &instruction.op {
                for step in &cleanup.steps {
                    let ty = match step.target {
                        b::CleanupTarget::Local(d::LocalId(local)) => {
                            Some(function.locals[local as usize].ty.ty)
                        }
                        b::CleanupTarget::Value(value) => {
                            Some(typed.places().functions()[index].values[value.0 as usize].ty)
                        }
                        b::CleanupTarget::Loan(_) => {
                            assert!(at < join);
                            None
                        }
                        _ => None,
                    };
                    if ty.is_some_and(|t| {
                        !declarations.types().properties(t).unwrap().is_copy
                            && !declarations.types().properties(t).unwrap().contains_loan
                    }) {
                        owning += 1;
                        assert!(at > join, "owned cleanup precedes implicit join");
                    }
                    if ty.is_some_and(|t| declarations.types().properties(t).unwrap().contains_loan)
                    {
                        assert!(at < join, "exiting references must retire before the join");
                    }
                }
            }
        }
        assert!(owning > 0);
    }
}

#[test]
fn nested_task_scopes_retire_their_own_locals_after_each_join() {
    let allocate =
        "core::result_unwrap::<core::Bytes,core::AllocError>(core::bytes_new(core::allocator()))";
    for returning in [false, true] {
        let tail = if returning { "return 42;" } else { "" };
        let outside = if returning { "" } else { "return 42;" };
        let source = format!(
            "async fn main()->i64{{let mut outer:core::Bytes={allocate};task_scope{{let mut inner:core::Bytes={allocate};{tail}}}{outside}}}"
        );
        let program = prepare(&source);
        let typed = program.verified().typed();
        let declarations = typed.places().declarations();
        let function = &declarations.functions()[0];
        let outer = function
            .locals
            .iter()
            .position(|l| l.name == "outer")
            .unwrap() as u32;
        let inner = function
            .locals
            .iter()
            .position(|l| l.name == "inner")
            .unwrap() as u32;
        let ops: Vec<_> = typed.functions()[0]
            .blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .collect();
        let position = |scope| {
            ops.iter()
                .position(|i| matches!(i.op,b::Op::TaskScopeExit(id) if id.0==scope))
                .unwrap()
        };
        let cleanup = |local| {
            ops.iter().position(|i|matches!(&i.op,b::Op::Cleanup(c) if c.steps.iter().any(|s|s.target==b::CleanupTarget::Local(d::LocalId(local))))).unwrap()
        };
        assert!(position(1) < cleanup(inner));
        assert!(cleanup(inner) < position(0));
        assert!(position(0) < cleanup(outer));
    }
}
