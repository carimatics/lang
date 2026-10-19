//! Native operations behind the shared RFC 0008 context gate. Raw pointer
//! validity and callback code/user lifetimes are trusted embedding contracts;
//! the common runtime performs descriptor, capability and value validation.
pub mod allocator;
pub mod context;
pub mod exports;
mod padding;
pub mod scalar;
