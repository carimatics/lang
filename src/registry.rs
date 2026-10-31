//! Host-side registry boundaries; archive admission performs no I/O or guest execution.
pub mod archive;
pub mod authority;
mod budget;
pub mod store;
pub mod transport;
