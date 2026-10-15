#![forbid(unsafe_code)]

use std::error::Error;
use std::fmt::{Display, Formatter};

use lang_bytecode::{BlockId, Instruction, Program, Terminator, VerificationError};
use lang_mir::HostFunctionId;
use lang_runtime::HostFunctionRegistry;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExecutionLimits {
    instruction_fuel: Option<u64>,
}

impl ExecutionLimits {
    pub const fn unbounded() -> Self {
        Self {
            instruction_fuel: None,
        }
    }

    pub const fn with_instruction_fuel(instruction_fuel: u64) -> Self {
        Self {
            instruction_fuel: Some(instruction_fuel),
        }
    }

    pub const fn instruction_fuel(self) -> Option<u64> {
        self.instruction_fuel
    }
}

impl Default for ExecutionLimits {
    fn default() -> Self {
        Self::unbounded()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExecutionOutcome {
    value: i64,
    instruction_fuel_consumed: u64,
}

impl ExecutionOutcome {
    pub const fn value(self) -> i64 {
        self.value
    }

    pub const fn instruction_fuel_consumed(self) -> u64 {
        self.instruction_fuel_consumed
    }
}

pub fn execute(program: &Program, arguments: &[i64]) -> Result<i64, VmError> {
    execute_with_limits(program, arguments, ExecutionLimits::unbounded())
        .map(ExecutionOutcome::value)
}

pub fn execute_with_limits(
    program: &Program,
    arguments: &[i64],
    limits: ExecutionLimits,
) -> Result<ExecutionOutcome, VmError> {
    execute_with_hosts_and_limits(program, arguments, &HostFunctionRegistry::new(), limits)
}

pub fn execute_with_hosts(
    program: &Program,
    arguments: &[i64],
    host_functions: &HostFunctionRegistry,
) -> Result<i64, VmError> {
    execute_with_hosts_and_limits(
        program,
        arguments,
        host_functions,
        ExecutionLimits::unbounded(),
    )
    .map(ExecutionOutcome::value)
}

pub fn execute_with_hosts_and_limits(
    program: &Program,
    arguments: &[i64],
    host_functions: &HostFunctionRegistry,
    limits: ExecutionLimits,
) -> Result<ExecutionOutcome, VmError> {
    program.verify().map_err(VmError::InvalidBytecode)?;

    if arguments.len() != program.parameter_count() {
        return Err(VmError::Arity {
            expected: program.parameter_count(),
            actual: arguments.len(),
        });
    }

    let mut registers = vec![0; program.register_count()];
    registers[..arguments.len()].copy_from_slice(arguments);
    let mut fuel = FuelMeter::new(limits);

    let mut current_block = BlockId::ENTRY;
    loop {
        let block = &program.blocks()[current_block.index()];
        for instruction in block.instructions() {
            fuel.consume()?;
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
                    registers[destination] = callback(registers[argument]);
                }
            }
        }

        fuel.consume()?;
        match block.terminator() {
            Terminator::Return(register) => {
                return Ok(ExecutionOutcome {
                    value: registers[register],
                    instruction_fuel_consumed: fuel.consumed,
                });
            }
            Terminator::Jump(target) => current_block = target,
            Terminator::Branch {
                condition,
                then_block,
                else_block,
            } => {
                current_block = if registers[condition] != 0 {
                    then_block
                } else {
                    else_block
                };
            }
        }
    }
}

struct FuelMeter {
    limit: Option<u64>,
    consumed: u64,
}

impl FuelMeter {
    const fn new(limits: ExecutionLimits) -> Self {
        Self {
            limit: limits.instruction_fuel(),
            consumed: 0,
        }
    }

    fn consume(&mut self) -> Result<(), VmError> {
        if self.limit.is_some_and(|limit| self.consumed >= limit) {
            return Err(VmError::InstructionFuelExhausted {
                limit: self.limit.expect("fuel exhaustion requires a limit"),
                consumed: self.consumed,
            });
        }
        self.consumed += 1;
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VmError {
    InvalidBytecode(VerificationError),
    Arity { expected: usize, actual: usize },
    UnknownHostFunction(HostFunctionId),
    InstructionFuelExhausted { limit: u64, consumed: u64 },
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
            Self::UnknownHostFunction(id) => {
                write!(formatter, "host function {id} is not registered")
            }
            Self::InstructionFuelExhausted { limit, consumed } => write!(
                formatter,
                "instruction fuel exhausted after consuming {consumed} of {limit} units"
            ),
        }
    }
}

impl Error for VmError {}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use lang_bytecode::{Block, BlockId, Instruction, Program};
    use lang_mir::HostFunctionId;
    use lang_runtime::HostFunctionRegistry;

    use super::{
        ExecutionLimits, ExecutionOutcome, VmError, execute, execute_with_hosts,
        execute_with_hosts_and_limits, execute_with_limits,
    };
    use lang_bytecode::Terminator;

    extern "C" fn wrapping_double(value: i64) -> i64 {
        value.wrapping_mul(2)
    }

    static COUNTED_HOST_CALLS: AtomicUsize = AtomicUsize::new(0);

    extern "C" fn counted_double(value: i64) -> i64 {
        COUNTED_HOST_CALLS.fetch_add(1, Ordering::SeqCst);
        value.wrapping_mul(2)
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
                        then_block: BlockId::new(1),
                        else_block: BlockId::new(2),
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
    fn fuel_stops_a_back_edge() {
        let program = Program::with_blocks(
            0,
            0,
            vec![
                Block::new(vec![], Terminator::Jump(BlockId::new(1))),
                Block::new(vec![], Terminator::Jump(BlockId::new(1))),
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
    fn exhausts_fuel_before_host_side_effects() {
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
                ExecutionLimits::with_instruction_fuel(3),
            ),
            Ok(ExecutionOutcome {
                value: 42,
                instruction_fuel_consumed: 3,
            })
        );
        assert_eq!(COUNTED_HOST_CALLS.load(Ordering::SeqCst), 1);
    }
}
