//! `flint.Table`: composes `pyo3_arrow::PyTable`, never hand-rolls PyCapsule/FFI marshalling.
//!
//! Per RESEARCH.md Pattern 1 and 01-PATTERNS.md, `Table` holds a `pyo3_arrow::PyTable` as an
//! internal field and delegates the Arrow PyCapsule Interface dunders to it. `PyTable`'s own
//! `__arrow_c_schema__`/`__arrow_c_stream__` methods are private to the `pyo3-arrow` crate (not
//! `pub`), so delegation here goes through Python's own method dispatch
//! (`Bound::call_method*`) rather than a direct Rust method call — this still delegates the
//! actual FFI_ArrowArray/FFI_ArrowSchema construction entirely to `pyo3-arrow`'s already-compiled,
//! already-registered Python methods; it does not reimplement any of that marshalling.

use pyo3::prelude::*;
use pyo3::types::{PyCapsule, PyType};
use pyo3_arrow::PyTable;

use crate::error::FlintError;

/// `flint.Table`: a thin `#[pyclass]` composing `pyo3_arrow::PyTable` (D-01).
#[pyclass(name = "Table")]
pub struct Table {
    inner: Py<PyTable>,
}

#[pymethods]
impl Table {
    /// Build a `Table` from a pandas DataFrame.
    ///
    /// Phase 1 Plan 01 leaves this unimplemented (RED) on purpose — the real numeric happy-path
    /// implementation (non-null `int64[pyarrow]`/`float64[pyarrow]` columns) lands in Task 2.
    #[classmethod]
    #[pyo3(signature = (df, strict=false))]
    fn from_pandas(_cls: &Bound<'_, PyType>, df: &Bound<'_, PyAny>, strict: bool) -> PyResult<Self> {
        let _ = (df, strict);
        Err(FlintError::NotImplemented("Table.from_pandas".to_string()).into())
    }

    /// Reconstruct a pandas DataFrame from this `Table`.
    ///
    /// Phase 1 Plan 01 leaves this unimplemented (RED) on purpose; implemented in Task 2.
    #[pyo3(signature = (strict=false))]
    fn to_pandas(&self, strict: bool) -> PyResult<Py<PyAny>> {
        let _ = strict;
        Err(FlintError::NotImplemented("Table.to_pandas".to_string()).into())
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
    /// consumed by `pyarrow.table(...)` in Task 2's smoke test.
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

    /// Return a single column by name.
    ///
    /// Phase 1 Plan 01 leaves this unimplemented (RED) on purpose; implemented in Task 2 (backs
    /// the Plan 03 pointer-identity proof alongside `buffer_address`).
    fn column(&self, name: String) -> PyResult<Py<PyAny>> {
        Err(FlintError::NotImplemented(format!("Table.column({name:?})")).into())
    }

    /// Return the integer address of a column's data buffer.
    ///
    /// Used by Plan 03's pointer-identity zero-copy proof (D-06a). Phase 1 Plan 01 leaves this
    /// unimplemented (RED) on purpose; implemented in Task 2 via the arrow-rs buffer API.
    fn buffer_address(&self, index: usize) -> PyResult<usize> {
        Err(FlintError::NotImplemented(format!("Table.buffer_address({index})")).into())
    }
}
