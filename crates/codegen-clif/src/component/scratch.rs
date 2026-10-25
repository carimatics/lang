//! Reserved, callback-local storage for optional destructor reports. No guest
//! allocator or control operation is needed after mandatory cleanup starts.
use super::{NativeContext, m, native};
use lang_runtime::allocation::Request;
use std::{ops::Range, ptr};

const BYTES: usize = 16 * 1024;
const HEADERS: usize = 64;

struct Slot {
    range: Option<Range<usize>>,
}
pub(super) struct Scratch {
    bytes: Vec<u8>,
    slots: Vec<Slot>,
    used: usize,
    omitted: usize,
}
impl Scratch {
    pub(super) fn prepare(budget: &mut m::Budget) -> Result<Self, m::Error> {
        let mut bytes = budget.vector(BYTES)?;
        bytes.resize(BYTES, 0);
        Ok(Self {
            bytes,
            slots: budget.vector(HEADERS)?,
            used: 0,
            omitted: 0,
        })
    }
    pub(super) fn reset(&mut self) {
        self.slots.clear();
        self.used = 0;
        self.omitted = 0;
    }
    pub(super) fn take_omitted(&mut self) -> usize {
        std::mem::take(&mut self.omitted)
    }
    fn refuse(&mut self) -> u32 {
        self.omitted = self.omitted.saturating_add(1);
        12
    }
    fn slot(&self, buffer: *mut native::Buffer) -> Result<usize, u32> {
        self.slots
            .iter()
            .position(|slot| ptr::from_ref(slot).cast_mut().cast::<native::Buffer>() == buffer)
            .ok_or(2)
    }
}
impl NativeContext for Scratch {
    fn allocator_identity(&self) -> u64 {
        // Scratch has no guest allocation authority. The bridge obtains the
        // original identity from its separately retained cleanup Memory.
        0
    }
    fn buffer_create(&mut self, length: u64, alignment: u64) -> Result<*mut native::Buffer, u32> {
        let request = Request::new(length, alignment).map_err(|_| 2u32)?;
        if self.slots.len() == HEADERS {
            return Err(self.refuse());
        }
        let base = self.bytes.as_ptr() as usize;
        let mask = request.alignment() as usize - 1;
        let start = base
            .checked_add(self.used)
            .and_then(|address| address.checked_add(mask))
            .map(|address| (address & !mask) - base)
            .ok_or_else(|| self.refuse())?;
        let end = start
            .checked_add(request.bytes() as usize)
            .filter(|end| *end <= self.bytes.len())
            .ok_or_else(|| self.refuse())?;
        self.bytes[start..end].fill(0);
        self.used = end;
        self.slots.push(Slot {
            range: Some(start..end),
        });
        Ok(ptr::from_ref(self.slots.last().expect("reserved header"))
            .cast_mut()
            .cast())
    }
    fn buffer_drop(&mut self, buffer: *mut native::Buffer) -> Result<(), u32> {
        let index = self.slot(buffer)?;
        self.slots[index].range.take().ok_or(5u32)?;
        Ok(())
    }
    fn buffer_mut_bytes(&mut self, buffer: *mut native::Buffer) -> Result<native::MutBytes, u32> {
        let index = self.slot(buffer)?;
        let range = self.slots[index].range.clone().ok_or(5u32)?;
        let bytes = &mut self.bytes[range];
        Ok(native::MutBytes {
            data: if bytes.is_empty() {
                ptr::null_mut()
            } else {
                bytes.as_mut_ptr()
            },
            length: bytes.len() as u64,
        })
    }
    fn resource_read(&self, _: u32) -> Result<native::Bytes, u32> {
        Err(4)
    }
    fn resource_write(&self, _: u32) -> Result<native::MutBytes, u32> {
        Err(4)
    }
    fn resource_publish(&mut self, _: &[u8], _: *mut native::Buffer) -> Result<u32, u32> {
        Err(4)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch() -> Scratch {
        Scratch::prepare(&mut m::Budget::new(Default::default())).unwrap()
    }

    #[test]
    fn cleanup_views_are_aligned_zeroed_and_keep_dropped_handles_as_tombstones() {
        let mut scratch = scratch();
        let first = scratch.buffer_create(17, 64).unwrap();
        let view = scratch.buffer_mut_bytes(first).unwrap();
        assert_eq!(view.data as usize % 64, 0);
        // SAFETY: this exact live scratch buffer retains 17 uniquely borrowed
        // initialized bytes until drop; no second view is used concurrently.
        unsafe {
            assert_eq!(std::slice::from_raw_parts(view.data, 17), &[0; 17]);
            view.data.write_bytes(0xff, 17);
        }
        scratch.buffer_drop(first).unwrap();
        assert_eq!(scratch.buffer_drop(first), Err(5));
        assert_eq!(scratch.buffer_mut_bytes(first).err(), Some(5));
        let second = scratch.buffer_create(17, 64).unwrap();
        assert_ne!(first, second);
        let view = scratch.buffer_mut_bytes(second).unwrap();
        // SAFETY: second is the current live unique 17-byte scratch buffer.
        assert_eq!(
            unsafe { std::slice::from_raw_parts(view.data, 17) },
            &[0; 17]
        );
        assert_eq!(scratch.buffer_mut_bytes(first).err(), Some(5));
        assert_eq!(scratch.take_omitted(), 0);
    }

    #[test]
    fn cleanup_capacity_refusal_is_bounded_even_after_drop_and_resets_at_exit() {
        let mut scratch = scratch();
        let first = scratch.buffer_create(BYTES as u64, 1).unwrap();
        scratch.buffer_drop(first).unwrap();
        assert_eq!(scratch.buffer_create(1, 1), Err(12));
        assert_eq!(scratch.take_omitted(), 1);
        scratch.reset();
        for _ in 0..HEADERS {
            let buffer = scratch.buffer_create(0, 1).unwrap();
            scratch.buffer_drop(buffer).unwrap();
        }
        assert_eq!(scratch.buffer_create(0, 1), Err(12));
        scratch.omitted = usize::MAX;
        assert_eq!(scratch.buffer_create(0, 1), Err(12));
        assert_eq!(scratch.take_omitted(), usize::MAX);
        scratch.reset();
        assert!(scratch.buffer_create(BYTES as u64, 1).is_ok());
        assert_eq!(scratch.take_omitted(), 0);
    }

    #[test]
    fn cleanup_refuses_resource_authority_without_consuming_the_buffer() {
        let mut scratch = scratch();
        assert_eq!(scratch.buffer_create(1, 3), Err(2));
        assert_eq!(scratch.buffer_create(u64::MAX, 1), Err(2));
        assert_eq!(scratch.take_omitted(), 0);
        let buffer = scratch.buffer_create(8, 8).unwrap();
        assert_eq!(scratch.resource_publish(b"untrusted", buffer), Err(4));
        assert_eq!(scratch.resource_read(1).err(), Some(4));
        assert_eq!(scratch.resource_write(1).err(), Some(4));
        assert_eq!(scratch.buffer_mut_bytes(buffer).unwrap().length, 8);
        scratch.buffer_drop(buffer).unwrap();
        assert_eq!(scratch.buffer_drop(ptr::null_mut()), Err(2));
    }
}
