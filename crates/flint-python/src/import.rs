//! CAP-02: `flint.from_arrow` — import a foreign Arrow PyCapsule-compliant object into a `Table`.
//!
//! Per RESEARCH.md Pattern 2 (lines 159-162, 296-307) and 01-PATTERNS.md, this composes
//! `pyo3_arrow::PyTable`'s own `FromPyObject` impl rather than hand-rolling
//! `FFI_ArrowArray`/`FFI_ArrowSchema` construction (`.claude/CLAUDE.md` "What NOT to Use"). That
//! impl already performs the untrusted-input validation this phase's Security Domain requires
//! (T-01-08, RESEARCH.md V5): it checks for a non-null capsule pointer
//! (`PyCapsule::pointer_checked`) and validates schema/array-length/offset consistency
//! (`arrow_array::ffi::from_ffi`) before any array is constructed, surfacing a `PyErr`
//! (`PyValueError`/`PyTypeError`) rather than panicking or segfaulting on a malformed capsule.
//! This module adds no unchecked `unsafe` shortcut around that path — its only job is to remap
//! whatever `PyErr` pyo3-arrow's import produced onto `flint.FlintError` (this project's own
//! catchable error type, per the established error-boundary convention) and compose the result
//! into this project's `Table` (D-01).

use pyo3::prelude::*;
use pyo3_arrow::PyTable;

use crate::diagnostics::PyFlintError;
use crate::table::Table;

/// Import a foreign Arrow object (pyarrow Table, Polars DataFrame, DuckDB relation, ...) into a
/// flint `Table`, zero-copy, via the Arrow PyCapsule Interface (CAP-02).
///
/// `obj` is intentionally typed `&Bound<'_, PyAny>` rather than `PyTable` directly in the
/// function signature. Binding the parameter as `PyTable` would make PyO3 run pyo3-arrow's
/// `FromPyObject` extraction (and thus consume `obj`'s `__arrow_c_stream__`/`__arrow_c_array__`
/// capsule) during PyO3's own argument-binding step, *before* this function's body executes --
/// leaving no place to catch a validation failure and remap it onto `flint.FlintError`. Calling
/// `.extract()` explicitly inside the body keeps a single, explicit call site for that
/// extraction, satisfying both the consume-exactly-once requirement (T-01-09, RESEARCH.md
/// Pitfall 3 -- DuckDB relations are the documented non-idempotent producer) and the
/// error-remapping requirement (T-01-08) in one place.
#[pyfunction]
pub fn from_arrow(py: Python<'_>, obj: &Bound<'_, PyAny>) -> PyResult<Table> {
    let py_table: PyTable = obj.extract().map_err(|err| {
        PyFlintError::new_err(format!(
            "from_arrow: foreign object does not comply with the Arrow PyCapsule Interface, or \
             its exported schema/array is inconsistent: {err}"
        ))
    })?;

    Table::from_pytable(py, py_table)
}
