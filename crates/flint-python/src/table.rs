//! `flint.Table`: composes `pyo3_arrow::PyTable`, never hand-rolls PyCapsule/FFI marshalling.
//!
//! Per RESEARCH.md Pattern 1 and 01-PATTERNS.md, `Table` holds a `pyo3_arrow::PyTable` as an
//! internal field and delegates the Arrow PyCapsule Interface dunders to it. `PyTable`'s own
//! `__arrow_c_schema__`/`__arrow_c_stream__`/`column` methods are private to the `pyo3-arrow`
//! crate (not `pub`), so delegation here goes through Python's own method dispatch
//! (`Bound::call_method*`) rather than a direct Rust method call — this still delegates the
//! actual FFI_ArrowArray/FFI_ArrowSchema construction entirely to `pyo3-arrow`'s already-compiled,
//! already-registered Python methods; it does not reimplement any of that marshalling.
//!
//! `from_pandas`'s per-column copy-vs-borrow decision logic lives in `crate::pandas` (driven by
//! `flint_core::pandas_plan::plan_column`, the single source of truth also consumed by Task 2's
//! strict mode / `copy_report()`). This module stays a thin `#[pyclass]` shell.
//!
//! ## `to_pandas` reverse-direction zero-copy confirmation (CONV-02)
//!
//! `to_pandas` goes through `pyo3_arrow::PyTable::into_pyarrow` (a real `pyarrow.Table`,
//! constructed from this `Table`'s own Arrow buffers with no data copy) and then pyarrow's own
//! `Table.to_pandas(types_mapper=pandas.ArrowDtype)`. This was empirically confirmed this plan
//! (against the pinned pandas 3.0.3 / pyarrow 25.0.0) to be genuinely zero-copy: constructing a
//! pyarrow `Table` from a known buffer address and round-tripping it through
//! `to_pandas(types_mapper=pandas.ArrowDtype)` produces a pandas `ArrowDtype` column whose
//! underlying `._pa_array` chunk buffer address is IDENTICAL to the original — pandas'
//! `ArrowExtensionArray` wraps the pyarrow `ChunkedArray` by reference, it does not copy the data
//! buffer. This is the "pyarrow-intermediary" mechanism RESEARCH.md anticipated as a possible
//! fallback; the spike confirms it is not a silently-copying one. No code change was needed here
//! versus Plan 01's implementation — it was already correct and already generic across dtypes
//! (not restricted to numeric), so Task 1's bool support flows through unchanged.

use arrow::array::Array;
use pyo3::prelude::*;
use pyo3::types::{PyCapsule, PyDict, PyType};
use pyo3_arrow::PyTable;

use crate::diagnostics;
use crate::error::FlintError;
use crate::pandas::{self, ColumnConversionRecord};

/// `flint.Table`: a thin `#[pyclass]` composing `pyo3_arrow::PyTable` (D-01).
#[pyclass(name = "Table")]
pub struct Table {
    inner: Py<PyTable>,
    /// The per-column conversion decision `from_pandas` actually made, retained so
    /// `copy_report()` reports the real outcome rather than re-deriving a (possibly-diverging)
    /// decision. Empty for a `Table` not constructed via `from_pandas` (e.g. future PyCapsule
    /// import, Plan 04).
    column_reports: Vec<ColumnConversionRecord>,
}

#[pymethods]
impl Table {
    /// Build a `Table` from a pandas DataFrame, driving every column's copy-vs-borrow decision
    /// through `plan_column` (CONV-01/CONV-02, full numeric+bool matrix).
    ///
    /// Any column outside this phase's numeric/bool scope raises `FlintError::UnsupportedColumn`
    /// naming the offending column and dtype (no silent copy).
    ///
    /// When `strict=True` (D-03, DIAG-01): `pandas::from_pandas` always computes AND applies
    /// every column's plan (conversion happens regardless of `strict`); this function then reads
    /// the resulting per-column records and, if ANY column's plan was `RequiresCopy`, discards the
    /// already-built batch and raises `flint.ZeroCopyRequiredError` naming the first offending
    /// column and its dtype. This is honest about being a per-column decision read off the real,
    /// already-computed plan for every column -- never a whole-table try/catch that loses
    /// per-column attribution (RESEARCH.md Pitfall 2) -- but it is NOT a zero-work pre-conversion
    /// gate: the copy this rejects has already happened once before being discarded. The
    /// *observable* contract (a caller never receives a copied `Table` under `strict=True`) is
    /// unaffected. With `strict=False` (default), `RequiresCopy` columns are converted with a
    /// copy and no exception is raised.
    #[classmethod]
    #[pyo3(signature = (df, strict=false))]
    fn from_pandas(
        _cls: &Bound<'_, PyType>,
        py: Python<'_>,
        df: &Bound<'_, PyAny>,
        strict: bool,
    ) -> PyResult<Self> {
        let outcome = pandas::from_pandas(py, df)?;

        if strict {
            diagnostics::check_strict(&outcome.records)?;
        }

        let schema = outcome.batch.schema();
        let py_table = PyTable::try_new(vec![outcome.batch], schema)?;

        Ok(Self {
            inner: Py::new(py, py_table)?,
            column_reports: outcome.records,
        })
    }

    /// Reconstruct a pandas DataFrame from this `Table`, with `ArrowDtype`-backed columns sharing
    /// the Table's Arrow buffers.
    ///
    /// Composes `pyo3_arrow::PyTable::into_pyarrow` (already-existing, already-correct PyCapsule
    /// export to a `pyarrow.Table`) with pyarrow's own documented
    /// `Table.to_pandas(types_mapper=pandas.ArrowDtype)` conversion, which pandas' own ArrowDtype
    /// machinery performs without copying when the target dtype already matches.
    ///
    /// Deliberately does NOT call `plan_column` per output column: every column of a `Table` is,
    /// by construction, already Arrow memory (`RecordBatch` columns), so `plan_column`'s backend
    /// input would always be `DtypeBackend::Arrow`, which always resolves to `ZeroCopyBorrow`
    /// regardless of `ArrowKind`. There is no copy-vs-borrow DECISION to make on the way out (see
    /// SUMMARY Deviations for the full reasoning) -- unlike `from_pandas`, which must classify an
    /// incoming column's backend/contiguity before it knows whether a copy is required.
    /// `strict` is accepted for API symmetry with `from_pandas` but is a no-op here: `to_pandas`
    /// is unconditionally zero-copy (confirmed above), so it can never have anything to reject.
    #[pyo3(signature = (strict=false))]
    fn to_pandas(&self, py: Python<'_>, strict: bool) -> PyResult<Py<PyAny>> {
        let _ = strict; // unconditionally zero-copy; see doc comment above

        let batches = self.inner.bind(py).get().batches().to_vec();
        let schema = batches.first().map(|batch| batch.schema()).ok_or_else(|| {
            FlintError::Other("cannot reconstruct a pandas DataFrame from an empty Table".to_string())
        })?;
        let owned_table = PyTable::try_new(batches, schema)?;
        let pa_table = owned_table.into_pyarrow(py)?;

        let pandas = py.import("pandas")?;
        let arrow_dtype = pandas.getattr("ArrowDtype")?;
        let kwargs = PyDict::new(py);
        kwargs.set_item("types_mapper", arrow_dtype)?;
        let df = pa_table.call_method("to_pandas", (), Some(&kwargs))?;
        Ok(df.unbind())
    }

    /// Export this table's schema via the Arrow PyCapsule Interface.
    ///
    /// Delegates directly to the composed `pyo3_arrow::PyTable`'s own `__arrow_c_schema__` — no
    /// hand-rolled `FFI_ArrowSchema` construction here.
    fn __arrow_c_schema__<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyCapsule>> {
        Ok(self
            .inner
            .bind(py)
            .call_method0("__arrow_c_schema__")?
            .extract()?)
    }

    /// Export this table's data as an Arrow C Stream via the Arrow PyCapsule Interface.
    ///
    /// Delegates directly to the composed `pyo3_arrow::PyTable`'s own `__arrow_c_stream__` — no
    /// hand-rolled `FFI_ArrowArray`/stream construction here. This is CAP-01's export path,
    /// consumed by `pyarrow.table(...)` in the export smoke test.
    #[pyo3(signature = (requested_schema=None))]
    fn __arrow_c_stream__<'py>(
        &self,
        py: Python<'py>,
        requested_schema: Option<Bound<'py, PyCapsule>>,
    ) -> PyResult<Bound<'py, PyCapsule>> {
        let bound = self.inner.bind(py);
        let capsule = match requested_schema {
            Some(schema) => bound.call_method1("__arrow_c_stream__", (schema,))?,
            None => bound.call_method0("__arrow_c_stream__")?,
        };
        Ok(capsule.extract()?)
    }

    /// Return a single column by name, delegating to the composed `pyo3_arrow::PyTable`'s own
    /// `column` method (Python dispatch, same rationale as the PyCapsule dunders above).
    fn column(&self, py: Python<'_>, name: String) -> PyResult<Py<PyAny>> {
        Ok(self
            .inner
            .bind(py)
            .call_method1("column", (name,))?
            .unbind())
    }

    /// Return the integer address of a column's data buffer (D-06, backs Plan 03's
    /// pointer-identity zero-copy proof).
    ///
    /// `index` selects the column (0-based) within this `Table`'s first `RecordBatch`. Uses the
    /// arrow-rs buffer API (`Array::to_data` / `ArrayData::buffers`) directly — `ArrayData::clone`
    /// only bumps buffer reference counts, it does not copy the underlying bytes.
    fn buffer_address(&self, py: Python<'_>, index: usize) -> PyResult<usize> {
        let bound = self.inner.bind(py);
        let batches = bound.get().batches();
        let batch = batches
            .first()
            .ok_or_else(|| FlintError::Other("Table has no record batches".to_string()))?;

        if index >= batch.num_columns() {
            return Err(FlintError::Other(format!(
                "column index {index} out of range (table has {} columns)",
                batch.num_columns()
            ))
            .into());
        }

        let array_data = batch.column(index).to_data();
        let address = array_data
            .buffers()
            .first()
            .map(|buffer| buffer.as_ptr() as usize)
            .unwrap_or(0);
        Ok(address)
    }

    /// Return per-column zero-copy diagnostics (DIAG-02, D-04): one `flint.ColumnCopyStatus` per
    /// column, derived from the SAME per-column plan `from_pandas` used to build this `Table` --
    /// not a re-derived decision, so this can never silently disagree with strict mode (T-01-05).
    fn copy_report(&self, py: Python<'_>) -> PyResult<Vec<Py<PyAny>>> {
        diagnostics::build_copy_report(py, &self.column_reports)
    }
}
