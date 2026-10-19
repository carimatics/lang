//! Restore only ordinary C padding in our own call-local output. This never
//! reads padding or repairs noncanonical fields, reserved or unused union bytes.
use lang_runtime::c_abi::{Artifact, Kind, TypeId};

/// # Safety
/// `pointer` is unique writable storage for the complete checked C type.
/// Successful callback fields, tag, reserved and unused union bytes are
/// initialized; ordinary padding need not be. No Rust view may read until all
/// padding is restored. The artifact's finite layout proves every range below.
pub(super) unsafe fn initialize(artifact: &Artifact, id: TypeId, pointer: *mut u8) {
    let ty = &artifact.types()[id.0];
    // SAFETY: caller supplies the complete checked layout; recursive fields and
    // gaps are disjoint/in-bounds. This writes bytes without reading old values.
    unsafe {
        match &ty.kind {
            Kind::I64 | Kind::Bool | Kind::Unit => {}
            Kind::Record { fields, .. } => {
                let mut end = 0;
                for field in fields {
                    pointer.add(end).write_bytes(0, field.offset as usize - end);
                    initialize(artifact, field.ty, pointer.add(field.offset as usize));
                    end = (field.offset + artifact.types()[field.ty.0].size) as usize;
                }
                // Empty records have a semantic reserved byte, not padding.
                if !fields.is_empty() {
                    pointer.add(end).write_bytes(0, ty.size as usize - end);
                }
            }
            Kind::Enum {
                cases,
                payload_offset,
                payload_size,
                ..
            } => {
                let tag = pointer.cast::<u32>().read();
                let base = *payload_offset as usize;
                pointer.add(4).write_bytes(0, base - 4);
                let payload = pointer.add(base);
                if let Some(case) = cases.get(tag as usize) {
                    let mut end = 0;
                    for (id, offset) in case.payload.iter().zip(&case.offsets) {
                        payload.add(end).write_bytes(0, *offset as usize - end);
                        initialize(artifact, *id, payload.add(*offset as usize));
                        end = (*offset + artifact.types()[id.0].size) as usize;
                    }
                    if !case.payload.is_empty() {
                        payload.add(end).write_bytes(0, case.size as usize - end);
                    }
                } else {
                    // An unknown tag has no initialized active payload. Keep
                    // the invalid tag for F10, but never inspect its union.
                    payload.write_bytes(0, *payload_size as usize);
                }
                let end = base + *payload_size as usize;
                pointer.add(end).write_bytes(0, ty.size as usize - end);
            }
        }
    }
}
