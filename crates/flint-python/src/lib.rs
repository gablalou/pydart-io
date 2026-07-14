//! `_flint`: the compiled Rust extension backing the `flint` Python package.
//!
//! This is the only crate in the workspace depending on `pyo3`/`pyo3-arrow` (SKELETON.md
//! Architectural Decisions — crate layout). `python/flint/__init__.py` re-exports `Table` from
//! this module.

mod diagnostics;
mod error;
mod pandas;
mod table;

use pyo3::prelude::*;

use diagnostics::{PyFlintError, PyZeroCopyRequiredError};
use table::Table;

/// The compiled extension module, imported by `python/flint/__init__.py` as `flint._flint`.
#[pymodule]
fn _flint(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Table>()?;
    // Registered under their Python-facing names ("FlintError"/"ZeroCopyRequiredError") even
    // though the Rust identifiers are prefixed `Py*` to avoid colliding with
    // `crate::error::FlintError` (see diagnostics.rs doc comment).
    m.add("FlintError", py.get_type::<PyFlintError>())?;
    m.add("ZeroCopyRequiredError", py.get_type::<PyZeroCopyRequiredError>())?;
    Ok(())
}
