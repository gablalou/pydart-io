//! `flint-core`: pure-Rust in-memory columnar core for Flint.
//!
//! This crate has **no** dependency on `pyo3`/`pyo3-arrow` (see SKELETON.md's Architectural
//! Decisions). It exists so the conversion/allocation logic can be tested in isolation, without a
//! Python interpreter attached — required for the Plan 03 `allocation-counter` no-heap-allocation
//! proof (D-06b).

pub mod table;

pub use table::{from_numpy_buffer, Table};
