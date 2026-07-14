//! `_flint`: the compiled Rust extension backing the `flint` Python package.
//!
//! This is the only crate in the workspace depending on `pyo3`/`pyo3-arrow` (SKELETON.md
//! Architectural Decisions — crate layout). `python/flint/__init__.py` re-exports `Table` from
//! this module.

mod error;
mod pandas;
mod table;

use pyo3::prelude::*;

use table::Table;

/// The compiled extension module, imported by `python/flint/__init__.py` as `flint._flint`.
#[pymodule]
fn _flint(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Table>()?;
    Ok(())
}
