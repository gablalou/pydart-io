//! Pure-Rust in-memory table representation.
//!
//! Phase 1 keeps this deliberately thin: `flint-core`'s `Table` is a re-export of arrow-rs's own
//! `RecordBatch`, since Plan 01's scope is the numeric happy-path round-trip, not a bespoke
//! in-memory model. The PyO3-facing `Table` type (which composes `pyo3_arrow::PyTable`) lives in
//! `crates/flint-python/src/table.rs`.

use arrow::array::ArrayRef;

/// `flint-core`'s in-memory table representation: a thin re-export of arrow-rs's `RecordBatch`.
pub type Table = arrow::record_batch::RecordBatch;

/// Stub entry point for building an Arrow array directly from a borrowed numpy buffer, without
/// copying the underlying data.
///
/// This is a placeholder target for Plan 03's `allocation-counter`-based no-heap-allocation proof
/// (D-06b, RESEARCH.md Code Examples). Phase 1 Plan 01 does not implement the borrowing logic
/// here — the numeric happy-path `from_pandas`/`to_pandas` conversion (Task 2 of this plan) reads
/// Arrow-backed pandas columns directly via `pyo3-arrow`/the Arrow PyCapsule Interface, which does
/// not need numpy-buffer borrowing. This stub exists purely so Plan 03 needs no `flint-core`
/// signature changes when it lands the numpy-buffer zero-copy proof.
///
/// # Panics
/// Always panics via `unimplemented!` until Plan 03 fills in the real implementation.
pub fn from_numpy_buffer(_ptr: *const u8, _len: usize) -> ArrayRef {
    unimplemented!("from_numpy_buffer will be implemented for the Plan 03 zero-copy proof")
}
