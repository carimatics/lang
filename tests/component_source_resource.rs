use lang::frontend::component as f;
use lang_diagnostics::SourceMap;
use lang_mir::unit as m;
use lang_runtime::{
    allocation as a,
    component::{
        bindings, execution, grants, idl, native,
        negotiation::{InterfaceKey, PackageKey, ResourceTypeKey},
        program,
        serialization::{DigestAlgorithm, VersionedDigest},
        storage, wire,
    },
    execution as e,
    task::runner::EntryArgument,
};
use serde_json::json;
use std::{cell::Cell, ffi::c_void, path::Path, ptr, rc::Rc};

struct Destroyed {
    calls: Cell<usize>,
    value: Cell<i64>,
    allocator: Cell<u64>,
    context: Cell<Option<native::Context>>,
    fail: bool,
    cancelled: bool,
}
#[derive(Clone, Copy)]
enum Case {
    Consume,
    Frame,
    FrameOutput,
    SharedLoan,
    ExclusiveLoan,
    MixedLoan(bool),
    LoanReturn(bool),
    LoanAliases,
    LoanCapacity,
    LoanConflict(u8),
    LoanOom(bool, usize),
    LoanFuel(bool, u64),
    LoanCells(usize),
    LoanCancelled,
    LoanCleanup,
    LoanFuelCleanup(u64),
    Nested,
    NestedReturn,
    NestedReturnBudget,
    NestedReturnMissing,
    Data,
    DataOom(usize),
    List,
    ListOom(usize),
    DeepList,
    DeepListOom(usize),
    DataList,
    DataListOom(usize),
    Scalars,
    Variant,
    Some,
    Ok,
    Err,
    Capacity,
    Return,
    ReturnFrame,
    Fuel(u64),
    FuelCleanup(u64),
    Oom(usize),
    Cancelled,
    ForeignControl,
    DestructorFailure,
    ReturnCleanupFailure,
}
impl Case {
    fn loan(self) -> Option<bool> {
        match self {
            Self::SharedLoan
            | Self::LoanAliases
            | Self::LoanCapacity
            | Self::LoanCells(_)
            | Self::LoanCancelled
            | Self::LoanCleanup
            | Self::LoanFuelCleanup(_) => Some(false),
            Self::ExclusiveLoan => Some(true),
            Self::LoanConflict(mask) => Some(mask & 1 != 0),
            Self::MixedLoan(exclusive)
            | Self::LoanReturn(exclusive)
            | Self::LoanOom(exclusive, _)
            | Self::LoanFuel(exclusive, _) => Some(exclusive),
            _ => None,
        }
    }
    fn loan_extra(self) -> bool {
        self.loan().is_some() && !matches!(self, Self::SharedLoan | Self::ExclusiveLoan)
    }
    fn loan_aliases(self) -> bool {
        matches!(
            self,
            Self::LoanAliases | Self::LoanCapacity | Self::LoanConflict(_)
        )
    }
    fn second_exclusive(self) -> bool {
        matches!(self,Self::LoanConflict(mask) if mask&2!=0)
    }
    fn envelope(self) -> usize {
        match self {
            Self::Variant => 4,
            Self::Some | Self::Ok | Self::Err => 1,
            _ => 0,
        }
    }
    fn scalars(self) -> bool {
        matches!(self, Self::Scalars)
    }
    fn data(self) -> bool {
        matches!(self, Self::Data | Self::DataOom(_))
    }
    fn list(self) -> bool {
        matches!(
            self,
            Self::List
                | Self::ListOom(_)
                | Self::DeepList
                | Self::DeepListOom(_)
                | Self::DataList
                | Self::DataListOom(_)
        )
    }
    fn list_depth(self) -> usize {
        match self {
            Self::DeepList => 32,
            Self::DeepListOom(_) => 4,
            _ => 1,
        }
    }
    fn data_list(self) -> bool {
        matches!(self, Self::DataList | Self::DataListOom(_))
    }
    fn nested(self) -> bool {
        self.scalars()
            || self.envelope() != 0
            || self.data()
            || self.list()
            || matches!(
                self,
                Self::Nested
                    | Self::NestedReturn
                    | Self::NestedReturnBudget
                    | Self::NestedReturnMissing
            )
    }
    fn returns_bundle(self) -> bool {
        self.scalars()
            || self.envelope() != 0
            || self.data()
            || self.list()
            || matches!(
                self,
                Self::NestedReturn | Self::NestedReturnBudget | Self::NestedReturnMissing
            )
    }
    fn returns_resource(self) -> bool {
        matches!(
            self,
            Self::Return | Self::ReturnFrame | Self::ReturnCleanupFailure | Self::LoanReturn(_)
        )
    }
    fn failing_cleanup(self) -> bool {
        matches!(
            self,
            Self::DestructorFailure
                | Self::ReturnCleanupFailure
                | Self::FuelCleanup(_)
                | Self::LoanCleanup
                | Self::LoanFuelCleanup(_)
        )
    }
}
struct Failing {
    attempts: Rc<Cell<usize>>,
    fail_at: Rc<Cell<Option<usize>>>,
}
impl a::Provider for Failing {
    type Region = a::Storage;
    fn allocate(&mut self, request: a::Request) -> Option<Self::Region> {
        let attempt = self.attempts.get();
        self.attempts.set(attempt + 1);
        if self.fail_at.get() == Some(attempt) {
            None
        } else {
            a::Storage::try_new(request).ok()
        }
    }
    fn deallocate(&mut self, region: Self::Region, _: a::Request) {
        drop(region);
    }
}
extern "C" fn unused(
    _: *mut c_void,
    _: *const native::Context,
    _: native::Bytes,
    _: *mut *mut native::Buffer,
) -> u32 {
    2
}
unsafe extern "C" fn destroy(
    user: *mut c_void,
    context: *const native::Context,
    backing: native::MutBytes,
    _: *mut *mut native::Buffer,
) -> u32 {
    // SAFETY: registration retains this actual user object and engine scope;
    // the runtime supplies the unique live detached eight-byte backing region.
    let state = unsafe { &*user.cast::<Destroyed>() };
    if unsafe { lang_codegen_clif::component::context_check(context) } != u32::from(state.cancelled)
    {
        return 1;
    }
    if unsafe { lang_codegen_clif::component::context_allocator_identity(context) }
        != state.allocator.get()
    {
        return 1;
    }
    if backing.length != 8 || backing.data.is_null() {
        return 1;
    }
    let bytes = unsafe { std::slice::from_raw_parts(backing.data, 8) };
    state.calls.set(state.calls.get() + 1);
    state
        .value
        .set(i64::from_le_bytes(bytes.try_into().unwrap()));
    state.context.set(Some(unsafe { *context }));
    u32::from(state.fail)
}

#[test]
fn source_resource_argument_executes_and_retires_real_native_backing_on_vm_and_native() {
    assert_eq!(run_source(Case::Consume), (true, true));
}

#[test]
fn contextual_resource_frame_executes_source_and_refuses_bad_indices_before_effects() {
    assert_eq!(run_source(Case::Frame), (true, true));
}

#[test]
fn whole_resource_batch_capacity_refusal_precedes_provider_effects() {
    assert_eq!(run_source(Case::Capacity), (false, false));
}

#[test]
fn nested_resource_frame_executes_the_record_source_body_on_both_backends() {
    assert_eq!(run_source(Case::Nested), (true, true));
}

#[test]
fn rich_resource_result_publishes_canonical_bytes_and_original_backing_after_cleanup() {
    assert_eq!(run_source(Case::NestedReturn), (true, true));
}

#[test]
fn rich_result_caller_budget_failure_retires_all_accepted_owners_without_publication() {
    assert_eq!(run_source(Case::NestedReturnBudget), (false, true));
}

#[test]
fn missing_rich_result_budget_is_refused_before_provider_effects() {
    assert_eq!(run_source(Case::NestedReturnMissing), (false, false));
}

#[test]
fn resource_text_and_bytes_record_round_trips_original_custody_on_both_engines() {
    assert_eq!(run_source(Case::Data), (true, true));
}

#[test]
fn every_mixed_data_provider_refusal_preserves_or_retires_original_owners() {
    let results: Vec<_> = (0..48)
        .map(|offset| run_source(Case::DataOom(offset)))
        .collect();
    assert!(results.contains(&(false, false)));
    assert!(results.contains(&(false, true)));
    assert!(results.contains(&(true, true)));
}

#[test]
fn resource_list_source_round_trips_actual_vector_storage_and_owner_custody() {
    assert_eq!(run_source(Case::List), (true, true));
}

#[test]
fn resource_list_provider_refusals_clean_partial_elements_and_preserve_callers() {
    let results: Vec<_> = (0..64)
        .map(|offset| run_source(Case::ListOom(offset)))
        .collect();
    assert!(results.contains(&(false, false)));
    assert!(results.contains(&(false, true)));
    assert!(results.contains(&(true, true)));
}

#[test]
fn deeply_nested_owned_lists_use_iterative_source_transport_and_retirement() {
    assert_eq!(run_source(Case::DeepList), (true, true));
}
#[test]
fn nested_list_provider_refusals_drain_partial_parent_and_element_storage() {
    let results: Vec<_> = (0..96)
        .map(|offset| run_source(Case::DeepListOom(offset)))
        .collect();
    assert!(results.contains(&(false, false)));
    assert!(results.contains(&(false, true)));
    assert!(results.contains(&(true, true)));
}
#[test]
fn lists_of_resource_and_builtin_data_records_round_trip_on_both_engines() {
    assert_eq!(run_source(Case::DataList), (true, true));
}
#[test]
fn partial_record_list_elements_retire_new_data_without_consuming_caller_resources() {
    let results: Vec<_> = (0..96)
        .map(|offset| run_source(Case::DataListOom(offset)))
        .collect();
    assert!(results.contains(&(false, false)));
    assert!(results.contains(&(false, true)));
    assert!(results.contains(&(true, true)));
}

#[test]
fn mixed_owned_scalar_and_empty_sum_cases_round_trip_on_both_engines() {
    assert_eq!(run_source(Case::Scalars), (true, true));
}

#[test]
fn nominal_variant_and_core_sum_resource_payloads_round_trip_on_both_engines() {
    for case in [Case::Variant, Case::Some, Case::Ok, Case::Err] {
        assert_eq!(run_source(case), (true, true));
    }
}

#[test]
fn source_resource_result_moves_original_backing_to_caller_on_vm_and_native() {
    assert_eq!(run_source(Case::Return), (true, true));
}

#[test]
fn explicit_frame_output_covers_scalar_and_direct_resource_results() {
    assert_eq!(run_source(Case::FrameOutput), (true, true));
    assert_eq!(run_source(Case::ReturnFrame), (true, true));
}

#[test]
fn direct_shared_and_exclusive_resource_loans_execute_without_consuming_caller() {
    assert_eq!(run_source(Case::SharedLoan), (true, true));
    assert_eq!(run_source(Case::ExclusiveLoan), (true, true));
}

#[test]
fn mixed_owning_resource_and_direct_loans_commit_as_one_call_and_expire_before_return() {
    for exclusive in [false, true] {
        assert_eq!(run_source(Case::MixedLoan(exclusive)), (true, true));
        assert_eq!(run_source(Case::LoanReturn(exclusive)), (true, true));
    }
    assert_eq!(run_source(Case::LoanAliases), (true, true));
}
#[test]
fn direct_loans_and_owning_inputs_preserve_callers_across_provider_refusals() {
    for exclusive in [false, true] {
        let results: Vec<_> = (0..48)
            .map(|offset| run_source(Case::LoanOom(exclusive, offset)))
            .collect();
        assert!(results.contains(&(false, false)));
        assert!(results.contains(&(false, true)));
        assert!(results.contains(&(true, true)));
    }
}
#[test]
fn direct_loans_expire_after_fuel_cell_and_interruption_failures() {
    for exclusive in [false, true] {
        let results: Vec<_> = (0..32)
            .map(|fuel| run_source(Case::LoanFuel(exclusive, fuel)))
            .collect();
        assert!(results.contains(&(false, true)));
        assert!(results.contains(&(true, true)));
    }
    let results: Vec<_> = (0..64)
        .map(|cap| run_source(Case::LoanCells(cap)))
        .collect();
    assert!(results.contains(&(false, false)));
    assert!(results.contains(&(false, true)));
    assert!(results.contains(&(true, true)));
    assert_eq!(run_source(Case::LoanCancelled), (false, false));
}
#[test]
fn complete_shared_alias_batch_capacity_is_refused_before_provider_effects() {
    assert_eq!(run_source(Case::LoanCapacity), (false, false));
}
#[test]
fn incompatible_direct_loan_aliases_are_refused_as_a_complete_batch() {
    for mask in [1, 2, 3] {
        assert_eq!(run_source(Case::LoanConflict(mask)), (false, false));
    }
}

#[test]
fn direct_loans_expire_before_owned_cleanup_failure_and_preserve_the_primary_error() {
    assert_eq!(run_source(Case::LoanCleanup), (false, true));
    assert_eq!(run_source(Case::LoanFuelCleanup(0)), (false, true));
}

#[test]
fn source_resource_fuel_prefixes_retire_once_after_commit_on_both_backends() {
    let results: Vec<_> = (0..16).map(|fuel| run_source(Case::Fuel(fuel))).collect();
    assert!(results.contains(&(false, true)));
    assert!(results.contains(&(true, true)));
}
#[test]
fn source_resource_provider_refusals_preserve_or_retire_original_owner_on_both_backends() {
    let results: Vec<_> = (0..12)
        .map(|offset| run_source(Case::Oom(offset)))
        .collect();
    assert!(results.contains(&(false, false)));
    assert!(results.contains(&(false, true)));
    assert!(results.contains(&(true, true)));
}
#[test]
fn source_resource_cancellation_before_commit_preserves_the_caller_owner() {
    run_source(Case::Cancelled);
}
#[test]
fn source_resource_foreign_control_is_refused_before_argument_commit() {
    assert_eq!(run_source(Case::ForeignControl), (false, false));
}
#[test]
fn source_resource_destructor_failure_poisoning_is_reported_on_both_backends() {
    run_source(Case::DestructorFailure);
}
#[test]
fn source_resource_result_is_not_published_after_callee_cleanup_failure() {
    run_source(Case::ReturnCleanupFailure);
}
#[test]
fn source_resource_cleanup_failure_preserves_the_original_fuel_failure() {
    run_source(Case::FuelCleanup(0));
}

fn release_caller(
    values: &mut storage::Resources<'_>,
    claim: storage::Claim,
    kind: native::ResourceKind<'_>,
    allocator: &mut a::Allocator<Failing>,
    invoker: &mut dyn execution::Invoker,
    budget: &mut m::Budget,
) {
    let mut backing = values.detach(claim, values.host(), allocator).unwrap();
    let mut cleanup =
        lang_runtime::cleanup::Session::prepare(std::num::NonZeroUsize::new(2).unwrap(), budget)
            .unwrap();
    let mut cleaner = lang_runtime::cleanup::Contextual::new(
        |_: &mut (), _, context: &mut lang_runtime::cleanup::Context<'_>| {
            execution::Invoker::destroy(invoker, kind, &mut backing, allocator, context)
        },
    );
    let retired = cleanup.retire(
        lang_runtime::cleanup::Slot(0),
        (),
        None,
        lang_runtime::cleanup::Reason::ScopeExit,
        &mut cleaner,
    );
    backing.release(allocator).unwrap();
    retired.unwrap();
    assert!(!cleanup.is_poisoned());
}

fn run_source(case: Case) -> (bool, bool) {
    let return_resource = case.returns_resource();
    let extra = case.loan_extra()
        || case.scalars()
        || case.envelope() != 0
        || case.data()
        || case.list()
        || matches!(
            case,
            Case::ReturnCleanupFailure
                | Case::Frame
                | Case::FrameOutput
                | Case::Nested
                | Case::NestedReturn
                | Case::NestedReturnBudget
                | Case::NestedReturnMissing
                | Case::Capacity
        );
    let host_json: serde_json::Value =
        serde_json::from_slice(include_bytes!("fixtures/component_counter.idl.json")).unwrap();
    let consumer = json!({"idl":1,"package":"demo:consumer","version":"1.0.0","interfaces":[host_json["interfaces"][0].clone(),
        {"key":"demo:consumer/api","resources":[],"types":[],"functions":[{"name":"consume","effect":"sync","params":[{"name":"value","type":{"own":"demo:counter/api.counter"}},{"name":"number","type":"i64"}],"result":if return_resource{json!({"own":"demo:counter/api.counter"})}else{json!("i64")},"capability":"demo.consumer.consume"}]}
    ],"imports":[{"interface":"demo:counter/api","version":{"exact":"1.0.0"},"members":["apply"]}],"exports":["demo:consumer/api"]});
    let mut consumer = consumer;
    if let Some(exclusive) = case.loan() {
        consumer["interfaces"][1]["functions"][0]["params"][0]["type"] = if exclusive {
            json!({"borrow-mut":"demo:counter/api.counter"})
        } else {
            json!({"borrow":"demo:counter/api.counter"})
        };
    }
    if extra {
        consumer["interfaces"][1]["functions"][0]["params"]
            .as_array_mut()
            .unwrap()
            .push(json!({"name":"extra","type":{"own":"demo:counter/api.counter"}}));
    }
    if case.loan_aliases() {
        consumer["interfaces"][1]["functions"][0]["params"][2]["type"] = if case.second_exclusive()
        {
            json!({"borrow-mut":"demo:counter/api.counter"})
        } else {
            json!({"borrow":"demo:counter/api.counter"})
        };
    }
    if matches!(case, Case::Capacity) {
        for name in ["third", "fourth"] {
            consumer["interfaces"][1]["functions"][0]["params"]
                .as_array_mut()
                .unwrap()
                .push(json!({"name":name,"type":{"own":"demo:counter/api.counter"}}));
        }
    }
    if case.nested() && !case.list() {
        consumer["interfaces"][1]["types"] = json!([{"name":"bundle","record":[
            {"name":"value","type":{"own":"demo:counter/api.counter"}},
            {"name":"number","type":"i64"},
            {"name":"extra","type":{"own":"demo:counter/api.counter"}}
        ]}]);
        consumer["interfaces"][1]["functions"][0]["params"] =
            json!([{"name":"bundle","type":"bundle"}]);
    }
    if case.returns_bundle() {
        consumer["interfaces"][1]["functions"][0]["result"] = json!("bundle");
    }
    if case.data() {
        consumer["interfaces"][1]["types"][0]["record"]
            .as_array_mut()
            .unwrap()
            .extend([
                json!({"name":"label","type":"text"}),
                json!({"name":"payload","type":"bytes"}),
            ]);
    }
    if case.list() {
        let mut ty = if case.data_list() {
            consumer["interfaces"][1]["types"] = json!([{"name":"bundle","record":[{"name":"value","type":{"own":"demo:counter/api.counter"}},{"name":"label","type":"text"},{"name":"payload","type":"bytes"}]}]);
            json!("bundle")
        } else {
            json!({"own":"demo:counter/api.counter"})
        };
        for _ in 0..case.list_depth() {
            ty = json!({"list":ty});
        }
        consumer["interfaces"][1]["functions"][0]["params"] =
            json!([{"name":"items","type":ty.clone()}]);
        consumer["interfaces"][1]["functions"][0]["result"] = ty;
    }
    if case.scalars() {
        consumer["interfaces"][1]["types"][0]["record"]
            .as_array_mut()
            .unwrap()
            .extend([
                json!({"name":"flag","type":"bool"}),
                json!({"name":"unit","type":"unit"}),
                json!({"name":"none","type":{"option":"i64"}}),
                json!({"name":"some","type":{"option":"i64"}}),
                json!({"name":"ok","type":{"result":{"ok":"i64","err":"bool"}}}),
                json!({"name":"err","type":{"result":{"ok":"i64","err":"bool"}}}),
                json!({"name":"empty","type":{"list":"i64"}}),
                json!({"name":"blank","type":"blank"}),
            ]);
        consumer["interfaces"][1]["types"].as_array_mut().unwrap().push(
            json!({"name":"blank","variant":[{"name":"empty","type":"unit"},{"name":"number","type":"i64"}]}));
    }
    if case.envelope() != 0 {
        let ty = match case {
            Case::Variant => {
                consumer["interfaces"][1]["types"].as_array_mut().unwrap().push(json!({"name":"choice","variant":[{"name":"hold","type":"bundle"},{"name":"empty","type":"unit"}]}));
                json!("choice")
            }
            Case::Some => json!({"option":"bundle"}),
            Case::Ok => json!({"result":{"ok":"bundle","err":"unit"}}),
            Case::Err => json!({"result":{"ok":"unit","err":"bundle"}}),
            _ => unreachable!(),
        };
        consumer["interfaces"][1]["functions"][0]["params"] =
            json!([{"name":"value","type":ty.clone()}]);
        consumer["interfaces"][1]["functions"][0]["result"] = ty;
    }
    let bytes = serde_json::to_vec(&consumer).unwrap();
    let checked = || idl::check(m::SourceId(82), &bytes, Default::default()).unwrap();
    let source = if matches!(case, Case::Capacity) {
        "pub struct CounterKind{}pub struct Request{delta:i64,label:core::Text}extern \"component\" fn host_apply(state:&mut core::ComponentResource<CounterKind>,request:Request)->core::Result<i64,core::Text>;pub fn consume(value:core::ComponentResource<CounterKind>,number:i64,extra:core::ComponentResource<CounterKind>,third:core::ComponentResource<CounterKind>,fourth:core::ComponentResource<CounterKind>)->i64{return number+1;}"
    } else if case.scalars() {
        "pub struct CounterKind{}pub struct Request{delta:i64,label:core::Text}extern \"component\" fn host_apply(state:&mut core::ComponentResource<CounterKind>,request:Request)->core::Result<i64,core::Text>;pub enum Blank{empty,number(i64)}pub struct Bundle{value:core::ComponentResource<CounterKind>,number:i64,extra:core::ComponentResource<CounterKind>,flag:bool,unit:core::Unit,none:core::Option<i64>,some:core::Option<i64>,ok:core::Result<i64,bool>,err:core::Result<i64,bool>,empty:core::Vec<i64>,blank:Blank}pub fn consume(bundle:Bundle)->Bundle{return bundle;}"
    } else if matches!(case, Case::Variant) {
        "pub struct CounterKind{}pub struct Request{delta:i64,label:core::Text}extern \"component\" fn host_apply(state:&mut core::ComponentResource<CounterKind>,request:Request)->core::Result<i64,core::Text>;pub struct Bundle{value:core::ComponentResource<CounterKind>,number:i64,extra:core::ComponentResource<CounterKind>}pub enum Choice{hold(Bundle),empty}pub fn consume(value:Choice)->Choice{return value;}"
    } else if matches!(case, Case::Some) {
        "pub struct CounterKind{}pub struct Request{delta:i64,label:core::Text}extern \"component\" fn host_apply(state:&mut core::ComponentResource<CounterKind>,request:Request)->core::Result<i64,core::Text>;pub struct Bundle{value:core::ComponentResource<CounterKind>,number:i64,extra:core::ComponentResource<CounterKind>}pub fn consume(value:core::Option<Bundle>)->core::Option<Bundle>{return value;}"
    } else if matches!(case, Case::Ok) {
        "pub struct CounterKind{}pub struct Request{delta:i64,label:core::Text}extern \"component\" fn host_apply(state:&mut core::ComponentResource<CounterKind>,request:Request)->core::Result<i64,core::Text>;pub struct Bundle{value:core::ComponentResource<CounterKind>,number:i64,extra:core::ComponentResource<CounterKind>}pub fn consume(value:core::Result<Bundle,core::Unit>)->core::Result<Bundle,core::Unit>{return value;}"
    } else if matches!(case, Case::Err) {
        "pub struct CounterKind{}pub struct Request{delta:i64,label:core::Text}extern \"component\" fn host_apply(state:&mut core::ComponentResource<CounterKind>,request:Request)->core::Result<i64,core::Text>;pub struct Bundle{value:core::ComponentResource<CounterKind>,number:i64,extra:core::ComponentResource<CounterKind>}pub fn consume(value:core::Result<core::Unit,Bundle>)->core::Result<core::Unit,Bundle>{return value;}"
    } else if case.list() {
        "pub struct CounterKind{}pub struct Request{delta:i64,label:core::Text}extern \"component\" fn host_apply(state:&mut core::ComponentResource<CounterKind>,request:Request)->core::Result<i64,core::Text>;pub fn consume(items:core::Vec<core::ComponentResource<CounterKind>>)->core::Vec<core::ComponentResource<CounterKind>>{return carry(items);}fn carry(items:core::Vec<core::ComponentResource<CounterKind>>)->core::Vec<core::ComponentResource<CounterKind>>{return items;}"
    } else if case.data() {
        "pub struct CounterKind{}pub struct Request{delta:i64,label:core::Text}extern \"component\" fn host_apply(state:&mut core::ComponentResource<CounterKind>,request:Request)->core::Result<i64,core::Text>;pub struct Bundle{value:core::ComponentResource<CounterKind>,number:i64,extra:core::ComponentResource<CounterKind>,label:core::Text,payload:core::Bytes}pub fn consume(bundle:Bundle)->Bundle{return carry(bundle);}fn carry(bundle:Bundle)->Bundle{return bundle;}"
    } else if case.returns_bundle() {
        "pub struct CounterKind{}pub struct Request{delta:i64,label:core::Text}extern \"component\" fn host_apply(state:&mut core::ComponentResource<CounterKind>,request:Request)->core::Result<i64,core::Text>;pub struct Bundle{value:core::ComponentResource<CounterKind>,number:i64,extra:core::ComponentResource<CounterKind>}pub fn consume(bundle:Bundle)->Bundle{return bundle;}"
    } else if case.nested() {
        "pub struct CounterKind{}pub struct Request{delta:i64,label:core::Text}extern \"component\" fn host_apply(state:&mut core::ComponentResource<CounterKind>,request:Request)->core::Result<i64,core::Text>;pub struct Bundle{value:core::ComponentResource<CounterKind>,number:i64,extra:core::ComponentResource<CounterKind>}pub fn consume(bundle:Bundle)->i64{return bundle.number+1;}"
    } else if extra && return_resource {
        "pub struct CounterKind{}pub struct Request{delta:i64,label:core::Text}extern \"component\" fn host_apply(state:&mut core::ComponentResource<CounterKind>,request:Request)->core::Result<i64,core::Text>;pub fn consume(value:core::ComponentResource<CounterKind>,number:i64,extra:core::ComponentResource<CounterKind>)->core::ComponentResource<CounterKind>{return value;}"
    } else if extra {
        "pub struct CounterKind{}pub struct Request{delta:i64,label:core::Text}extern \"component\" fn host_apply(state:&mut core::ComponentResource<CounterKind>,request:Request)->core::Result<i64,core::Text>;pub fn consume(value:core::ComponentResource<CounterKind>,number:i64,extra:core::ComponentResource<CounterKind>)->i64{return number+1;}"
    } else if return_resource {
        "pub struct CounterKind{}pub struct Request{delta:i64,label:core::Text}extern \"component\" fn host_apply(state:&mut core::ComponentResource<CounterKind>,request:Request)->core::Result<i64,core::Text>;pub fn consume(value:core::ComponentResource<CounterKind>,number:i64)->core::ComponentResource<CounterKind>{return value;}"
    } else if matches!(case, Case::Oom(_)) {
        "pub struct CounterKind{}pub struct Request{delta:i64,label:core::Text}extern \"component\" fn host_apply(state:&mut core::ComponentResource<CounterKind>,request:Request)->core::Result<i64,core::Text>;pub fn consume(value:core::ComponentResource<CounterKind>,number:i64)->i64{return add(number);}fn add(number:i64)->i64{return number+1;}"
    } else {
        "pub struct CounterKind{}pub struct Request{delta:i64,label:core::Text}extern \"component\" fn host_apply(state:&mut core::ComponentResource<CounterKind>,request:Request)->core::Result<i64,core::Text>;pub fn consume(value:core::ComponentResource<CounterKind>,number:i64)->i64{return number+1;}"
    };
    let source = if let Some(exclusive) = case.loan() {
        let ty = if exclusive {
            "&mut core::ComponentResource<CounterKind>"
        } else {
            "&core::ComponentResource<CounterKind>"
        };
        let prefix = source.split("pub fn consume").next().unwrap();
        let extra = if case.loan_aliases() {
            format!(
                ",extra:{}",
                if case.second_exclusive() {
                    "&mut core::ComponentResource<CounterKind>"
                } else {
                    "&core::ComponentResource<CounterKind>"
                }
            )
        } else if extra {
            ",extra:core::ComponentResource<CounterKind>".into()
        } else {
            String::new()
        };
        let (result, body) = if return_resource {
            (
                "core::ComponentResource<CounterKind>",
                "let mut n:i64=add(value,number);return extra;",
            )
        } else {
            ("i64", "return add(value,number);")
        };
        format!(
            "{prefix}pub fn consume(value:{ty},number:i64{extra})->{result}{{{body}}}fn add(value:{ty},number:i64)->i64{{return leaf(value,number);}}fn leaf(value:{ty},number:i64)->i64{{return number+1;}}"
        )
    } else {
        source.to_owned()
    };
    let interface = InterfaceKey::new(PackageKey::new("demo", "counter"), "api");
    let source = if case.data_list() {
        "pub struct CounterKind{}pub struct Request{delta:i64,label:core::Text}extern \"component\" fn host_apply(state:&mut core::ComponentResource<CounterKind>,request:Request)->core::Result<i64,core::Text>;pub struct Bundle{value:core::ComponentResource<CounterKind>,label:core::Text,payload:core::Bytes}pub fn consume(items:core::Vec<Bundle>)->core::Vec<Bundle>{return carry(items);}fn carry(items:core::Vec<Bundle>)->core::Vec<Bundle>{return items;}".to_owned()
    } else if case.list() && case.list_depth() > 1 {
        let mut ty = "core::ComponentResource<CounterKind>".to_owned();
        for _ in 0..case.list_depth() {
            ty = format!("core::Vec<{ty}>");
        }
        format!(
            r#"pub struct CounterKind{{}}pub struct Request{{delta:i64,label:core::Text}}extern "component" fn host_apply(state:&mut core::ComponentResource<CounterKind>,request:Request)->core::Result<i64,core::Text>;pub fn consume(items:{ty})->{ty}{{return carry(items);}}fn carry(items:{ty})->{ty}{{return items;}}"#
        )
    } else {
        source.to_owned()
    };
    let out = f::analyze(f::Request {
        sources: SourceMap::new(),
        display: Path::new("component-consume.lang"),
        bytes: source.as_bytes(),
        root: None,
        compile: Default::default(),
        document: checked(),
        bindings: f::Input {
            version: 1,
            resources: vec![f::Resource {
                tag: "CounterKind".into(),
                resource: ResourceTypeKey::new(interface.clone(), "counter"),
            }],
            imports: vec![f::Import {
                source: "host_apply".into(),
                member: bindings::MemberKey {
                    interface,
                    member: "apply".into(),
                },
            }],
            exports: vec![f::Export {
                source: "consume".into(),
                member: bindings::MemberKey {
                    interface: InterfaceKey::new(PackageKey::new("demo", "consumer"), "api"),
                    member: "consume".into(),
                },
            }],
        },
        limits: Default::default(),
    });
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    let unit = out.result.unwrap();
    let mut session = wire::Session::new(Default::default());
    let consumer = wire::Contract::new(checked(), vec![], &mut session).unwrap();
    let host = wire::Contract::new(
        idl::check(
            m::SourceId(71),
            include_bytes!("fixtures/component_counter.idl.json"),
            Default::default(),
        )
        .unwrap(),
        vec![],
        &mut session,
    )
    .unwrap();
    let digest = VersionedDigest::new(DigestAlgorithm::Sha256, vec![1; 32]).unwrap();
    let destroyed = Destroyed {
        calls: Cell::new(0),
        value: Cell::new(0),
        allocator: Cell::new(0),
        context: Cell::new(None),
        fail: case.failing_cleanup(),
        cancelled: matches!(case, Case::Cancelled | Case::LoanCancelled),
    };
    let methods = [native::Method {
        interface: 0,
        member: 0,
        invoke: unused,
        user: ptr::null_mut(),
        flags: 0,
    }];
    let resources = [native::ResourceType {
        interface: 0,
        resource: 0,
        destroy,
        user: ptr::from_ref(&destroyed).cast_mut().cast(),
    }];
    let providers = [native::Provider::with_resources(
        wire::Provider {
            contract: &host,
            digest: &digest,
        },
        &methods,
        &resources,
        &destroyed,
        &mut session,
    )
    .unwrap()];
    let member = &unit.mappings().imports[0].member;
    let signature =
        wire::Signature::new(&consumer, &member.interface, &member.member, &mut session)
            .unwrap()
            .preimage(&mut session)
            .unwrap();
    let grant = grants::Grant {
        member,
        capability: "demo.counter.apply",
        signature: &signature,
    };
    let bindings =
        grants::prepare(&unit, &consumer, &[grant], || &providers, &mut session).unwrap();
    let program = program::Program::prepare(&bindings, Default::default()).unwrap();
    let native = lang_codegen_clif::tasks::CompiledProgram::compile_component(
        &program,
        m::FunctionId(0),
        Default::default(),
    )
    .unwrap();
    let kind = bindings
        .resource_kind(m::ImportId(0), m::DefinitionId(0), &mut session)
        .unwrap();
    let mut previous = None;
    for compiled in [false, true] {
        destroyed.calls.set(0);
        let attempts = Rc::new(Cell::new(0));
        let fail_at = Rc::new(Cell::new(None));
        let mut allocator = a::Allocator::new(
            Failing {
                attempts: attempts.clone(),
                fail_at: fail_at.clone(),
            },
            Default::default(),
        )
        .unwrap();
        destroyed.allocator.set(allocator.identity().get());
        let mut budget = m::Budget::new(Default::default());
        let mut values = storage::Resources::prepare(
            &allocator,
            if matches!(case, Case::Capacity) { 2 } else { 4 },
            if matches!(case, Case::LoanCapacity) {
                1
            } else {
                4
            },
            &mut budget,
        )
        .unwrap();
        let holder = values.host();
        let input = values
            .publish(kind, holder, &41_i64.to_le_bytes(), &mut allocator)
            .unwrap();
        let extra_input = extra.then(|| {
            values
                .publish(kind, holder, &41_i64.to_le_bytes(), &mut allocator)
                .unwrap()
        });
        if let Case::Oom(offset)
        | Case::DataOom(offset)
        | Case::ListOom(offset)
        | Case::DeepListOom(offset)
        | Case::LoanOom(_, offset)
        | Case::DataListOom(offset) = case
        {
            fail_at.set(Some(attempts.get() + offset));
        }
        let interruption = e::InterruptionFlag::new();
        if matches!(case, Case::Cancelled | Case::LoanCancelled) {
            interruption.interrupt();
        }
        let control = e::Control::with_limits(e::Limits {
            fuel: if let Case::Fuel(fuel)
            | Case::FuelCleanup(fuel)
            | Case::LoanFuel(_, fuel)
            | Case::LoanFuelCleanup(fuel) = case
            {
                Some(fuel)
            } else {
                e::Limits::default().fuel
            },
            live_cells: if let Case::LoanCells(cap) = case {
                Some(cap)
            } else {
                None
            },
            ..Default::default()
        })
        .and_interruption(&interruption);
        let mut engine = lang_codegen_clif::component::Engine::prepare(&mut budget).unwrap();
        let mut limited_output = wire::Session::new(wire::Limits {
            work: 0,
            ..Default::default()
        });
        let bridge_control = if matches!(case, Case::ForeignControl) {
            e::Control::with_limits(e::Limits {
                fuel: Some(1),
                ..Default::default()
            })
            .and_interruption(&interruption)
        } else {
            control
        };
        let mut bridge = engine.begin(bridge_control).unwrap();
        let mut invocation = execution::Invocation::prepare(
            &program,
            m::FunctionId(0),
            &mut values,
            &mut bridge,
            &mut budget,
        )
        .unwrap();
        let mut arguments = vec![EntryArgument::Resource(input), EntryArgument::I64(41)];
        if let Some(extra) = extra_input {
            arguments.push(EntryArgument::Resource(extra));
        }
        if matches!(case, Case::Capacity) {
            arguments.push(EntryArgument::Resource(input));
            arguments.push(EntryArgument::Resource(extra_input.unwrap()));
        }
        let mut expected_body = None;
        if case.loan().is_some() {
            let mut frame_values = wire::Values {
                roots: vec![wire::ValueId(0), wire::ValueId(1)],
                nodes: vec![
                    wire::Value::Resource(
                        lang_runtime::component::adapter::BoundaryResourceIndex::new(1).unwrap(),
                    ),
                    wire::Value::I64(41),
                ],
            };
            let mut claims = vec![input];
            if extra {
                claims.push(extra_input.unwrap());
                frame_values.nodes.push(wire::Value::Resource(
                    lang_runtime::component::adapter::BoundaryResourceIndex::new(
                        if case.loan_aliases() { 1 } else { 2 },
                    )
                    .unwrap(),
                ));
                frame_values.roots.push(wire::ValueId(2));
            }
            let encoded =
                invocation.encode_arguments(&frame_values, &claims, &allocator, &mut session);
            if matches!(case, Case::LoanCapacity | Case::LoanConflict(_)) {
                if matches!(case, Case::LoanCapacity) {
                    assert!(matches!(
                        encoded,
                        Err(lang_runtime::task::runner::Reason::Component(
                            storage::Error::Capacity
                        ))
                    ));
                } else {
                    assert!(matches!(
                        encoded,
                        Err(lang_runtime::task::runner::Reason::Component(
                            storage::Error::AliasedOwner
                        ))
                    ));
                }
                assert!(!execution::Host::arguments_committed(&invocation));
                assert_eq!(invocation.active_loans(), 0);
                drop(invocation);
                assert_eq!(attempts.get(), 4);
                assert_eq!(destroyed.calls.get(), 0);
                for claim in claims {
                    release_caller(
                        &mut values,
                        claim,
                        kind,
                        &mut allocator,
                        &mut bridge,
                        &mut budget,
                    );
                }
                assert_eq!(destroyed.calls.get(), 2);
                assert_eq!(allocator.stats().live_allocations, 0);
                previous = Some((false, false, Default::default()));
                continue;
            }
            let encoded = encoded.unwrap();
            let before = allocator.stats();
            for index in [0_u32, 3] {
                let mut bad = encoded.clone();
                bad[45..49].copy_from_slice(&index.to_le_bytes());
                assert!(
                    invocation
                        .decode_arguments(&bad, &claims, &allocator, &mut session)
                        .is_err()
                );
                assert_eq!(allocator.stats(), before);
                assert_eq!(invocation.active_loans(), 0);
            }
            let mut bad = encoded.clone();
            bad[9] ^= 1;
            assert!(matches!(
                invocation.decode_arguments(&bad, &claims, &allocator, &mut session),
                Err(lang_runtime::task::runner::Reason::ComponentFrame(
                    wire::Error {
                        kind: wire::ErrorKind::Signature,
                        ..
                    }
                ))
            ));
            if extra && !case.loan_aliases() {
                let mut aliased = encoded.clone();
                aliased[57..61].copy_from_slice(&1_u32.to_le_bytes());
                assert!(matches!(
                    invocation.decode_arguments(&aliased, &claims, &allocator, &mut session),
                    Err(lang_runtime::task::runner::Reason::Component(
                        storage::Error::AliasedOwner
                    ))
                ));
                assert_eq!(attempts.get(), 4);
                assert_eq!(invocation.active_loans(), 0);
            }
            arguments = invocation
                .decode_arguments(&encoded, &claims, &allocator, &mut session)
                .unwrap();
            if matches!(case, Case::SharedLoan | Case::ExclusiveLoan) {
                let foreign = arguments.clone();
                drop(invocation);
                invocation = execution::Invocation::prepare(
                    &program,
                    m::FunctionId(0),
                    &mut values,
                    &mut bridge,
                    &mut budget,
                )
                .unwrap();
                arguments = invocation
                    .decode_arguments(&encoded, &claims, &allocator, &mut session)
                    .unwrap();
                let stale = arguments.clone();
                arguments = invocation
                    .decode_arguments(&encoded, &claims, &allocator, &mut session)
                    .unwrap();
                for rejected in [foreign, stale] {
                    let failure = if compiled {
                        native.execute_component(
                            &mut allocator,
                            control,
                            Default::default(),
                            &rejected,
                            &mut invocation,
                        )
                    } else {
                        lang_vm::unit::tasks::execute_component(
                            &program,
                            m::FunctionId(0),
                            &mut allocator,
                            control,
                            Default::default(),
                            &rejected,
                            &mut invocation,
                        )
                    }
                    .err()
                    .expect("foreign or replaced loan frame must be refused");
                    assert!(matches!(
                        failure.reason,
                        lang_runtime::task::runner::Reason::Entry
                    ));
                    assert!(!execution::Host::arguments_committed(&invocation));
                    assert_eq!(invocation.active_loans(), 0);
                    assert_eq!(allocator.stats(), before);
                }
            }
        }
        if matches!(case, Case::Frame | Case::FrameOutput) || case.nested() {
            let mut values = wire::Values {
                roots: vec![wire::ValueId(0), wire::ValueId(1), wire::ValueId(2)],
                nodes: vec![
                    wire::Value::Resource(
                        lang_runtime::component::adapter::BoundaryResourceIndex::new(1).unwrap(),
                    ),
                    wire::Value::I64(41),
                    wire::Value::Resource(
                        lang_runtime::component::adapter::BoundaryResourceIndex::new(2).unwrap(),
                    ),
                ],
            };
            if case.list() {
                if case.data_list() {
                    values.nodes = vec![
                        wire::Value::Resource(
                            lang_runtime::component::adapter::BoundaryResourceIndex::new(1)
                                .unwrap(),
                        ),
                        wire::Value::Text("寿司".into()),
                        wire::Value::Bytes(vec![0, 128, 255]),
                        wire::Value::Record(vec![
                            wire::ValueId(0),
                            wire::ValueId(1),
                            wire::ValueId(2),
                        ]),
                        wire::Value::Resource(
                            lang_runtime::component::adapter::BoundaryResourceIndex::new(2)
                                .unwrap(),
                        ),
                        wire::Value::Text("X".into()),
                        wire::Value::Bytes(vec![1]),
                        wire::Value::Record(vec![
                            wire::ValueId(4),
                            wire::ValueId(5),
                            wire::ValueId(6),
                        ]),
                        wire::Value::List(vec![wire::ValueId(3), wire::ValueId(7)]),
                    ];
                    values.roots = vec![wire::ValueId(8)];
                } else {
                    values.nodes = vec![
                        wire::Value::Resource(
                            lang_runtime::component::adapter::BoundaryResourceIndex::new(1)
                                .unwrap(),
                        ),
                        wire::Value::Resource(
                            lang_runtime::component::adapter::BoundaryResourceIndex::new(2)
                                .unwrap(),
                        ),
                        wire::Value::List(vec![wire::ValueId(0), wire::ValueId(1)]),
                    ];
                    values.roots = vec![wire::ValueId(2)];
                    for _ in 1..case.list_depth() {
                        let child = values.roots[0];
                        let id = wire::ValueId(values.nodes.len());
                        values.nodes.push(wire::Value::List(vec![child]));
                        values.roots = vec![id];
                    }
                }
            } else if case.nested() {
                if case.scalars() {
                    values.nodes.extend([
                        wire::Value::Bool(true),
                        wire::Value::Unit,
                        wire::Value::None,
                        wire::Value::Some(wire::ValueId(1)),
                        wire::Value::Ok(wire::ValueId(1)),
                        wire::Value::Err(wire::ValueId(3)),
                        wire::Value::List(vec![]),
                        wire::Value::Variant {
                            case: 0,
                            payload: wire::ValueId(4),
                        },
                    ]);
                    values
                        .nodes
                        .push(wire::Value::Record((0..11).map(wire::ValueId).collect()));
                    values.roots = vec![wire::ValueId(11)];
                } else if case.data() {
                    values.nodes.push(wire::Value::Text("寿司".into()));
                    values.nodes.push(wire::Value::Bytes(vec![0, 128, 255]));
                    values
                        .nodes
                        .push(wire::Value::Record((0..5).map(wire::ValueId).collect()));
                    values.roots = vec![wire::ValueId(5)];
                } else {
                    values.nodes.push(wire::Value::Record(vec![
                        wire::ValueId(0),
                        wire::ValueId(1),
                        wire::ValueId(2),
                    ]));
                    values.roots = vec![wire::ValueId(3)];
                }
                if case.envelope() != 0 {
                    let payload = values.roots[0];
                    let value = match case {
                        Case::Variant => wire::Value::Variant { case: 0, payload },
                        Case::Some => wire::Value::Some(payload),
                        Case::Ok => wire::Value::Ok(payload),
                        Case::Err => wire::Value::Err(payload),
                        _ => unreachable!(),
                    };
                    let id = wire::ValueId(values.nodes.len());
                    values.nodes.push(value);
                    values.roots = vec![id];
                }
            }
            let export = &unit.mappings().exports[0].member;
            let signature =
                wire::Signature::new(&consumer, &export.interface, &export.member, &mut session)
                    .unwrap();
            assert!(matches!(
                wire::encode_values(
                    &signature,
                    wire::FrameKind::Parameters,
                    &values,
                    &mut session
                )
                .unwrap_err()
                .kind,
                wire::ErrorKind::ResourceContextRequired
            ));
            let claims = [input, extra_input.unwrap()];
            let encoded = invocation
                .encode_arguments(&values, &claims, &allocator, &mut session)
                .unwrap();
            let first = if case.list() {
                45 + 4 * case.list_depth()
            } else {
                45 + case.envelope()
            };
            let last = if case.data_list() {
                70
            } else if case.list() {
                first + 4
            } else {
                57 + case.envelope()
            };
            assert_eq!(&encoded[first..first + 4], &1_u32.to_le_bytes());
            if !case.list() {
                assert_eq!(&encoded[first + 4..last], &41_i64.to_le_bytes());
            }
            assert_eq!(&encoded[last..last + 4], &2_u32.to_le_bytes());
            let before = allocator.stats();
            for index in [0_u32, 3] {
                let mut bad = encoded.clone();
                bad[first..first + 4].copy_from_slice(&index.to_le_bytes());
                assert!(
                    invocation
                        .decode_arguments(&bad, &claims, &allocator, &mut session)
                        .is_err()
                );
                assert!(!execution::Host::arguments_committed(&invocation));
                assert_eq!(allocator.stats(), before);
                assert_eq!(destroyed.calls.get(), 0);
            }
            let mut bad = encoded.clone();
            bad[9] ^= 1;
            assert!(matches!(
                invocation.decode_arguments(&bad, &claims, &allocator, &mut session),
                Err(lang_runtime::task::runner::Reason::ComponentFrame(
                    wire::Error {
                        kind: wire::ErrorKind::Signature,
                        ..
                    }
                ))
            ));
            let mut duplicate = encoded.clone();
            duplicate[last..last + 4].copy_from_slice(&1_u32.to_le_bytes());
            assert!(matches!(
                invocation.decode_arguments(&duplicate, &claims, &allocator, &mut session),
                Err(lang_runtime::task::runner::Reason::Component(
                    storage::Error::AliasedOwner
                ))
            ));
            if matches!(case, Case::Frame) {
                let duplicated = [
                    EntryArgument::Resource(input),
                    EntryArgument::I64(41),
                    EntryArgument::Resource(input),
                ];
                let refused = if compiled {
                    native.execute_component(
                        &mut allocator,
                        control,
                        Default::default(),
                        &duplicated,
                        &mut invocation,
                    )
                } else {
                    lang_vm::unit::tasks::execute_component(
                        &program,
                        m::FunctionId(0),
                        &mut allocator,
                        control,
                        Default::default(),
                        &duplicated,
                        &mut invocation,
                    )
                };
                assert!(matches!(
                    refused,
                    Err(lang_runtime::task::runner::Failure {
                        reason: lang_runtime::task::runner::Reason::Component(
                            storage::Error::AliasedOwner
                        ),
                        ..
                    })
                ));
            }
            assert!(!execution::Host::arguments_committed(&invocation));
            let stale = invocation
                .decode_arguments(&encoded, &claims, &allocator, &mut session)
                .unwrap();
            arguments = invocation
                .decode_arguments(&encoded, &claims, &allocator, &mut session)
                .unwrap();
            let refused = if compiled {
                native.execute_component(
                    &mut allocator,
                    control,
                    Default::default(),
                    &stale,
                    &mut invocation,
                )
            } else {
                lang_vm::unit::tasks::execute_component(
                    &program,
                    m::FunctionId(0),
                    &mut allocator,
                    control,
                    Default::default(),
                    &stale,
                    &mut invocation,
                )
            };
            assert!(matches!(
                refused,
                Err(lang_runtime::task::runner::Failure {
                    reason: lang_runtime::task::runner::Reason::Entry,
                    ..
                })
            ));
            assert!(!execution::Host::arguments_committed(&invocation));
            assert_eq!(arguments.len(), if case.nested() { 1 } else { 3 });
            assert_eq!(allocator.stats(), before);
            expected_body = Some(encoded[45..].to_vec());
        }
        if matches!(
            case,
            Case::NestedReturn | Case::FrameOutput | Case::ReturnFrame
        ) || case.data()
            || case.list()
            || case.scalars()
            || case.envelope() != 0
        {
            invocation.prepare_output(&mut session).unwrap();
        } else if matches!(case, Case::NestedReturnBudget) {
            invocation.prepare_output(&mut limited_output).unwrap();
        }
        let outcome = if compiled {
            native.execute_component(
                &mut allocator,
                control,
                Default::default(),
                &arguments,
                &mut invocation,
            )
        } else {
            lang_vm::unit::tasks::execute_component(
                &program,
                m::FunctionId(0),
                &mut allocator,
                control,
                Default::default(),
                &arguments,
                &mut invocation,
            )
        };
        let committed = execution::Host::arguments_committed(&invocation);
        assert_eq!(
            invocation.active_loans(),
            0,
            "call returned a live resource loan"
        );
        drop(invocation);
        if let Err(failure) = outcome {
            if matches!(case, Case::NestedReturnBudget) {
                assert!(matches!(
                    failure.reason,
                    lang_runtime::task::runner::Reason::ComponentFrame(_)
                ));
            }
            if matches!(case, Case::NestedReturnMissing) {
                assert!(matches!(
                    failure.reason,
                    lang_runtime::task::runner::Reason::Unsupported
                ));
                assert_eq!(attempts.get(), 4);
            }
            if case.failing_cleanup() {
                assert!(failure.poisoned);
                assert!(failure.report.as_ref().unwrap().primary().is_some());
            } else {
                assert!(!failure.poisoned, "{failure:?}");
            }
            if matches!(
                case,
                Case::DestructorFailure | Case::ReturnCleanupFailure | Case::LoanCleanup
            ) {
                assert_eq!(
                    failure.report.as_ref().unwrap().primary().unwrap().kind,
                    lang_runtime::cleanup::FailureKind::Cleanup(19)
                );
            }
            if matches!(
                case,
                Case::Fuel(_)
                    | Case::FuelCleanup(_)
                    | Case::LoanFuel(..)
                    | Case::LoanFuelCleanup(_)
            ) {
                assert!(matches!(
                    failure.report.as_ref().unwrap().primary().unwrap().kind,
                    lang_runtime::cleanup::FailureKind::Execution(
                        lang_runtime::cleanup::ExecutionFailure::FuelLimit
                    )
                ));
            }
            if matches!(case, Case::Cancelled | Case::LoanCancelled) {
                assert!(matches!(
                    failure.reason,
                    lang_runtime::task::runner::Reason::Control(e::Failure::Interrupted)
                ));
            }
            if matches!(case, Case::ForeignControl) {
                assert!(matches!(
                    failure.reason,
                    lang_runtime::task::runner::Reason::Entry
                ));
            }
            if matches!(
                case,
                Case::Oom(_)
                    | Case::DataOom(_)
                    | Case::ListOom(_)
                    | Case::DeepListOom(_)
                    | Case::DataListOom(_)
                    | Case::LoanOom(..)
            ) {
                assert!(matches!(
                    failure.reason,
                    lang_runtime::task::runner::Reason::Layout(
                        lang_runtime::layout::Error::Allocation(a::Error::OutOfMemory)
                    )
                ));
            }
            if matches!(case, Case::LoanCells(_)) {
                assert!(matches!(
                    failure.reason,
                    lang_runtime::task::runner::Reason::Execution(
                        lang_mir::unit::ownership::failure::ExecutionFailure::CellLimit
                    )
                ));
            }
            if committed && case.loan().is_none() {
                assert!(values.is_empty());
                assert_eq!(destroyed.calls.get(), if extra { 2 } else { 1 });
            } else {
                assert_eq!(
                    destroyed.calls.get(),
                    usize::from(committed && extra && !case.loan_aliases())
                );
                if matches!(case, Case::Capacity) {
                    assert!(matches!(
                        failure.reason,
                        lang_runtime::task::runner::Reason::Component(storage::Error::Capacity)
                    ));
                    assert_eq!(attempts.get(), 4);
                }
                assert_eq!(
                    values.bytes(input, holder, kind, &allocator).unwrap(),
                    41_i64.to_le_bytes()
                );
                if committed && extra && !case.loan_aliases() {
                    assert!(
                        values
                            .validate(extra_input.unwrap(), holder, kind.key(), &allocator)
                            .is_err()
                    );
                }
                for claim in [
                    Some(input),
                    if !committed || case.loan_aliases() {
                        extra_input
                    } else {
                        None
                    },
                ]
                .into_iter()
                .flatten()
                {
                    let mut backing = values.detach(claim, holder, &allocator).unwrap();
                    let mut cleanup = lang_runtime::cleanup::Session::prepare(
                        std::num::NonZeroUsize::new(2).unwrap(),
                        &mut budget,
                    )
                    .unwrap();
                    let mut cleaner = lang_runtime::cleanup::Contextual::new(
                        |_: &mut (), _, context: &mut lang_runtime::cleanup::Context<'_>| {
                            execution::Invoker::destroy(
                                &mut bridge,
                                kind,
                                &mut backing,
                                &mut allocator,
                                context,
                            )
                        },
                    );
                    let retired = cleanup.retire(
                        lang_runtime::cleanup::Slot(0),
                        (),
                        None,
                        lang_runtime::cleanup::Reason::ScopeExit,
                        &mut cleaner,
                    );
                    backing.release(&mut allocator).unwrap();
                    if case.failing_cleanup() {
                        assert!(retired.is_err());
                        assert!(cleanup.is_poisoned());
                    } else {
                        retired.unwrap();
                    }
                }
            }
            if case.loan().is_some() {
                assert_eq!(destroyed.calls.get(), if extra { 2 } else { 1 });
                assert_eq!(destroyed.value.get(), 41);
                assert_eq!(engine.check(&destroyed.context.get().unwrap()), 5);
            }
            assert_eq!(allocator.stats().live_allocations, 0);
            let evidence = (false, committed, failure.usage);
            if let Some(previous) = previous {
                assert_eq!(evidence, previous);
            }
            previous = Some(evidence);
            continue;
        }
        let mut outcome = outcome.unwrap_or_else(|_| unreachable!());
        assert!(!case.failing_cleanup());
        assert!(committed);
        if matches!(case, Case::FrameOutput | Case::ReturnFrame) {
            let execution::Value::Frame(frame) = &outcome.value else {
                panic!("explicit frame result required");
            };
            assert_eq!(frame.bytes()[8], 1);
            if return_resource {
                assert_eq!(frame.resources().len(), 1);
                assert_eq!(&frame.bytes()[45..], &1_u32.to_le_bytes());
                assert_eq!(frame.values().roots.len(), 1);
                assert_eq!(
                    frame.values().nodes,
                    vec![wire::Value::Resource(
                        lang_runtime::component::adapter::BoundaryResourceIndex::new(1).unwrap()
                    )]
                );
                outcome.value = execution::Value::Resource(frame.resources()[0]);
            } else {
                assert!(frame.resources().is_empty());
                assert_eq!(&frame.bytes()[45..], &42_i64.to_le_bytes());
                assert_eq!(frame.values().nodes, vec![wire::Value::I64(42)]);
                outcome.value = execution::Value::Copy(lang_runtime::layout::Immediate::I64(42));
            }
        }
        if matches!(case, Case::NestedReturn)
            || case.data()
            || case.list()
            || case.scalars()
            || case.envelope() != 0
        {
            let execution::Value::Frame(frame) = &outcome.value else {
                panic!("rich result required");
            };
            assert_eq!(frame.resources().len(), 2);
            assert_eq!(frame.bytes()[8], 1);
            assert_eq!(&frame.bytes()[45..], expected_body.as_ref().unwrap());
            if case.list() {
                assert_eq!(
                    &frame.bytes()[45..49],
                    &(if case.list_depth() > 1 { 1_u32 } else { 2_u32 }).to_le_bytes()
                );
                let first = 45 + 4 * case.list_depth();
                let last = if case.data_list() { 70 } else { first + 4 };
                assert_eq!(&frame.bytes()[first..first + 4], &1_u32.to_le_bytes());
                assert_eq!(&frame.bytes()[last..last + 4], &2_u32.to_le_bytes());
            } else {
                let prefix = case.envelope();
                assert_eq!(
                    &frame.bytes()[45 + prefix..49 + prefix],
                    &1_u32.to_le_bytes()
                );
                assert_eq!(
                    &frame.bytes()[49 + prefix..57 + prefix],
                    &41_i64.to_le_bytes()
                );
                assert_eq!(
                    &frame.bytes()[57 + prefix..61 + prefix],
                    &2_u32.to_le_bytes()
                );
            }
            if case.data() {
                assert!(
                    frame
                        .values()
                        .nodes
                        .contains(&wire::Value::Text("寿司".into()))
                );
                assert!(
                    frame
                        .values()
                        .nodes
                        .contains(&wire::Value::Bytes(vec![0, 128, 255]))
                );
            }
            assert_eq!(destroyed.calls.get(), 0);
            assert_eq!(allocator.stats().live_allocations, 4);
            assert!(
                values
                    .validate(input, holder, kind.key(), &allocator)
                    .is_err()
            );
            assert!(
                values
                    .validate(extra_input.unwrap(), holder, kind.key(), &allocator)
                    .is_err()
            );
            for claim in frame.resources() {
                assert_eq!(
                    values.bytes(*claim, holder, kind, &allocator).unwrap(),
                    41_i64.to_le_bytes()
                );
                let mut backing = values.detach(*claim, holder, &allocator).unwrap();
                let mut cleanup = lang_runtime::cleanup::Session::prepare(
                    std::num::NonZeroUsize::new(2).unwrap(),
                    &mut budget,
                )
                .unwrap();
                let mut cleaner = lang_runtime::cleanup::Contextual::new(
                    |_: &mut (), _, context: &mut lang_runtime::cleanup::Context<'_>| {
                        execution::Invoker::destroy(
                            &mut bridge,
                            kind,
                            &mut backing,
                            &mut allocator,
                            context,
                        )
                    },
                );
                cleanup
                    .retire(
                        lang_runtime::cleanup::Slot(0),
                        (),
                        None,
                        lang_runtime::cleanup::Reason::ScopeExit,
                        &mut cleaner,
                    )
                    .unwrap();
                backing.release(&mut allocator).unwrap();
            }
        } else if return_resource {
            let execution::Value::Resource(result) = outcome.value else {
                panic!("resource result required")
            };
            let original = if case.loan().is_some() {
                extra_input.unwrap()
            } else {
                input
            };
            assert_ne!(original, result);
            assert_eq!(destroyed.calls.get(), 0);
            assert!(
                values
                    .validate(original, holder, kind.key(), &allocator)
                    .is_err()
            );
            assert_eq!(
                values.bytes(result, holder, kind, &allocator).unwrap(),
                41_i64.to_le_bytes()
            );
            assert_eq!(
                allocator.stats().live_allocations,
                if case.loan().is_some() { 4 } else { 2 }
            );
            let mut backing = values.detach(result, holder, &allocator).unwrap();
            let mut cleanup = lang_runtime::cleanup::Session::prepare(
                std::num::NonZeroUsize::new(2).unwrap(),
                &mut budget,
            )
            .unwrap();
            let mut cleaner = lang_runtime::cleanup::Contextual::new(
                |_: &mut (), _, context: &mut lang_runtime::cleanup::Context<'_>| {
                    execution::Invoker::destroy(
                        &mut bridge,
                        kind,
                        &mut backing,
                        &mut allocator,
                        context,
                    )
                },
            );
            cleanup
                .retire(
                    lang_runtime::cleanup::Slot(0),
                    (),
                    None,
                    lang_runtime::cleanup::Reason::ScopeExit,
                    &mut cleaner,
                )
                .unwrap();
            backing.release(&mut allocator).unwrap();
            assert!(!cleanup.is_poisoned());
        } else {
            assert_eq!(
                outcome.value,
                execution::Value::Copy(lang_runtime::layout::Immediate::I64(42))
            );
        }
        if case.loan().is_some() {
            assert_eq!(
                destroyed.calls.get(),
                usize::from(extra && !case.loan_aliases())
            );
            assert_eq!(values.len(), if case.loan_aliases() { 2 } else { 1 });
            assert_eq!(
                values.bytes(input, holder, kind, &allocator).unwrap(),
                41_i64.to_le_bytes()
            );
            // Detach also proves every call loan has expired before returning.
            release_caller(
                &mut values,
                input,
                kind,
                &mut allocator,
                &mut bridge,
                &mut budget,
            );
            if case.loan_aliases() {
                release_caller(
                    &mut values,
                    extra_input.unwrap(),
                    kind,
                    &mut allocator,
                    &mut bridge,
                    &mut budget,
                );
            }
        }
        assert!(values.is_empty());
        assert_eq!(destroyed.calls.get(), if extra { 2 } else { 1 });
        assert_eq!(destroyed.value.get(), 41);
        assert_eq!(engine.check(&destroyed.context.get().unwrap()), 5);
        assert_eq!(allocator.stats().live_allocations, 0);
        assert!(!outcome.poisoned);
        if let Case::LoanCells(cap) = case {
            assert!(outcome.usage.peak_live_cells <= cap);
        }
        let evidence = (true, committed, outcome.usage);
        if let Some(previous) = previous {
            assert_eq!(evidence, previous);
        }
        previous = Some(evidence);
    }
    let (success, committed, _) = previous.unwrap();
    (success, committed)
}
