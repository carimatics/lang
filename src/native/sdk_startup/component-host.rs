mod bindings {
    include!("bindings.rs");
}
use lang_runtime::{
    allocation as a,
    component::{execution, grants, native, program, storage, wire},
    execution as e,
    task::runner as r,
};
fn main() {
    let unit = lang::artifact::component::decode(bindings::SEMANTIC, Default::default())
        .unwrap()
        .into_unit();
    let mut session = wire::Session::new(Default::default());
    let contract = wire::artifact::source_contract(&unit, &mut session, 65_536).unwrap();
    // This actual source has no imports, so its real grant/inventory set is empty.
    let providers: [native::Provider<'_>; 0] = [];
    let bound = grants::prepare(&unit, &contract, &[], || &providers, &mut session).unwrap();
    let program = program::Program::prepare(&bound, Default::default()).unwrap();
    let entry = unit.exports()[0].0;
    let input = lang_codegen_clif::sdk::runtime::Input {
        target: bindings::TARGET,
        build_id: bindings::BUILD_ID,
        semantic: bindings::SEMANTIC,
        native: bindings::NATIVE,
    };
    // SAFETY: the verified original object and generated addresses are linked
    // with this exact SDK and stay executable until process exit.
    let linked = unsafe {
        lang::native::component::bind(input, &program, (), bindings::resolve, Default::default())
    }
    .unwrap();
    let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
    let mut budget = lang_mir::unit::Budget::new(Default::default());
    let mut resources = storage::Resources::prepare(&allocator, 4, 4, &mut budget).unwrap();
    let mut state = execution::InstanceState::prepare(&mut resources).unwrap();
    let mut engine = lang_codegen_clif::component::Engine::prepare(&mut budget).unwrap();
    for (value, fuel) in [(41, Some(0)), (41, None), (-1, None), (0, None)] {
        let control = e::Control::with_limits(e::Limits {
            fuel,
            ..Default::default()
        });
        let mut invoker = engine.begin(control).unwrap();
        let result = state
            .with_program(&program, |instance| {
                let mut invocation = execution::Invocation::prepare_owned(
                    instance,
                    entry,
                    &mut resources,
                    &mut invoker,
                    &mut budget,
                )
                .unwrap();
                invocation.prepare_output(&mut session).unwrap();
                linked.execute_component(
                    &mut allocator,
                    control,
                    Default::default(),
                    &[r::EntryArgument::I64(value)],
                    &mut invocation,
                )
            })
            .unwrap();
        if fuel.is_some() {
            let failure = result.err().unwrap();
            assert!(matches!(failure.reason, r::Reason::Control(_)));
            assert!(!failure.poisoned);
        } else if value < 0 {
            let failure = result.err().unwrap();
            assert!(matches!(failure.reason, r::Reason::Panic(Some(-42))));
            assert!(!failure.poisoned);
        } else {
            let outcome = result.unwrap();
            let execution::Value::Frame(frame) = outcome.value else {
                panic!("scalar frame")
            };
            assert!(
                matches!(frame.values().nodes.as_slice(), [wire::Value::I64(answer)] if *answer == value + 1)
            );
        }
        assert!(resources.is_empty());
        assert_eq!(allocator.stats().live_allocations, 0);
        assert!(!state.poisoned());
    }
    println!("component 42/panic -42/reuse 1/fuel refusal; zero inventory");
}
