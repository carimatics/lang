//! Scoped native callback storage. Payloads use the original allocator; stable
//! handle headers are bounded caller metadata and remain until callback close.
use super::*;
use crate::allocation as a;
use std::cell::{Cell, RefCell};
use std::ptr;

#[derive(Clone, Copy)]
pub struct NativeReturn {
    pub status: u32,
    pub buffer: *mut super::super::native::Buffer,
}
#[derive(Debug)]
pub enum NativeFailure {
    Invocation(u32),
    Status {
        code: u32,
        bytes: Vec<u8>,
    },
    MissingResult,
    InvalidResult(wire::Error),
    Resource {
        index: u32,
        kind: NativeResourceFailure,
    },
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeResourceFailure {
    UnknownIndex,
    NotOwned,
    WrongKind,
    Aliased,
}
#[derive(Clone, Copy)]
pub struct ResourceArgument {
    pub claim: Claim,
    pub parent: Option<usize>,
}
pub struct NativeArguments {
    pub(crate) values: wire::Values,
    pub(crate) resources: Vec<ResourceArgument>,
    /// A commit receipt, not another owner store. Source cells use these exact
    /// old claims to discard consumed tokens while retiring their containers.
    pub(crate) consumed: Vec<Claim>,
    pub(crate) cleanup: Option<c::Session>,
}
/// Unpublished native result. The original table retains custody until every
/// guest representation is ready. A failed staging operation must abort this
/// receipt and retire its original claims through the same cleanup Session.
pub struct NativeResult {
    pub(crate) values: wire::Values,
    pub(crate) nodes: Vec<usize>,
    pub(crate) claims: Vec<Claim>,
    pub(crate) originals: Vec<Claim>,
    pub(crate) holder: Option<Holder>,
    pub(crate) transfer: Option<Transfer>,
    pub(crate) committed: bool,
}
impl NativeResult {
    pub(crate) fn claim(&self, node: usize) -> Result<Claim, Reason> {
        let index = self
            .nodes
            .iter()
            .position(|value| *value == node)
            .ok_or(Reason::Entry)?;
        self.claims.get(index).copied().ok_or(Reason::Entry)
    }
}
pub(crate) enum ReadRoot<'l, 't> {
    Value(l::ReadView<'l, 't>),
    Resource(ResourceArgument),
}

/// The actual callback's borrowed capabilities. Raw native pointers never
/// construct this service, and the bridge expires it before returning to guest.
pub trait StorageOperation {
    fn run(
        &mut self,
        resources: &mut Resources<'_>,
        memory: &mut dyn Memory,
        budget: &mut m::Budget,
        holder: Holder,
        accounting: &crate::execution::InvocationAccounting,
    ) -> Result<(), u32>;
}
pub trait TaskStorageOperation {
    fn run(
        &mut self,
        resources: &mut Resources<'_>,
        memory: &mut dyn Memory,
        budget: &mut m::Budget,
        holder: Holder,
        accounting: &crate::execution::InvocationAccounting,
        task: &mut crate::task::CurrentTask<'_>,
    ) -> Result<(), u32>;
}
/// A scoped borrow of the current native call's actual index-to-view mapping.
/// Wire indices alone never construct this resource authority.
pub struct CallbackArguments<'a, 'code> {
    views: RefCell<&'a mut Vec<NativeView<'code>>>,
    owners: RefCell<&'a mut Vec<NativeOwner>>,
    holder: Holder,
    index_limit: u32,
}
/// A custody receipt is retired only after the original table commits its move.
/// It carries no resource authority and never outlives the native call.
pub(crate) struct NativeOwner {
    pub claim: Claim,
    pub position: usize,
    pub held: Cell<bool>,
}
impl NativeOwner {
    pub fn new(claim: Claim, position: usize) -> Self {
        Self {
            claim,
            position,
            held: Cell::new(true),
        }
    }
}
pub(crate) trait CallbackCustody<'code> {
    fn holder(&self) -> Holder;
    fn check(&self, inputs: &[storage::Input]) -> Result<(), Reason>;
    fn committed(&self, inputs: &[storage::Input]);
    fn result_namespace(&self, count: usize) -> Result<ResourceNamespace, Reason>;
    fn publish(&self, views: Vec<NativeView<'code>>);
}
impl<'code> CallbackCustody<'code> for CallbackArguments<'_, 'code> {
    fn holder(&self) -> Holder {
        self.holder
    }
    fn check(&self, inputs: &[storage::Input]) -> Result<(), Reason> {
        for input in inputs
            .iter()
            .filter(|input| matches!(input.access, storage::Access::Own))
        {
            if !self
                .owners
                .borrow()
                .iter()
                .any(|owner| owner.claim == input.claim && owner.held.get())
            {
                return Err(fault(storage::Error::State));
            }
        }
        Ok(())
    }
    fn committed(&self, inputs: &[storage::Input]) {
        for owner in self.owners.borrow().iter() {
            if inputs.iter().any(|input| {
                input.claim == owner.claim && matches!(input.access, storage::Access::Own)
            }) {
                owner.held.set(false);
            }
        }
    }
    fn result_namespace(&self, count: usize) -> Result<ResourceNamespace, Reason> {
        let views = self.views.borrow();
        let owners = self.owners.borrow();
        let issued = u32::try_from(views.len()).map_err(|_| fault(storage::Error::Exhausted))?;
        let namespace = ResourceNamespace::new(issued, self.index_limit);
        namespace.result(count)?;
        if count > views.capacity() - views.len() || count > owners.capacity() - owners.len() {
            return Err(fault(storage::Error::Capacity));
        }
        Ok(namespace)
    }
    fn publish(&self, returned: Vec<NativeView<'code>>) {
        let mut views = self.views.borrow_mut();
        let mut owners = self.owners.borrow_mut();
        // The scoped context is unavailable during guest execution. Both exact
        // capacities and every issued index were admitted before table commit.
        debug_assert!(returned.len() <= views.capacity() - views.len());
        debug_assert!(returned.len() <= owners.capacity() - owners.len());
        for view in returned {
            let NativeAuthority::Own(claim, holder) = view.authority else {
                unreachable!("result admission accepts owners only")
            };
            debug_assert_eq!(holder, self.holder);
            owners.push(NativeOwner::new(claim, views.len()));
            views.push(view);
        }
    }
}
impl CallbackArguments<'_, '_> {
    /// Issued locators in this exact native call; this grants no ownership.
    pub fn resource_indices(&self) -> usize {
        self.views.borrow().len()
    }
    pub(crate) fn receipt_work(&self, inputs: usize) -> usize {
        self.owners
            .borrow()
            .len()
            .saturating_add(1)
            .saturating_mul(inputs)
            .saturating_mul(3)
    }
    pub(crate) fn input(
        &self,
        used: &wire::ResourceUse<'_>,
        resources: &Resources<'_>,
        caller: Holder,
        memory: &dyn Memory,
    ) -> Result<storage::Input, Reason> {
        if caller != self.holder {
            return Err(fault(storage::Error::WrongHolder));
        }
        let views = self.views.borrow();
        let view = views
            .get((used.index.get() - 1) as usize)
            .ok_or(Reason::Entry)?;
        if view.kind.key() != used.key {
            return Err(fault(storage::Error::StaticType));
        }
        let access = match used.access {
            wire::ResourceAccess::Own => storage::Access::Own,
            wire::ResourceAccess::Borrow(super::super::idl::Access::Shared) => {
                storage::Access::Shared
            }
            wire::ResourceAccess::Borrow(super::super::idl::Access::Exclusive) => {
                storage::Access::Exclusive
            }
        };
        if matches!(access, storage::Access::Exclusive)
            && view.access != super::super::idl::Access::Exclusive
        {
            return Err(fault(storage::Error::State));
        }
        match view.authority {
            NativeAuthority::Own(claim, holder) => {
                if holder != caller {
                    return Err(fault(storage::Error::WrongHolder));
                }
                resources
                    .validate(claim, holder, used.key, memory)
                    .map_err(fault)?;
                resources
                    .bytes(claim, holder, view.kind.clone(), memory)
                    .map_err(fault)?;
                Ok(storage::Input {
                    claim,
                    access,
                    parent: None,
                })
            }
            NativeAuthority::Loan(loan) => {
                let requested = match used.access {
                    wire::ResourceAccess::Own => return Err(fault(storage::Error::State)),
                    wire::ResourceAccess::Borrow(access) => access,
                };
                resources
                    .callback_loan(loan, view.kind.clone(), requested, caller, memory)
                    .map_err(fault)
            }
        }
    }
}
/// All operands are borrowed from the current dynamic native call.
pub trait CallbackOperation {
    #[allow(
        clippy::too_many_arguments,
        reason = "Keep the original instance, task and storage authority explicit."
    )]
    fn run<'code>(
        &mut self,
        instance: &mut Instance<'_, '_>,
        resources: &mut Resources<'code>,
        memory: &mut dyn Memory,
        budget: &mut m::Budget,
        session: &mut wire::Session,
        arguments: &CallbackArguments<'_, 'code>,
        holder: Holder,
        accounting: &crate::execution::InvocationAccounting,
        task: &mut crate::task::CurrentTask<'_>,
    ) -> Result<(), u32>;
}
pub trait NativeContext {
    fn with_callback(&mut self, _: &mut dyn CallbackOperation) -> Result<(), u32> {
        Err(4)
    }
    fn with_task_storage(&mut self, _: &mut dyn TaskStorageOperation) -> Result<(), u32> {
        Err(4)
    }

    fn parent_live(&self) -> crate::execution::LiveUsage {
        Default::default()
    }
    fn accounting(&self) -> Option<&crate::execution::InvocationAccounting> {
        None
    }
    fn child_profile_supported(&self) -> bool {
        false
    }
    fn with_storage(&mut self, _: &mut dyn StorageOperation) -> Result<(), u32> {
        Err(4)
    }
    fn allocator_identity(&self) -> u64;
    fn buffer_create(
        &mut self,
        length: u64,
        alignment: u64,
    ) -> Result<*mut super::super::native::Buffer, u32>;
    fn buffer_drop(&mut self, buffer: *mut super::super::native::Buffer) -> Result<(), u32>;
    fn buffer_mut_bytes(
        &mut self,
        buffer: *mut super::super::native::Buffer,
    ) -> Result<super::super::native::MutBytes, u32>;
    fn resource_read(&self, index: u32) -> Result<super::super::native::Bytes, u32>;
    fn resource_write(&self, index: u32) -> Result<super::super::native::MutBytes, u32>;
    fn resource_publish(
        &mut self,
        key: &[u8],
        buffer: *mut super::super::native::Buffer,
    ) -> Result<u32, u32>;
}

pub(crate) struct NativeView<'a> {
    pub authority: NativeAuthority,
    pub kind: ResourceKind<'a>,
    pub access: super::super::idl::Access,
    pub bytes: super::super::native::MutBytes,
}
pub(crate) enum NativeAuthority {
    Loan(storage::Loan),
    Own(Claim, storage::Holder),
}
struct BufferHeader {
    allocation: Option<a::Handle>,
}
struct BufferSlot {
    header: Vec<BufferHeader>,
}
impl BufferSlot {
    fn pointer(&self) -> *mut super::super::native::Buffer {
        self.header.as_ptr().cast_mut().cast()
    }
}
type KindResolver<'scope, 'code> =
    dyn Fn(&[u8], &mut m::Budget) -> Result<ResourceKind<'code>, u32> + 'scope;
type KindAuthority<'scope> = dyn for<'kind> Fn(&ResourceKind<'kind>) -> bool + 'scope;

pub(crate) struct CallContext<'r, 'm, 'b, 'scope, 'code> {
    pub resources: &'r mut Resources<'code>,
    pub memory: &'m mut dyn Memory,
    pub budget: &'b mut m::Budget,
    pub views: &'r mut Vec<NativeView<'code>>,
    pub owners: &'r mut Vec<NativeOwner>,
    pub holder: Holder,
    kind: &'scope KindResolver<'scope, 'code>,
    implementation: Option<&'scope KindAuthority<'scope>>,
    buffers: Vec<BufferSlot>,
    index_limit: u32,
    accounting: Option<&'scope crate::execution::InvocationAccounting>,
    parent_live: crate::execution::LiveUsage,
    task: Option<crate::task::CurrentTask<'r>>,
    instance: Option<&'r mut dyn instance::CallbackInstance>,
    session: Option<&'b mut wire::Session>,
}
impl<'r, 'm, 'b, 'scope, 'code> CallContext<'r, 'm, 'b, 'scope, 'code> {
    pub fn new(
        resources: &'r mut Resources<'code>,
        memory: &'m mut dyn Memory,
        budget: &'b mut m::Budget,
        views: &'r mut Vec<NativeView<'code>>,
        owners: &'r mut Vec<NativeOwner>,
        holder: Holder,
        kind: &'scope KindResolver<'scope, 'code>,
    ) -> Self {
        Self {
            resources,
            memory,
            budget,
            views,
            owners,
            holder,
            kind,
            implementation: None,
            buffers: Vec::new(),
            index_limit: u32::MAX,
            accounting: None,
            parent_live: Default::default(),
            task: None,
            instance: None,
            session: None,
        }
    }
    pub fn with_index_limit(mut self, limit: u32) -> Self {
        self.index_limit = limit;
        self
    }
    pub fn with_implementation(mut self, check: &'scope KindAuthority<'scope>) -> Self {
        self.implementation = Some(check);
        self
    }
    pub fn with_accounting(
        mut self,
        accounting: &'scope crate::execution::InvocationAccounting,
    ) -> Self {
        self.accounting = Some(accounting);
        self
    }
    pub fn with_session(mut self, session: &'b mut wire::Session) -> Self {
        self.session = Some(session);
        self
    }
    pub fn take_session(&mut self) -> Result<&'b mut wire::Session, Reason> {
        self.session.take().ok_or(Reason::Invariant)
    }
    pub fn with_instance(
        mut self,
        instance: &'r mut dyn instance::CallbackInstance,
        permitted: bool,
    ) -> Self {
        if permitted {
            self.instance = Some(instance);
        }
        self
    }
    pub fn with_task(mut self, task: crate::task::CurrentTask<'r>) -> Self {
        self.task = Some(task);
        self
    }
    pub fn instance_poisoned(&self) -> bool {
        self.instance
            .as_deref()
            .is_some_and(instance::CallbackInstance::poisoned)
    }
    pub fn task_closing(&self) -> Result<bool, crate::task::Error> {
        self.task
            .as_ref()
            .map_or(Ok(false), crate::task::CurrentTask::closing_requested)
    }
    pub fn with_parent_live(mut self, live: crate::execution::LiveUsage) -> Self {
        self.parent_live = live;
        self
    }
    fn slot(&self, buffer: *mut super::super::native::Buffer) -> Result<usize, u32> {
        if buffer.is_null() {
            return Err(2);
        }
        self.buffers
            .iter()
            .position(|slot| slot.pointer() == buffer && slot.header[0].allocation.is_some())
            .ok_or(5)
    }
    pub fn result(
        &mut self,
        buffer: *mut super::super::native::Buffer,
        session: &mut wire::Session,
    ) -> Result<Vec<u8>, Reason> {
        let index = self.slot(buffer).map_err(|_| Reason::Invariant)?;
        let allocation = self.buffers[index].header[0]
            .allocation
            .ok_or(Reason::Invariant)?;
        let bytes = self.memory.bytes(allocation).map_err(|e| fault(e.into()))?;
        let mut preparation = session
            .scope_preparation(self.budget)
            .map_err(Reason::from)?;
        let session = &mut *preparation;
        session.value_bytes(bytes.len(), 0).map_err(Reason::from)?;
        session.value_work(bytes.len(), 0).map_err(Reason::from)?;
        let mut out = session.vector(bytes.len(), 0).map_err(Reason::from)?;
        out.extend_from_slice(bytes);
        Ok(out)
    }
    /// Unmetered, unlink-before-release draining, including a rejected result.
    pub fn close(&mut self) -> Result<(), Reason> {
        let mut failure = None;
        for slot in self.buffers.iter_mut().rev() {
            if let Some(allocation) = slot.header[0].allocation.take()
                && let Err(error) = self.memory.release(allocation)
            {
                failure.get_or_insert(error);
            }
        }
        failure.map_or(Ok(()), |e| Err(fault(e.into())))
    }
    fn view(&self, index: u32) -> Result<&NativeView<'code>, u32> {
        let index = index.checked_sub(1).ok_or(7u32)? as usize;
        let view = self.views.get(index).ok_or(7u32)?;
        if !self.implementation.is_some_and(|check| check(&view.kind)) {
            return Err(7);
        }
        match view.authority {
            NativeAuthority::Loan(loan) => {
                self.resources
                    .check_view(loan, view.kind.clone(), view.access)
            }
            NativeAuthority::Own(claim, holder) => self
                .resources
                .bytes(claim, holder, view.kind.clone(), self.memory)
                .map(|_| ()),
        }
        .map_err(|_| 7u32)?;
        Ok(view)
    }
}
impl NativeContext for CallContext<'_, '_, '_, '_, '_> {
    fn with_callback(&mut self, operation: &mut dyn CallbackOperation) -> Result<(), u32> {
        let task = self.task.as_mut().ok_or(4u32)?;
        let accounting = self.accounting.ok_or(4u32)?;
        if accounting.current_task().as_ref() != Some(task.id()) {
            return Err(4);
        }
        let instance = self.instance.as_deref_mut().ok_or(4u32)?;
        let session = self.session.as_deref_mut().ok_or(4u32)?;
        let arguments = CallbackArguments {
            views: RefCell::new(self.views),
            owners: RefCell::new(self.owners),
            holder: self.holder,
            index_limit: self.index_limit,
        };
        instance.with_callback(self.holder, &mut |child| {
            operation.run(
                child,
                self.resources,
                self.memory,
                self.budget,
                session,
                &arguments,
                self.holder,
                accounting,
                task,
            )
        })
    }
    fn with_task_storage(&mut self, operation: &mut dyn TaskStorageOperation) -> Result<(), u32> {
        let task = self.task.as_mut().ok_or(4u32)?;
        let accounting = self.accounting.ok_or(4u32)?;
        if accounting.current_task().as_ref() != Some(task.id()) {
            return Err(4);
        }
        operation.run(
            self.resources,
            self.memory,
            self.budget,
            self.holder,
            accounting,
            task,
        )
    }

    fn parent_live(&self) -> crate::execution::LiveUsage {
        self.parent_live
    }
    fn accounting(&self) -> Option<&crate::execution::InvocationAccounting> {
        self.accounting
    }
    fn child_profile_supported(&self) -> bool {
        true
    }
    fn with_storage(&mut self, operation: &mut dyn StorageOperation) -> Result<(), u32> {
        operation.run(
            self.resources,
            self.memory,
            self.budget,
            self.holder,
            self.accounting.ok_or(4u32)?,
        )
    }
    fn resource_publish(
        &mut self,
        key: &[u8],
        buffer: *mut super::super::native::Buffer,
    ) -> Result<u32, u32> {
        if self.views.len() >= self.index_limit as usize {
            return Err(12);
        }
        self.budget.charge(self.buffers.len()).map_err(|_| 12u32)?;
        let slot = self.slot(buffer)?;
        let kind = (self.kind)(key, self.budget)?;
        let index = u32::try_from(self.views.len())
            .ok()
            .and_then(|n| n.checked_add(1))
            .ok_or(12u32)?;
        if self.views.len() == self.views.capacity() || self.owners.len() == self.owners.capacity()
        {
            return Err(12);
        }
        let allocation = self.buffers[slot].header[0].allocation.ok_or(5u32)?;
        let bytes = self.memory.bytes_mut(allocation).map_err(|_| 17u32)?;
        let bytes = super::super::native::MutBytes {
            data: bytes.as_mut_ptr(),
            length: bytes.len() as u64,
        };
        let claim = self
            .resources
            .publish_backing(kind.clone(), self.holder, allocation, self.memory)
            .map_err(|_| 12u32)?;
        // The complete metadata capacity and index were checked before custody
        // moved; publication now performs no allocation or foreign callback.
        self.buffers[slot].header[0].allocation = None;
        self.owners.push(NativeOwner::new(claim, self.views.len()));
        self.views.push(NativeView {
            authority: NativeAuthority::Own(claim, self.holder),
            kind,
            access: super::super::idl::Access::Exclusive,
            bytes,
        });
        Ok(index)
    }
    fn allocator_identity(&self) -> u64 {
        self.memory.identity().get()
    }
    fn buffer_create(
        &mut self,
        length: u64,
        alignment: u64,
    ) -> Result<*mut super::super::native::Buffer, u32> {
        let request = a::Request::new(length, alignment).map_err(|_| 2u32)?;
        if self.buffers.len() == self.buffers.capacity() {
            let capacity = self
                .buffers
                .len()
                .checked_add(1)
                .and_then(|n| n.checked_mul(2))
                .ok_or(12u32)?;
            let mut slots = self.budget.vector(capacity).map_err(|_| 12u32)?;
            slots.append(&mut self.buffers);
            self.buffers = slots;
        }
        let mut header = self.budget.vector(1).map_err(|_| 12u32)?;
        let allocation = self.memory.allocate(request).map_err(|_| 12u32)?;
        header.push(BufferHeader {
            allocation: Some(allocation),
        });
        let slot = BufferSlot { header };
        let pointer = slot.pointer();
        self.buffers.push(slot);
        Ok(pointer)
    }
    fn buffer_drop(&mut self, buffer: *mut super::super::native::Buffer) -> Result<(), u32> {
        let index = self.slot(buffer)?;
        let allocation = self.buffers[index].header[0]
            .allocation
            .take()
            .ok_or(5u32)?;
        self.memory.release(allocation).map_err(|_| 17u32)
    }
    fn buffer_mut_bytes(
        &mut self,
        buffer: *mut super::super::native::Buffer,
    ) -> Result<super::super::native::MutBytes, u32> {
        self.budget.charge(self.buffers.len()).map_err(|_| 12u32)?;
        let index = self.slot(buffer)?;
        let allocation = self.buffers[index].header[0].allocation.ok_or(5u32)?;
        let bytes = self.memory.bytes_mut(allocation).map_err(|_| 17u32)?;
        Ok(super::super::native::MutBytes {
            data: if bytes.is_empty() {
                ptr::null_mut()
            } else {
                bytes.as_mut_ptr()
            },
            length: bytes.len() as u64,
        })
    }
    fn resource_read(&self, index: u32) -> Result<super::super::native::Bytes, u32> {
        let view = self.view(index)?;
        Ok(super::super::native::Bytes {
            data: view.bytes.data.cast_const(),
            length: view.bytes.length,
        })
    }
    fn resource_write(&self, index: u32) -> Result<super::super::native::MutBytes, u32> {
        let view = self.view(index)?;
        if view.access != super::super::idl::Access::Exclusive {
            return Err(7);
        }
        Ok(view.bytes)
    }
}

#[cfg(test)]
mod callback_authority_tests {
    use super::*;
    use crate::component::{
        adapter::BoundaryResourceIndex,
        idl, native,
        negotiation::{InterfaceKey, PackageKey, ResourceTypeKey},
        serialization::{DigestAlgorithm, VersionedDigest},
    };
    use std::ffi::c_void;

    extern "C" fn unused(
        _: *mut c_void,
        _: *const native::Context,
        _: native::Bytes,
        _: *mut *mut native::Buffer,
    ) -> u32 {
        0
    }
    extern "C" fn destroy(
        _: *mut c_void,
        _: *const native::Context,
        _: native::MutBytes,
        _: *mut *mut native::Buffer,
    ) -> u32 {
        0
    }

    #[test]
    fn foreign_implementation_owners_stay_opaque_but_transfer_through_original_custody() {
        let document = idl::check(
            m::SourceId(1),
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../tests/fixtures/component_counter.idl.json"
            )),
            Default::default(),
        )
        .unwrap();
        let mut session = wire::Session::new(Default::default());
        let contract = wire::Contract::new(document, vec![], &mut session).unwrap();
        let digest = VersionedDigest::new(DigestAlgorithm::Sha256, vec![1; 32]).unwrap();
        let methods = [native::Method {
            interface: 0,
            member: 0,
            invoke: unused,
            user: ptr::null_mut(),
            flags: 0,
        }];
        let descriptors = [native::ResourceType {
            interface: 0,
            resource: 0,
            destroy,
            user: ptr::null_mut(),
        }];
        let mut register = || {
            native::Provider::with_resources(
                wire::Provider {
                    contract: &contract,
                    digest: &digest,
                },
                &methods,
                &descriptors,
                &(),
                &mut session,
            )
            .unwrap()
        };
        let first = register();
        let other = register();
        assert!(!first.same_registration(&other));
        let key = ResourceTypeKey::new(
            InterfaceKey::new(PackageKey::new("demo", "counter"), "api"),
            "counter",
        );
        let local = first.resource_kind(&key, &mut session).unwrap();
        let foreign = other.resource_kind(&key, &mut session).unwrap();
        let mut memory = a::Allocator::new(a::System, Default::default()).unwrap();
        let mut budget = m::Budget::new(Default::default());
        let mut resources = Resources::prepare(&memory, 2, 1, &mut budget).unwrap();
        let holder = resources.new_holder().unwrap();
        let guest = resources.new_holder().unwrap();
        let a = resources
            .publish(local.clone(), holder, &41_i64.to_le_bytes(), &mut memory)
            .unwrap();
        let b = resources
            .publish(foreign.clone(), holder, &42_i64.to_le_bytes(), &mut memory)
            .unwrap();
        let mut views = Vec::with_capacity(3);
        for (claim, kind) in [(a, local), (b, foreign)] {
            let bytes = resources
                .owned_view(claim, holder, kind.clone(), &mut memory)
                .unwrap();
            views.push(NativeView {
                authority: NativeAuthority::Own(claim, holder),
                kind,
                access: idl::Access::Exclusive,
                bytes,
            });
        }
        let mut owners = vec![NativeOwner::new(a, 0), NativeOwner::new(b, 1)];
        let kind = |_: &[u8], _: &mut m::Budget| Err(7);
        let implementation = |kind: &ResourceKind<'_>| kind.provider().same_registration(&first);
        let context = CallContext::new(
            &mut resources,
            &mut memory,
            &mut budget,
            &mut views,
            &mut owners,
            holder,
            &kind,
        )
        .with_implementation(&implementation);
        assert_eq!(context.resource_read(1).unwrap().length, 8);
        assert!(context.resource_write(1).is_ok());
        assert!(matches!(context.resource_read(2), Err(7)));
        assert!(matches!(context.resource_write(2), Err(7)));
        let next = {
            let arguments = CallbackArguments {
                views: RefCell::new(&mut *context.views),
                owners: RefCell::new(&mut *context.owners),
                holder,
                index_limit: 3,
            };
            let used = wire::ResourceUse {
                index: BoundaryResourceIndex::new(2).unwrap(),
                key: &key,
                access: wire::ResourceAccess::Own,
                value: wire::ValueId(0),
                offset: 0,
            };
            let input = arguments
                .input(&used, context.resources, holder, context.memory)
                .unwrap();
            arguments.check(&[input]).unwrap();
            let transfer = context
                .resources
                .prepare_call([input].into_iter(), holder, guest, context.memory)
                .unwrap();
            let next = context.resources.prepared_claim(transfer, 0).unwrap();
            context
                .resources
                .commit_call(transfer, context.memory, &mut [None])
                .unwrap();
            arguments.committed(&[input]);
            next
        };
        assert!(!context.owners[1].held.get());
        assert!(context.owners[0].held.get());
        assert_eq!(context.resource_read(1).unwrap().length, 8);
        assert!(matches!(context.resource_read(2), Err(7)));
        context
            .resources
            .validate(next, guest, &key, context.memory)
            .unwrap();
        context
            .resources
            .detach(next, guest, context.memory)
            .unwrap()
            .release(context.memory)
            .unwrap();
        context
            .resources
            .detach(a, holder, context.memory)
            .unwrap()
            .release(context.memory)
            .unwrap();
        assert!(context.resources.is_empty());
        drop(context);
        assert_eq!(memory.stats().live_allocations, 0);
    }
}
