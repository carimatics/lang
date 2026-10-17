#![forbid(unsafe_code)]

pub mod unit;

use std::error::Error;
use std::fmt::{Display, Formatter};
use std::num::NonZeroU32;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use lang_bytecode::{BlockId, BlockTarget, Instruction, Program, Terminator, VerificationError};
use lang_mir::{HostFunctionId, ResourceTypeId, Type};
use lang_runtime::HostFunctionRegistry;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VmValue {
    I64(i64),
    Bool(bool),
    OwnResource {
        resource_type: ResourceTypeId,
        index: NonZeroU32,
    },
}

impl VmValue {
    pub fn own_resource(resource_type: ResourceTypeId, index: u32) -> Result<Self, VmValueError> {
        let Some(index) = NonZeroU32::new(index) else {
            return Err(VmValueError::InvalidOwnedResourceIndex(index));
        };
        Ok(Self::OwnResource {
            resource_type,
            index,
        })
    }

    pub const fn value_type(self) -> Type {
        match self {
            Self::I64(_) => Type::I64,
            Self::Bool(_) => Type::Bool,
            Self::OwnResource { resource_type, .. } => Type::Own(resource_type),
        }
    }

    pub const fn owned_resource_index(self) -> Option<u32> {
        match self {
            Self::OwnResource { index, .. } => Some(index.get()),
            Self::I64(_) | Self::Bool(_) => None,
        }
    }

    const fn raw(self) -> i64 {
        match self {
            Self::I64(value) => value,
            Self::Bool(value) => value as i64,
            Self::OwnResource { index, .. } => index.get() as i64,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VmValueError {
    InvalidOwnedResourceIndex(u32),
}

impl Display for VmValueError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidOwnedResourceIndex(index) => {
                write!(formatter, "owned resource index {index} is invalid")
            }
        }
    }
}

impl Error for VmValueError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExecutionLimits {
    instruction_fuel: Option<u64>,
    host_call_budget: Option<u64>,
    value_slot_limit: Option<usize>,
}

impl ExecutionLimits {
    pub const fn unbounded() -> Self {
        Self {
            instruction_fuel: None,
            host_call_budget: None,
            value_slot_limit: None,
        }
    }

    pub const fn with_instruction_fuel(instruction_fuel: u64) -> Self {
        Self {
            instruction_fuel: Some(instruction_fuel),
            host_call_budget: None,
            value_slot_limit: None,
        }
    }

    pub const fn with_host_call_budget(host_call_budget: u64) -> Self {
        Self {
            instruction_fuel: None,
            host_call_budget: Some(host_call_budget),
            value_slot_limit: None,
        }
    }

    pub const fn with_value_slot_limit(value_slot_limit: usize) -> Self {
        Self {
            instruction_fuel: None,
            host_call_budget: None,
            value_slot_limit: Some(value_slot_limit),
        }
    }

    pub const fn and_host_call_budget(mut self, host_call_budget: u64) -> Self {
        self.host_call_budget = Some(host_call_budget);
        self
    }

    pub const fn and_value_slot_limit(mut self, value_slot_limit: usize) -> Self {
        self.value_slot_limit = Some(value_slot_limit);
        self
    }

    pub const fn instruction_fuel(self) -> Option<u64> {
        self.instruction_fuel
    }

    pub const fn host_call_budget(self) -> Option<u64> {
        self.host_call_budget
    }

    pub const fn value_slot_limit(self) -> Option<usize> {
        self.value_slot_limit
    }
}

impl Default for ExecutionLimits {
    fn default() -> Self {
        Self::unbounded()
    }
}

#[derive(Debug, Default)]
pub struct InterruptionFlag {
    interrupted: AtomicBool,
}

impl InterruptionFlag {
    pub const fn new() -> Self {
        Self {
            interrupted: AtomicBool::new(false),
        }
    }

    pub fn interrupt(&self) {
        self.interrupted.store(true, Ordering::Release);
    }

    pub fn is_interrupted(&self) -> bool {
        self.interrupted.load(Ordering::Acquire)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ExecutionControl<'a> {
    limits: ExecutionLimits,
    interruption: Option<&'a InterruptionFlag>,
    deadline: Option<Instant>,
}

impl ExecutionControl<'_> {
    pub const fn unbounded() -> Self {
        Self {
            limits: ExecutionLimits::unbounded(),
            interruption: None,
            deadline: None,
        }
    }

    pub const fn with_limits(limits: ExecutionLimits) -> Self {
        Self {
            limits,
            interruption: None,
            deadline: None,
        }
    }

    pub const fn limits(self) -> ExecutionLimits {
        self.limits
    }
}

impl<'a> ExecutionControl<'a> {
    pub const fn and_interruption(mut self, interruption: &'a InterruptionFlag) -> Self {
        self.interruption = Some(interruption);
        self
    }

    pub const fn interruption(self) -> Option<&'a InterruptionFlag> {
        self.interruption
    }

    pub fn and_deadline(mut self, deadline: Instant) -> Self {
        self.deadline = Some(deadline);
        self
    }

    pub fn deadline(self) -> Option<Instant> {
        self.deadline
    }
}

impl Default for ExecutionControl<'_> {
    fn default() -> Self {
        Self::unbounded()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExecutionOutcome {
    value: i64,
    instruction_fuel_consumed: u64,
    host_calls_consumed: u64,
    value_slots_used: usize,
}

impl ExecutionOutcome {
    pub const fn value(self) -> i64 {
        self.value
    }

    pub const fn instruction_fuel_consumed(self) -> u64 {
        self.instruction_fuel_consumed
    }

    pub const fn host_calls_consumed(self) -> u64 {
        self.host_calls_consumed
    }

    pub const fn value_slots_used(self) -> usize {
        self.value_slots_used
    }
}

pub fn execute(program: &Program, arguments: &[i64]) -> Result<i64, VmError> {
    execute_with_control(program, arguments, ExecutionControl::unbounded())
        .map(ExecutionOutcome::value)
}

pub fn execute_typed(program: &Program, arguments: &[VmValue]) -> Result<VmValue, VmError> {
    program.verify().map_err(VmError::InvalidBytecode)?;
    if arguments.len() != program.parameter_count() {
        return Err(VmError::Arity {
            expected: program.parameter_count(),
            actual: arguments.len(),
        });
    }
    for (argument, (actual, &expected)) in arguments
        .iter()
        .copied()
        .map(VmValue::value_type)
        .zip(program.parameter_types())
        .enumerate()
    {
        if actual != expected {
            return Err(VmError::ArgumentType {
                argument,
                expected,
                actual,
            });
        }
    }

    let raw_arguments = arguments
        .iter()
        .copied()
        .map(VmValue::raw)
        .collect::<Vec<_>>();
    let outcome = execute_raw_with_hosts_and_control(
        program,
        &raw_arguments,
        &HostFunctionRegistry::new(),
        ExecutionControl::unbounded(),
    )?;
    decode_typed_result(program.result_type(), outcome.value())
}

pub fn execute_with_limits(
    program: &Program,
    arguments: &[i64],
    limits: ExecutionLimits,
) -> Result<ExecutionOutcome, VmError> {
    execute_with_control(program, arguments, ExecutionControl::with_limits(limits))
}

pub fn execute_with_control(
    program: &Program,
    arguments: &[i64],
    control: ExecutionControl<'_>,
) -> Result<ExecutionOutcome, VmError> {
    execute_with_hosts_and_control(program, arguments, &HostFunctionRegistry::new(), control)
}

pub fn execute_with_hosts(
    program: &Program,
    arguments: &[i64],
    host_functions: &HostFunctionRegistry,
) -> Result<i64, VmError> {
    execute_with_hosts_and_control(
        program,
        arguments,
        host_functions,
        ExecutionControl::unbounded(),
    )
    .map(ExecutionOutcome::value)
}

pub fn execute_with_hosts_and_limits(
    program: &Program,
    arguments: &[i64],
    host_functions: &HostFunctionRegistry,
    limits: ExecutionLimits,
) -> Result<ExecutionOutcome, VmError> {
    execute_with_hosts_and_control(
        program,
        arguments,
        host_functions,
        ExecutionControl::with_limits(limits),
    )
}

pub fn execute_with_hosts_and_control(
    program: &Program,
    arguments: &[i64],
    host_functions: &HostFunctionRegistry,
    control: ExecutionControl<'_>,
) -> Result<ExecutionOutcome, VmError> {
    if program
        .parameter_types()
        .iter()
        .any(|value_type| *value_type != Type::I64)
        || program.result_type() != Type::I64
    {
        return Err(VmError::TypedEntryPointRequired);
    }
    execute_raw_with_hosts_and_control(program, arguments, host_functions, control)
}

fn execute_raw_with_hosts_and_control(
    program: &Program,
    arguments: &[i64],
    host_functions: &HostFunctionRegistry,
    control: ExecutionControl<'_>,
) -> Result<ExecutionOutcome, VmError> {
    program.verify().map_err(VmError::InvalidBytecode)?;

    if arguments.len() != program.parameter_count() {
        return Err(VmError::Arity {
            expected: program.parameter_count(),
            actual: arguments.len(),
        });
    }

    if let Some(limit) = control.limits().value_slot_limit()
        && program.register_count() > limit
    {
        return Err(VmError::ValueSlotLimitExceeded {
            limit,
            required: program.register_count(),
        });
    }

    let mut meter = ExecutionMeter::new(control);
    meter.check_control()?;
    let mut registers = vec![0; program.register_count()];
    registers[..arguments.len()].copy_from_slice(arguments);

    let mut current_block = BlockId::ENTRY;
    loop {
        let block = &program.blocks()[current_block.index()];
        for instruction in block.instructions() {
            meter.consume_instruction()?;
            match *instruction {
                Instruction::I64Constant { destination, value } => {
                    registers[destination] = value;
                }
                Instruction::I64AddWrapping {
                    destination,
                    left,
                    right,
                } => {
                    registers[destination] = registers[left].wrapping_add(registers[right]);
                }
                Instruction::I64LessThanSigned {
                    destination,
                    left,
                    right,
                } => {
                    registers[destination] = i64::from(registers[left] < registers[right]);
                }
                Instruction::CallHostI64 {
                    destination,
                    function,
                    argument,
                } => {
                    let callback = host_functions
                        .resolve_i64(function)
                        .ok_or(VmError::UnknownHostFunction(function))?;
                    meter.consume_host_call()?;
                    registers[destination] = callback(registers[argument]);
                }
            }
        }

        meter.consume_instruction()?;
        match block.terminator() {
            Terminator::Return(register) => {
                return Ok(ExecutionOutcome {
                    value: registers[*register],
                    instruction_fuel_consumed: meter.instruction_fuel_consumed,
                    host_calls_consumed: meter.host_calls_consumed,
                    value_slots_used: program.register_count(),
                });
            }
            Terminator::Jump(target) => {
                current_block = transfer(target, program, &mut registers);
            }
            Terminator::Branch {
                condition,
                then_target,
                else_target,
            } => {
                let target = if registers[*condition] != 0 {
                    then_target
                } else {
                    else_target
                };
                current_block = transfer(target, program, &mut registers);
            }
        }
    }
}

fn decode_typed_result(result_type: Type, value: i64) -> Result<VmValue, VmError> {
    match result_type {
        Type::I64 => Ok(VmValue::I64(value)),
        Type::Bool => Ok(VmValue::Bool(value != 0)),
        Type::Own(resource_type) => {
            let index = u32::try_from(value)
                .ok()
                .and_then(NonZeroU32::new)
                .ok_or(VmError::InvalidOwnedResourceIndex(value))?;
            Ok(VmValue::OwnResource {
                resource_type,
                index,
            })
        }
    }
}

fn transfer(target: &BlockTarget, program: &Program, registers: &mut [i64]) -> BlockId {
    let arguments = target
        .arguments()
        .iter()
        .map(|&register| registers[register])
        .collect::<Vec<_>>();
    let parameters = program.blocks()[target.block().index()].parameters();
    for (parameter, argument) in parameters.iter().zip(arguments) {
        registers[parameter.register()] = argument;
    }
    target.block()
}

struct ExecutionMeter<'a> {
    interruption: Option<&'a InterruptionFlag>,
    deadline: Option<Instant>,
    instruction_fuel_limit: Option<u64>,
    instruction_fuel_consumed: u64,
    host_call_limit: Option<u64>,
    host_calls_consumed: u64,
}

impl<'a> ExecutionMeter<'a> {
    fn new(control: ExecutionControl<'a>) -> Self {
        Self {
            interruption: control.interruption(),
            deadline: control.deadline(),
            instruction_fuel_limit: control.limits().instruction_fuel(),
            instruction_fuel_consumed: 0,
            host_call_limit: control.limits().host_call_budget(),
            host_calls_consumed: 0,
        }
    }

    fn check_control(&self) -> Result<(), VmError> {
        if self
            .interruption
            .is_some_and(InterruptionFlag::is_interrupted)
        {
            return Err(VmError::Interrupted {
                instruction_fuel_consumed: self.instruction_fuel_consumed,
                host_calls_consumed: self.host_calls_consumed,
            });
        }
        if self
            .deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            return Err(VmError::DeadlineExceeded {
                instruction_fuel_consumed: self.instruction_fuel_consumed,
                host_calls_consumed: self.host_calls_consumed,
            });
        }
        Ok(())
    }

    fn consume_instruction(&mut self) -> Result<(), VmError> {
        self.check_control()?;
        if self
            .instruction_fuel_limit
            .is_some_and(|limit| self.instruction_fuel_consumed >= limit)
        {
            return Err(VmError::InstructionFuelExhausted {
                limit: self
                    .instruction_fuel_limit
                    .expect("fuel exhaustion requires a limit"),
                consumed: self.instruction_fuel_consumed,
            });
        }
        self.instruction_fuel_consumed += 1;
        Ok(())
    }

    fn consume_host_call(&mut self) -> Result<(), VmError> {
        self.check_control()?;
        if self
            .host_call_limit
            .is_some_and(|limit| self.host_calls_consumed >= limit)
        {
            return Err(VmError::HostCallBudgetExhausted {
                limit: self
                    .host_call_limit
                    .expect("host-call exhaustion requires a limit"),
                consumed: self.host_calls_consumed,
            });
        }
        self.host_calls_consumed += 1;
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VmError {
    InvalidBytecode(VerificationError),
    Arity {
        expected: usize,
        actual: usize,
    },
    ArgumentType {
        argument: usize,
        expected: Type,
        actual: Type,
    },
    TypedEntryPointRequired,
    InvalidOwnedResourceIndex(i64),
    UnknownHostFunction(HostFunctionId),
    InstructionFuelExhausted {
        limit: u64,
        consumed: u64,
    },
    HostCallBudgetExhausted {
        limit: u64,
        consumed: u64,
    },
    ValueSlotLimitExceeded {
        limit: usize,
        required: usize,
    },
    Interrupted {
        instruction_fuel_consumed: u64,
        host_calls_consumed: u64,
    },
    DeadlineExceeded {
        instruction_fuel_consumed: u64,
        host_calls_consumed: u64,
    },
}

impl Display for VmError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidBytecode(error) => write!(formatter, "invalid bytecode: {error}"),
            Self::Arity { expected, actual } => {
                write!(
                    formatter,
                    "expected {expected} arguments, received {actual}"
                )
            }
            Self::ArgumentType {
                argument,
                expected,
                actual,
            } => write!(
                formatter,
                "argument {argument} has type {actual}, expected {expected}"
            ),
            Self::TypedEntryPointRequired => formatter.write_str(
                "owned-resource bytecode must be executed through the typed VM entry point",
            ),
            Self::InvalidOwnedResourceIndex(index) => {
                write!(formatter, "owned resource index {index} is invalid")
            }
            Self::UnknownHostFunction(id) => {
                write!(formatter, "host function {id} is not registered")
            }
            Self::InstructionFuelExhausted { limit, consumed } => write!(
                formatter,
                "instruction fuel exhausted after consuming {consumed} of {limit} units"
            ),
            Self::HostCallBudgetExhausted { limit, consumed } => write!(
                formatter,
                "host-call budget exhausted after consuming {consumed} of {limit} calls"
            ),
            Self::ValueSlotLimitExceeded { limit, required } => write!(
                formatter,
                "value-slot limit of {limit} is below the required {required} slots"
            ),
            Self::Interrupted {
                instruction_fuel_consumed,
                host_calls_consumed,
            } => write!(
                formatter,
                "execution interrupted after consuming {instruction_fuel_consumed} instruction-fuel units and {host_calls_consumed} host calls"
            ),
            Self::DeadlineExceeded {
                instruction_fuel_consumed,
                host_calls_consumed,
            } => write!(
                formatter,
                "execution deadline exceeded after consuming {instruction_fuel_consumed} instruction-fuel units and {host_calls_consumed} host calls"
            ),
        }
    }
}

impl Error for VmError {}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{Duration, Instant};

    use lang_bytecode::{Block, BlockId, BlockParameter, BlockTarget, Instruction, Program, lower};
    use lang_mir::{
        Function, HostFunctionId, ResourceTypeId, Signature, Terminator as MirTerminator, Type,
        ValueId,
    };
    use lang_runtime::HostFunctionRegistry;

    use super::{
        ExecutionControl, ExecutionLimits, ExecutionOutcome, InterruptionFlag, VmError, VmValue,
        VmValueError, execute, execute_typed, execute_with_control, execute_with_hosts,
        execute_with_hosts_and_control, execute_with_hosts_and_limits, execute_with_limits,
    };
    use lang_bytecode::Terminator;

    fn target(block: BlockId) -> BlockTarget {
        BlockTarget::new(block, vec![])
    }

    extern "C" fn wrapping_double(value: i64) -> i64 {
        value.wrapping_mul(2)
    }

    static COUNTED_HOST_CALLS: AtomicUsize = AtomicUsize::new(0);

    extern "C" fn counted_double(value: i64) -> i64 {
        COUNTED_HOST_CALLS.fetch_add(1, Ordering::SeqCst);
        value.wrapping_mul(2)
    }

    static INTERRUPTING_HOST_CALLS: AtomicUsize = AtomicUsize::new(0);
    static CALLBACK_INTERRUPT: InterruptionFlag = InterruptionFlag::new();

    extern "C" fn interrupting_double(value: i64) -> i64 {
        INTERRUPTING_HOST_CALLS.fetch_add(1, Ordering::SeqCst);
        CALLBACK_INTERRUPT.interrupt();
        value.wrapping_mul(2)
    }

    #[test]
    fn executes_a_nominal_owned_resource_value() {
        let resource = ResourceTypeId::new(7);
        let function = Function::new(
            "transfer_resource",
            Signature::new(vec![Type::Own(resource)], Type::Own(resource)),
            vec![],
            MirTerminator::Return(ValueId::new(0)),
        );
        let program = lower(&function).unwrap();
        let owner = VmValue::own_resource(resource, 17).unwrap();

        assert_eq!(execute_typed(&program, &[owner]), Ok(owner));
        assert_eq!(
            execute_typed(&program, &[VmValue::I64(17)]),
            Err(VmError::ArgumentType {
                argument: 0,
                expected: Type::Own(resource),
                actual: Type::I64,
            })
        );
        assert_eq!(
            execute(&program, &[17]),
            Err(VmError::TypedEntryPointRequired)
        );
        assert_eq!(
            VmValue::own_resource(resource, 0),
            Err(VmValueError::InvalidOwnedResourceIndex(0))
        );
    }

    #[test]
    fn executes_wrapping_addition() {
        let program = Program::new(
            2,
            3,
            vec![Instruction::I64AddWrapping {
                destination: 2,
                left: 0,
                right: 1,
            }],
            2,
        )
        .unwrap();

        assert_eq!(execute(&program, &[i64::MAX, 1]), Ok(i64::MIN));
        assert_eq!(
            execute(&program, &[1]),
            Err(VmError::Arity {
                expected: 2,
                actual: 1,
            })
        );
    }

    #[test]
    fn executes_explicit_control_flow() {
        let program = Program::with_blocks(
            2,
            3,
            vec![
                Block::new(
                    vec![Instruction::I64LessThanSigned {
                        destination: 2,
                        left: 0,
                        right: 1,
                    }],
                    Terminator::Branch {
                        condition: 2,
                        then_target: target(BlockId::new(1)),
                        else_target: target(BlockId::new(2)),
                    },
                ),
                Block::new(vec![], Terminator::Return(0)),
                Block::new(vec![], Terminator::Return(1)),
            ],
        )
        .unwrap();

        assert_eq!(execute(&program, &[-7, 4]), Ok(-7));
        assert_eq!(execute(&program, &[9, 3]), Ok(3));
        assert_eq!(execute(&program, &[5, 5]), Ok(5));
    }

    #[test]
    fn executes_loop_carried_block_parameters() {
        let program = countdown_program();

        for value in [i64::MIN, -7, -1, 0, 1, 2, 17, 100] {
            assert_eq!(execute(&program, &[value]), Ok(value.min(0)));
        }

        let host_functions = HostFunctionRegistry::new();
        assert_eq!(
            execute_with_hosts_and_limits(
                &program,
                &[2],
                &host_functions,
                ExecutionLimits::with_instruction_fuel(14),
            ),
            Ok(ExecutionOutcome {
                value: 0,
                instruction_fuel_consumed: 14,
                host_calls_consumed: 0,
                value_slots_used: 7,
            })
        );
        assert_eq!(
            execute_with_limits(&program, &[2], ExecutionLimits::with_instruction_fuel(13),),
            Err(VmError::InstructionFuelExhausted {
                limit: 13,
                consumed: 13,
            })
        );
    }

    #[test]
    fn fuel_stops_a_back_edge() {
        let program = Program::with_blocks(
            0,
            0,
            vec![
                Block::new(vec![], Terminator::Jump(target(BlockId::new(1)))),
                Block::new(vec![], Terminator::Jump(target(BlockId::new(1)))),
            ],
        )
        .unwrap();

        assert_eq!(
            execute_with_limits(&program, &[], ExecutionLimits::with_instruction_fuel(3)),
            Err(VmError::InstructionFuelExhausted {
                limit: 3,
                consumed: 3,
            })
        );
    }

    fn countdown_program() -> Program {
        Program::with_blocks(
            1,
            7,
            vec![
                Block::new(
                    vec![
                        Instruction::I64Constant {
                            destination: 1,
                            value: -1,
                        },
                        Instruction::I64Constant {
                            destination: 2,
                            value: 1,
                        },
                    ],
                    Terminator::Jump(BlockTarget::new(BlockId::new(1), vec![0])),
                ),
                Block::with_parameters(
                    vec![BlockParameter::new(3, Type::I64)],
                    vec![Instruction::I64LessThanSigned {
                        destination: 4,
                        left: 3,
                        right: 2,
                    }],
                    Terminator::Branch {
                        condition: 4,
                        then_target: BlockTarget::new(BlockId::new(3), vec![3]),
                        else_target: target(BlockId::new(2)),
                    },
                ),
                Block::new(
                    vec![Instruction::I64AddWrapping {
                        destination: 5,
                        left: 3,
                        right: 1,
                    }],
                    Terminator::Jump(BlockTarget::new(BlockId::new(1), vec![5])),
                ),
                Block::with_parameters(
                    vec![BlockParameter::new(6, Type::I64)],
                    vec![],
                    Terminator::Return(6),
                ),
            ],
        )
        .unwrap()
    }

    #[test]
    fn executes_registered_host_function() {
        let function = HostFunctionId::new(3);
        let program = Program::new(
            1,
            2,
            vec![Instruction::CallHostI64 {
                destination: 1,
                function,
                argument: 0,
            }],
            1,
        )
        .unwrap();
        let mut hosts = HostFunctionRegistry::new();
        hosts.register_i64(function, wrapping_double).unwrap();

        assert_eq!(execute_with_hosts(&program, &[21], &hosts), Ok(42));
        assert_eq!(
            execute(&program, &[21]),
            Err(VmError::UnknownHostFunction(function))
        );
    }

    #[test]
    fn accounts_for_instructions_and_the_terminator() {
        let program = Program::new(
            2,
            3,
            vec![Instruction::I64AddWrapping {
                destination: 2,
                left: 0,
                right: 1,
            }],
            2,
        )
        .unwrap();

        assert_eq!(
            execute_with_limits(
                &program,
                &[20, 22],
                ExecutionLimits::with_instruction_fuel(2)
            ),
            Ok(ExecutionOutcome {
                value: 42,
                instruction_fuel_consumed: 2,
                host_calls_consumed: 0,
                value_slots_used: 3,
            })
        );
        assert_eq!(
            execute_with_limits(
                &program,
                &[20, 22],
                ExecutionLimits::with_instruction_fuel(1)
            ),
            Err(VmError::InstructionFuelExhausted {
                limit: 1,
                consumed: 1,
            })
        );
    }

    #[test]
    fn exhausts_limits_before_host_side_effects() {
        let function = HostFunctionId::new(4);
        let program = Program::new(
            0,
            2,
            vec![
                Instruction::I64Constant {
                    destination: 0,
                    value: 21,
                },
                Instruction::CallHostI64 {
                    destination: 1,
                    function,
                    argument: 0,
                },
            ],
            1,
        )
        .unwrap();
        let mut hosts = HostFunctionRegistry::new();
        hosts.register_i64(function, counted_double).unwrap();
        COUNTED_HOST_CALLS.store(0, Ordering::SeqCst);

        assert_eq!(
            execute_with_hosts_and_limits(
                &program,
                &[],
                &hosts,
                ExecutionLimits::with_instruction_fuel(1),
            ),
            Err(VmError::InstructionFuelExhausted {
                limit: 1,
                consumed: 1,
            })
        );
        assert_eq!(COUNTED_HOST_CALLS.load(Ordering::SeqCst), 0);

        assert_eq!(
            execute_with_hosts_and_limits(
                &program,
                &[],
                &hosts,
                ExecutionLimits::with_host_call_budget(0),
            ),
            Err(VmError::HostCallBudgetExhausted {
                limit: 0,
                consumed: 0,
            })
        );
        assert_eq!(COUNTED_HOST_CALLS.load(Ordering::SeqCst), 0);

        assert_eq!(
            execute_with_hosts_and_limits(
                &program,
                &[],
                &hosts,
                ExecutionLimits::with_value_slot_limit(1),
            ),
            Err(VmError::ValueSlotLimitExceeded {
                limit: 1,
                required: 2,
            })
        );
        assert_eq!(COUNTED_HOST_CALLS.load(Ordering::SeqCst), 0);

        assert_eq!(
            execute_with_hosts_and_limits(
                &program,
                &[],
                &hosts,
                ExecutionLimits::with_instruction_fuel(3)
                    .and_host_call_budget(1)
                    .and_value_slot_limit(2),
            ),
            Ok(ExecutionOutcome {
                value: 42,
                instruction_fuel_consumed: 3,
                host_calls_consumed: 1,
                value_slots_used: 2,
            })
        );
        assert_eq!(COUNTED_HOST_CALLS.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn observes_interruption_at_vm_safe_points() {
        let function = HostFunctionId::new(5);
        let program = Program::new(
            0,
            2,
            vec![
                Instruction::I64Constant {
                    destination: 0,
                    value: 21,
                },
                Instruction::CallHostI64 {
                    destination: 1,
                    function,
                    argument: 0,
                },
            ],
            1,
        )
        .unwrap();
        let mut hosts = HostFunctionRegistry::new();
        hosts.register_i64(function, interrupting_double).unwrap();
        INTERRUPTING_HOST_CALLS.store(0, Ordering::SeqCst);

        let interrupted_before_execution = InterruptionFlag::new();
        interrupted_before_execution.interrupt();
        assert_eq!(
            execute_with_hosts_and_control(
                &program,
                &[],
                &hosts,
                ExecutionControl::with_limits(
                    ExecutionLimits::with_instruction_fuel(3)
                        .and_host_call_budget(1)
                        .and_value_slot_limit(2),
                )
                .and_interruption(&interrupted_before_execution),
            ),
            Err(VmError::Interrupted {
                instruction_fuel_consumed: 0,
                host_calls_consumed: 0,
            })
        );
        assert_eq!(INTERRUPTING_HOST_CALLS.load(Ordering::SeqCst), 0);

        assert!(!CALLBACK_INTERRUPT.is_interrupted());
        assert_eq!(
            execute_with_hosts_and_control(
                &program,
                &[],
                &hosts,
                ExecutionControl::with_limits(
                    ExecutionLimits::with_instruction_fuel(3)
                        .and_host_call_budget(1)
                        .and_value_slot_limit(2),
                )
                .and_interruption(&CALLBACK_INTERRUPT),
            ),
            Err(VmError::Interrupted {
                instruction_fuel_consumed: 2,
                host_calls_consumed: 1,
            })
        );
        assert_eq!(INTERRUPTING_HOST_CALLS.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn observes_deadline_at_vm_safe_points() {
        let program = Program::with_blocks(
            0,
            0,
            vec![
                Block::new(vec![], Terminator::Jump(target(BlockId::new(1)))),
                Block::new(vec![], Terminator::Jump(target(BlockId::new(1)))),
            ],
        )
        .unwrap();

        assert_eq!(
            execute_with_control(
                &program,
                &[],
                ExecutionControl::unbounded().and_deadline(Instant::now()),
            ),
            Err(VmError::DeadlineExceeded {
                instruction_fuel_consumed: 0,
                host_calls_consumed: 0,
            })
        );

        let interrupted = InterruptionFlag::new();
        interrupted.interrupt();
        assert_eq!(
            execute_with_control(
                &program,
                &[],
                ExecutionControl::unbounded()
                    .and_interruption(&interrupted)
                    .and_deadline(Instant::now()),
            ),
            Err(VmError::Interrupted {
                instruction_fuel_consumed: 0,
                host_calls_consumed: 0,
            })
        );

        let error = execute_with_control(
            &program,
            &[],
            ExecutionControl::unbounded().and_deadline(Instant::now() + Duration::from_millis(2)),
        )
        .expect_err("the deadline stops a back-edge loop");
        let VmError::DeadlineExceeded {
            instruction_fuel_consumed,
            host_calls_consumed,
        } = error
        else {
            panic!("expected deadline exhaustion, received {error}");
        };
        assert!(instruction_fuel_consumed > 0);
        assert_eq!(host_calls_consumed, 0);
    }
}
