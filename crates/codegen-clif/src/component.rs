//! Native component callback bridge. Engine tokens reference stable engine
//! storage; active borrowed pointers are cleared before a callback scope ends.
use lang_mir::unit as m;
use lang_runtime::{
    cleanup as c,
    component::{
        execution::{Invoker, NativeContext, NativeReturn},
        native,
        storage::{Detached, Memory},
    },
    execution as e,
};
use std::{
    ffi::c_void,
    ptr::{self, NonNull},
    sync::atomic::{AtomicU64, Ordering},
    thread::{self, ThreadId},
};
mod context;
pub use context::{
    buffer_mut_bytes, context_buffer_create, context_buffer_drop, context_resource_publish,
    context_resource_read, context_resource_write,
};

static NEXT_INVOCATION: AtomicU64 = AtomicU64::new(1);
const MAGIC: u64 = 0x636f_6d70_6374_7831;
struct State {
    magic: u64,
    thread: ThreadId,
    invocation: u64,
    scope: Option<u64>,
    reentry: bool,
    next_scope: Option<u64>,
    control: *const e::Control<'static>,
    memory: Option<*mut (dyn Memory + 'static)>,
    call: Option<*mut (dyn NativeContext + 'static)>,
    last: Option<native::Context>,
    gateway: Option<NonNull<c_void>>,
}
pub struct Engine {
    state: Vec<State>,
}
impl Engine {
    pub fn prepare(budget: &mut m::Budget) -> Result<Self, m::Error> {
        let mut state = budget.vector(1)?;
        state.push(State {
            magic: MAGIC,
            thread: thread::current().id(),
            invocation: 0,
            scope: None,
            reentry: false,
            next_scope: Some(1),
            control: ptr::null(),
            memory: None,
            call: None,
            last: None,
            gateway: None,
        });
        Ok(Self { state })
    }
    /// Stable opaque identity used by callback contexts. Moving this wrapper
    /// does not move its reserved State allocation. It grants no live scope.
    pub fn native_handle(&self) -> *mut native::Engine {
        ptr::from_ref(&self.state[0]).cast_mut().cast()
    }
    /// Attach immutable gateway routing identity once. This bridge never
    /// dereferences or owns the pointer; its caller retains the actual owner.
    pub fn attach_gateway(&mut self, owner: NonNull<c_void>) -> Result<*mut native::Engine, u32> {
        let state = &mut self.state[0];
        if state.thread != thread::current().id() || state.scope.is_some() {
            return Err(4);
        }
        if state.gateway.is_some() {
            return Err(2);
        }
        state.gateway = Some(owner);
        Ok(self.native_handle())
    }
    pub fn begin<'a, 'ctrl>(
        &'a mut self,
        control: e::Control<'ctrl>,
    ) -> Result<Invocation<'a, 'ctrl>, u32> {
        let state = &mut self.state[0];
        if state.thread != thread::current().id() || state.scope.is_some() {
            return Err(4);
        }
        let id = NEXT_INVOCATION
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map_err(|_| 2u32)?;
        state.invocation = id;
        Ok(Invocation {
            state,
            control,
            accounting: None,
        })
    }
    pub fn last_context(&self) -> Option<native::Context> {
        self.state[0].last
    }
    pub fn check(&self, context: &native::Context) -> u32 {
        if context.engine != ptr::from_ref(&self.state[0]).cast_mut().cast() {
            return 5;
        }
        check(&self.state[0], context)
    }
}
/// Read only the engine's routing identity. Dynamic context authority still
/// requires the original invocation/scope checks; this creates no context.
///
/// # Safety
/// A non-null handle must originate from a live Engine::native_handle. The
/// bridge and the gateway's separately retained owner must remain live for
/// any caller access. No borrowed program/native-call pointer is returned.
pub unsafe fn gateway_owner(engine: *mut native::Engine) -> Result<NonNull<c_void>, u32> {
    if engine.is_null() {
        return Err(2);
    }
    // SAFETY: caller retains the exact live engine allocation through this
    // access. Foreign-thread calls read only its immutable identity fields.
    let state = unsafe { &*engine.cast::<State>() };
    if state.magic != MAGIC {
        return Err(2);
    }
    if state.thread != thread::current().id() {
        return Err(4);
    }
    state.gateway.ok_or(2)
}
fn check(state: &State, context: &native::Context) -> u32 {
    if state.magic != MAGIC
        || state.thread != thread::current().id()
        || context.header.size != size_of::<native::Context>() as u32
        || context.header.version != 1
        || context.reserved != [0; 2]
    {
        return 2;
    }
    if state.scope != Some(context.scope)
        || state.invocation != context.invocation
        || state.control.is_null()
        || (state.memory.is_none() && state.call.is_none())
    {
        return 5;
    }
    0
}
/// Check a token supplied to this private bridge. The full public component C
/// table is a later embedding delivery; this primitive shares original control.
///
/// # Safety
/// `context` must point to readable initialized bytes, and its engine must be
/// an Engine created here that remains alive. A copied expired scope is valid
/// input and is refused before any active borrowed pointer is accessed.
pub unsafe extern "C" fn context_check(context: *const native::Context) -> u32 {
    if context.is_null() {
        return 2;
    }
    // SAFETY: the caller supplies readable context bytes and a live matching
    // engine. Scope/thread/header checks precede active pointer restoration.
    let context = unsafe { &*context };
    if context.engine.is_null() {
        return 5;
    }
    // SAFETY: engine provenance/liveness is the trusted native caller contract.
    let state = unsafe { &*context.engine.cast::<State>() };
    let status = check(state, context);
    if status != 0 {
        return status;
    }
    // SAFETY: the active scope guard keeps the original control alive and clears
    // this pointer before callback return; no stack address is in the token.
    match unsafe { &*state.control }.check() {
        Ok(()) => 0,
        Err(_) => 1,
    }
}
/// Private bridge inspection used by native adapters and conformance probes.
///
/// # Safety
/// The pointer/engine requirements are the same as `context_check`.
pub unsafe extern "C" fn context_allocator_identity(context: *const native::Context) -> u64 {
    if context.is_null() {
        return 0;
    }
    // SAFETY: caller supplies readable context and matching live engine bytes.
    let context = unsafe { &*context };
    if context.engine.is_null() {
        return 0;
    }
    let state = unsafe { &*context.engine.cast::<State>() };
    if check(state, context) != 0 {
        return 0;
    }
    // SAFETY: the scope is active and owner-thread; Active retains and clears
    // this original borrowed allocator before callback return.
    if let Some(call) = state.call {
        // SAFETY: the active callback guard retains the original context.
        unsafe { &*call }.allocator_identity()
    } else {
        unsafe { &*state.memory.expect("active memory") }
            .identity()
            .get()
    }
}

pub struct Invocation<'a, 'ctrl> {
    state: &'a mut State,
    control: e::Control<'ctrl>,
    accounting: Option<e::InvocationAccounting>,
}
impl Invocation<'_, '_> {
    /// Lend counters before component preparation. This does not admit reentry
    /// or reuse an expired dynamic context; ordinary begin remains independent.
    pub fn with_accounting(mut self, accounting: &e::InvocationAccounting) -> Result<Self, u32> {
        if !accounting.matches(self.control) {
            return Err(2);
        }
        self.accounting = Some(accounting.clone());
        Ok(self)
    }
}
struct Active {
    state: *mut State,
}
impl Drop for Active {
    fn drop(&mut self) {
        // SAFETY: the engine's reserved single state never moves; the guard ends
        // before its unique engine borrow. No native callback is active at Drop.
        let state = unsafe { &mut *self.state };
        state.scope = None;
        state.reentry = false;
        state.control = ptr::null();
        state.memory = None;
        state.call = None;
    }
}
impl Invoker for Invocation<'_, '_> {
    fn reentry_supported(&self) -> bool {
        true
    }
    fn control(&self) -> e::Control<'_> {
        self.control
    }
    fn accounting(&self) -> Option<&e::InvocationAccounting> {
        self.accounting.as_ref()
    }
    fn service_work_limit(&self) -> Option<u64> {
        self.control.service_work_limit()
    }
    fn check_control(&self) -> Result<(), e::Failure> {
        self.control.check()
    }
    fn control_matches(&self, control: e::Control<'_>) -> bool {
        self.control.same_configuration(control)
    }
    fn invoke(
        &mut self,
        method: &native::Method,
        parameters: &[u8],
        call: &mut dyn NativeContext,
    ) -> Result<NativeReturn, u32> {
        let state = &mut *self.state;
        if state.thread != thread::current().id() || state.scope.is_some() {
            return Err(4);
        }
        let scope = state.next_scope.ok_or(2u32)?;
        state.next_scope = scope.checked_add(1);
        let context = native::Context {
            header: native::Header {
                size: size_of::<native::Context>() as u32,
                version: 1,
            },
            engine: ptr::from_mut(state).cast(),
            invocation: state.invocation,
            scope,
            reserved: [0; 2],
        };
        state.scope = Some(scope);
        state.reentry = method.flags & 1 != 0;
        state.last = Some(context);
        // SAFETY: Active clears both borrowed pointers before this invocation
        // or the original callback capabilities can move or be released.
        state.control = unsafe {
            std::mem::transmute::<*const e::Control<'_>, *const e::Control<'static>>(ptr::from_ref(
                &self.control,
            ))
        };
        state.call = Some(unsafe {
            std::mem::transmute::<*mut (dyn NativeContext + '_), *mut (dyn NativeContext + 'static)>(
                ptr::from_mut(call),
            )
        });
        let _active = Active {
            state: ptr::from_mut(state),
        };
        let arguments = native::Bytes {
            data: parameters.as_ptr(),
            length: parameters.len() as u64,
        };
        let mut buffer = ptr::null_mut();
        // SAFETY: the binding retains actual code/user owners. Input bytes and
        // original capabilities stay live through this owner-thread callback;
        // native implementations obey the fixed no-unwind/view contract.
        let status = unsafe { (method.invoke)(method.user, &context, arguments, &mut buffer) };
        Ok(NativeReturn { status, buffer })
    }
    fn destroy(
        &mut self,
        kind: native::ResourceKind<'_>,
        backing: &mut Detached<'_>,
        memory: &mut dyn Memory,
        _: &mut c::Context<'_>,
    ) -> Result<(), u32> {
        let state = &mut *self.state;
        if state.thread != thread::current().id() || state.scope.is_some() {
            return Err(4);
        }
        let scope = state.next_scope.ok_or(2u32)?;
        state.next_scope = scope.checked_add(1);
        let context = native::Context {
            header: native::Header {
                size: size_of::<native::Context>() as u32,
                version: 1,
            },
            engine: ptr::from_mut(state).cast(),
            invocation: state.invocation,
            scope,
            reserved: [0; 2],
        };
        state.scope = Some(scope);
        state.last = Some(context);
        // SAFETY: both lifetimes are erased only during Active's scope. The
        // callback uses the original allocator/control; Active clears all
        // pointers before either borrowed value can be moved or released.
        state.control = unsafe {
            std::mem::transmute::<*const e::Control<'_>, *const e::Control<'static>>(ptr::from_ref(
                &self.control,
            ))
        };
        state.memory = Some(unsafe {
            std::mem::transmute::<*mut (dyn Memory + '_), *mut (dyn Memory + 'static)>(
                ptr::from_mut(memory),
            )
        });
        let _active = Active {
            state: ptr::from_mut(state),
        };
        let bytes = backing.bytes_mut(memory).map_err(|_| 17u32)?;
        let bytes = native::MutBytes {
            data: bytes.as_mut_ptr(),
            length: bytes.len() as u64,
        };
        let record = kind.registration();
        let mut failure = ptr::null_mut();
        // SAFETY: the registration retains the real code/user owner. Backing is
        // uniquely detached and remains live through the call. The context
        // points at stable engine storage with this live owner-thread scope;
        // the native provider obeys the component-v1 no-unwind/view contract.
        let status = unsafe { (record.destroy)(record.user, &context, bytes, &mut failure) };
        if !failure.is_null() {
            return Err(18);
        }
        match status {
            0 => Ok(()),
            1 => Err(19),
            _ => Err(18),
        }
    }
}

/// Temporarily lend the original state and current capabilities to a checked
/// synchronous child. This private Rust hook publishes no C API entry.
///
/// # Safety
/// Context storage and its original live engine obey context_check's contract.
/// The operation must use the supplied original capabilities, reject task
/// exports and contain foreign unwinding. No borrowed capability may escape.
pub unsafe fn with_child<T>(
    context: *const native::Context,
    operation: impl for<'scope> FnOnce(
        &'scope mut dyn Invoker,
        &'scope mut dyn NativeContext,
        e::Control<'scope>,
    ) -> Result<T, u32>,
) -> Result<T, u32> {
    if context.is_null() {
        return Err(2);
    }
    // SAFETY: exact native context/engine liveness is the caller contract.
    let context = unsafe { &*context };
    if context.engine.is_null() {
        return Err(5);
    }
    let pointer = context.engine.cast::<State>();
    // SAFETY: only immutable identity fields are read on a foreign thread;
    // no State or active-capability reference is formed until owner validation.
    if unsafe { ptr::addr_of!((*pointer).magic).read() } != MAGIC {
        return Err(2);
    }
    if unsafe { ptr::addr_of!((*pointer).thread).read() } != thread::current().id() {
        return Err(4);
    }
    let state = unsafe { &mut *pointer };
    let status = check(state, context);
    if status != 0 {
        return Err(status);
    }
    if !state.reentry {
        return Err(4);
    }
    let call = state.call.ok_or(4u32)?;
    // SAFETY: the validated active scope retains the original control/call.
    let control = unsafe { *state.control };
    control.check().map_err(|_| 1u32)?;
    let call = unsafe { &mut *call };
    if !call.child_profile_supported() {
        return Err(4);
    }
    let accounting = call.accounting().ok_or(4u32)?.clone();
    if !accounting.matches(control) {
        return Err(4);
    }
    let parent_live = call.parent_live();
    struct Restore {
        state: *mut State,
        scope: Option<u64>,
        reentry: bool,
        control: *const e::Control<'static>,
        memory: Option<*mut (dyn Memory + 'static)>,
        call: Option<*mut (dyn NativeContext + 'static)>,
        last: Option<native::Context>,
    }
    impl Drop for Restore {
        fn drop(&mut self) {
            // SAFETY: the original reserved State outlives this synchronous
            // guard. Its child borrows have ended before restoration/unwind.
            let state = unsafe { &mut *self.state };
            state.scope = self.scope;
            state.reentry = self.reentry;
            state.control = self.control;
            state.memory = self.memory;
            state.call = self.call;
            state.last = self.last;
        }
    }
    let _restore = Restore {
        state: ptr::from_mut(state),
        scope: state.scope,
        reentry: state.reentry,
        control: state.control,
        memory: state.memory,
        call: state.call,
        last: state.last,
    };
    state.scope = None;
    state.reentry = false;
    state.control = ptr::null();
    state.memory = None;
    state.call = None;
    let mut child = Invocation {
        state,
        control,
        accounting: Some(accounting.clone()),
    };
    accounting
        .with_parent(parent_live, || operation(&mut child, call, control))
        .map_err(|_| 1u32)?
}

#[cfg(test)]
mod child_scope_tests {
    use super::*;
    use std::cell::Cell;

    struct Capabilities(e::InvocationAccounting);
    impl NativeContext for Capabilities {
        fn accounting(&self) -> Option<&e::InvocationAccounting> {
            Some(&self.0)
        }
        fn child_profile_supported(&self) -> bool {
            true
        }
        fn allocator_identity(&self) -> u64 {
            1
        }
        fn buffer_create(&mut self, _: u64, _: u64) -> Result<*mut native::Buffer, u32> {
            Err(4)
        }
        fn buffer_drop(&mut self, _: *mut native::Buffer) -> Result<(), u32> {
            Err(4)
        }
        fn buffer_mut_bytes(&mut self, _: *mut native::Buffer) -> Result<native::MutBytes, u32> {
            Err(4)
        }
        fn resource_read(&self, _: u32) -> Result<native::Bytes, u32> {
            Err(4)
        }
        fn resource_write(&self, _: u32) -> Result<native::MutBytes, u32> {
            Err(4)
        }
        fn resource_publish(&mut self, _: &[u8], _: *mut native::Buffer) -> Result<u32, u32> {
            Err(4)
        }
    }
    unsafe extern "C" fn contained_unwind(
        user: *mut c_void,
        context: *const native::Context,
        _: native::Bytes,
        _: *mut *mut native::Buffer,
    ) -> u32 {
        let failure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            // SAFETY: invocation retains this exact context while this known
            // Rust unwind is contained inside the native callback.
            unsafe {
                with_child(context, |_, _, _| -> Result<(), u32> {
                    panic!("child scope unwind")
                })
            }
        }));
        assert!(failure.is_err());
        assert_eq!(unsafe { context_check(context) }, 0);
        // SAFETY: test retains the exact initialized cell through invoke.
        unsafe { &*user.cast::<Cell<bool>>() }.set(true);
        1
    }

    #[test]
    fn child_scope_restores_original_token_before_a_contained_rust_unwind_returns() {
        let mut budget = m::Budget::new(Default::default());
        let mut engine = Engine::prepare(&mut budget).unwrap();
        let control = e::Control::with_limits(e::Limits::unbounded());
        let accounting = e::InvocationAccounting::prepare(control, &mut budget).unwrap();
        let mut capabilities = Capabilities(accounting);
        let restored = Cell::new(false);
        let method = native::Method {
            interface: 0,
            member: 0,
            invoke: contained_unwind,
            user: ptr::from_ref(&restored).cast_mut().cast(),
            flags: 1,
        };
        {
            let mut invocation = engine.begin(control).unwrap();
            let returned =
                Invoker::invoke(&mut invocation, &method, &[], &mut capabilities).unwrap();
            assert_eq!(returned.status, 1);
        }
        assert!(restored.get());
        assert_eq!(engine.check(&engine.last_context().unwrap()), 5);
        // Logical guest-instance host-panic poisoning remains E03; this probe
        // has no guest instance and verifies only the original scope borrow.
    }
}
