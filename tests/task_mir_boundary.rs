use lang_mir::unit::{
    self as m,
    ownership::{self as o, bodies as b, declarations as d, places as p, tasks},
};
use lang_runtime::layout::{Error, FramePlan, Layouts};

#[test]
fn verified_scalar_tasks_cannot_enter_ordinary_storage_execution_or_codec_paths() {
    let span = m::Span {
        source: m::SourceId(0),
        start: 0,
        end: 5,
    };
    let ty = o::TypeUse {
        ty: o::Type::I64,
        span,
    };
    let instruction = |results, op| b::Instruction { results, op, span };
    let raw = b::BodyTable {
        places: p::PlaceTable {
            declarations: d::Declarations {
                types: o::TypeTable {
                    sources: vec![m::Source {
                        path: "task.lang".into(),
                        text: "async".into(),
                    }],
                    ..Default::default()
                },
                functions: vec![d::Function {
                    effect: d::FunctionEffect::Asynchronous { span },
                    abi: d::FunctionAbi::Guest,
                    name: "main".into(),
                    public: false,
                    span,
                    signature_span: span,
                    parameters: vec![],
                    result: ty,
                    locals: vec![],
                }],
                imports: vec![],
            },
            functions: vec![p::FunctionPlaces {
                values: vec![ty],
                places: vec![],
            }],
        },
        functions: vec![b::FunctionBody {
            task_scopes: vec![b::TaskScope { parent: None, span }],
            blocks: vec![b::Block {
                parameters: vec![],
                instructions: vec![
                    instruction(vec![m::Register(0)], b::Op::I64(42)),
                    instruction(vec![], b::Op::TaskYield),
                    instruction(vec![], b::Op::TaskSaveReturn(m::Register(0))),
                    instruction(vec![], b::Op::TaskScopeExit(b::TaskScopeId(0))),
                ],
                terminator: b::Terminator {
                    kind: b::Term::ReturnSaved,
                    span,
                },
                span,
            }],
            ..Default::default()
        }],
    };
    assert!(
        lang_bytecode::ownership::encode(raw.clone(), Some(m::FunctionId(0)), Default::default())
            .is_err()
    );
    let verified = tasks::verify(raw, Default::default()).unwrap();
    let typed = verified.typed();
    let layouts =
        Layouts::compile(typed.places().declarations().types(), Default::default()).unwrap();
    assert!(
        matches!(FramePlan::compile(&layouts,typed,m::FunctionId(0),Default::default(),1024),Err(Error::UnsupportedTaskProfile{span:s}) if s==span)
    );
    let flow = verified.plan();
    assert!(matches!(lang_codegen_clif::owned::FunctionLayout::compile(
        flow, &layouts, m::FunctionId(0), Default::default(), 1024
    ), Err(lang_codegen_clif::owned::Error::UnsupportedTaskProfile { span: s }) if s == span));
    use lang_runtime::{allocation as a, cleanup as c};
    use lang_vm::unit::{Control, owned as vm};
    let mut instance = c::Instance::default();
    for selected in [false, true] {
        let control = Control::with_limits(Default::default());
        let result = if selected {
            vm::execute_function::<a::System>(flow, m::FunctionId(0), &mut instance, None, control)
        } else {
            vm::execute_entry::<a::System>(flow, &mut instance, None, control)
        };
        let e = result.err().unwrap();
        assert!(matches!(e.reason, vm::Reason::UnsupportedTaskProfile));
        assert_eq!(e.fault, Some(span));
        assert!(e.report.is_none());
        assert_eq!(e.usage.fuel, 0);
        assert!(!instance.is_poisoned());
    }
    let e = vm::execute_preview::<a::System>(
        flow,
        m::FunctionId(0),
        &mut instance,
        None,
        Control::with_limits(Default::default()),
        1024,
    )
    .err()
    .unwrap();
    assert!(matches!(e.reason, vm::Reason::UnsupportedTaskProfile));
    assert!(!instance.is_poisoned());
}
