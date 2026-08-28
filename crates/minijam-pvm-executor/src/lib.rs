//! Stable, chain-free MiniJAM PVM execution boundary.
//!
//! Workload repositories implement [`Host`] and do not depend on Jambda VM
//! internals. Jambda remains the execution implementation behind this crate.

use jp_vm_engine::{run_standalone, StandaloneProgram};
use jp_vm_interp::{memory::InnerInterpMemory, register::InterpRegister, InterpBackend};
use jp_vm_primitives::{
    error::VmError,
    host::HostCallTrait,
    state::{VmMemory, VmRegister, VmState},
    ExitKind, VmResult,
};

const HOST_NONE: u64 = u64::MAX;
const HOST_GAS: u32 = 0;
const HOST_FETCH: u32 = 1;
const HOST_READ: u32 = 3;
const HOST_WRITE: u32 = 4;
const HOST_YIELD: u32 = 25;
const HOST_LOG: u32 = 100;

/// Workload-owned host state exposed through the stable MiniJAM SDK calls.
pub trait Host {
    fn fetch(&self, mode: u64, index: usize) -> Option<&[u8]>;
    fn storage_read(&self, key: &[u8]) -> Option<&[u8]>;
    fn storage_write(&mut self, key: Vec<u8>, value: Vec<u8>);
    fn storage_delete(&mut self, key: &[u8]);
    fn yield_value(&mut self, value: Vec<u8>);
    fn log(&mut self, _message: &[u8]) {}
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Execution {
    pub output: Vec<u8>,
    pub gas_used: u64,
    pub gas_remaining: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("PVM decode error: {0}")]
    Decode(String),
    #[error("PVM execution error: {0}")]
    Execution(String),
    #[error("PVM exhausted its gas limit")]
    OutOfGas,
    #[error("PVM trapped")]
    Panic,
}

/// Execute one converter dispatch entry with Jambda hidden behind MiniJAM.
pub fn execute(
    program_bytes: &[u8],
    host: &mut impl Host,
    payload: &[u8],
    entry_point: u32,
    gas_limit: u64,
) -> Result<Execution, Error> {
    let program = StandaloneProgram::from_bytes(program_bytes)
        .map_err(|error| Error::Decode(error.to_string()))?;
    let mut bridge = HostBridge(host);
    let result = run_standalone(
        &program,
        InterpBackend::new(),
        &mut bridge,
        std::sync::Arc::from(payload.to_vec()),
        entry_point,
        gas_limit,
    )
    .map_err(|error| Error::Execution(error.to_string()))?;
    let output = match result.result {
        VmResult::Ok(Some(output)) => output.into_vec(),
        VmResult::Ok(None) => Vec::new(),
        VmResult::Oog => return Err(Error::OutOfGas),
        VmResult::Panic => return Err(Error::Panic),
    };
    Ok(Execution {
        output,
        gas_used: result.gas_used,
        gas_remaining: result.gas_remaining as u64,
    })
}

/// Encode one JAM natural number without exposing Jambda primitives.
pub fn encode_fnencode(value: u64, output: &mut Vec<u8>) {
    jp_vm_primitives::encode_fnencode(value, output);
}

struct HostBridge<'a, H>(&'a mut H);

impl<H: Host> HostCallTrait<InterpRegister, InnerInterpMemory> for HostBridge<'_, H> {
    fn ecalli(
        &mut self,
        id: u32,
        state: &mut VmState<InterpRegister, InnerInterpMemory>,
        gas: &mut i64,
    ) -> Result<ExitKind, VmError> {
        let arg = |index: u8| state.registers.get(index);
        let set_result = |state: &mut VmState<InterpRegister, InnerInterpMemory>, value: u64| {
            state.registers.set_a0(value);
        };
        match id {
            HOST_GAS => set_result(state, (*gas).max(0) as u64),
            HOST_FETCH => {
                let ptr = arg(7) as u32;
                let offset = arg(8) as usize;
                let capacity = arg(9) as usize;
                let mode = arg(10);
                let index = arg(11) as usize;
                let Some(item) = self.0.fetch(mode, index) else {
                    set_result(state, HOST_NONE);
                    return Ok(ExitKind::Continue);
                };
                let remaining = item.get(offset..).unwrap_or_default();
                let copy_len = remaining.len().min(capacity);
                state
                    .memory
                    .write_bytes(ptr, &remaining[..copy_len])
                    .map_err(|_| VmError::Panic)?;
                set_result(state, item.len() as u64);
            }
            HOST_READ => {
                let key_ptr = arg(8) as u32;
                let key_len = arg(9) as usize;
                let out_ptr = arg(10) as u32;
                let capacity = arg(12) as usize;
                let mut key = vec![0; key_len];
                state
                    .memory
                    .read_bytes_into(key_ptr, &mut key)
                    .map_err(|_| VmError::Panic)?;
                let Some(value) = self.0.storage_read(&key) else {
                    set_result(state, HOST_NONE);
                    return Ok(ExitKind::Continue);
                };
                let copy_len = value.len().min(capacity);
                state
                    .memory
                    .write_bytes(out_ptr, &value[..copy_len])
                    .map_err(|_| VmError::Panic)?;
                set_result(state, value.len() as u64);
            }
            HOST_WRITE => {
                let key_ptr = arg(7) as u32;
                let key_len = arg(8) as usize;
                let value_ptr = arg(9) as u32;
                let value_len = arg(10) as usize;
                let mut key = vec![0; key_len];
                let mut value = vec![0; value_len];
                state
                    .memory
                    .read_bytes_into(key_ptr, &mut key)
                    .map_err(|_| VmError::Panic)?;
                state
                    .memory
                    .read_bytes_into(value_ptr, &mut value)
                    .map_err(|_| VmError::Panic)?;
                if value.is_empty() {
                    self.0.storage_delete(&key);
                } else {
                    self.0.storage_write(key, value);
                }
                set_result(state, 0);
            }
            HOST_YIELD => {
                let ptr = arg(7) as u32;
                let mut value = vec![0; 32];
                state
                    .memory
                    .read_bytes_into(ptr, &mut value)
                    .map_err(|_| VmError::Panic)?;
                self.0.yield_value(value);
                set_result(state, 0);
            }
            HOST_LOG => {
                let ptr = arg(7) as u32;
                let len = arg(8) as usize;
                let mut message = vec![0; len];
                state
                    .memory
                    .read_bytes_into(ptr, &mut message)
                    .map_err(|_| VmError::Panic)?;
                self.0.log(&message);
                set_result(state, 0);
            }
            _ => set_result(state, 0),
        }
        Ok(ExitKind::Continue)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnencode_is_available_without_jambda_types() {
        let mut encoded = Vec::new();
        encode_fnencode(0, &mut encoded);
        encode_fnencode(128, &mut encoded);
        assert_eq!(encoded, vec![0, 128, 128]);
    }
}
