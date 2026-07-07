//! The NURBS geometry now lives in the `shared` crate so the backend can reuse
//! it for trajectory interpolation. Re-exported here so the simulator's existing
//! `crate::nurbs::*` paths keep working.
pub use rustrack_shared::nurbs::*;
