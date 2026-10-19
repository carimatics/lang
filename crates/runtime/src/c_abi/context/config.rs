//! Borrowed, already-readable descriptor views. These are not raw C pointer
//! readers: the native adapter must establish validity and copy initialized
//! descriptor fields individually. No caller descriptor/name reference escapes
//! `Snapshot::check`; code/user pointer lifetimes remain the caller's contract.
use super::{Fault, Kind};
use crate::c_abi::{Artifact, Digest, Direction};
use crate::{allocation, execution};
use std::ffi::c_void;

pub const VERSION: u32 = 1;
pub const NATIVE_FFI: u64 = 1;
pub const MAX_IMPORTS: usize = 4096;
pub const MAX_NAME_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_CLEANUP_RECORDS: u32 = 4096;
pub type Allocate = unsafe extern "C" fn(*mut c_void, u64, u64) -> *mut c_void;
pub type Deallocate = unsafe extern "C" fn(*mut c_void, *mut c_void, u64, u64);
/// A code pointer, restored only to the generated exact callback type by the
/// trusted adapter. It must never travel through a data-pointer representation.
pub type ErasedFunction = unsafe extern "C" fn();

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Allocator {
    pub version: u32,
    pub size: u32,
    pub user: *mut c_void,
    pub allocate: Option<Allocate>,
    pub deallocate: Option<Deallocate>,
}
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct LimitsDescriptor {
    pub version: u32,
    pub size: u32,
    pub live_bytes: u64,
    pub cumulative_bytes: u64,
    pub live_allocations: u64,
    pub frames: u64,
    pub cells: u64,
    pub host_calls: u64,
    pub fuel: u64,
    pub cleanup_records: u32,
    pub reserved: u32,
}
impl Default for LimitsDescriptor {
    fn default() -> Self {
        let allocation = allocation::Limits::default();
        Self {
            version: VERSION,
            size: 72,
            live_bytes: allocation.live_bytes,
            cumulative_bytes: allocation.cumulative_bytes,
            live_allocations: allocation.live_allocations as u64,
            frames: 256,
            cells: 1_000_000,
            host_calls: 1_000,
            fuel: 1_000_000,
            cleanup_records: 256,
            reserved: 0,
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Binding<'a> {
    pub version: u32,
    pub size: u32,
    pub name: Option<&'a [u8]>,
    pub name_bytes: u64,
    pub signature: Digest,
    pub function: Option<ErasedFunction>,
    pub user: *mut c_void,
    pub flags: u64,
}
#[derive(Clone, Copy, Debug)]
pub struct Config<'a> {
    pub version: u32,
    pub size: u32,
    pub allocator: Option<&'a Allocator>,
    pub limits: Option<&'a LimitsDescriptor>,
    pub imports: Option<&'a [Binding<'a>]>,
    pub import_count: u64,
    pub artifact: Digest,
    pub flags: u64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Descriptor {
    Config,
    Allocator,
    Limits,
    Binding(usize),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Limit {
    Frames,
    Cells,
    LiveAllocations,
    CleanupRecords,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvalidConfig {
    Header(Descriptor),
    Missing(Descriptor),
    Flags(Descriptor),
    Artifact,
    ImportCount,
    Name(usize),
    NameBytes,
    Duplicate(usize),
    Unknown(usize),
    MissingImport(usize),
    Signature(usize),
    Function(usize),
    Grant(usize),
    Limit {
        kind: Limit,
        requested: u64,
        maximum: usize,
    },
}
impl From<InvalidConfig> for Fault {
    fn from(error: InvalidConfig) -> Self {
        Self::new(Kind::Config(error))
    }
}

/// The backend supplies actual metadata strides, then may lower these maxima
/// further. Counts must fit addressable storage before allocating anything.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Capacities {
    frames: usize,
    cells: usize,
    allocations: usize,
}
impl Capacities {
    pub fn from_strides(
        frame: usize,
        cell: usize,
        allocation_record: usize,
    ) -> Result<Self, Fault> {
        fn capacity(stride: usize) -> Result<usize, Fault> {
            if stride == 0 || stride > isize::MAX as usize {
                return Err(Fault::new(Kind::Capacity));
            }
            Ok(isize::MAX as usize / stride)
        }
        Ok(Self {
            frames: capacity(frame)?,
            cells: capacity(cell)?,
            allocations: capacity(allocation_record)?,
        })
    }
    pub const fn bounded(self, frames: usize, cells: usize, allocations: usize) -> Self {
        Self {
            frames: if frames < self.frames {
                frames
            } else {
                self.frames
            },
            cells: if cells < self.cells {
                cells
            } else {
                self.cells
            },
            allocations: if allocations < self.allocations {
                allocations
            } else {
                self.allocations
            },
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct CheckedLimits {
    pub allocation: allocation::Limits,
    pub execution: execution::Limits,
    pub cleanup_records: usize,
}
impl CheckedLimits {
    fn check(raw: &LimitsDescriptor, capacity: Capacities) -> Result<Self, Fault> {
        fn count(value: u64, maximum: usize, kind: Limit) -> Result<usize, Fault> {
            usize::try_from(value)
                .ok()
                .filter(|n| *n <= maximum)
                .ok_or_else(|| {
                    InvalidConfig::Limit {
                        kind,
                        requested: value,
                        maximum,
                    }
                    .into()
                })
        }
        let live_allocations = count(
            raw.live_allocations,
            capacity.allocations,
            Limit::LiveAllocations,
        )?;
        let frames = count(raw.frames, capacity.frames, Limit::Frames)?;
        let cells = count(raw.cells, capacity.cells, Limit::Cells)?;
        let cleanup_records = count(
            u64::from(raw.cleanup_records),
            MAX_CLEANUP_RECORDS as usize,
            Limit::CleanupRecords,
        )?;
        Ok(Self {
            allocation: allocation::Limits {
                live_bytes: raw.live_bytes,
                cumulative_bytes: raw.cumulative_bytes,
                live_allocations,
                ..allocation::Limits::default()
            },
            execution: execution::Limits {
                fuel: (raw.fuel != u64::MAX).then_some(raw.fuel),
                host_calls: (raw.host_calls != u64::MAX).then_some(raw.host_calls),
                frames: Some(frames),
                live_cells: Some(cells),
            },
            cleanup_records,
        })
    }
}
#[derive(Debug)]
pub struct BoundImport {
    pub(super) index: usize,
    name: String,
    signature: Digest,
    function: ErasedFunction,
    user: *mut c_void,
}
impl BoundImport {
    pub fn name(&self) -> &str {
        &self.name
    }
    pub const fn signature(&self) -> Digest {
        self.signature
    }
    pub const fn function(&self) -> ErasedFunction {
        self.function
    }
    pub const fn user(&self) -> *mut c_void {
        self.user
    }
    pub const fn index(&self) -> usize {
        self.index
    }
}
#[derive(Debug)]
pub(super) struct Snapshot {
    pub allocator: Allocator,
    pub limits: CheckedLimits,
    pub imports: Vec<BoundImport>,
}
impl Snapshot {
    pub fn check(
        artifact: &Artifact,
        config: Config<'_>,
        capacity: Capacities,
    ) -> Result<Self, Fault> {
        fn header(version: u32, size: u32, expected: u32, which: Descriptor) -> Result<(), Fault> {
            if version != VERSION || size != expected {
                return Err(InvalidConfig::Header(which).into());
            }
            Ok(())
        }
        header(config.version, config.size, 80, Descriptor::Config)?;
        if config.flags != 0 {
            return Err(InvalidConfig::Flags(Descriptor::Config).into());
        }
        if config.artifact != artifact.digest() {
            return Err(InvalidConfig::Artifact.into());
        }
        let allocator = config
            .allocator
            .ok_or(InvalidConfig::Missing(Descriptor::Allocator))?;
        header(allocator.version, allocator.size, 32, Descriptor::Allocator)?;
        if allocator.allocate.is_none() || allocator.deallocate.is_none() {
            return Err(InvalidConfig::Missing(Descriptor::Allocator).into());
        }
        let limits = config
            .limits
            .ok_or(InvalidConfig::Missing(Descriptor::Limits))?;
        header(limits.version, limits.size, 72, Descriptor::Limits)?;
        if limits.reserved != 0 {
            return Err(InvalidConfig::Flags(Descriptor::Limits).into());
        }
        let limits = CheckedLimits::check(limits, capacity)?;
        let count = usize::try_from(config.import_count)
            .ok()
            .filter(|n| *n <= MAX_IMPORTS)
            .ok_or(InvalidConfig::ImportCount)?;
        let bindings = config.imports.unwrap_or(&[]);
        if bindings.len() != count || (config.imports.is_none() && count != 0) {
            return Err(InvalidConfig::ImportCount.into());
        }
        let mut name_bytes = 0usize;
        // Complete structural validation before allocating name snapshots.
        for (index, binding) in bindings.iter().enumerate() {
            header(
                binding.version,
                binding.size,
                80,
                Descriptor::Binding(index),
            )?;
            if binding.flags & !NATIVE_FFI != 0 {
                return Err(InvalidConfig::Flags(Descriptor::Binding(index)).into());
            }
            let name = binding.name.ok_or(InvalidConfig::Name(index))?;
            if name.is_empty()
                || u64::try_from(name.len()).ok() != Some(binding.name_bytes)
                || name.contains(&0)
                || std::str::from_utf8(name).is_err()
            {
                return Err(InvalidConfig::Name(index).into());
            }
            name_bytes = name_bytes
                .checked_add(name.len())
                .filter(|n| *n <= MAX_NAME_BYTES)
                .ok_or(InvalidConfig::NameBytes)?;
            if binding.function.is_none() {
                return Err(InvalidConfig::Function(index).into());
            }
        }
        let allocation = || Fault::new(Kind::Allocation);
        let mut expected = Vec::new();
        expected
            .try_reserve_exact(artifact.functions().len())
            .map_err(|_| allocation())?;
        for (index, function) in artifact.functions().iter().enumerate() {
            if function.direction == Direction::Import {
                expected.push((function.name.as_bytes(), index));
            }
        }
        expected.sort_unstable_by_key(|entry| entry.0);
        let mut checked = Vec::new();
        checked.try_reserve_exact(count).map_err(|_| allocation())?;
        for (index, binding) in bindings.iter().enumerate() {
            let name = binding.name.expect("validated name");
            let found = expected
                .binary_search_by_key(&name, |entry| entry.0)
                .map_err(|_| InvalidConfig::Unknown(index))?;
            let function_index = expected[found].1;
            if binding.signature != artifact.functions()[function_index].digest {
                return Err(InvalidConfig::Signature(index).into());
            }
            if binding.flags & NATIVE_FFI == 0 {
                return Err(InvalidConfig::Grant(index).into());
            }
            checked.push((function_index, index, binding));
        }
        checked.sort_unstable_by_key(|entry| entry.0);
        for pair in checked.windows(2) {
            if pair[0].0 == pair[1].0 {
                return Err(InvalidConfig::Duplicate(pair[1].1).into());
            }
        }
        for (_, index) in expected {
            if checked
                .binary_search_by_key(&index, |entry| entry.0)
                .is_err()
            {
                return Err(InvalidConfig::MissingImport(index).into());
            }
        }
        // No caller allocator/foreign callback has run. The full request and
        // every explicit grant now pass; only owned descriptor copies escape.
        let mut imports = Vec::new();
        imports.try_reserve_exact(count).map_err(|_| allocation())?;
        for (index, _, binding) in checked {
            let mut name = String::new();
            let bytes = binding.name.expect("validated name");
            name.try_reserve_exact(bytes.len())
                .map_err(|_| allocation())?;
            name.push_str(std::str::from_utf8(bytes).expect("validated UTF-8"));
            imports.push(BoundImport {
                index,
                name,
                signature: binding.signature,
                function: binding.function.expect("validated function"),
                user: binding.user,
            });
        }
        Ok(Self {
            allocator: *allocator,
            limits,
            imports,
        })
    }
}
const _: () =
    assert!(std::mem::size_of::<Allocator>() == 32 && std::mem::align_of::<Allocator>() == 8);
const _: () = assert!(
    std::mem::size_of::<LimitsDescriptor>() == 72 && std::mem::align_of::<LimitsDescriptor>() == 8
);
