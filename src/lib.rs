use std::error::Error;
use std::fmt::{Display, Formatter};

use lang_bytecode::{LowerError, lower};
use lang_codegen_clif::{CompileError, CompiledFunction, InvokeError};
use lang_mir::{
    BasicBlock, BlockId, BlockParameter, BlockTarget, Function, HostFunctionId, Instruction,
    Signature, Terminator, Type, ValueId,
};
use lang_runtime::{HostFunctionRegistry, RegistrationError};
use lang_vm::{VmError, execute, execute_with_hosts};

pub mod linker;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExecutionResults {
    pub bytecode: i64,
    pub native: i64,
}

#[derive(Debug)]
pub enum PrototypeError {
    Bytecode(LowerError),
    HostRegistration(RegistrationError),
    Vm(VmError),
    NativeCompile(CompileError),
    NativeInvoke(InvokeError),
}

impl Display for PrototypeError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Bytecode(error) => write!(formatter, "bytecode lowering failed: {error}"),
            Self::HostRegistration(error) => {
                write!(formatter, "host function registration failed: {error}")
            }
            Self::Vm(error) => write!(formatter, "bytecode execution failed: {error}"),
            Self::NativeCompile(error) => write!(formatter, "native compilation failed: {error}"),
            Self::NativeInvoke(error) => write!(formatter, "native invocation failed: {error}"),
        }
    }
}

pub const PROTOTYPE_DOUBLE_HOST: HostFunctionId = HostFunctionId::new(0);
pub const PROTOTYPE_ALLOCATE_HOST: HostFunctionId = HostFunctionId::new(1);

extern "C" fn prototype_wrapping_double(value: i64) -> i64 {
    value.wrapping_mul(2)
}

extern "C" fn prototype_allocate_roundtrip(value: i64) -> i64 {
    let allocation = std::hint::black_box(Box::new(value));
    std::hint::black_box(*allocation)
}

impl Error for PrototypeError {}

pub fn prototype_add_function() -> Function {
    Function::new(
        "prototype_add",
        Signature::new(vec![Type::I64, Type::I64], Type::I64),
        vec![Instruction::I64AddWrapping {
            result: ValueId::new(2),
            left: ValueId::new(0),
            right: ValueId::new(1),
        }],
        Terminator::Return(ValueId::new(2)),
    )
}

pub fn run_prototype_add(left: i64, right: i64) -> Result<ExecutionResults, PrototypeError> {
    let function = prototype_add_function();
    let program = lower(&function).map_err(PrototypeError::Bytecode)?;
    let bytecode = execute(&program, &[left, right]).map_err(PrototypeError::Vm)?;
    let compiled = CompiledFunction::compile(&function).map_err(PrototypeError::NativeCompile)?;
    let native = compiled
        .invoke_i64_2(left, right)
        .map_err(PrototypeError::NativeInvoke)?;

    Ok(ExecutionResults { bytecode, native })
}

pub fn prototype_min_function() -> Function {
    Function::with_blocks(
        "prototype_min",
        Signature::new(vec![Type::I64, Type::I64], Type::I64),
        vec![
            BasicBlock::new(
                vec![Instruction::I64LessThanSigned {
                    result: ValueId::new(2),
                    left: ValueId::new(0),
                    right: ValueId::new(1),
                }],
                Terminator::Branch {
                    condition: ValueId::new(2),
                    then_target: BlockTarget::new(BlockId::new(1), vec![]),
                    else_target: BlockTarget::new(BlockId::new(2), vec![]),
                },
            ),
            BasicBlock::new(vec![], Terminator::Return(ValueId::new(0))),
            BasicBlock::new(vec![], Terminator::Return(ValueId::new(1))),
        ],
    )
}

pub fn run_prototype_min(left: i64, right: i64) -> Result<ExecutionResults, PrototypeError> {
    let function = prototype_min_function();
    let program = lower(&function).map_err(PrototypeError::Bytecode)?;
    let bytecode = execute(&program, &[left, right]).map_err(PrototypeError::Vm)?;
    let compiled = CompiledFunction::compile(&function).map_err(PrototypeError::NativeCompile)?;
    let native = compiled
        .invoke_i64_2(left, right)
        .map_err(PrototypeError::NativeInvoke)?;

    Ok(ExecutionResults { bytecode, native })
}

pub fn prototype_countdown_function() -> Function {
    Function::with_blocks(
        "prototype_countdown",
        Signature::new(vec![Type::I64], Type::I64),
        vec![
            BasicBlock::new(
                vec![
                    Instruction::I64Constant {
                        result: ValueId::new(1),
                        value: -1,
                    },
                    Instruction::I64Constant {
                        result: ValueId::new(2),
                        value: 1,
                    },
                ],
                Terminator::Jump(BlockTarget::new(BlockId::new(1), vec![ValueId::new(0)])),
            ),
            BasicBlock::with_parameters(
                vec![BlockParameter::new(ValueId::new(3), Type::I64)],
                vec![Instruction::I64LessThanSigned {
                    result: ValueId::new(4),
                    left: ValueId::new(3),
                    right: ValueId::new(2),
                }],
                Terminator::Branch {
                    condition: ValueId::new(4),
                    then_target: BlockTarget::new(BlockId::new(3), vec![ValueId::new(3)]),
                    else_target: BlockTarget::new(BlockId::new(2), vec![]),
                },
            ),
            BasicBlock::new(
                vec![Instruction::I64AddWrapping {
                    result: ValueId::new(5),
                    left: ValueId::new(3),
                    right: ValueId::new(1),
                }],
                Terminator::Jump(BlockTarget::new(BlockId::new(1), vec![ValueId::new(5)])),
            ),
            BasicBlock::with_parameters(
                vec![BlockParameter::new(ValueId::new(6), Type::I64)],
                vec![],
                Terminator::Return(ValueId::new(6)),
            ),
        ],
    )
}

pub fn run_prototype_countdown(value: i64) -> Result<ExecutionResults, PrototypeError> {
    let function = prototype_countdown_function();
    let program = lower(&function).map_err(PrototypeError::Bytecode)?;
    let bytecode = execute(&program, &[value]).map_err(PrototypeError::Vm)?;
    let compiled = CompiledFunction::compile(&function).map_err(PrototypeError::NativeCompile)?;
    let native = compiled
        .invoke_i64_1(value)
        .map_err(PrototypeError::NativeInvoke)?;

    Ok(ExecutionResults { bytecode, native })
}

pub fn prototype_sum_to_function() -> Function {
    Function::with_blocks(
        "prototype_sum_to",
        Signature::new(vec![Type::I64], Type::I64),
        vec![
            BasicBlock::new(
                vec![
                    Instruction::I64Constant {
                        result: ValueId::new(1),
                        value: -1,
                    },
                    Instruction::I64Constant {
                        result: ValueId::new(2),
                        value: 0,
                    },
                    Instruction::I64Constant {
                        result: ValueId::new(3),
                        value: 1,
                    },
                ],
                Terminator::Jump(BlockTarget::new(
                    BlockId::new(1),
                    vec![ValueId::new(0), ValueId::new(2)],
                )),
            ),
            BasicBlock::with_parameters(
                vec![
                    BlockParameter::new(ValueId::new(4), Type::I64),
                    BlockParameter::new(ValueId::new(5), Type::I64),
                ],
                vec![Instruction::I64LessThanSigned {
                    result: ValueId::new(6),
                    left: ValueId::new(4),
                    right: ValueId::new(3),
                }],
                Terminator::Branch {
                    condition: ValueId::new(6),
                    then_target: BlockTarget::new(BlockId::new(3), vec![ValueId::new(5)]),
                    else_target: BlockTarget::new(BlockId::new(2), vec![]),
                },
            ),
            BasicBlock::new(
                vec![
                    Instruction::I64AddWrapping {
                        result: ValueId::new(7),
                        left: ValueId::new(5),
                        right: ValueId::new(4),
                    },
                    Instruction::I64AddWrapping {
                        result: ValueId::new(8),
                        left: ValueId::new(4),
                        right: ValueId::new(1),
                    },
                ],
                Terminator::Jump(BlockTarget::new(
                    BlockId::new(1),
                    vec![ValueId::new(8), ValueId::new(7)],
                )),
            ),
            BasicBlock::with_parameters(
                vec![BlockParameter::new(ValueId::new(9), Type::I64)],
                vec![],
                Terminator::Return(ValueId::new(9)),
            ),
        ],
    )
}

pub fn run_prototype_sum_to(value: i64) -> Result<ExecutionResults, PrototypeError> {
    let function = prototype_sum_to_function();
    let program = lower(&function).map_err(PrototypeError::Bytecode)?;
    let bytecode = execute(&program, &[value]).map_err(PrototypeError::Vm)?;
    let compiled = CompiledFunction::compile(&function).map_err(PrototypeError::NativeCompile)?;
    let native = compiled
        .invoke_i64_1(value)
        .map_err(PrototypeError::NativeInvoke)?;

    Ok(ExecutionResults { bytecode, native })
}

pub fn prototype_sum_below_function() -> Function {
    Function::with_blocks(
        "prototype_sum_below",
        Signature::new(vec![Type::I64, Type::I64], Type::I64),
        vec![
            BasicBlock::new(
                vec![
                    Instruction::I64Constant {
                        result: ValueId::new(2),
                        value: -1,
                    },
                    Instruction::I64Constant {
                        result: ValueId::new(3),
                        value: 0,
                    },
                    Instruction::I64Constant {
                        result: ValueId::new(4),
                        value: 1,
                    },
                ],
                Terminator::Jump(BlockTarget::new(
                    BlockId::new(1),
                    vec![ValueId::new(0), ValueId::new(3)],
                )),
            ),
            BasicBlock::with_parameters(
                vec![
                    BlockParameter::new(ValueId::new(5), Type::I64),
                    BlockParameter::new(ValueId::new(6), Type::I64),
                ],
                vec![Instruction::I64LessThanSigned {
                    result: ValueId::new(7),
                    left: ValueId::new(5),
                    right: ValueId::new(4),
                }],
                Terminator::Branch {
                    condition: ValueId::new(7),
                    then_target: BlockTarget::new(BlockId::new(6), vec![ValueId::new(6)]),
                    else_target: BlockTarget::new(BlockId::new(2), vec![]),
                },
            ),
            BasicBlock::new(
                vec![Instruction::I64LessThanSigned {
                    result: ValueId::new(8),
                    left: ValueId::new(5),
                    right: ValueId::new(1),
                }],
                Terminator::Branch {
                    condition: ValueId::new(8),
                    then_target: BlockTarget::new(BlockId::new(3), vec![]),
                    else_target: BlockTarget::new(BlockId::new(4), vec![]),
                },
            ),
            BasicBlock::new(
                vec![Instruction::I64AddWrapping {
                    result: ValueId::new(9),
                    left: ValueId::new(6),
                    right: ValueId::new(5),
                }],
                Terminator::Jump(BlockTarget::new(
                    BlockId::new(5),
                    vec![ValueId::new(5), ValueId::new(9)],
                )),
            ),
            BasicBlock::new(
                vec![],
                Terminator::Jump(BlockTarget::new(
                    BlockId::new(5),
                    vec![ValueId::new(5), ValueId::new(6)],
                )),
            ),
            BasicBlock::with_parameters(
                vec![
                    BlockParameter::new(ValueId::new(10), Type::I64),
                    BlockParameter::new(ValueId::new(11), Type::I64),
                ],
                vec![Instruction::I64AddWrapping {
                    result: ValueId::new(12),
                    left: ValueId::new(10),
                    right: ValueId::new(2),
                }],
                Terminator::Jump(BlockTarget::new(
                    BlockId::new(1),
                    vec![ValueId::new(12), ValueId::new(11)],
                )),
            ),
            BasicBlock::with_parameters(
                vec![BlockParameter::new(ValueId::new(13), Type::I64)],
                vec![],
                Terminator::Return(ValueId::new(13)),
            ),
        ],
    )
}

pub fn run_prototype_sum_below(
    value: i64,
    cutoff: i64,
) -> Result<ExecutionResults, PrototypeError> {
    let function = prototype_sum_below_function();
    let program = lower(&function).map_err(PrototypeError::Bytecode)?;
    let bytecode = execute(&program, &[value, cutoff]).map_err(PrototypeError::Vm)?;
    let compiled = CompiledFunction::compile(&function).map_err(PrototypeError::NativeCompile)?;
    let native = compiled
        .invoke_i64_2(value, cutoff)
        .map_err(PrototypeError::NativeInvoke)?;

    Ok(ExecutionResults { bytecode, native })
}

pub fn prototype_host_callback_function() -> Function {
    Function::new(
        "prototype_host_callback",
        Signature::new(vec![Type::I64], Type::I64),
        vec![Instruction::CallHostI64 {
            result: ValueId::new(1),
            function: PROTOTYPE_DOUBLE_HOST,
            argument: ValueId::new(0),
        }],
        Terminator::Return(ValueId::new(1)),
    )
}

pub fn run_prototype_host_callback(value: i64) -> Result<ExecutionResults, PrototypeError> {
    let mut host_functions = HostFunctionRegistry::new();
    host_functions
        .register_i64(PROTOTYPE_DOUBLE_HOST, prototype_wrapping_double)
        .map_err(PrototypeError::HostRegistration)?;

    let function = prototype_host_callback_function();
    let program = lower(&function).map_err(PrototypeError::Bytecode)?;
    let bytecode =
        execute_with_hosts(&program, &[value], &host_functions).map_err(PrototypeError::Vm)?;
    let compiled = CompiledFunction::compile_with_hosts(&function, &host_functions)
        .map_err(PrototypeError::NativeCompile)?;
    let native = compiled
        .invoke_i64_1(value)
        .map_err(PrototypeError::NativeInvoke)?;

    Ok(ExecutionResults { bytecode, native })
}

pub fn prototype_host_allocation_function() -> Function {
    Function::new(
        "prototype_host_allocation",
        Signature::new(vec![Type::I64], Type::I64),
        vec![Instruction::CallHostI64 {
            result: ValueId::new(1),
            function: PROTOTYPE_ALLOCATE_HOST,
            argument: ValueId::new(0),
        }],
        Terminator::Return(ValueId::new(1)),
    )
}

pub fn run_prototype_host_allocation(value: i64) -> Result<ExecutionResults, PrototypeError> {
    let mut host_functions = HostFunctionRegistry::new();
    host_functions
        .register_i64(PROTOTYPE_ALLOCATE_HOST, prototype_allocate_roundtrip)
        .map_err(PrototypeError::HostRegistration)?;

    let function = prototype_host_allocation_function();
    let program = lower(&function).map_err(PrototypeError::Bytecode)?;
    let bytecode =
        execute_with_hosts(&program, &[value], &host_functions).map_err(PrototypeError::Vm)?;
    let compiled = CompiledFunction::compile_with_hosts(&function, &host_functions)
        .map_err(PrototypeError::NativeCompile)?;
    let native = compiled
        .invoke_i64_1(value)
        .map_err(PrototypeError::NativeInvoke)?;

    Ok(ExecutionResults { bytecode, native })
}

#[cfg(test)]
mod tests {
    use super::{
        ExecutionResults, run_prototype_add, run_prototype_countdown,
        run_prototype_host_allocation, run_prototype_host_callback, run_prototype_min,
        run_prototype_sum_below, run_prototype_sum_to,
    };

    #[test]
    fn execution_modes_are_equivalent() {
        let cases = [(2, 2), (-7, 4), (i64::MAX, 1), (i64::MIN, -1)];

        for (left, right) in cases {
            let expected = left.wrapping_add(right);
            assert_eq!(
                run_prototype_add(left, right).unwrap(),
                ExecutionResults {
                    bytecode: expected,
                    native: expected,
                }
            );
        }
    }

    #[test]
    fn host_callbacks_are_equivalent() {
        let cases = [0_i64, 21, -7, i64::MAX, i64::MIN];

        for value in cases {
            let expected = value.wrapping_mul(2);
            assert_eq!(
                run_prototype_host_callback(value).unwrap(),
                ExecutionResults {
                    bytecode: expected,
                    native: expected,
                }
            );
        }
    }

    #[test]
    fn host_allocation_callbacks_are_equivalent() {
        let cases = [0_i64, 21, -7, i64::MAX, i64::MIN];

        for value in cases {
            assert_eq!(
                run_prototype_host_allocation(value).unwrap(),
                ExecutionResults {
                    bytecode: value,
                    native: value,
                }
            );
        }
    }

    #[test]
    fn explicit_control_flow_is_equivalent() {
        let cases = [
            (2, 2),
            (-7, 4),
            (9, 3),
            (i64::MAX, i64::MIN),
            (i64::MIN, i64::MAX),
        ];

        for (left, right) in cases {
            let expected = left.min(right);
            assert_eq!(
                run_prototype_min(left, right).unwrap(),
                ExecutionResults {
                    bytecode: expected,
                    native: expected,
                }
            );
        }
    }

    #[test]
    fn loop_carried_values_are_equivalent() {
        let cases = [i64::MIN, -7, -1, 0, 1, 2, 17, 100];

        for value in cases {
            let expected = value.min(0);
            assert_eq!(
                run_prototype_countdown(value).unwrap(),
                ExecutionResults {
                    bytecode: expected,
                    native: expected,
                }
            );
        }
    }

    #[test]
    fn multiple_loop_carried_values_are_equivalent() {
        let cases = [
            (i64::MIN, 0),
            (-7, 0),
            (0, 0),
            (1, 1),
            (2, 3),
            (17, 153),
            (64, 2_080),
        ];

        for (value, expected) in cases {
            assert_eq!(
                run_prototype_sum_to(value).unwrap(),
                ExecutionResults {
                    bytecode: expected,
                    native: expected,
                }
            );
        }
    }

    #[test]
    fn conditional_loop_paths_are_equivalent() {
        let cases = [
            (-7, 10, 0),
            (0, 10, 0),
            (1, 1, 0),
            (1, 2, 1),
            (5, -3, 0),
            (5, 3, 3),
            (5, 10, 15),
            (10, 5, 10),
            (64, 33, 528),
        ];

        for (value, cutoff, expected) in cases {
            assert_eq!(
                run_prototype_sum_below(value, cutoff).unwrap(),
                ExecutionResults {
                    bytecode: expected,
                    native: expected,
                }
            );
        }
    }
}
