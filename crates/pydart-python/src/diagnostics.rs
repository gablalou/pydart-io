//! Strict-mode (DIAG-01, D-03) and `copy_report()` (DIAG-02, D-04) diagnostics surface.
//!
//! Both features consume the SAME per-column `ColumnConversionRecord`s produced by
//! `crate::pandas::from_pandas` (itself driven by `pydart_core::pandas_plan::plan_column`, the one
//! decision matrix) -- neither re-derives the copy-vs-borrow decision, so they can never silently
//! disagree (T-01-05 / RESEARCH.md Pitfall 2 / apache/arrow#39194).

use pyo3::create_exception;
use pyo3::exceptions::PyException;
use pyo3::prelude::*;

use crate::pandas::ColumnConversionRecord;

// Rust identifiers are prefixed `Py*` to avoid colliding with `crate::error::PydartError` (the
// internal thiserror enum used for generic conversion errors) -- the Python-visible class names
// ("PydartError"/"ZeroCopyRequiredError", registered in `lib.rs`) are unaffected by this.
create_exception!(
    _pydart,
    PyPydartError,
    PyException,
    "Base class for all pydart-raised errors."
);

create_exception!(
    _pydart,
    PyZeroCopyRequiredError,
    PyPydartError,
    "Raised in strict zero-copy mode (`from_pandas(df, strict=True)`) when a column would \
     require a copy (D-03)."
);

/// Pre-flight, per-column strict-mode check (D-03).
///
/// Takes the FULL per-column plan (`records`, already computed by `from_pandas` for every
/// column) and decides: if ANY column's plan is `RequiresCopy`, raise
/// `pydart.ZeroCopyRequiredError` naming the first offending column and dtype, with the reason a
/// copy was required. This is never a whole-table try/catch around the conversion itself
/// (RESEARCH.md Pitfall 2 / apache/arrow#39194) -- the decision is read directly off the explicit
/// per-column plan.
pub fn check_strict(records: &[ColumnConversionRecord]) -> PyResult<()> {
    if let Some(offending) = records.iter().find(|record| !record.zero_copy) {
        let reason = offending
            .reason
            .as_deref()
            .unwrap_or("reason unavailable");
        return Err(PyZeroCopyRequiredError::new_err(format!(
            "column {:?} (dtype={}) requires a copy: {}",
            offending.column, offending.dtype, reason
        )));
    }
    Ok(())
}

/// Build the `list[pydart.ColumnCopyStatus]` returned by `Table.copy_report()` (D-04, DIAG-02),
/// from the SAME per-column records `from_pandas` produced when the `Table` was built -- this
/// reflects the actual conversion that occurred, not a re-derived (possibly-diverging) decision.
pub fn build_copy_report(
    py: Python<'_>,
    records: &[ColumnConversionRecord],
) -> PyResult<Vec<Py<PyAny>>> {
    let pydart = py.import("pydart")?;
    let status_type = pydart.getattr("ColumnCopyStatus")?;

    records
        .iter()
        .map(|record| {
            let instance = status_type.call1((
                record.column.clone(),
                record.dtype.clone(),
                record.zero_copy,
                record.reason.clone(),
            ))?;
            Ok(instance.unbind())
        })
        .collect()
}
