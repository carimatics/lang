use super::io::{Reader, Writer};
use super::*;

macro_rules! tags {
    ($name:ident {$($variant:ident=$tag:literal),+ $(,)?}) => {
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        #[repr(u8)]
        pub enum $name {$($variant=$tag),+}
        impl $name {
            fn read(r:&mut Reader<'_, '_>)->Result<Self,Error> {
                let at=r.at;
                match r.u8()? {$($tag=>Ok(Self::$variant),)+_=>Err(r.session.error(at,ErrorKind::Tag))}
            }
        }
    };
}
tags!(MessagePolicy {Redacted=0, Sanitized=1});
tags!(Category {Panic=0, Trap=1, Destructor=2, Cancellation=3, Interruption=4, Limit=5, Admission=6, HostContract=7});
tags!(Phase {Call=0, Cleanup=1, Admission=2});
tags!(Mode {Vm=0, Jit=1, Aot=2, Host=3, Adapter=4});
tags!(ReportLimitKind {
    Fuel=0, ServiceWork=1, HostCalls=2, HeapBytes=3, LiveAllocations=4,
    Frames=5, LiveCells=6, LiveTasks=7, CanonicalIndices=8, PreparationWork=9,
    MetadataBytes=10, ValueElements=11, SerializationBytes=12, Depth=13,
    DecoderAllocationBytes=14, SuppressedRecords=15, TraceFrames=16
});
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Diagnostic {
    InvalidRequest,
    UnsupportedVersion,
    MissingCapability,
    IncompatibleInterface,
    InvalidMetadata,
    InvalidWire,
    Resource,
    Reentry,
    UnsupportedProfile,
    UnknownExport,
    InstancePoisoned,
    Limit,
    Cancelled,
    Interrupted,
    HostContract,
    Panic,
    Trap,
    Destructor,
}
impl Diagnostic {
    /// Fixed C admission identity from RFC 0027, independent of enum order.
    pub const fn c_id(self) -> u32 {
        match self {
            Self::InvalidRequest => 1,
            Self::UnsupportedVersion => 2,
            Self::MissingCapability => 3,
            Self::IncompatibleInterface => 4,
            Self::InvalidMetadata => 5,
            Self::InvalidWire => 6,
            Self::Resource => 7,
            Self::Reentry => 8,
            Self::UnsupportedProfile => 9,
            Self::UnknownExport => 10,
            Self::InstancePoisoned => 11,
            Self::Limit => 12,
            Self::Cancelled => 13,
            Self::Interrupted => 14,
            Self::HostContract => 15,
            Self::Panic => 16,
            Self::Trap => 17,
            Self::Destructor => 18,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidRequest => "component.invalid-request",
            Self::UnsupportedVersion => "component.unsupported-version",
            Self::MissingCapability => "component.missing-capability",
            Self::IncompatibleInterface => "component.incompatible-interface",
            Self::InvalidMetadata => "component.invalid-metadata",
            Self::InvalidWire => "component.invalid-wire",
            Self::Resource => "component.resource",
            Self::Reentry => "component.reentry",
            Self::UnsupportedProfile => "component.unsupported-profile",
            Self::UnknownExport => "component.unknown-export",
            Self::InstancePoisoned => "component.instance-poisoned",
            Self::Limit => "component.limit",
            Self::Cancelled => "component.call.cancelled",
            Self::Interrupted => "component.call.interrupted",
            Self::HostContract => "component.host-contract",
            Self::Panic => "component.boundary.panic",
            Self::Trap => "component.boundary.trap",
            Self::Destructor => "component.boundary.destructor-failure",
        }
    }
    fn parse(s: &str) -> Option<Self> {
        const ALL: [Diagnostic; 18] = [
            Diagnostic::InvalidRequest,
            Diagnostic::UnsupportedVersion,
            Diagnostic::MissingCapability,
            Diagnostic::IncompatibleInterface,
            Diagnostic::InvalidMetadata,
            Diagnostic::InvalidWire,
            Diagnostic::Resource,
            Diagnostic::Reentry,
            Diagnostic::UnsupportedProfile,
            Diagnostic::UnknownExport,
            Diagnostic::InstancePoisoned,
            Diagnostic::Limit,
            Diagnostic::Cancelled,
            Diagnostic::Interrupted,
            Diagnostic::HostContract,
            Diagnostic::Panic,
            Diagnostic::Trap,
            Diagnostic::Destructor,
        ];
        ALL.into_iter().find(|d| d.as_str() == s)
    }
    fn matches(self, category: Category, phase: Phase) -> bool {
        match self {
            Self::Limit => {
                category == Category::Limit && matches!(phase, Phase::Call | Phase::Admission)
            }
            Self::Cancelled => category == Category::Cancellation && phase == Phase::Call,
            Self::Interrupted => category == Category::Interruption && phase == Phase::Call,
            Self::HostContract => {
                category == Category::HostContract && matches!(phase, Phase::Call | Phase::Cleanup)
            }
            Self::Panic => category == Category::Panic && phase == Phase::Call,
            Self::Trap => category == Category::Trap && phase == Phase::Call,
            Self::Destructor => category == Category::Destructor && phase == Phase::Cleanup,
            _ => category == Category::Admission && phase == Phase::Admission,
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Frame {
    pub mode: Mode,
    pub digest: [u8; 32],
    pub member: u32,
    pub span: Option<m::Span>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LimitDetail {
    pub kind: ReportLimitKind,
    pub limit: u64,
    pub attempted: u64,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Record {
    pub category: Category,
    pub phase: Phase,
    pub diagnostic: Diagnostic,
    /// Sanitization is the embedding policy's responsibility before construction.
    pub message: Option<String>,
    pub limit: Option<LimitDetail>,
    pub origin: Option<Frame>,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ReportUsage {
    pub fuel: u64,
    pub service_work: u64,
    pub host_calls: u64,
    pub peak_heap_bytes: u64,
    pub peak_live_allocations: u64,
    pub peak_frames: u64,
    pub peak_live_cells: u64,
    pub peak_live_tasks: u64,
    pub canonical_indices: u64,
}
impl ReportUsage {
    fn array(self) -> [u64; 9] {
        [
            self.fuel,
            self.service_work,
            self.host_calls,
            self.peak_heap_bytes,
            self.peak_live_allocations,
            self.peak_frames,
            self.peak_live_cells,
            self.peak_live_tasks,
            self.canonical_indices,
        ]
    }
    fn from_array(a: [u64; 9]) -> Self {
        Self {
            fuel: a[0],
            service_work: a[1],
            host_calls: a[2],
            peak_heap_bytes: a[3],
            peak_live_allocations: a[4],
            peak_frames: a[5],
            peak_live_cells: a[6],
            peak_live_tasks: a[7],
            canonical_indices: a[8],
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Report {
    pub policy: MessagePolicy,
    pub primary: Record,
    pub suppressed: Vec<Record>,
    pub poisoned: bool,
    pub omitted: u64,
    pub usage: ReportUsage,
    pub frames: Vec<Frame>,
}
impl Report {
    fn validate(&self, s: &mut Session) -> Result<(), Error> {
        fn frame(f: &Frame) -> bool {
            f.span.is_none_or(|s| s.start <= s.end)
        }
        for (record, suppressed) in
            std::iter::once((&self.primary, false)).chain(self.suppressed.iter().map(|r| (r, true)))
        {
            s.charge(1, 0)?;
            let poison = matches!(
                record.category,
                Category::Destructor | Category::HostContract
            ) || record.diagnostic == Diagnostic::InstancePoisoned
                || (record.category == Category::Panic
                    && record.origin.as_ref().is_some_and(|f| f.mode == Mode::Host));
            if !record.diagnostic.matches(record.category, record.phase)
                || (suppressed && record.phase != Phase::Cleanup)
                || (self.policy == MessagePolicy::Redacted && record.message.is_some())
                || (record.category == Category::Limit) != record.limit.is_some()
                || (poison && !self.poisoned)
                || record.origin.as_ref().is_some_and(|f| !frame(f))
            {
                return Err(s.error(0, ErrorKind::InvalidReport));
            }
        }
        for f in &self.frames {
            s.charge(1, 0)?;
            if !frame(f) {
                return Err(s.error(0, ErrorKind::InvalidReport));
            }
        }
        Ok(())
    }
}
fn depth(s: &Session, n: usize, at: usize) -> Result<(), Error> {
    s.cap(n, s.limits.depth, LimitKind::Depth, at)
}
fn write_frame(w: &mut Writer<'_>, f: &Frame) -> Result<(), Error> {
    depth(w.session, 3, w.at())?;
    w.u8(f.mode as u8)?;
    w.raw(&f.digest)?;
    w.u32(f.member)?;
    w.u8(u8::from(f.span.is_some()))?;
    if let Some(span) = f.span {
        depth(w.session, 4, w.at())?;
        w.u32(span.source.0)?;
        w.u32(span.start)?;
        w.u32(span.end)?;
    }
    Ok(())
}
fn read_frame(r: &mut Reader<'_, '_>) -> Result<Frame, Error> {
    depth(r.session, 3, r.at)?;
    let mode = Mode::read(r)?;
    let digest = r.array()?;
    let member = r.u32()?;
    let span = if r.boolean()? {
        depth(r.session, 4, r.at)?;
        Some(m::Span {
            source: m::SourceId(r.u32()?),
            start: r.u32()?,
            end: r.u32()?,
        })
    } else {
        None
    };
    Ok(Frame {
        mode,
        digest,
        member,
        span,
    })
}
fn write_record(w: &mut Writer<'_>, r: &Record) -> Result<(), Error> {
    depth(w.session, 2, w.at())?;
    w.u8(r.category as u8)?;
    w.u8(r.phase as u8)?;
    w.string(r.diagnostic.as_str())?;
    w.u8(u8::from(r.message.is_some()))?;
    if let Some(s) = &r.message {
        w.string(s)?;
    }
    w.u8(u8::from(r.limit.is_some()))?;
    if let Some(l) = r.limit {
        w.u8(l.kind as u8)?;
        w.u64(l.limit)?;
        w.u64(l.attempted)?;
    }
    w.u8(u8::from(r.origin.is_some()))?;
    if let Some(f) = &r.origin {
        w.session.count(1, w.at())?;
        write_frame(w, f)?;
    }
    Ok(())
}
fn read_record(r: &mut Reader<'_, '_>) -> Result<Record, Error> {
    depth(r.session, 2, r.at)?;
    let category = Category::read(r)?;
    let phase = Phase::read(r)?;
    let at = r.at;
    let name = r.string()?;
    r.session
        .charge(18usize.saturating_mul(name.len().min(37) + 1), at)?;
    let diagnostic =
        Diagnostic::parse(&name).ok_or_else(|| r.session.error(at, ErrorKind::InvalidReport))?;
    let message = if r.boolean()? {
        Some(r.string()?)
    } else {
        None
    };
    let limit = if r.boolean()? {
        Some(LimitDetail {
            kind: ReportLimitKind::read(r)?,
            limit: r.u64()?,
            attempted: r.u64()?,
        })
    } else {
        None
    };
    let origin = if r.boolean()? {
        r.session.count(1, r.at)?;
        Some(read_frame(r)?)
    } else {
        None
    };
    Ok(Record {
        category,
        phase,
        diagnostic,
        message,
        limit,
        origin,
    })
}
pub fn encode_report(report: &Report, session: &mut Session) -> Result<Vec<u8>, Error> {
    depth(session, 1, 0)?;
    report.validate(session)?;
    session.count(1, 0)?;
    let mut w = Writer::new(session);
    w.header(b"LCMP", 3, 1)?;
    w.u8(report.policy as u8)?;
    write_record(&mut w, &report.primary)?;
    w.count(report.suppressed.len())?;
    for r in &report.suppressed {
        write_record(&mut w, r)?;
    }
    w.u8(u8::from(report.poisoned))?;
    w.u64(report.omitted)?;
    for n in report.usage.array() {
        w.u64(n)?;
    }
    w.count(report.frames.len())?;
    for f in &report.frames {
        write_frame(&mut w, f)?;
    }
    Ok(w.bytes)
}
pub fn decode_report(bytes: &[u8], session: &mut Session) -> Result<Report, Error> {
    depth(session, 1, 0)?;
    session.count(1, 0)?;
    let mut r = Reader::new(bytes, session)?;
    r.header(b"LCMP", 3, 1)?;
    let policy = MessagePolicy::read(&mut r)?;
    let primary = read_record(&mut r)?;
    // The shortest valid cleanup diagnostic is component.host-contract:
    // category/phase + length/name + three absent optional fields = 32 bytes.
    let n = r.count(32)?;
    let mut suppressed = r.session.vector(n, r.at)?;
    for _ in 0..n {
        suppressed.push(read_record(&mut r)?);
    }
    let poisoned = r.boolean()?;
    let omitted = r.u64()?;
    let mut usage = [0; 9];
    for n in &mut usage {
        *n = r.u64()?;
    }
    let n = r.count(38)?;
    let mut frames = r.session.vector(n, r.at)?;
    for _ in 0..n {
        frames.push(read_frame(&mut r)?);
    }
    r.finish()?;
    let report = Report {
        policy,
        primary,
        suppressed,
        poisoned,
        omitted,
        usage: ReportUsage::from_array(usage),
        frames,
    };
    report.validate(r.session)?;
    Ok(report)
}
