//! `_pydart`: the compiled Rust extension backing the `pydart` Python package.
//!
//! This is the only crate in the workspace depending on `pyo3`/`pyo3-arrow` (SKELETON.md
//! Architectural Decisions — crate layout). `python/pydart/__init__.py` re-exports `Table` from
//! this module.

mod diagnostics;
mod error;
mod import;
mod pandas;
mod table;

use pyo3::prelude::*;

use diagnostics::{PyPydartError, PyZeroCopyRequiredError};
use import::from_arrow;
use table::Table;

/// The compiled extension module, imported by `python/pydart/__init__.py` as `pydart._pydart`.
#[pymodule]
fn _pydart(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Table>()?;
    m.add_function(pyo3::wrap_pyfunction!(from_arrow, m)?)?;
    // Registered under their Python-facing names ("PydartError"/"ZeroCopyRequiredError") even
    // though the Rust identifiers are prefixed `Py*` to avoid colliding with
    // `crate::error::PydartError` (see diagnostics.rs doc comment).
    m.add("PydartError", py.get_type::<PyPydartError>())?;
    m.add("ZeroCopyRequiredError", py.get_type::<PyZeroCopyRequiredError>())?;
    Ok(())
}
