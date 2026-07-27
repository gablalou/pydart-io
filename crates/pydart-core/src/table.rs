//! Pure-Rust in-memory table representation.
//!
//! Phase 1 keeps this deliberately thin: `pydart-core`'s `Table` is a re-export of arrow-rs's own
//! `RecordBatch`, since Plan 01's scope is the numeric happy-path round-trip, not a bespoke
//! in-memory model. The PyO3-facing `Table` type (which composes `pyo3_arrow::PyTable`) lives in
//! `crates/pydart-python/src/table.rs`.

use std::ptr::NonNull;
use std::sync::Arc;

use arrow::alloc::Allocation;
use arrow::array::{ArrayRef, PrimitiveArray};
use arrow::buffer::{Buffer, ScalarBuffer};
use arrow::datatypes::Int64Type;

/// `pydart-core`'s in-memory table representation: a thin re-export of arrow-rs's `RecordBatch`.
pub type Table = arrow::record_batch::RecordBatch;

/// Build an Arrow `Int64Array` directly from an existing, pre-allocated `i64` buffer, with NO
/// copy of the underlying data bytes.
///
/// This is the `pydart-core` (pyo3-free) analog of `pydart-python`'s `borrow_numpy_numeric_column`
/// (`crates/pydart-python/src/pandas.rs`): both wrap an existing buffer in an
/// `arrow_buffer::Buffer` via `Buffer::from_custom_allocation`, which is the specific technique
/// Plan 03's D-06b allocation-counting proof exists to certify makes zero heap allocations for
/// the data buffer. This crate has no `pyo3` dependency (see crate-level doc comment), so unlike
/// the `pydart-python` version it cannot itself tie the returned array's lifetime to a `Py<T>`
/// owner — this function only proves the buffer-wrapping technique itself is allocation-free; it
/// is exercised directly by `tests/rust/zero_copy_alloc.rs`, not by the production pandas
/// conversion path (which continues to go through `pydart-python::pandas::borrow_numpy_numeric_column`
/// for the real, GIL-safe ownership handoff).
///
/// # Safety
/// `ptr` must be valid for reads of `len` bytes, and the memory it points to must remain valid
/// (not freed, not mutated in a way that would violate `Buffer`'s immutability contract) for as
/// long as the returned array (or any clone of its underlying buffer) is used. This function
/// takes no ownership handle to keep the source buffer alive — the caller is fully responsible
/// for the source buffer's lifetime.
pub unsafe fn from_numpy_buffer(ptr: *const u8, len: usize) -> ArrayRef {
    let non_null =
        NonNull::new(ptr as *mut u8).expect("from_numpy_buffer: pointer must not be null");
    // No real owner to keep alive here (this crate has no `pyo3` dependency to hold a `Py<T>`
    // handle) -- this mirrors `Deallocation::Custom`'s drop glue running on a no-op `()`, per the
    // safety contract above: the caller manages the source buffer's actual lifetime.
    let owner: Arc<dyn Allocation> = Arc::new(());
    // SAFETY: forwarding this function's own safety contract -- `ptr`/`len` describe a caller-
    // guaranteed-valid region for the lifetime of the returned buffer.
    let buffer = unsafe { Buffer::from_custom_allocation(non_null, len, owner) };
    let scalar_buffer = ScalarBuffer::<i64>::new(buffer, 0, len / std::mem::size_of::<i64>());
    Arc::new(PrimitiveArray::<Int64Type>::new(scalar_buffer, None))
}
