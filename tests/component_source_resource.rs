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
    method_calls: Cell<usize>,
    method_changed: Cell<bool>,
    method_control_checked: Cell<bool>,
    method_context: Cell<Option<native::Context>>,
    native: Option<NativeCase>,
    batch: bool,
    list: bool,
    native_result: bool,
    fresh_result: bool,
    published: Cell<usize>,
    result_shape: Option<NativeShape>,
    interruption: Cell<*const e::InterruptionFlag>,
    value: Cell<i64>,
    allocator: Cell<u64>,
    context: Cell<Option<native::Context>>,
    fail: bool,
}
#[derive(Clone, Copy)]
enum NativeCase {
    Owned,
    OwnedReturn,
    Shared,
    Reborrow(bool),
    Abandoned,
    Declared,
    WrongKind,
    WrongKindOwned,
    MissingSession,
    Malformed,
    NoResult,
    ForeignResult,
    BadStatus,
    Interrupt,
    Oom(usize),
    Fuel(u64),
    HostCalls(u64),
    Cells(usize),
    ResultIndex(u32),
}
#[derive(Clone, Copy, Debug)]
enum NativeShape {
    List,
    Record,
}
#[derive(Clone, Copy)]
enum Case {
    Consume,
    Frame,
    FrameOutput,
    SharedLoan,
    ExclusiveLoan,
    NativeView,
    Native(NativeCase),
    NativeTransfer(NativeCase, bool),
    NativeBatch(NativeCase, bool),
    NativeList(NativeCase, bool),
    NativeResult(NativeCase),
    NativeFresh(NativeCase),
    NativeBorrowResult(NativeCase),
    NativeFreshUnused(NativeCase),
    NativeFreshFailure(NativeCase),
    NativeAggregate(NativeCase, NativeShape),
    NativeFreshLoan(NativeCase),
    NativeWrongResultKind,
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
    fn native(self) -> Option<NativeCase> {
        match self {
            Self::NativeWrongResultKind => Some(NativeCase::Owned),
            Self::NativeView => Some(NativeCase::Owned),
            Self::Native(native)
            | Self::NativeTransfer(native, _)
            | Self::NativeBatch(native, _)
            | Self::NativeList(native, _)
            | Self::NativeResult(native)
            | Self::NativeFresh(native)
            | Self::NativeFreshUnused(native)
            | Self::NativeFreshFailure(native)
            | Self::NativeAggregate(native, _)
            | Self::NativeFreshLoan(native)
            | Self::NativeBorrowResult(native) => Some(native),
            _ => None,
        }
    }
    fn native_owned(self) -> bool {
        matches!(
            self,
            Self::NativeWrongResultKind
                | Self::Native(
                    NativeCase::Owned | NativeCase::OwnedReturn | NativeCase::WrongKindOwned
                )
                | Self::NativeTransfer(..)
                | Self::NativeBatch(..)
                | Self::NativeList(..)
                | Self::NativeResult(_)
                | Self::NativeFresh(_)
                | Self::NativeFreshUnused(_)
                | Self::NativeFreshFailure(_)
                | Self::NativeAggregate(..)
                | Self::NativeBorrowResult(_)
        )
    }
    fn native_transfer(self) -> bool {
        matches!(
            self,
            Self::NativeWrongResultKind
                | Self::NativeTransfer(..)
                | Self::NativeBatch(_, false)
                | Self::NativeList(..)
                | Self::NativeResult(_)
                | Self::NativeFresh(_)
                | Self::NativeFreshUnused(_)
                | Self::NativeFreshFailure(_)
                | Self::NativeAggregate(..)
        )
    }
    fn native_batch(self) -> bool {
        matches!(
            self,
            Self::NativeBatch(..) | Self::NativeList(..) | Self::NativeAggregate(..)
        )
    }
    fn native_declared(self) -> bool {
        matches!(self.native(), Some(NativeCase::Declared))
    }
    fn native_result(self) -> bool {
        matches!(
            self,
            Self::NativeWrongResultKind
                | Self::NativeResult(_)
                | Self::NativeFresh(_)
                | Self::NativeFreshFailure(_)
                | Self::NativeBorrowResult(_)
                | Self::NativeFreshLoan(_)
        )
    }
    fn fresh_result(self) -> bool {
        matches!(
            self,
            Self::NativeFresh(_)
                | Self::NativeFreshUnused(_)
                | Self::NativeFreshFailure(_)
                | Self::NativeFreshLoan(_)
        )
    }
    fn native_shape(self) -> Option<NativeShape> {
        if let Self::NativeAggregate(_, shape) = self {
            Some(shape)
        } else {
            None
        }
    }
    fn native_poison(self) -> bool {
        matches!(
            self,
            Self::NativeBorrowResult(_) | Self::NativeWrongResultKind
        ) || matches!(
            self.native(),
            Some(
                NativeCase::Malformed
                    | NativeCase::NoResult
                    | NativeCase::ForeignResult
                    | NativeCase::BadStatus
                    | NativeCase::ResultIndex(_)
            )
        )
    }
    fn loan(self) -> Option<bool> {
        match self {
            Self::NativeFreshLoan(_) => Some(true),
            Self::SharedLoan
            | Self::LoanAliases
            | Self::LoanCapacity
            | Self::LoanCells(_)
            | Self::LoanCancelled
            | Self::LoanCleanup
            | Self::LoanFuelCleanup(_) => Some(false),
            Self::Native(native) => {
                if matches!(
                    native,
                    NativeCase::Owned | NativeCase::OwnedReturn | NativeCase::WrongKindOwned
                ) {
                    None
                } else {
                    Some(!matches!(
                        native,
                        NativeCase::Shared | NativeCase::Reborrow(false)
                    ))
                }
            }
            Self::ExclusiveLoan | Self::NativeView => Some(true),
            Self::LoanConflict(mask) => Some(mask & 1 != 0),
            Self::MixedLoan(exclusive)
            | Self::LoanReturn(exclusive)
            | Self::LoanOom(exclusive, _)
            | Self::LoanFuel(exclusive, _) => Some(exclusive),
            _ => None,
        }
    }
    fn loan_extra(self) -> bool {
        self.loan().is_some()
            && self.native().is_none()
            && !matches!(self, Self::SharedLoan | Self::ExclusiveLoan)
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
            Self::NativeWrongResultKind
                | Self::Return
                | Self::ReturnFrame
                | Self::ReturnCleanupFailure
                | Self::LoanReturn(_)
                | Self::Native(NativeCase::OwnedReturn)
                | Self::NativeResult(_)
                | Self::NativeFresh(_)
                | Self::NativeFreshFailure(_)
                | Self::NativeBorrowResult(_)
                | Self::NativeFreshLoan(_)
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
                | Self::NativeTransfer(_, true)
                | Self::NativeList(_, true)
                | Self::NativeFreshFailure(_)
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
unsafe extern "C" fn native_apply(
    user: *mut c_void,
    context: *const native::Context,
    arguments: native::Bytes,
    output: *mut *mut native::Buffer,
) -> u32 {
    // SAFETY: registration retains the actual user/code owner. The engine
    // supplies live canonical bytes and a unique original callback scope.
    let state = unsafe { &*user.cast::<Destroyed>() };
    let result = (|| -> Result<(), u32> {
        if unsafe { lang_codegen_clif::component::context_check(context) } != 0
            || unsafe { lang_codegen_clif::component::context_allocator_identity(context) }
                != state.allocator.get()
        {
            return Err(2);
        }
        state.method_calls.set(state.method_calls.get() + 1);
        let previous = state.method_context.replace(Some(unsafe { *context }));
        if matches!(state.native, Some(NativeCase::Reborrow(_)))
            && let Some(previous) = previous
        {
            let mut stale = native::Bytes {
                data: ptr::null(),
                length: 99,
            };
            if unsafe {
                lang_codegen_clif::component::context_resource_read(&previous, 1, &mut stale)
            } != 5
                || !stale.data.is_null()
                || stale.length != 99
            {
                return Err(2);
            }
        }
        if arguments.length < 61
            || arguments.length > 1024
            || arguments.data.is_null()
            || output.is_null()
        {
            return Err(2);
        }
        let arguments =
            unsafe { std::slice::from_raw_parts(arguments.data, arguments.length as usize) };
        let label_len = u32::from_le_bytes(arguments[57..61].try_into().unwrap()) as usize;
        if arguments.len()
            != 61 + label_len + if state.batch { 4 } else { 0 } + if state.list { 4 } else { 0 }
        {
            return Err(2);
        }
        let declared = matches!(state.native, Some(NativeCase::Declared));
        if declared && &arguments[61..] != "寿司".as_bytes() {
            return Err(2);
        }
        let payload = match state.result_shape {
            Some(NativeShape::List) => 12usize,
            Some(NativeShape::Record) => 29,
            None if state.native_result => 4,
            None if declared => 11,
            None => 9,
        };
        let index = u32::from_le_bytes(arguments[45..49].try_into().unwrap());
        let delta = i64::from_le_bytes(arguments[49..57].try_into().unwrap());
        let mut buffer = ptr::null_mut();
        let status = unsafe {
            lang_codegen_clif::component::context_buffer_create(
                context,
                (45 + payload) as u64,
                1,
                &mut buffer,
            )
        };
        if status != 0 {
            return Err(status);
        }
        let mut encoded = native::MutBytes {
            data: ptr::null_mut(),
            length: 0,
        };
        let status = unsafe {
            lang_codegen_clif::component::buffer_mut_bytes(context, buffer, &mut encoded)
        };
        if status != 0 {
            return Err(status);
        }
        if encoded.length != (45 + payload) as u64 || encoded.data.is_null() {
            return Err(2);
        }
        let mut invalid = native::Bytes {
            data: ptr::null(),
            length: 99,
        };
        if unsafe { lang_codegen_clif::component::context_resource_read(context, 0, &mut invalid) }
            != 7
            || !invalid.data.is_null()
            || invalid.length != 99
        {
            return Err(2);
        }
        let mut original = native::Bytes {
            data: ptr::null(),
            length: 0,
        };
        let status = unsafe {
            lang_codegen_clif::component::context_resource_read(context, index, &mut original)
        };
        if status != 0 {
            return Err(status);
        }
        if original.length != 8 || original.data.is_null() {
            return Err(2);
        }
        let before = i64::from_le_bytes(
            unsafe { std::slice::from_raw_parts(original.data, 8) }
                .try_into()
                .unwrap(),
        );
        let mut writable = native::MutBytes {
            data: ptr::null_mut(),
            length: 0,
        };
        let status = unsafe {
            lang_codegen_clif::component::context_resource_write(context, index, &mut writable)
        };
        let after = before + delta;
        if matches!(
            state.native,
            Some(NativeCase::Shared | NativeCase::Reborrow(false))
        ) {
            if status != 7 || !writable.data.is_null() || writable.length != 0 {
                return Err(2);
            }
        } else {
            if status != 0 {
                return Err(status);
            }
            if writable.length != 8 || writable.data.is_null() {
                return Err(2);
            }
            unsafe { std::slice::from_raw_parts_mut(writable.data, 8) }
                .copy_from_slice(&after.to_le_bytes());
            state.method_changed.set(true);
        }
        if state.batch {
            if state.list && arguments[61 + label_len..65 + label_len] != 1_u32.to_le_bytes() {
                return Err(2);
            }
            let extra = u32::from_le_bytes(arguments[arguments.len() - 4..].try_into().unwrap());
            if extra != 2 || index != 1 {
                return Err(2);
            }
            let status = unsafe {
                lang_codegen_clif::component::context_resource_write(context, extra, &mut writable)
            };
            if status != 0 || writable.length != 8 || writable.data.is_null() {
                return Err(2);
            }
            unsafe { std::slice::from_raw_parts_mut(writable.data, 8) }
                .copy_from_slice(&after.to_le_bytes());
        }
        let mut result_index = index;
        if state.fresh_result {
            let mut backing = ptr::null_mut();
            let status = unsafe {
                lang_codegen_clif::component::context_buffer_create(context, 8, 8, &mut backing)
            };
            if status != 0 {
                return Err(status);
            }
            let mut bytes = native::MutBytes {
                data: ptr::null_mut(),
                length: 0,
            };
            let status = unsafe {
                lang_codegen_clif::component::buffer_mut_bytes(context, backing, &mut bytes)
            };
            if status != 0 || bytes.length != 8 || bytes.data.is_null() {
                return Err(2);
            }
            unsafe { std::slice::from_raw_parts_mut(bytes.data, 8) }
                .copy_from_slice(&after.to_le_bytes());
            let mut untouched = 777_u32;
            let wrong = b"demo:counter/api.unknown";
            if unsafe {
                lang_codegen_clif::component::context_resource_publish(
                    context,
                    native::Bytes {
                        data: wrong.as_ptr(),
                        length: wrong.len() as u64,
                    },
                    backing,
                    &mut untouched,
                )
            } != 7
                || untouched != 777
            {
                return Err(2);
            }
            let key = b"demo:counter/api.counter";
            let status = unsafe {
                lang_codegen_clif::component::context_resource_publish(
                    context,
                    native::Bytes {
                        data: key.as_ptr(),
                        length: key.len() as u64,
                    },
                    backing,
                    &mut result_index,
                )
            };
            if status != 0 {
                return Err(status);
            }
            state.published.set(state.published.get() + 1);
            if result_index != 2
                || unsafe { lang_codegen_clif::component::context_buffer_drop(context, backing) }
                    != 5
            {
                return Err(2);
            }
        }
        if let Some(NativeCase::ResultIndex(index)) = state.native {
            result_index = index;
        }
        let encoded = unsafe { std::slice::from_raw_parts_mut(encoded.data, 45 + payload) };
        encoded[..45].copy_from_slice(&arguments[..45]);
        encoded[8] = 1;
        encoded[41..45].copy_from_slice(&(payload as u32).to_le_bytes());
        if let Some(shape) = state.result_shape {
            let extra = match state.native {
                Some(NativeCase::ResultIndex(index)) => index,
                _ => 2,
            };
            match shape {
                NativeShape::List => {
                    encoded[45..49].copy_from_slice(&2_u32.to_le_bytes());
                    encoded[49..53].copy_from_slice(&index.to_le_bytes());
                    encoded[53..57].copy_from_slice(&extra.to_le_bytes());
                }
                NativeShape::Record => {
                    encoded[45..49].copy_from_slice(&index.to_le_bytes());
                    encoded[49..53].copy_from_slice(&6_u32.to_le_bytes());
                    encoded[53..59].copy_from_slice("寿司".as_bytes());
                    encoded[59..63].copy_from_slice(&3_u32.to_le_bytes());
                    encoded[63..66].copy_from_slice(&[0, 128, 255]);
                    encoded[66..70].copy_from_slice(&1_u32.to_le_bytes());
                    encoded[70..74].copy_from_slice(&extra.to_le_bytes());
                }
            }
        } else if state.native_result {
            encoded[45..49].copy_from_slice(&result_index.to_le_bytes());
        } else if declared {
            encoded[45] = 1;
            encoded[46..50].copy_from_slice(&6_u32.to_le_bytes());
            encoded[50..56].copy_from_slice("寿司".as_bytes());
        } else {
            encoded[45] = 0;
            encoded[46..54].copy_from_slice(&after.to_le_bytes());
        }
        if matches!(state.native, Some(NativeCase::Malformed)) {
            encoded[9] ^= 1;
        }
        unsafe {
            output.write(if matches!(state.native, Some(NativeCase::NoResult)) {
                ptr::null_mut()
            } else if matches!(state.native, Some(NativeCase::ForeignResult)) {
                ptr::without_provenance_mut(1)
            } else {
                buffer
            })
        };
        if matches!(state.native, Some(NativeCase::Interrupt)) {
            // SAFETY: the real invocation flag outlives this synchronous callback.
            unsafe { &*state.interruption.get() }.interrupt();
            let mut refused = native::Bytes {
                data: ptr::null(),
                length: 99,
            };
            let read = unsafe {
                lang_codegen_clif::component::context_resource_read(context, index, &mut refused)
            };
            let mut untouched = ptr::without_provenance_mut(1);
            let create = unsafe {
                lang_codegen_clif::component::context_buffer_create(context, 1, 1, &mut untouched)
            };
            let drop =
                unsafe { lang_codegen_clif::component::context_buffer_drop(context, buffer) };
            let again =
                unsafe { lang_codegen_clif::component::context_buffer_drop(context, buffer) };
            state.method_control_checked.set(
                read == 1
                    && refused.data.is_null()
                    && refused.length == 99
                    && create == 1
                    && untouched == ptr::without_provenance_mut(1)
                    && drop == 0
                    && again == 5,
            );
        }
        Ok(())
    })();
    if matches!(state.native, Some(NativeCase::BadStatus)) {
        2
    } else {
        result.err().map(|_| 1).unwrap_or(0)
    }
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
    // Mandatory cleanup has a live report context even when the original
    // invocation is interrupted. Its scratch never borrows guest allocation.
    if unsafe { lang_codegen_clif::component::context_check(context) } != 0 {
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
    let mut buffer = std::ptr::null_mut();
    let mut view = native::MutBytes {
        data: std::ptr::null_mut(),
        length: 0,
    };
    // SAFETY: this callback retains the exact live cleanup context and unique
    // initialized output slots. The view expires before its buffer is dropped.
    unsafe {
        if lang_codegen_clif::component::context_buffer_create(context, 64, 64, &mut buffer) != 0
            || lang_codegen_clif::component::buffer_mut_bytes(context, buffer, &mut view) != 0
            || view.length != 64
            || !(view.data as usize).is_multiple_of(64)
        {
            return 1;
        }
        view.data.write_bytes(0xa5, 64);
        if lang_codegen_clif::component::context_buffer_drop(context, buffer) != 0 {
            return 1;
        }
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
fn source_native_member_operates_on_the_original_borrowed_backing() {
    assert_eq!(run_source(Case::NativeView), (true, true));
    assert_eq!(run_source(Case::Native(NativeCase::Owned)), (true, true));
    assert_eq!(run_source(Case::Native(NativeCase::Shared)), (true, true));
    assert_eq!(run_source(Case::Native(NativeCase::Declared)), (true, true));
}

#[test]
fn source_native_member_accepts_and_retires_the_actual_owner() {
    assert_eq!(
        run_source(Case::NativeTransfer(NativeCase::Owned, false)),
        (true, true)
    );
}

#[test]
fn native_owned_result_returns_the_original_mutated_owner_on_both_engines() {
    assert_eq!(
        run_source(Case::NativeResult(NativeCase::Owned)),
        (true, true)
    );
}

#[test]
fn native_buffer_publication_returns_a_fresh_owner_and_preserves_refused_buffers() {
    assert_eq!(
        run_source(Case::NativeFresh(NativeCase::Owned)),
        (true, true)
    );
}

#[test]
fn native_result_refuses_borrowed_and_out_of_range_resource_authority() {
    assert_eq!(run_source(Case::NativeWrongResultKind), (false, true));
    assert_eq!(
        run_source(Case::NativeBorrowResult(NativeCase::Owned)),
        (false, true)
    );
    for index in [0, 2, u32::MAX] {
        assert_eq!(
            run_source(Case::NativeResult(NativeCase::ResultIndex(index))),
            (false, true)
        );
    }
}

#[test]
fn native_unreturned_and_unpublished_owners_retire_before_success_or_failure() {
    assert_eq!(
        run_source(Case::NativeFreshUnused(NativeCase::Owned)),
        (true, true)
    );
    for mode in [
        NativeCase::Owned,
        NativeCase::Malformed,
        NativeCase::Interrupt,
    ] {
        assert_eq!(run_source(Case::NativeFreshFailure(mode)), (false, true));
    }
}

#[test]
fn native_owned_results_expire_and_retire_after_malformed_or_interrupted_returns() {
    for mode in [
        NativeCase::Malformed,
        NativeCase::NoResult,
        NativeCase::ForeignResult,
        NativeCase::BadStatus,
        NativeCase::Interrupt,
    ] {
        assert_eq!(run_source(Case::NativeResult(mode)), (false, true));
        assert_eq!(run_source(Case::NativeFresh(mode)), (false, true));
    }
}

#[test]
fn native_owned_result_provider_and_fuel_prefixes_keep_original_custody_or_retire_it() {
    for fresh in [false, true] {
        let cases: Vec<_> = (0..112)
            .map(|offset| {
                let mode = NativeCase::Oom(offset);
                run_source(if fresh {
                    Case::NativeFresh(mode)
                } else {
                    Case::NativeResult(mode)
                })
            })
            .collect();
        assert!(cases.contains(&(false, false)));
        assert!(cases.contains(&(false, true)));
        assert!(cases.contains(&(true, true)));
        for fuel in 0..160 {
            let mode = NativeCase::Fuel(fuel);
            run_source(if fresh {
                Case::NativeFresh(mode)
            } else {
                Case::NativeResult(mode)
            });
        }
    }
}

#[test]
fn native_owned_lists_and_records_stage_all_storage_before_owner_publication() {
    for shape in [NativeShape::List, NativeShape::Record] {
        assert_eq!(
            run_source(Case::NativeAggregate(NativeCase::Owned, shape)),
            (true, true)
        );
        for mode in [
            NativeCase::ResultIndex(0),
            NativeCase::ResultIndex(1),
            NativeCase::ResultIndex(u32::MAX),
            NativeCase::Malformed,
            NativeCase::Interrupt,
        ] {
            assert_eq!(
                run_source(Case::NativeAggregate(mode, shape)),
                (false, true)
            );
        }
        let results: Vec<_> = (0..128)
            .map(|offset| run_source(Case::NativeAggregate(NativeCase::Oom(offset), shape)))
            .collect();
        assert!(results.contains(&(false, false)));
        assert!(results.contains(&(false, true)));
        assert!(results.contains(&(true, true)));
    }
}

#[test]
fn native_fresh_results_preserve_the_original_caller_and_expire_root_loans() {
    assert_eq!(
        run_source(Case::NativeFreshLoan(NativeCase::Owned)),
        (true, true)
    );
    for mode in [NativeCase::Malformed, NativeCase::Interrupt] {
        assert_eq!(run_source(Case::NativeFreshLoan(mode)), (false, true));
    }
    let results: Vec<_> = (0..112)
        .map(|offset| run_source(Case::NativeFreshLoan(NativeCase::Oom(offset))))
        .collect();
    assert!(results.contains(&(false, false)));
    assert!(results.contains(&(false, true)));
    assert!(results.contains(&(true, true)));
}

#[test]
fn owning_native_arguments_retire_after_declared_and_malformed_results() {
    assert_eq!(
        run_source(Case::NativeTransfer(NativeCase::Declared, false)),
        (true, true)
    );
    for mode in [
        NativeCase::Malformed,
        NativeCase::NoResult,
        NativeCase::ForeignResult,
        NativeCase::BadStatus,
        NativeCase::WrongKindOwned,
    ] {
        assert_eq!(run_source(Case::NativeTransfer(mode, false)), (false, true));
    }
}

#[test]
fn owning_native_batch_rebases_direct_and_nested_resources_and_retires_both() {
    for borrowed in [false, true] {
        assert_eq!(
            run_source(Case::NativeBatch(NativeCase::Owned, borrowed)),
            (true, true)
        );
        assert_eq!(
            run_source(Case::NativeBatch(NativeCase::Malformed, borrowed)),
            (false, true)
        );
        let results: Vec<_> = (0..96)
            .map(|offset| run_source(Case::NativeBatch(NativeCase::Oom(offset), borrowed)))
            .collect();
        assert!(results.contains(&(false, false)));
        assert!(results.contains(&(false, true)));
        assert!(results.contains(&(true, true)));
    }
}

#[test]
fn owning_native_list_arguments_drain_consumed_tokens_and_actual_vector_storage() {
    assert_eq!(
        run_source(Case::NativeList(NativeCase::Owned, false)),
        (true, true)
    );
    for mode in [NativeCase::Malformed, NativeCase::Interrupt] {
        assert_eq!(run_source(Case::NativeList(mode, false)), (false, true));
    }
    assert_eq!(
        run_source(Case::NativeList(NativeCase::Malformed, true)),
        (false, true)
    );
    let results: Vec<_> = (0..112)
        .map(|offset| run_source(Case::NativeList(NativeCase::Oom(offset), false)))
        .collect();
    assert!(results.contains(&(false, false)));
    assert!(results.contains(&(false, true)));
    assert!(results.contains(&(true, true)));
}

#[test]
fn owning_native_arguments_retire_after_interruption_and_destructor_failure() {
    assert_eq!(
        run_source(Case::NativeTransfer(NativeCase::Interrupt, false)),
        (false, true)
    );
    assert_eq!(
        run_source(Case::NativeTransfer(NativeCase::Owned, true)),
        (false, true)
    );
    assert_eq!(
        run_source(Case::NativeTransfer(NativeCase::Malformed, true)),
        (false, true)
    );
}

#[test]
fn owning_native_arguments_preserve_or_retire_custody_at_every_provider_prefix() {
    let results: Vec<_> = (0..96)
        .map(|offset| run_source(Case::NativeTransfer(NativeCase::Oom(offset), false)))
        .collect();
    assert!(results.contains(&(false, false)));
    assert!(results.contains(&(false, true)));
    assert!(results.contains(&(true, true)));
    for fuel in 0..160 {
        run_source(Case::NativeTransfer(NativeCase::Fuel(fuel), false));
    }
}

#[test]
fn source_helper_reborrows_reach_the_original_native_backing() {
    for exclusive in [false, true] {
        assert_eq!(
            run_source(Case::Native(NativeCase::Reborrow(exclusive))),
            (true, true)
        );
    }
}

#[test]
fn abandoned_committed_loan_expires_but_keeps_the_instance_poisoned() {
    assert_eq!(
        run_source(Case::Native(NativeCase::Abandoned)),
        (false, true)
    );
}

#[test]
fn native_view_expires_before_the_mutated_original_owner_returns_to_the_caller() {
    assert_eq!(
        run_source(Case::Native(NativeCase::OwnedReturn)),
        (true, true)
    );
}

#[test]
fn native_member_kind_and_missing_session_refusals_preserve_original_provenance() {
    assert_eq!(
        run_source(Case::Native(NativeCase::WrongKind)),
        (false, true)
    );
    assert_eq!(
        run_source(Case::Native(NativeCase::WrongKindOwned)),
        (false, true)
    );
    assert_eq!(
        run_source(Case::Native(NativeCase::MissingSession)),
        (false, false)
    );
}
#[test]
fn malformed_native_results_poison_after_unmetered_buffer_and_loan_cleanup() {
    for mode in [
        NativeCase::Malformed,
        NativeCase::NoResult,
        NativeCase::ForeignResult,
        NativeCase::BadStatus,
    ] {
        assert_eq!(run_source(Case::Native(mode)), (false, true));
    }
}
#[test]
fn native_member_provider_refusals_preserve_or_retire_every_accepted_owner() {
    let results: Vec<_> = (0..64)
        .map(|offset| run_source(Case::Native(NativeCase::Oom(offset))))
        .collect();
    assert!(results.contains(&(false, false)));
    assert!(results.contains(&(false, true)));
    assert!(results.contains(&(true, true)));
}
#[test]
fn native_member_control_prefixes_and_callback_interruption_do_not_publish_success() {
    let fuel: Vec<_> = (0..64)
        .map(|fuel| run_source(Case::Native(NativeCase::Fuel(fuel))))
        .collect();
    assert!(fuel.contains(&(false, true)));
    assert!(fuel.contains(&(true, true)));
    let cells: Vec<_> = (0..64)
        .map(|cap| run_source(Case::Native(NativeCase::Cells(cap))))
        .collect();
    assert!(cells.contains(&(false, false)));
    assert!(cells.contains(&(true, true)));
    assert_eq!(
        run_source(Case::Native(NativeCase::HostCalls(0))),
        (false, true)
    );
    assert_eq!(
        run_source(Case::Native(NativeCase::HostCalls(1))),
        (true, true)
    );
    assert_eq!(
        run_source(Case::Native(NativeCase::Interrupt)),
        (false, true)
    );
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
            execution::Invoker::destroy(invoker, kind.clone(), &mut backing, allocator, context)
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
    run_source_mode(case, false)
}
fn run_source_mode(case: Case, stored: bool) -> (bool, bool) {
    let (success, committed, _) = run_source_mode_work(case, stored, None);
    (success, committed)
}
fn run_source_mode_work(
    case: Case,
    stored: bool,
    work_limit: Option<u64>,
) -> (bool, bool, e::Usage) {
    run_source_configuration(case, stored, work_limit, 0, false)
}
fn run_source_configuration(
    case: Case,
    stored: bool,
    work_limit: Option<u64>,
    native_flags: u32,
    force_cleanup_failure: bool,
) -> (bool, bool, e::Usage) {
    run_source_registration(
        case,
        stored,
        work_limit,
        native_flags,
        force_cleanup_failure,
        false,
    )
}
fn run_source_registration(
    case: Case,
    stored: bool,
    work_limit: Option<u64>,
    native_flags: u32,
    force_cleanup_failure: bool,
    owning_registration: bool,
) -> (bool, bool, e::Usage) {
    run_source_namespace(
        case,
        stored,
        work_limit,
        native_flags,
        force_cleanup_failure,
        owning_registration,
        None,
    )
}
fn run_source_namespace(
    case: Case,
    stored: bool,
    work_limit: Option<u64>,
    native_flags: u32,
    force_cleanup_failure: bool,
    owning_registration: bool,
    namespace: Option<execution::ResourceNamespace>,
) -> (bool, bool, e::Usage) {
    let cleanup_failure = force_cleanup_failure || case.failing_cleanup();
    let return_resource = case.returns_resource();
    let extra = case.loan_extra()
        || case.native_batch()
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
    let mut host_json: serde_json::Value =
        serde_json::from_slice(include_bytes!("fixtures/component_counter.idl.json")).unwrap();
    if case.native_transfer() {
        host_json["interfaces"][0]["functions"][0]["params"][0]["type"] = json!({"own":"counter"});
    }
    if case.native_result() {
        host_json["interfaces"][0]["functions"][0]["result"] = json!({"own":"counter"});
    }
    if matches!(case, Case::NativeWrongResultKind) {
        host_json["interfaces"][0]["resources"]
            .as_array_mut()
            .unwrap()
            .push(json!("other"));
        host_json["interfaces"][0]["functions"][0]["result"] = json!({"own":"other"});
    }
    if case.native_batch() {
        host_json["interfaces"][0]["types"][0]["record"]
            .as_array_mut()
            .unwrap()
            .push(json!({"name":"extra","type":{"own":"counter"}}));
        if matches!(case, Case::NativeList(..)) {
            host_json["interfaces"][0]["types"][0]["record"][2]["type"] =
                json!({"list":{"own":"counter"}});
        }
    }
    if let Some(shape) = case.native_shape() {
        host_json["interfaces"][0]["functions"][0]["result"] = match shape {
            NativeShape::List => json!({"list":{"own":"counter"}}),
            NativeShape::Record => {
                host_json["interfaces"][0]["types"].as_array_mut().unwrap().push(json!({"name":"bundle","record":[
                    {"name":"value","type":{"own":"counter"}}, {"name":"label","type":"text"},
                    {"name":"payload","type":"bytes"}, {"name":"items","type":{"list":{"own":"counter"}}}
                ]}));
                json!("bundle")
            }
        };
    }
    if matches!(
        case,
        Case::Native(NativeCase::Shared | NativeCase::Reborrow(false))
    ) {
        host_json["interfaces"][0]["functions"][0]["params"][0]["type"] =
            json!({"borrow":"counter"});
    }
    let consumer = json!({"idl":1,"package":"demo:consumer","version":"1.0.0","interfaces":[host_json["interfaces"][0].clone(),
        {"key":"demo:consumer/api","resources":[],"types":[],"functions":[{"name":"consume","effect":"sync","params":[{"name":"value","type":{"own":"demo:counter/api.counter"}},{"name":"number","type":"i64"}],"result":if return_resource{json!({"own":"demo:counter/api.counter"})}else{json!("i64")},"capability":"demo.consumer.consume"}]}
    ],"imports":[{"interface":"demo:counter/api","version":{"exact":"1.0.0"},"members":["apply"]}],"exports":["demo:consumer/api"]});
    let mut consumer = consumer;
    if matches!(case, Case::NativeWrongResultKind) {
        consumer["interfaces"][1]["functions"][0]["result"] =
            json!({"own":"demo:counter/api.other"});
    }
    if let Some(shape) = case.native_shape() {
        consumer["interfaces"][1]["functions"][0]["result"] = match shape {
            NativeShape::List => json!({"list":{"own":"demo:counter/api.counter"}}),
            NativeShape::Record => json!("demo:counter/api.bundle"),
        };
    }
    if case.native_declared() {
        consumer["interfaces"][1]["functions"][0]["result"] =
            json!({"result":{"ok":"i64","err":"text"}});
        consumer["interfaces"][1]["functions"][0]["params"]
            .as_array_mut()
            .unwrap()
            .push(json!({"name":"label","type":"text"}));
    }
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
    let source = if case.loan().is_some() || case.native().is_some() {
        let ty = if case.native_owned() {
            "core::ComponentResource<CounterKind>"
        } else if case.loan() == Some(true) {
            "&mut core::ComponentResource<CounterKind>"
        } else {
            "&core::ComponentResource<CounterKind>"
        };
        let prefix = source.split("pub fn consume").next().unwrap();
        let extra = if case.native_declared() {
            ",label:core::Text".into()
        } else if case.loan_aliases() {
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
        let (result, body) = if case.native_declared() {
            (
                "core::Result<i64,core::Text>",
                "let mut request:Request=Request{delta:1,label:label};return host_apply(value,request);",
            )
        } else if matches!(case, Case::Native(NativeCase::OwnedReturn)) {
            (
                "core::ComponentResource<CounterKind>",
                "let mut state:core::ComponentResource<CounterKind>=value;let mut raw:core::Bytes=core::result_unwrap::<core::Bytes,core::AllocError>(core::bytes_new(core::allocator()));let mut label:core::Text=core::result_unwrap::<core::Text,core::Utf8Error>(core::text_from_utf8(raw));let mut request:Request=Request{delta:1,label:label};let mut ignored:i64=core::result_unwrap::<i64,core::Text>(host_apply(state,request));return state;",
            )
        } else if let Some(shape) = case.native_shape() {
            (
                match shape {
                    NativeShape::List => "core::Vec<core::ComponentResource<CounterKind>>",
                    NativeShape::Record => "Bundle",
                },
                "let mut state:core::ComponentResource<CounterKind>=value;let mut raw:core::Bytes=core::result_unwrap::<core::Bytes,core::AllocError>(core::bytes_new(core::allocator()));let mut label:core::Text=core::result_unwrap::<core::Text,core::Utf8Error>(core::text_from_utf8(raw));let mut request:Request=Request{delta:1,label:label};return host_apply(state,request);",
            )
        } else if matches!(case, Case::NativeFreshLoan(_)) {
            (
                "core::ComponentResource<CounterKind>",
                "let mut raw:core::Bytes=core::result_unwrap::<core::Bytes,core::AllocError>(core::bytes_new(core::allocator()));let mut label:core::Text=core::result_unwrap::<core::Text,core::Utf8Error>(core::text_from_utf8(raw));let mut request:Request=Request{delta:1,label:label};return host_apply(value,request);",
            )
        } else if case.native_result() {
            (
                "core::ComponentResource<CounterKind>",
                "let mut state:core::ComponentResource<CounterKind>=value;let mut raw:core::Bytes=core::result_unwrap::<core::Bytes,core::AllocError>(core::bytes_new(core::allocator()));let mut label:core::Text=core::result_unwrap::<core::Text,core::Utf8Error>(core::text_from_utf8(raw));let mut request:Request=Request{delta:1,label:label};return host_apply(state,request);",
            )
        } else if case.native_owned() {
            (
                "i64",
                "let mut state:core::ComponentResource<CounterKind>=value;let mut raw:core::Bytes=core::result_unwrap::<core::Bytes,core::AllocError>(core::bytes_new(core::allocator()));let mut label:core::Text=core::result_unwrap::<core::Text,core::Utf8Error>(core::text_from_utf8(raw));let mut request:Request=Request{delta:1,label:label};return core::result_unwrap::<i64,core::Text>(host_apply(state,request));",
            )
        } else if case.native().is_some() {
            (
                "i64",
                "let mut raw:core::Bytes=core::result_unwrap::<core::Bytes,core::AllocError>(core::bytes_new(core::allocator()));let mut label:core::Text=core::result_unwrap::<core::Text,core::Utf8Error>(core::text_from_utf8(raw));let mut request:Request=Request{delta:1,label:label};return core::result_unwrap::<i64,core::Text>(host_apply(value,request));",
            )
        } else if return_resource {
            (
                "core::ComponentResource<CounterKind>",
                "let mut n:i64=add(value,number);return extra;",
            )
        } else {
            ("i64", "return add(value,number);")
        };
        let source = format!(
            "{prefix}pub fn consume(value:{ty},number:i64{extra})->{result}{{{body}}}fn add(value:{ty},number:i64)->i64{{return leaf(value,number);}}fn leaf(value:{ty},number:i64)->i64{{return number+1;}}"
        );
        let source = if matches!(case, Case::Native(NativeCase::Reborrow(_))) {
            let source = source.replace(
                "return core::result_unwrap::<i64,core::Text>(host_apply(value,request));",
                "return native_middle(value,request);",
            );
            format!(
                "{source}fn native_middle(value:{ty},request:Request)->i64{{return native_leaf(value,request);}}fn native_leaf(value:{ty},request:Request)->i64{{return core::result_unwrap::<i64,core::Text>(host_apply(value,request));}}"
            )
        } else {
            source
        };
        if matches!(
            case,
            Case::Native(NativeCase::Shared | NativeCase::Reborrow(false))
        ) {
            source.replace("host_apply(state:&mut", "host_apply(state:&")
        } else {
            source
        }
    } else {
        source.to_owned()
    };
    let source = if case.native_transfer() {
        source.replace("host_apply(state:&mut", "host_apply(state:")
    } else {
        source
    };
    let source = if case.native_result() {
        source.replace(
            "->core::Result<i64,core::Text>;",
            "->core::ComponentResource<CounterKind>;",
        )
    } else {
        source
    };
    let source = if let Some(shape) = case.native_shape() {
        let ty = match shape {
            NativeShape::List => "core::Vec<core::ComponentResource<CounterKind>>",
            NativeShape::Record => "Bundle",
        };
        let source = source.replace("->core::Result<i64,core::Text>;", &format!("->{ty};"));
        if matches!(shape, NativeShape::Record) {
            format!(
                "{source}pub struct Bundle{{value:core::ComponentResource<CounterKind>,label:core::Text,payload:core::Bytes,items:core::Vec<core::ComponentResource<CounterKind>>}}"
            )
        } else {
            source
        }
    } else {
        source
    };
    let source = if case.native_batch() {
        source
            .replace(
                "delta:i64,label:core::Text}",
                "delta:i64,label:core::Text,extra:core::ComponentResource<CounterKind>}",
            )
            .replace(
                "Request{delta:1,label:label}",
                "Request{delta:1,label:label,extra:extra}",
            )
    } else {
        source
    };
    let source = if matches!(case, Case::NativeList(..)) {
        source.replace("label:core::Text,extra:core::ComponentResource<CounterKind>", "label:core::Text,extra:core::Vec<core::ComponentResource<CounterKind>>")
            .replace("let mut request:Request=Request", "let mut items:core::Vec<core::ComponentResource<CounterKind>> =core::result_unwrap::<core::Vec<core::ComponentResource<CounterKind>>,core::AllocError>(core::vec_new::<core::ComponentResource<CounterKind>>(core::allocator()));let mut pushed:core::Unit=core::result_unwrap::<core::Unit,core::AllocError>(core::vec_push::<core::ComponentResource<CounterKind>>(items,extra));let mut request:Request=Request")
            .replace("label:label,extra:extra", "label:label,extra:items")
    } else {
        source
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
    let source = if matches!(case, Case::NativeWrongResultKind) {
        format!(
            "{}pub struct OtherKind{{}}",
            source.replace(
                "->core::ComponentResource<CounterKind>",
                "->core::ComponentResource<OtherKind>"
            )
        )
    } else {
        source
    };
    let mut resource_bindings = vec![f::Resource {
        tag: "CounterKind".into(),
        resource: ResourceTypeKey::new(interface.clone(), "counter"),
    }];
    if matches!(case, Case::NativeWrongResultKind) {
        resource_bindings.push(f::Resource {
            tag: "OtherKind".into(),
            resource: ResourceTypeKey::new(interface.clone(), "other"),
        });
    }
    let out = f::analyze(f::Request {
        sources: SourceMap::new(),
        display: Path::new("component-consume.lang"),
        bytes: source.as_bytes(),
        root: None,
        compile: Default::default(),
        document: checked(),
        bindings: f::Input {
            version: 1,
            resources: resource_bindings,
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
    assert!(
        out.diagnostics.is_empty(),
        "{:?}\n{source}",
        out.diagnostics
    );
    let unit = out.result.unwrap();
    let unit = if stored {
        let artifact = lang::artifact::component::encode(
            unit,
            m::Source {
                path: "component-consumer.idl.json".into(),
                text: String::from_utf8(bytes.clone()).unwrap(),
            },
            None,
            Default::default(),
        )
        .unwrap();
        let loaded = lang::artifact::component::decode(&artifact, Default::default()).unwrap();
        assert_eq!(
            loaded.unit().document().document().source.0 as usize,
            loaded
                .unit()
                .code()
                .declarations()
                .types()
                .table()
                .sources
                .len()
                - 1
        );
        loaded.into_unit()
    } else {
        unit
    };
    let mut session = wire::Session::new(Default::default());
    let consumer = if stored {
        wire::artifact::source_contract(&unit, &mut session, 65_536).unwrap()
    } else {
        wire::Contract::new(checked(), vec![], &mut session).unwrap()
    };
    let host = wire::Contract::new(
        idl::check(
            m::SourceId(71),
            &serde_json::to_vec(&host_json).unwrap(),
            Default::default(),
        )
        .unwrap(),
        vec![],
        &mut session,
    )
    .unwrap();
    let digest = VersionedDigest::new(DigestAlgorithm::Sha256, vec![1; 32]).unwrap();
    let destroyed = Rc::new(Destroyed {
        calls: Cell::new(0),
        method_calls: Cell::new(0),
        method_changed: Cell::new(false),
        method_control_checked: Cell::new(false),
        method_context: Cell::new(None),
        native: case.native(),
        batch: case.native_batch(),
        list: matches!(case, Case::NativeList(..)),
        native_result: case.native_result(),
        fresh_result: case.fresh_result(),
        published: Cell::new(0),
        result_shape: case.native_shape(),
        interruption: Cell::new(ptr::null()),
        value: Cell::new(0),
        allocator: Cell::new(0),
        context: Cell::new(None),
        fail: cleanup_failure,
    });
    let methods = [native::Method {
        interface: 0,
        member: 0,
        invoke: if case.native().is_some() {
            native_apply
        } else {
            unused
        },
        user: if case.native().is_some() {
            ptr::from_ref(destroyed.as_ref()).cast_mut().cast()
        } else {
            ptr::null_mut()
        },
        flags: native_flags,
    }];
    let mut resources = vec![native::ResourceType {
        interface: 0,
        resource: 0,
        destroy,
        user: ptr::from_ref(destroyed.as_ref()).cast_mut().cast(),
    }];
    if matches!(case, Case::NativeWrongResultKind) {
        resources.push(native::ResourceType {
            interface: 0,
            resource: 1,
            destroy,
            user: ptr::from_ref(destroyed.as_ref()).cast_mut().cast(),
        });
    }
    let acquisitions = Cell::new(0);
    let providers = [if owning_registration {
        native::Provider::new_owned(
            host,
            digest.clone(),
            methods.to_vec(),
            resources.clone(),
            &mut || {
                acquisitions.set(acquisitions.get() + 1);
                destroyed.clone()
            },
            &mut session,
        )
        .unwrap()
    } else {
        native::Provider::with_resources(
            wire::Provider {
                contract: &host,
                digest: &digest,
            },
            &methods,
            &resources,
            destroyed.as_ref(),
            &mut session,
        )
        .unwrap()
    }];
    assert_eq!(acquisitions.get(), usize::from(owning_registration));
    assert_eq!(
        Rc::strong_count(&destroyed),
        if owning_registration { 2 } else { 1 }
    );
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
    let alternative = native::Provider::with_resources(
        providers[0].metadata(),
        &methods,
        &resources,
        &destroyed,
        &mut session,
    )
    .unwrap();
    let kind = if matches!(
        case.native(),
        Some(NativeCase::WrongKind | NativeCase::WrongKindOwned)
    ) {
        alternative.resource_kind(kind.key(), &mut session).unwrap()
    } else {
        kind
    };
    let mut previous = None;
    for compiled in [false, true] {
        destroyed.calls.set(0);
        destroyed.published.set(0);
        destroyed.method_calls.set(0);
        destroyed.method_changed.set(false);
        destroyed.method_control_checked.set(false);
        destroyed.method_context.set(None);
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
            .publish(kind.clone(), holder, &41_i64.to_le_bytes(), &mut allocator)
            .unwrap();
        let extra_input = extra.then(|| {
            values
                .publish(kind.clone(), holder, &41_i64.to_le_bytes(), &mut allocator)
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
        if let Some(NativeCase::Oom(offset)) = case.native() {
            fail_at.set(Some(attempts.get() + offset));
        }
        let interruption = e::InterruptionFlag::new();
        destroyed.interruption.set(ptr::from_ref(&interruption));
        if matches!(case, Case::Cancelled | Case::LoanCancelled) {
            interruption.interrupt();
        }
        let control = e::Control::with_limits(e::Limits {
            fuel: if let Some(NativeCase::Fuel(fuel)) = case.native() {
                Some(fuel)
            } else if let Case::Fuel(fuel)
            | Case::FuelCleanup(fuel)
            | Case::LoanFuel(_, fuel)
            | Case::LoanFuelCleanup(fuel) = case
            {
                Some(fuel)
            } else {
                e::Limits::default().fuel
            },
            live_cells: if let Some(NativeCase::Cells(cap)) = case.native() {
                Some(cap)
            } else if let Case::LoanCells(cap) = case {
                Some(cap)
            } else {
                None
            },
            host_calls: if let Some(NativeCase::HostCalls(cap)) = case.native() {
                Some(cap)
            } else {
                e::Limits::default().host_calls
            },
            ..Default::default()
        })
        .and_service_work(work_limit)
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
        let mut instance = execution::Instance::new(&program);
        let mut invocation = execution::Invocation::prepare(
            &mut instance,
            m::FunctionId(0),
            &mut values,
            &mut bridge,
            &mut budget,
        )
        .unwrap();
        if let Some(namespace) = namespace {
            invocation.resource_namespace(namespace).unwrap();
        }
        let mut arguments = vec![EntryArgument::Resource(input), EntryArgument::I64(41)];
        if let Some(extra) = extra_input {
            arguments.push(EntryArgument::Resource(extra));
        }
        if matches!(case, Case::Capacity) {
            arguments.push(EntryArgument::Resource(input));
            arguments.push(EntryArgument::Resource(extra_input.unwrap()));
        }
        let mut expected_body = None;
        let mut replay = None;
        let mut preparation_failure = None;
        if case.loan().is_some() || case.native_declared() {
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
            if case.native_declared() {
                frame_values.nodes.push(wire::Value::Text("寿司".into()));
                frame_values.roots.push(wire::ValueId(2));
            }
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
            if work_limit.is_some() {
                let prepared = encoded.and_then(|bytes| {
                    invocation.decode_arguments(&bytes, &claims, &allocator, &mut session)
                });
                match prepared {
                    Ok(prepared) => arguments = prepared,
                    Err(lang_runtime::task::runner::Reason::Control(failure)) => {
                        preparation_failure = Some(failure);
                        let mut retry_session = wire::Session::new(Default::default());
                        assert!(matches!(
                            invocation.encode_arguments(&frame_values, &claims, &allocator, &mut retry_session),
                            Err(lang_runtime::task::runner::Reason::Control(repeated)) if repeated == failure
                        ));
                        assert_eq!(destroyed.method_calls.get(), 0);
                        assert!(!execution::Host::arguments_committed(&invocation));
                    }
                    Err(error) => panic!("unexpected finite-work argument preparation: {error:?}"),
                }
            } else {
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
                            kind.clone(),
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
                if matches!(case, Case::Native(NativeCase::Reborrow(_))) {
                    replay = Some(encoded.clone());
                }
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
                        &mut instance,
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
            Case::NestedReturn | Case::FrameOutput | Case::ReturnFrame | Case::NativeView
        ) || (case.native().is_some()
            && !matches!(case, Case::Native(NativeCase::MissingSession)))
            || case.data()
            || case.list()
            || case.scalars()
            || case.envelope() != 0
        {
            invocation.prepare_output(&mut session).unwrap();
        } else if matches!(case, Case::NestedReturnBudget) {
            invocation.prepare_output(&mut limited_output).unwrap();
        }
        if matches!(case, Case::Native(NativeCase::Abandoned)) {
            execution::Host::prepare_arguments(&mut invocation, &arguments, &allocator).unwrap();
            execution::Host::commit_arguments(&mut invocation, &mut allocator).unwrap();
            assert_eq!(invocation.active_loans(), 1);
            drop(invocation);
            assert!(instance.poisoned());
            let before = attempts.get();
            assert!(matches!(
                execution::Invocation::prepare(
                    &mut instance,
                    m::FunctionId(0),
                    &mut values,
                    &mut bridge,
                    &mut budget
                )
                .err(),
                Some(lang_runtime::task::runner::Reason::Instance(
                    execution::InstanceError::Poisoned
                ))
            ));
            assert_eq!(attempts.get(), before);
            assert_eq!(destroyed.method_calls.get(), 0);
            release_caller(
                &mut values,
                input,
                kind.clone(),
                &mut allocator,
                &mut bridge,
                &mut budget,
            );
            assert!(values.is_empty());
            assert_eq!(destroyed.calls.get(), 1);
            assert_eq!(destroyed.value.get(), 41);
            assert_eq!(allocator.stats().live_allocations, 0);
            previous = Some((false, true, Default::default()));
            continue;
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
        let poisoned = match &outcome {
            Ok(outcome) => outcome.poisoned,
            Err(failure) => failure.poisoned,
        };
        assert_eq!(instance.poisoned(), poisoned);
        if poisoned {
            let before = attempts.get();
            let refused = execution::Invocation::prepare(
                &mut instance,
                m::FunctionId(0),
                &mut values,
                &mut bridge,
                &mut budget,
            )
            .err()
            .expect("poison must persist into the next root entry");
            assert!(matches!(
                refused,
                lang_runtime::task::runner::Reason::Instance(execution::InstanceError::Poisoned)
            ));
            assert_eq!(attempts.get(), before);
        } else {
            let retry = execution::Invocation::prepare(
                &mut instance,
                m::FunctionId(0),
                &mut values,
                &mut bridge,
                &mut budget,
            )
            .expect("complete non-poisoned cleanup permits the next root preparation");
            if work_limit.is_some() {
                assert_eq!(execution::Host::service_work(&retry).used(), 0);
                assert!(execution::Host::work_failure(&retry).is_none());
            }
            drop(retry);
        }
        if let Some(encoded) = replay {
            assert!(outcome.is_ok());
            let mut again = execution::Invocation::prepare(
                &mut instance,
                m::FunctionId(0),
                &mut values,
                &mut bridge,
                &mut budget,
            )
            .unwrap();
            let arguments = again
                .decode_arguments(&encoded, &[input], &allocator, &mut session)
                .unwrap();
            again.prepare_output(&mut session).unwrap();
            let repeated = if compiled {
                native.execute_component(
                    &mut allocator,
                    control,
                    Default::default(),
                    &arguments,
                    &mut again,
                )
            } else {
                lang_vm::unit::tasks::execute_component(
                    &program,
                    m::FunctionId(0),
                    &mut allocator,
                    control,
                    Default::default(),
                    &arguments,
                    &mut again,
                )
            }
            .unwrap();
            assert_eq!(again.active_loans(), 0);
            drop(again);
            assert!(!instance.poisoned());
            assert!(!repeated.poisoned);
            let execution::Value::Frame(frame) = repeated.value else {
                panic!("native replay requires its canonical result");
            };
            assert_eq!(
                frame.values().nodes,
                vec![wire::Value::I64(if case.loan() == Some(true) {
                    43
                } else {
                    42
                })]
            );
        }
        if matches!(case.native(), Some(NativeCase::Interrupt)) {
            assert!(destroyed.method_control_checked.get());
        }
        if let Err(failure) = outcome {
            if native_flags & !1 != 0 {
                assert!(matches!(
                    failure.reason,
                    lang_runtime::task::runner::Reason::Unsupported
                ));
                assert!(!committed);
                assert_eq!(destroyed.method_calls.get(), 0);
            } else if let Some(limit) = work_limit {
                let lang_runtime::task::runner::Reason::Control(error) = failure.reason else {
                    panic!("finite component work lost control identity: {failure:?}");
                };
                assert!(
                    matches!(error, e::Failure::ServiceWorkLimit { limit: actual, used, requested }
                    if actual == limit && used == failure.usage.service_work && used <= limit
                        && used.checked_add(requested).is_none_or(|next| next > limit))
                );
                if let Some(expected) = preparation_failure {
                    assert_eq!(error, expected);
                    assert!(!committed);
                    assert_eq!(destroyed.method_calls.get(), 0);
                }
                assert_eq!(failure.span.unwrap().source, m::SourceId(0));
                if !cleanup_failure {
                    assert!(
                        !failure.poisoned,
                        "work refusal alone poisoned the instance"
                    );
                } else if committed {
                    let report = failure.report.as_ref().unwrap();
                    assert!(matches!(
                        report.primary().unwrap().kind,
                        lang_runtime::cleanup::FailureKind::Execution(
                            lang_runtime::cleanup::ExecutionFailure::ServiceWorkLimit { .. }
                        )
                    ));
                    assert!(report.cleanup_failures() > 0);
                    assert!(!report.secondary().is_empty());
                    assert!(
                        report
                            .secondary()
                            .iter()
                            .all(|failure| failure.span.is_some())
                    );
                }
            }
            if let Some(NativeCase::WrongKind | NativeCase::WrongKindOwned) = case.native() {
                assert!(matches!(
                    failure.reason,
                    lang_runtime::task::runner::Reason::Component(storage::Error::Implementation)
                ));
                assert_eq!(destroyed.method_calls.get(), 0);
            }
            if matches!(case, Case::Native(NativeCase::MissingSession)) {
                assert!(matches!(
                    failure.reason,
                    lang_runtime::task::runner::Reason::Unsupported
                ));
                assert_eq!(attempts.get(), 2);
            }
            if let Some(NativeCase::Oom(offset)) = case.native() {
                assert!(
                    matches!(
                        failure.reason.primary_cause(),
                        lang_runtime::task::runner::Reason::Panic(_)
                            | lang_runtime::task::runner::Reason::Layout(
                                lang_runtime::layout::Error::Allocation(a::Error::OutOfMemory)
                            )
                    ),
                    "offset {offset}, shape {:?}: {failure:?}",
                    case.native_shape()
                );
            }
            if let Some(NativeCase::Fuel(_)) = case.native() {
                assert!(matches!(
                    failure.report.as_ref().unwrap().primary().unwrap().kind,
                    lang_runtime::cleanup::FailureKind::Execution(
                        lang_runtime::cleanup::ExecutionFailure::FuelLimit
                    )
                ));
            }
            if let Some(NativeCase::HostCalls(_)) = case.native() {
                assert!(matches!(
                    failure.reason,
                    lang_runtime::task::runner::Reason::Control(e::Failure::HostCallLimit)
                ));
                assert_eq!(destroyed.method_calls.get(), 0);
            }
            if let Some(NativeCase::Cells(_)) = case.native() {
                assert!(matches!(
                    failure.reason,
                    lang_runtime::task::runner::Reason::CellLimit { .. }
                ));
            }
            if matches!(case.native(), Some(NativeCase::Interrupt)) {
                assert!(matches!(
                    failure.reason,
                    lang_runtime::task::runner::Reason::Control(e::Failure::Interrupted)
                ));
                assert_eq!(destroyed.method_calls.get(), 1);
                assert!(destroyed.method_changed.get());
            }
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
            if cleanup_failure || case.native_poison() {
                assert!(failure.poisoned);
                assert!(failure.report.as_ref().unwrap().primary().is_some());
                if case.native_transfer() && cleanup_failure && destroyed.method_calls.get() != 0 {
                    let report = failure.report.as_ref().unwrap();
                    assert!(report.cleanup_failures() >= 1);
                    if case.native_poison() {
                        assert!(matches!(
                            report.primary().unwrap().kind,
                            lang_runtime::cleanup::FailureKind::Execution(
                                lang_runtime::cleanup::ExecutionFailure::Invariant
                            )
                        ));
                        assert!(!report.secondary().is_empty());
                        assert!(
                            report
                                .secondary()
                                .iter()
                                .all(|failure| failure.span.is_some())
                        );
                    }
                }
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
                    lang_runtime::task::runner::Reason::CellLimit { .. }
                ));
            }
            if committed && case.loan().is_none() {
                assert!(values.is_empty());
                assert_eq!(
                    destroyed.calls.get(),
                    (if extra { 2 } else { 1 }) + destroyed.published.get()
                );
            } else {
                assert_eq!(
                    destroyed.calls.get(),
                    usize::from(committed && extra && !case.loan_aliases())
                        + destroyed.published.get()
                );
                if matches!(case, Case::Capacity) {
                    assert!(matches!(
                        failure.reason,
                        lang_runtime::task::runner::Reason::Component(storage::Error::Capacity)
                    ));
                    assert_eq!(attempts.get(), 4);
                }
                assert_eq!(
                    values
                        .bytes(input, holder, kind.clone(), &allocator)
                        .unwrap(),
                    (if destroyed.method_changed.get() {
                        42_i64
                    } else {
                        41_i64
                    })
                    .to_le_bytes()
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
                                kind.clone(),
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
                    if cleanup_failure {
                        assert!(retired.is_err());
                        assert!(cleanup.is_poisoned());
                    } else {
                        retired.unwrap();
                    }
                }
            }
            if case.loan().is_some() {
                assert_eq!(
                    destroyed.calls.get(),
                    (if extra { 2 } else { 1 }) + destroyed.published.get()
                );
                assert_eq!(
                    destroyed.value.get(),
                    if destroyed.method_changed.get() {
                        42
                    } else {
                        41
                    }
                );
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
        assert!(!cleanup_failure);
        assert!(committed);
        if matches!(case, Case::FrameOutput | Case::ReturnFrame)
            || (case.native().is_some() && !case.native_declared() && case.native_shape().is_none())
        {
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
        if let Some(shape) = case.native_shape() {
            let execution::Value::Frame(frame) = &outcome.value else {
                panic!("native aggregate result frame");
            };
            let claims = frame.resources();
            assert_eq!(claims.len(), 2);
            assert_eq!(destroyed.calls.get(), 0);
            assert_eq!(claims[0].token()[..24], input.token()[..24]);
            assert_eq!(claims[1].token()[..24], extra_input.unwrap().token()[..24]);
            let root = &frame.values().nodes[frame.values().roots[0].0];
            let ids = match (shape, root) {
                (NativeShape::List, wire::Value::List(ids)) => ids.clone(),
                (NativeShape::Record, wire::Value::Record(fields)) => {
                    assert_eq!(fields.len(), 4);
                    assert_eq!(
                        frame.values().nodes[fields[1].0],
                        wire::Value::Text("寿司".into())
                    );
                    assert_eq!(
                        frame.values().nodes[fields[2].0],
                        wire::Value::Bytes(vec![0, 128, 255])
                    );
                    let wire::Value::List(items) = &frame.values().nodes[fields[3].0] else {
                        panic!("native owned list field");
                    };
                    assert_eq!(items.len(), 1);
                    vec![fields[0], items[0]]
                }
                _ => panic!("wrong native aggregate result"),
            };
            assert_eq!(ids.len(), 2);
            for (position, id) in ids.iter().enumerate() {
                assert_eq!(
                    frame.values().nodes[id.0],
                    wire::Value::Resource(
                        lang_runtime::component::adapter::BoundaryResourceIndex::new(
                            position as u32 + 1 + if namespace.is_some() { 17 } else { 0 }
                        )
                        .unwrap()
                    )
                );
                assert_eq!(
                    values
                        .bytes(claims[position], holder, kind.clone(), &allocator)
                        .unwrap(),
                    42_i64.to_le_bytes()
                );
                release_caller(
                    &mut values,
                    claims[position],
                    kind.clone(),
                    &mut allocator,
                    &mut bridge,
                    &mut budget,
                );
            }
        } else if case.native_declared() {
            let execution::Value::Frame(frame) = &outcome.value else {
                panic!("declared native result frame required");
            };
            assert!(frame.resources().is_empty());
            let wire::Value::Err(payload) = &frame.values().nodes[frame.values().roots[0].0] else {
                panic!("declared Err must remain a value");
            };
            assert_eq!(
                frame.values().nodes[payload.0],
                wire::Value::Text("寿司".into())
            );
            assert_eq!(
                &frame.bytes()[45..],
                &[&[1_u8][..], &6_u32.to_le_bytes(), "寿司".as_bytes()].concat()
            );
        } else if matches!(case, Case::NestedReturn)
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
                    values
                        .bytes(*claim, holder, kind.clone(), &allocator)
                        .unwrap(),
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
                            kind.clone(),
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
            let original = if case.loan().is_some() && !case.fresh_result() {
                extra_input.unwrap()
            } else {
                input
            };
            assert_ne!(original, result);
            assert_eq!(
                destroyed.calls.get(),
                if case.loan().is_some() {
                    0
                } else {
                    destroyed.published.get()
                }
            );
            assert_eq!(
                original.token()[..24] == result.token()[..24],
                !case.fresh_result()
            );
            assert_eq!(
                values
                    .validate(original, holder, kind.key(), &allocator)
                    .is_ok(),
                case.loan().is_some() && case.fresh_result()
            );
            assert_eq!(
                values
                    .bytes(result, holder, kind.clone(), &allocator)
                    .unwrap(),
                (if destroyed.method_changed.get() {
                    42_i64
                } else {
                    41_i64
                })
                .to_le_bytes()
            );
            if case.native().is_some() {
                assert_eq!(destroyed.method_calls.get(), 1);
                let expired = destroyed.method_context.get().unwrap();
                let mut bytes = native::Bytes {
                    data: ptr::null(),
                    length: 99,
                };
                assert_eq!(
                    unsafe {
                        lang_codegen_clif::component::context_resource_read(&expired, 1, &mut bytes)
                    },
                    5
                );
                assert!(bytes.data.is_null());
                assert_eq!(bytes.length, 99);
            }
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
                        kind.clone(),
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
            if case.native().is_some() {
                assert_eq!(
                    destroyed.method_calls.get(),
                    if matches!(case, Case::Native(NativeCase::Reborrow(_))) {
                        2
                    } else {
                        1
                    }
                );
                assert_eq!(
                    destroyed.method_changed.get(),
                    !matches!(
                        case,
                        Case::Native(NativeCase::Shared | NativeCase::Reborrow(false))
                    )
                );
                let expired = destroyed.method_context.get().unwrap();
                let mut bytes = native::Bytes {
                    data: ptr::null(),
                    length: 99,
                };
                assert_eq!(
                    unsafe {
                        lang_codegen_clif::component::context_resource_read(&expired, 1, &mut bytes)
                    },
                    5
                );
                assert!(bytes.data.is_null());
                assert_eq!(bytes.length, 99);
            }
            assert_eq!(
                destroyed.calls.get(),
                usize::from(extra && !case.loan_aliases()) + destroyed.published.get()
            );
            assert_eq!(values.len(), if case.loan_aliases() { 2 } else { 1 });
            assert_eq!(
                values
                    .bytes(input, holder, kind.clone(), &allocator)
                    .unwrap(),
                (if destroyed.method_changed.get() {
                    41_i64 + destroyed.method_calls.get() as i64
                } else {
                    41_i64
                })
                .to_le_bytes()
            );
            // Detach also proves every call loan has expired before returning.
            release_caller(
                &mut values,
                input,
                kind.clone(),
                &mut allocator,
                &mut bridge,
                &mut budget,
            );
            if case.loan_aliases() {
                release_caller(
                    &mut values,
                    extra_input.unwrap(),
                    kind.clone(),
                    &mut allocator,
                    &mut bridge,
                    &mut budget,
                );
            }
        }
        assert!(values.is_empty());
        assert_eq!(
            destroyed.calls.get(),
            (if extra { 2 } else { 1 }) + destroyed.published.get()
        );
        assert_eq!(
            destroyed.value.get(),
            if destroyed.method_changed.get() {
                41 + destroyed.method_calls.get() as i64
            } else {
                41
            }
        );
        assert_eq!(engine.check(&destroyed.context.get().unwrap()), 5);
        assert_eq!(allocator.stats().live_allocations, 0);
        assert!(!outcome.poisoned);
        if let Some(NativeCase::Cells(cap)) = case.native() {
            assert!(outcome.usage.peak_live_cells <= cap);
        }
        if case.native().is_some() {
            assert_eq!(outcome.usage.host_calls, 1);
        }
        if let Case::LoanCells(cap) = case {
            assert!(outcome.usage.peak_live_cells <= cap);
        }
        let evidence = (true, committed, outcome.usage);
        if let Some(limit) = work_limit {
            assert!(outcome.usage.service_work <= limit);
        }
        if let Some(previous) = previous {
            assert_eq!(evidence, previous);
        }
        previous = Some(evidence);
    }
    previous.unwrap()
}

#[test]
fn external_resource_namespace_rebases_nested_native_list_and_record_results() {
    for stored in [false, true] {
        for shape in [NativeShape::List, NativeShape::Record] {
            let (success, committed, _) = run_source_namespace(
                Case::NativeAggregate(NativeCase::Owned, shape),
                stored,
                None,
                0,
                false,
                false,
                Some(execution::ResourceNamespace::new(17, 64)),
            );
            assert!(success && committed);
        }
    }
}

#[test]
fn stored_components_execute_actual_resources_imports_rich_results_and_cleanup() {
    for case in [
        Case::Consume,
        Case::Data,
        Case::List,
        Case::Some,
        Case::SharedLoan,
        Case::ExclusiveLoan,
        Case::NativeView,
        Case::Native(NativeCase::Shared),
        Case::NativeTransfer(NativeCase::Owned, false),
        Case::NativeResult(NativeCase::Owned),
        Case::NativeFresh(NativeCase::Owned),
        Case::NativeAggregate(NativeCase::Owned, NativeShape::List),
        Case::NativeAggregate(NativeCase::Owned, NativeShape::Record),
    ] {
        assert_eq!(run_source_mode(case, true), (true, true));
    }
}

#[test]
fn stored_component_import_failures_keep_original_cleanup_and_poison() {
    for case in [
        Case::NativeResult(NativeCase::Malformed),
        Case::NativeFresh(NativeCase::Interrupt),
        Case::NativeBorrowResult(NativeCase::Owned),
        Case::NativeWrongResultKind,
        Case::NativeAggregate(NativeCase::Malformed, NativeShape::Record),
    ] {
        assert_eq!(run_source_mode(case, true), (false, true));
    }
}

#[test]
fn stored_native_import_zero_work_preserves_inputs_and_first_preparation_failure() {
    let observed = run_source_mode_work(Case::NativeView, true, Some(0));
    assert_eq!((observed.0, observed.1), (false, false));
    assert_eq!(observed.2.service_work, 0);
}

#[test]
fn stored_native_import_service_work_exact_and_every_prefix_matches_vm_and_native() {
    let baseline = run_source_mode_work(Case::NativeView, true, Some(u64::MAX));
    assert_eq!((baseline.0, baseline.1), (true, true));
    let used = baseline.2.service_work;
    assert!(used > 0);
    eprintln!("stored native import work: {used} units; checking caps 0..={used}");
    for limit in 0..=used {
        let evidence = run_source_mode_work(Case::NativeView, true, Some(limit));
        assert_eq!(evidence.0, limit == used, "work cap {limit} of {used}");
        assert!(evidence.2.service_work <= limit);
    }
}

#[test]
fn finite_component_admission_keeps_work_limits_with_unused_reentry_methods() {
    let baseline = run_source_configuration(Case::SharedLoan, true, Some(u64::MAX), 1, false);
    assert_eq!((baseline.0, baseline.1), (true, true));
    let plain = run_source_configuration(Case::SharedLoan, true, Some(u64::MAX), 0, false);
    assert_eq!(baseline, plain, "unused reentry changes no value-work cost");
    let used = baseline.2.service_work;
    assert!(used > 0);
    for limit in [0, used - 1, used] {
        let observed = run_source_configuration(Case::SharedLoan, true, Some(limit), 1, false);
        assert_eq!(observed.0, limit == used);
        assert!(observed.2.service_work <= limit);
        if limit == 0 {
            assert!(!observed.1, "zero work preserves caller ownership");
        }
    }
}

#[test]
fn stored_native_rich_results_and_owners_keep_custody_at_work_boundaries() {
    for case in [
        Case::Native(NativeCase::Shared),
        Case::Native(NativeCase::Declared),
        Case::NativeTransfer(NativeCase::Owned, false),
        Case::NativeResult(NativeCase::Owned),
        Case::NativeFresh(NativeCase::Owned),
        Case::NativeAggregate(NativeCase::Owned, NativeShape::List),
        Case::NativeAggregate(NativeCase::Owned, NativeShape::Record),
    ] {
        let baseline = run_source_mode_work(case, true, Some(u64::MAX));
        assert_eq!((baseline.0, baseline.1), (true, true));
        let used = baseline.2.service_work;
        assert!(used > 0);
        for limit in [0, used / 2, used - 1, used] {
            let observed = run_source_mode_work(case, true, Some(limit));
            assert_eq!(observed.0, limit == used, "cap {limit} of {used}");
        }
    }
}

#[test]
fn work_refusal_remains_primary_when_mandatory_native_cleanup_also_fails() {
    let observed = run_source_configuration(
        Case::NativeTransfer(NativeCase::Owned, false),
        true,
        Some(0),
        0,
        true,
    );
    assert_eq!((observed.0, observed.1), (false, true));
    assert_eq!(observed.2.service_work, 0);
}

#[test]
fn owning_native_registrations_execute_stored_source_and_keep_original_costs() {
    for case in [
        Case::Native(NativeCase::Shared),
        Case::NativeResult(NativeCase::OwnedReturn),
        Case::NativeFresh(NativeCase::OwnedReturn),
        Case::NativeAggregate(NativeCase::OwnedReturn, NativeShape::Record),
        Case::DestructorFailure,
    ] {
        let borrowed = run_source_registration(case, true, None, 0, false, false);
        let owning = run_source_registration(case, true, None, 0, false, true);
        assert_eq!(borrowed, owning);
        if matches!(case, Case::NativeFresh(NativeCase::OwnedReturn)) {
            for limit in [
                0,
                borrowed.2.service_work.saturating_sub(1),
                borrowed.2.service_work,
            ] {
                assert_eq!(
                    run_source_registration(case, true, Some(limit), 0, false, false),
                    run_source_registration(case, true, Some(limit), 0, false, true),
                );
            }
        }
    }
}
