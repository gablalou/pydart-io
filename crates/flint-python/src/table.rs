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
//! Task 2 implements the numeric happy path only (D-02, CONV-01/CONV-02): non-null
//! `int64[pyarrow]`/`float64[pyarrow]` (`pandas.ArrowDtype`-backed) columns. The full per-column
//! decision matrix, numpy-backed borrow, and bool handling are Plan 02.

use arrow::array::Array;
use pyo3::prelude::*;
use pyo3::types::{PyCapsule, PyDict, PyType};
use pyo3_arrow::PyTable;

use crate::error::FlintError;

/// The `pandas.ArrowDtype.name` values this phase's numeric happy path accepts.
///
/// Note: pandas canonicalizes the constructor alias `"float64[pyarrow]"` to `"double[pyarrow]"`
/// (Arrow's own type name for float64) — both are accepted at construction time, but `.dtype.name`
/// always reports `"double[pyarrow]"`.
const SUPPORTED_ARROW_DTYPE_NAMES: [&str; 2] = ["int64[pyarrow]", "double[pyarrow]"];

/// `flint.Table`: a thin `#[pyclass]` composing `pyo3_arrow::PyTable` (D-01).
#[pyclass(name = "Table")]
pub struct Table {
    inner: Py<PyTable>,
}

#[pymethods]
impl Table {
    /// Build a `Table` from a pandas DataFrame (numeric happy path only, D-02).
    ///
    /// Reads each column's already-Arrow-owned memory (via pandas' own
    /// `DataFrame.__arrow_c_stream__` PyCapsule export) and imports it into the composed
    /// `pyo3_arrow::PyTable` without copying the data buffers. Any column that is not a supported
    /// numeric `ArrowDtype` raises a `FlintError::UnsupportedColumn` naming the offending column
    /// (no silent copy) — the full strict-mode/diagnostics surface is Plan 02.
    #[classmethod]
    #[pyo3(signature = (df, strict=false))]
    fn from_pandas(
        _cls: &Bound<'_, PyType>,
        py: Python<'_>,
        df: &Bound<'_, PyAny>,
        strict: bool,
    ) -> PyResult<Self> {
        let _ = strict; // full strict-mode surface (DIAG-01) is Plan 02

        reject_unsupported_columns(py, df)?;

        // Delegate the actual Arrow C Data Interface marshalling to pandas' own PyCapsule export
        // and pyo3-arrow's own PyCapsule import — no hand-rolled FFI_ArrowArray/Schema here.
        let capsule: Bound<'_, PyCapsule> = df.call_method0("__arrow_c_stream__")?.extract()?;
        let py_table = PyTable::from_arrow_pycapsule(&capsule)?;

        Ok(Self {
            inner: Py::new(py, py_table)?,
        })
    }

    /// Reconstruct a pandas DataFrame from this `Table`, with `ArrowDtype`-backed columns sharing
    /// the Table's Arrow buffers.
    ///
    /// Composes `pyo3_arrow::PyTable::into_pyarrow` (already-existing, already-correct PyCapsule
    /// export to a `pyarrow.Table`) with pyarrow's own documented
    /// `Table.to_pandas(types_mapper=pandas.ArrowDtype)` conversion, which pandas' own ArrowDtype
    /// machinery performs without copying when the target dtype already matches.
    #[pyo3(signature = (strict=false))]
    fn to_pandas(&self, py: Python<'_>, strict: bool) -> PyResult<Py<PyAny>> {
        let _ = strict; // full strict-mode surface (DIAG-01) is Plan 02

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
}

/// Validate that every column in `df` is a supported numeric `pandas.ArrowDtype` column (Phase 1
/// happy path: non-null `int64[pyarrow]`/`float64[pyarrow]`).
///
/// Raises `FlintError::UnsupportedColumn` naming the offending column and its dtype for the first
/// column outside this scope, rather than silently copying (Pitfall 1/anti-pattern in
/// RESEARCH.md/01-PATTERNS.md — bool and any non-`ArrowDtype` column are explicitly out of scope
/// for this plan and must never be silently accepted).
fn reject_unsupported_columns(py: Python<'_>, df: &Bound<'_, PyAny>) -> PyResult<()> {
    let pandas = py.import("pandas")?;
    let arrow_dtype_type = pandas.getattr("ArrowDtype")?;

    let columns = df.getattr("columns")?;
    for column_name in columns.try_iter()? {
        let column_name = column_name?;
        let column_name_str: String = column_name.str()?.extract()?;
        let series = df.get_item(&column_name)?;
        let dtype = series.getattr("dtype")?;

        if !dtype.is_instance(&arrow_dtype_type)? {
            let dtype_str: String = dtype.str()?.extract()?;
            return Err(FlintError::UnsupportedColumn {
                column: column_name_str,
                dtype: dtype_str,
                reason: "only pandas.ArrowDtype-backed numeric (int64/float64) columns are \
                         supported in this phase"
                    .to_string(),
            }
            .into());
        }

        let dtype_name: String = dtype.getattr("name")?.extract()?;
        if !SUPPORTED_ARROW_DTYPE_NAMES.contains(&dtype_name.as_str()) {
            return Err(FlintError::UnsupportedColumn {
                column: column_name_str,
                dtype: dtype_name,
                reason: "only int64[pyarrow]/double[pyarrow] (float64) numeric columns are \
                         supported in this phase"
                    .to_string(),
            }
            .into());
        }
    }

    Ok(())
}
