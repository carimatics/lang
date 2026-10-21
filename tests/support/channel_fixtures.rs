use lang_runtime::{allocation as a, execution as e};

pub(crate) const MANAGED_CHANNEL_FIXTURES: &[(&str, &str)] = &[
    (
        "copy-unit",
        include_str!("../fixtures/task_channel_copy_unit.lang"),
    ),
    (
        "copy-option",
        include_str!("../fixtures/task_channel_copy_sum.lang"),
    ),
    (
        "copy-aggregate",
        include_str!("../fixtures/task_channel_copy.lang"),
    ),
    (
        "copy-cancel",
        include_str!("../fixtures/task_channel_copy_cancel.lang"),
    ),
    (
        "copy-recover",
        include_str!("../fixtures/task_channel_copy_recover.lang"),
    ),
    (
        "self-cycle",
        include_str!("../fixtures/task_channel_self_cycle.lang"),
    ),
    (
        "reinspect",
        include_str!("../fixtures/task_channel_reinspect.lang"),
    ),
    ("mixed", include_str!("../fixtures/task_channel_mixed.lang")),
    (
        "shared-sender",
        include_str!("../fixtures/task_channel_shared_sender.lang"),
    ),
    (
        "mixed-cancel",
        include_str!("../fixtures/task_channel_mixed_cancel.lang"),
    ),
    ("text", include_str!("../fixtures/task_channel_text.lang")),
];

#[derive(Default)]
pub(crate) struct ChannelTrace {
    pub(crate) calls: usize,
    pub(crate) live: std::collections::BTreeSet<usize>,
    pub(crate) buffers: Vec<u8>,
    pub(crate) panic_buffers: Vec<u8>,
    pub(crate) interrupt: Option<(usize, std::rc::Rc<e::InterruptionFlag>)>,
}

pub(crate) fn copy_quota_source(service: bool) -> String {
    let text = if service {
        include_str!("../fixtures/task_service_copy_recover.lang")
    } else {
        include_str!("../fixtures/task_channel_copy_recover.lang")
    };
    format!(
        "{}async fn main()->i64{{return core::result_unwrap::<i64,core::TaskError>(await recovery(false));}}",
        text.split("async fn main()").next().unwrap()
    )
}
pub(crate) struct ChannelRegion {
    id: usize,
    storage: a::Storage,
}
impl a::Region for ChannelRegion {
    fn bytes(&self) -> &[u8] {
        self.storage.bytes()
    }
    fn bytes_mut(&mut self) -> &mut [u8] {
        self.storage.bytes_mut()
    }
}
pub(crate) struct ChannelProvider {
    refuse: usize,
    trace: std::rc::Rc<std::cell::RefCell<ChannelTrace>>,
}
impl a::Provider for ChannelProvider {
    type Region = ChannelRegion;
    fn allocate(&mut self, request: a::Request) -> Option<ChannelRegion> {
        let id = {
            let mut trace = self.trace.borrow_mut();
            trace.calls += 1;
            trace.calls
        };
        if self.refuse == id {
            return None;
        }
        let storage = a::Storage::try_new(request).ok()?;
        let mut trace = self.trace.borrow_mut();
        assert!(trace.live.insert(id));
        if let Some((at, flag)) = &trace.interrupt
            && *at == id
        {
            flag.interrupt();
        }
        Some(ChannelRegion { id, storage })
    }
    fn deallocate(&mut self, region: ChannelRegion, request: a::Request) {
        use a::Region as _;
        let mut trace = self.trace.borrow_mut();
        assert!(trace.live.remove(&region.id), "duplicate guest release");
        let data = region.storage.bytes();
        let byte = (data.len() == 25
            && data[..8] == 1u64.to_le_bytes()
            && data[8..16] == 1u64.to_le_bytes()
            && data[16..24] == [0; 8])
            .then(|| data[24]);
        let panic = byte.is_some_and(|byte| trace.panic_buffers.contains(&byte));
        if let Some(byte) = byte {
            trace.buffers.push(byte);
        }
        a::System.deallocate(region.storage, request);
        assert!(!panic, "contained mixed channel payload panic");
    }
}
pub(crate) fn channel_allocator(
    refuse: usize,
) -> (
    a::Allocator<ChannelProvider>,
    std::rc::Rc<std::cell::RefCell<ChannelTrace>>,
) {
    let trace = std::rc::Rc::new(std::cell::RefCell::new(ChannelTrace::default()));
    let allocator = a::Allocator::new(
        ChannelProvider {
            refuse,
            trace: trace.clone(),
        },
        Default::default(),
    )
    .unwrap();
    (allocator, trace)
}
