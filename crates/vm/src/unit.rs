//! Bounded RFC 0006 register execution with an explicit guest frame stack.
//!
//! Each frame owns one flat, statically charged cell buffer. Aggregate copies,
//! return copies, and parallel block arguments never allocate value buffers.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::time::Instant;

use lang_bytecode::unit::Program;
use lang_mir::{HostFunctionId, ResourceTypeId, unit as m};
use lang_runtime::{HostFunctionRegistry, I64HostFunction};

use crate::InterruptionFlag;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Value {
    value_type: m::Type,
    cells: Vec<u64>,
}
impl Value {
    pub fn i64(value: i64) -> Self {
        Self {
            value_type: m::Type::I64,
            cells: vec![value as u64],
        }
    }
    pub fn boolean(value: bool) -> Self {
        Self {
            value_type: m::Type::Bool,
            cells: vec![u64::from(value)],
        }
    }
    pub fn own_resource(resource_type: ResourceTypeId, index: u32) -> Result<Self, Reason> {
        if index == 0 {
            return Err(Reason::InvalidValue);
        }
        Ok(Self {
            value_type: m::Type::Own(resource_type),
            cells: vec![u64::from(index)],
        })
    }
    /// Flat representation is VM storage, not a stable native or wire ABI.
    /// Records have a zero header; enums have a case index and zero padding
    /// after their active payload up to the largest-case layout.
    pub fn from_cells(
        program: &Program,
        value_type: m::Type,
        cells: Vec<u64>,
    ) -> Result<Self, Reason> {
        validate_value(program, value_type, &cells)?;
        Ok(Self { value_type, cells })
    }
    pub const fn value_type(&self) -> m::Type {
        self.value_type
    }
    pub fn cells(&self) -> &[u64] {
        &self.cells
    }
    pub fn as_i64(&self) -> Option<i64> {
        (self.value_type == m::Type::I64).then(|| self.cells[0] as i64)
    }
    pub fn as_bool(&self) -> Option<bool> {
        (self.value_type == m::Type::Bool).then(|| self.cells[0] != 0)
    }
    pub fn owned_index(&self) -> Option<u32> {
        matches!(self.value_type, m::Type::Own(_)).then(|| self.cells[0] as u32)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Limits {
    pub fuel: Option<u64>,
    pub host_calls: Option<u64>,
    pub frames: Option<usize>,
    pub live_cells: Option<usize>,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            fuel: Some(1_000_000),
            host_calls: Some(1_000),
            frames: Some(256),
            live_cells: Some(1_000_000),
        }
    }
}
impl Limits {
    pub const fn unbounded() -> Self {
        Self {
            fuel: None,
            host_calls: None,
            frames: None,
            live_cells: None,
        }
    }
}
#[derive(Clone, Copy, Debug, Default)]
pub struct Control<'a> {
    limits: Limits,
    interruption: Option<&'a InterruptionFlag>,
    deadline: Option<Instant>,
}
impl<'a> Control<'a> {
    pub const fn with_limits(limits: Limits) -> Self {
        Self {
            limits,
            interruption: None,
            deadline: None,
        }
    }
    pub const fn and_interruption(mut self, flag: &'a InterruptionFlag) -> Self {
        self.interruption = Some(flag);
        self
    }
    pub fn and_deadline(mut self, deadline: Instant) -> Self {
        self.deadline = Some(deadline);
        self
    }
    pub const fn limits(self) -> Limits {
        self.limits
    }
    fn check(self) -> Result<(), Reason> {
        if self
            .interruption
            .is_some_and(InterruptionFlag::is_interrupted)
        {
            return Err(Reason::Interrupted);
        }
        if self
            .deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            return Err(Reason::DeadlineExceeded);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default)]
pub struct HostBindings {
    names: BTreeMap<String, HostFunctionId>,
}
impl HostBindings {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn bind(
        &mut self,
        name: impl Into<String>,
        function: HostFunctionId,
    ) -> Result<(), DuplicateBinding> {
        use std::collections::btree_map::Entry;
        match self.names.entry(name.into()) {
            Entry::Vacant(e) => {
                e.insert(function);
                Ok(())
            }
            Entry::Occupied(_) => Err(DuplicateBinding),
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DuplicateBinding;
impl Display for DuplicateBinding {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("duplicate source host binding")
    }
}
impl Error for DuplicateBinding {}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Usage {
    pub fuel: u64,
    pub host_calls: u64,
    pub peak_frames: usize,
    pub peak_live_cells: usize,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Outcome {
    pub value: Value,
    pub usage: Usage,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Reason {
    MissingEntry,
    InvalidFunction(m::FunctionId),
    Arity {
        expected: usize,
        actual: usize,
    },
    ArgumentType {
        argument: usize,
        expected: m::Type,
        actual: m::Type,
    },
    InvalidValue,
    MissingImport(m::ImportId),
    UnknownHost(HostFunctionId),
    Interrupted,
    DeadlineExceeded,
    FuelExhausted,
    HostCallLimit,
    FrameLimit,
    LiveCellLimit,
    CounterOverflow,
    AllocationFailed,
    InvalidVariantProjection {
        expected: u32,
        actual: u32,
    },
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StackFrame {
    pub function: m::FunctionId,
    pub span: Option<m::Span>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Failure {
    pub reason: Reason,
    pub usage: Usage,
    pub fault: Option<m::Span>,
    pub stack: Vec<StackFrame>,
    /// Only an allocation failure while constructing the error trace can make
    /// this false; the runtime reason and counters are still retained.
    pub trace_complete: bool,
}
impl Display for Failure {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "VM execution failed: {:?}", self.reason)
    }
}
impl Error for Failure {}
fn preflight_failure(reason: Reason) -> Failure {
    Failure {
        reason,
        usage: Usage::default(),
        fault: None,
        stack: Vec::new(),
        trace_complete: true,
    }
}
fn vector<T: Clone>(n: usize, value: T) -> Result<Vec<T>, Reason> {
    let mut out = Vec::new();
    out.try_reserve_exact(n)
        .map_err(|_| Reason::AllocationFailed)?;
    out.resize(n, value);
    Ok(out)
}

fn validate_value(program: &Program, value_type: m::Type, cells: &[u64]) -> Result<(), Reason> {
    let verified = program.verified();
    let u = verified.unit();
    let facts = verified.facts();
    if facts.type_cells(value_type) != Some(cells.len()) {
        return Err(Reason::InvalidValue);
    }
    let mut pending = Vec::new();
    pending
        .try_reserve_exact(cells.len())
        .map_err(|_| Reason::AllocationFailed)?;
    pending.push((value_type, 0usize));
    while let Some((t, start)) = pending.pop() {
        match t {
            m::Type::I64 => {}
            m::Type::Bool => {
                if cells[start] > 1 {
                    return Err(Reason::InvalidValue);
                }
            }
            m::Type::Own(_) => {
                if cells[start] == 0 || cells[start] > u64::from(u32::MAX) {
                    return Err(Reason::InvalidValue);
                }
            }
            m::Type::Record(d) => {
                if cells[start] != 0 {
                    return Err(Reason::InvalidValue);
                }
                let m::DefinitionKind::Record(fields) = &u.definitions[d.0 as usize].kind else {
                    unreachable!()
                };
                for (field, offset) in fields.iter().zip(facts.layouts()[d.0 as usize].fields()) {
                    pending.push((field.value_type, start + offset));
                }
            }
            m::Type::Enum(d) => {
                let m::DefinitionKind::Enum(cases) = &u.definitions[d.0 as usize].kind else {
                    unreachable!()
                };
                let index = usize::try_from(cells[start]).map_err(|_| Reason::InvalidValue)?;
                let case = cases.get(index).ok_or(Reason::InvalidValue)?;
                let layout = &facts.layouts()[d.0 as usize];
                let mut used = 1;
                for (t, offset) in case.payload.iter().zip(&layout.cases()[index]) {
                    pending.push((*t, start + offset));
                    used += facts.type_cells(*t).expect("verified type");
                }
                if cells[start + used..start + layout.cells()]
                    .iter()
                    .any(|c| *c != 0)
                {
                    return Err(Reason::InvalidValue);
                }
            }
        }
    }
    Ok(())
}

pub fn execute_entry(
    program: &Program,
    hosts: &HostFunctionRegistry,
    bindings: &HostBindings,
    control: Control<'_>,
) -> Result<Outcome, Failure> {
    let entry = program
        .verified()
        .unit()
        .entry
        .ok_or_else(|| preflight_failure(Reason::MissingEntry))?;
    execute(program, entry, &[], hosts, bindings, control)
}
pub fn execute(
    program: &Program,
    function: m::FunctionId,
    arguments: &[Value],
    hosts: &HostFunctionRegistry,
    bindings: &HostBindings,
    control: Control<'_>,
) -> Result<Outcome, Failure> {
    let verified = program.verified();
    let f = verified
        .unit()
        .functions
        .get(function.0 as usize)
        .ok_or_else(|| preflight_failure(Reason::InvalidFunction(function)))?;
    if arguments.len() != f.signature.parameters.len() {
        return Err(preflight_failure(Reason::Arity {
            expected: f.signature.parameters.len(),
            actual: arguments.len(),
        }));
    }
    for (index, (argument, t)) in arguments.iter().zip(&f.signature.parameters).enumerate() {
        if argument.value_type != *t {
            return Err(preflight_failure(Reason::ArgumentType {
                argument: index,
                expected: *t,
                actual: argument.value_type,
            }));
        }
        validate_value(program, *t, &argument.cells).map_err(preflight_failure)?;
    }
    let mut callbacks = Vec::new();
    callbacks
        .try_reserve_exact(verified.unit().imports.len())
        .map_err(|_| preflight_failure(Reason::AllocationFailed))?;
    for (index, import) in verified.unit().imports.iter().enumerate() {
        let id = bindings
            .names
            .get(&import.name)
            .ok_or_else(|| preflight_failure(Reason::MissingImport(m::ImportId(index as u32))))?;
        callbacks.push(
            hosts
                .resolve_i64(*id)
                .ok_or_else(|| preflight_failure(Reason::UnknownHost(*id)))?,
        );
    }
    let mut machine = Machine {
        verified,
        callbacks,
        control,
        usage: Usage::default(),
        live_cells: 0,
        frames: Vec::new(),
        fault: None,
    };
    let start = || -> Result<Frame, Reason> {
        machine.prospective(function)?;
        let mut frame = Frame::new(verified, function, None, None)?;
        for (index, value) in arguments.iter().enumerate() {
            let range = frame.range(m::Register(index as u32));
            frame.cells[range].copy_from_slice(&value.cells);
        }
        Ok(frame)
    }();
    let frame = start.map_err(|reason| machine.failure(reason))?;
    machine
        .enter(frame)
        .map_err(|reason| machine.failure(reason))?;
    match machine.run() {
        Ok(value) => Ok(Outcome {
            value,
            usage: machine.usage,
        }),
        Err(reason) => Err(machine.failure(reason)),
    }
}

struct Frame {
    function: m::FunctionId,
    block: m::BlockId,
    pc: usize,
    return_to: Option<m::Register>,
    call_site: Option<m::Span>,
    cells: Vec<u64>,
    offsets: Vec<usize>,
    moves: Vec<Option<m::Register>>,
    readers: Vec<usize>,
    ready: Vec<m::Register>,
}
impl Frame {
    fn new(
        verified: &m::VerifiedUnit,
        function: m::FunctionId,
        return_to: Option<m::Register>,
        call_site: Option<m::Span>,
    ) -> Result<Self, Reason> {
        let f = &verified.unit().functions[function.0 as usize];
        let facts = &verified.facts().functions()[function.0 as usize];
        let mut offsets = Vec::new();
        offsets
            .try_reserve_exact(f.registers.len() + 1)
            .map_err(|_| Reason::AllocationFailed)?;
        offsets.push(0);
        for n in facts.register_cells() {
            offsets.push(offsets.last().unwrap() + n);
        }
        let mut ready = Vec::new();
        ready
            .try_reserve_exact(
                f.blocks
                    .iter()
                    .map(|b| b.parameters.len())
                    .max()
                    .unwrap_or(0),
            )
            .map_err(|_| Reason::AllocationFailed)?;
        Ok(Self {
            function,
            block: m::BlockId(0),
            pc: 0,
            return_to,
            call_site,
            cells: vector(facts.cells(), 0)?,
            offsets,
            moves: vector(f.registers.len(), None)?,
            readers: vector(f.registers.len(), 0)?,
            ready,
        })
    }
    fn range(&self, r: m::Register) -> std::ops::Range<usize> {
        self.offsets[r.0 as usize]..self.offsets[r.0 as usize + 1]
    }
    fn scalar(&self, r: m::Register) -> u64 {
        self.cells[self.offsets[r.0 as usize]]
    }
    fn set_scalar(&mut self, r: m::Register, value: u64) {
        self.cells[self.offsets[r.0 as usize]] = value;
    }
    fn copy(&mut self, source: m::Register, destination: m::Register) {
        let range = self.range(source);
        self.cells
            .copy_within(range, self.offsets[destination.0 as usize]);
    }
    fn jump(&mut self, target: &m::Target, parameters: &[m::Register]) {
        // Parallel copy without aggregate temporaries. Process destinations
        // whose old contents have no remaining readers, then rotate cycles a
        // single cell at a time. Scratch is register metadata, never value cells.
        for (destination, source) in parameters
            .iter()
            .copied()
            .zip(target.arguments.iter().copied())
        {
            if destination != source {
                self.moves[destination.0 as usize] = Some(source);
                self.readers[source.0 as usize] += 1;
            }
        }
        for d in parameters {
            if self.moves[d.0 as usize].is_some() && self.readers[d.0 as usize] == 0 {
                self.ready.push(*d);
            }
        }
        while let Some(destination) = self.ready.pop() {
            let source = self.moves[destination.0 as usize]
                .take()
                .expect("scheduled copy");
            self.copy(source, destination);
            self.readers[source.0 as usize] -= 1;
            if self.readers[source.0 as usize] == 0 && self.moves[source.0 as usize].is_some() {
                self.ready.push(source);
            }
        }
        for destination in parameters {
            if self.moves[destination.0 as usize].is_none() {
                continue;
            }
            for cell in 0..self.range(*destination).len() {
                let temporary = self.cells[self.offsets[destination.0 as usize] + cell];
                let mut current = *destination;
                loop {
                    let source = self.moves[current.0 as usize].expect("copy cycle");
                    let value = if source == *destination {
                        temporary
                    } else {
                        self.cells[self.offsets[source.0 as usize] + cell]
                    };
                    self.cells[self.offsets[current.0 as usize] + cell] = value;
                    if source == *destination {
                        break;
                    }
                    current = source;
                }
            }
            let mut current = *destination;
            loop {
                let source = self.moves[current.0 as usize].take().expect("copy cycle");
                self.readers[source.0 as usize] -= 1;
                if source == *destination {
                    break;
                }
                current = source;
            }
        }
        self.block = target.block;
        self.pc = 0;
    }
}

struct Machine<'a> {
    verified: &'a m::VerifiedUnit,
    callbacks: Vec<I64HostFunction>,
    control: Control<'a>,
    usage: Usage,
    live_cells: usize,
    frames: Vec<Frame>,
    fault: Option<m::Span>,
}
impl Machine<'_> {
    fn failure(&self, reason: Reason) -> Failure {
        let mut stack = Vec::new();
        let trace_complete = stack.try_reserve_exact(self.frames.len()).is_ok();
        if trace_complete {
            let mut span = self.fault;
            for frame in self.frames.iter().rev() {
                stack.push(StackFrame {
                    function: frame.function,
                    span,
                });
                span = frame.call_site;
            }
        }
        Failure {
            reason,
            usage: self.usage,
            fault: self.fault,
            stack,
            trace_complete,
        }
    }
    fn prospective(&self, function: m::FunctionId) -> Result<(), Reason> {
        self.control.check()?;
        let depth = self
            .frames
            .len()
            .checked_add(1)
            .ok_or(Reason::CounterOverflow)?;
        if self.control.limits.frames.is_some_and(|n| depth > n) {
            return Err(Reason::FrameLimit);
        }
        let cells = self
            .live_cells
            .checked_add(self.verified.facts().functions()[function.0 as usize].cells())
            .ok_or(Reason::CounterOverflow)?;
        if self.control.limits.live_cells.is_some_and(|n| cells > n) {
            return Err(Reason::LiveCellLimit);
        }
        Ok(())
    }
    fn enter(&mut self, frame: Frame) -> Result<(), Reason> {
        self.frames
            .try_reserve_exact(1)
            .map_err(|_| Reason::AllocationFailed)?;
        self.live_cells += frame.cells.len();
        self.frames.push(frame);
        self.usage.peak_frames = self.usage.peak_frames.max(self.frames.len());
        self.usage.peak_live_cells = self.usage.peak_live_cells.max(self.live_cells);
        Ok(())
    }
    fn charge(&mut self) -> Result<(), Reason> {
        self.control.check()?;
        if self
            .control
            .limits
            .fuel
            .is_some_and(|n| self.usage.fuel >= n)
        {
            return Err(Reason::FuelExhausted);
        }
        self.usage.fuel = self
            .usage
            .fuel
            .checked_add(1)
            .ok_or(Reason::CounterOverflow)?;
        Ok(())
    }
    fn host_call(&mut self, function: I64HostFunction, argument: i64) -> Result<i64, Reason> {
        self.control.check()?;
        if self
            .control
            .limits
            .host_calls
            .is_some_and(|n| self.usage.host_calls >= n)
        {
            return Err(Reason::HostCallLimit);
        }
        self.usage.host_calls = self
            .usage
            .host_calls
            .checked_add(1)
            .ok_or(Reason::CounterOverflow)?;
        Ok(function(argument))
    }
    fn run(&mut self) -> Result<Value, Reason> {
        loop {
            let frame = self.frames.last().expect("entered frame");
            let f = &self.verified.unit().functions[frame.function.0 as usize];
            let block = &f.blocks[frame.block.0 as usize];
            let instruction = block.instructions.get(frame.pc);
            self.fault = instruction.map_or(block.terminator.span, |i| i.span);
            self.charge()?;
            if let Some(instruction) = instruction {
                if let m::Op::Call {
                    destination,
                    function,
                    arguments,
                } = &instruction.op
                {
                    self.prospective(*function)?;
                    let mut child = Frame::new(
                        self.verified,
                        *function,
                        Some(*destination),
                        instruction.span,
                    )?;
                    let caller = self.frames.last_mut().unwrap();
                    for (index, source) in arguments.iter().enumerate() {
                        let destination = child.range(m::Register(index as u32));
                        child.cells[destination]
                            .copy_from_slice(&caller.cells[caller.range(*source)]);
                    }
                    // A refused call leaves the caller at the faulting call,
                    // and never publishes an unentered child in its trace.
                    self.enter(child)?;
                    let parent = self.frames.len() - 2;
                    self.frames[parent].pc += 1;
                    continue;
                }
                if let m::Op::HostCall {
                    destination,
                    import,
                    argument,
                } = &instruction.op
                {
                    let argument = self.frames.last().unwrap().scalar(*argument) as i64;
                    let result = self.host_call(self.callbacks[import.0 as usize], argument)?;
                    let frame = self.frames.last_mut().unwrap();
                    frame.set_scalar(*destination, result as u64);
                    frame.pc += 1;
                    continue;
                }
                let frame = self.frames.last_mut().unwrap();
                match &instruction.op {
                    m::Op::I64Constant { destination, value } => {
                        frame.set_scalar(*destination, *value as u64)
                    }
                    m::Op::BoolConstant { destination, value } => {
                        frame.set_scalar(*destination, u64::from(*value))
                    }
                    m::Op::Add {
                        destination,
                        left,
                        right,
                    } => frame.set_scalar(
                        *destination,
                        frame.scalar(*left).wrapping_add(frame.scalar(*right)),
                    ),
                    m::Op::LessThan {
                        destination,
                        left,
                        right,
                    } => frame.set_scalar(
                        *destination,
                        u64::from((frame.scalar(*left) as i64) < (frame.scalar(*right) as i64)),
                    ),
                    m::Op::EqualI64 {
                        destination,
                        left,
                        right,
                    } => frame.set_scalar(
                        *destination,
                        u64::from(frame.scalar(*left) == frame.scalar(*right)),
                    ),
                    m::Op::Record {
                        destination,
                        definition,
                        fields,
                    } => {
                        let start = frame.offsets[destination.0 as usize];
                        let range = frame.range(*destination);
                        frame.cells[range].fill(0);
                        for (field, offset) in fields
                            .iter()
                            .zip(self.verified.facts().layouts()[definition.0 as usize].fields())
                        {
                            let range = frame.range(*field);
                            frame.cells.copy_within(range, start + offset);
                        }
                    }
                    m::Op::Enum {
                        destination,
                        definition,
                        case,
                        payload,
                    } => {
                        let start = frame.offsets[destination.0 as usize];
                        let range = frame.range(*destination);
                        frame.cells[range].fill(0);
                        frame.cells[start] = u64::from(*case);
                        for (field, offset) in payload.iter().zip(
                            &self.verified.facts().layouts()[definition.0 as usize].cases()
                                [*case as usize],
                        ) {
                            let range = frame.range(*field);
                            frame.cells.copy_within(range, start + offset);
                        }
                    }
                    m::Op::Field {
                        destination,
                        receiver,
                        field,
                    } => {
                        let m::Type::Record(d) = f.registers[receiver.0 as usize] else {
                            unreachable!()
                        };
                        let start = frame.offsets[receiver.0 as usize]
                            + self.verified.facts().layouts()[d.0 as usize].fields()
                                [*field as usize];
                        let len = frame.range(*destination).len();
                        frame
                            .cells
                            .copy_within(start..start + len, frame.offsets[destination.0 as usize]);
                    }
                    m::Op::IsCase {
                        destination,
                        receiver,
                        case,
                    } => frame.set_scalar(
                        *destination,
                        u64::from(frame.scalar(*receiver) == u64::from(*case)),
                    ),
                    m::Op::EnumField {
                        destination,
                        receiver,
                        case,
                        field,
                    } => {
                        let actual = frame.scalar(*receiver) as u32;
                        if actual != *case {
                            return Err(Reason::InvalidVariantProjection {
                                expected: *case,
                                actual,
                            });
                        }
                        let m::Type::Enum(d) = f.registers[receiver.0 as usize] else {
                            unreachable!()
                        };
                        let start = frame.offsets[receiver.0 as usize]
                            + self.verified.facts().layouts()[d.0 as usize].cases()[*case as usize]
                                [*field as usize];
                        let len = frame.range(*destination).len();
                        frame
                            .cells
                            .copy_within(start..start + len, frame.offsets[destination.0 as usize]);
                    }
                    m::Op::Call { .. } | m::Op::HostCall { .. } => unreachable!(),
                }
                frame.pc += 1;
            } else {
                match &block.terminator.kind {
                    m::Term::Return(result) => {
                        if self.frames.len() == 1 {
                            let mut frame = self.frames.pop().unwrap();
                            let range = frame.range(*result);
                            let len = range.len();
                            frame.cells.copy_within(range, 0);
                            frame.cells.truncate(len);
                            self.live_cells = 0;
                            return Ok(Value {
                                value_type: f.signature.result,
                                cells: frame.cells,
                            });
                        }
                        let last = self.frames.len() - 1;
                        let (parents, child) = self.frames.split_at_mut(last);
                        let caller = parents.last_mut().unwrap();
                        let child = &child[0];
                        let destination =
                            caller.range(child.return_to.expect("guest return destination"));
                        caller.cells[destination]
                            .copy_from_slice(&child.cells[child.range(*result)]);
                        self.live_cells -= child.cells.len();
                        self.frames.pop();
                    }
                    m::Term::Jump(target) => {
                        let frame = self.frames.last_mut().unwrap();
                        frame.jump(target, &f.blocks[target.block.0 as usize].parameters);
                    }
                    m::Term::Branch {
                        condition,
                        then_target,
                        else_target,
                    } => {
                        let frame = self.frames.last_mut().unwrap();
                        let target = if frame.scalar(*condition) != 0 {
                            then_target
                        } else {
                            else_target
                        };
                        frame.jump(target, &f.blocks[target.block.0 as usize].parameters);
                    }
                }
            }
        }
    }
}
