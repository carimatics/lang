#![forbid(unsafe_code)]

pub mod allocation;
pub mod arena;
pub mod buffer;
pub mod cleanup;
pub mod component;
pub mod layout;
pub mod shared;

use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::error::Error;
use std::fmt::{Display, Formatter};

use lang_mir::HostFunctionId;

pub type I64HostFunction = extern "C" fn(i64) -> i64;

#[derive(Default)]
pub struct HostFunctionRegistry {
    i64_functions: HashMap<HostFunctionId, I64HostFunction>,
}

impl HostFunctionRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_i64(
        &mut self,
        id: HostFunctionId,
        function: I64HostFunction,
    ) -> Result<(), RegistrationError> {
        match self.i64_functions.entry(id) {
            Entry::Vacant(entry) => {
                entry.insert(function);
                Ok(())
            }
            Entry::Occupied(_) => Err(RegistrationError::DuplicateFunction(id)),
        }
    }

    pub fn resolve_i64(&self, id: HostFunctionId) -> Option<I64HostFunction> {
        self.i64_functions.get(&id).copied()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegistrationError {
    DuplicateFunction(HostFunctionId),
}

impl Display for RegistrationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateFunction(id) => {
                write!(formatter, "host function {id} is already registered")
            }
        }
    }
}

impl Error for RegistrationError {}

#[cfg(test)]
mod tests {
    use lang_mir::HostFunctionId;

    use super::{HostFunctionRegistry, RegistrationError};

    extern "C" fn identity(value: i64) -> i64 {
        value
    }

    #[test]
    fn rejects_duplicate_function_ids() {
        let id = HostFunctionId::new(7);
        let mut registry = HostFunctionRegistry::new();

        assert_eq!(registry.register_i64(id, identity), Ok(()));
        assert_eq!(
            registry.register_i64(id, identity),
            Err(RegistrationError::DuplicateFunction(id))
        );
    }
}
