//! Centralized error boundary: one Rust error enum, one `impl From<FlintError> for PyErr`.
//!
//! Per RESEARCH.md's "Don't Hand-Roll" table and 01-PATTERNS.md's "Shared Patterns" section, Rust
//! errors are never converted to `PyErr` ad hoc scattered across conversion code — every Rust
//! error in this crate flows through `FlintError` and this single `From` impl. This is what makes
//! D-03's "clear exception naming the offending column/dtype" achievable consistently, and gives
//! later plans (strict mode, diagnostics) one place to extend.

use pyo3::exceptions::{PyNotImplementedError, PyValueError};
use pyo3::PyErr;
use thiserror::Error;

use crate::diagnostics::PyFlintError;

/// All errors raised by the Rust side of the `flint` extension.
#[derive(Debug, Error)]
pub enum FlintError {
    /// A feature that exists in the public API surface but is not yet implemented.
    #[error("{0} is not yet implemented")]
    NotImplemented(String),

    /// A pandas column's dtype is outside this phase's supported numeric happy path.
    ///
    /// Carries the offending column name so the raised Python exception can name it directly
    /// (D-03) rather than surfacing a generic, unattributed conversion failure.
    #[error("column {column:?} (dtype={dtype}) is not supported: {reason}")]
    UnsupportedColumn {
        column: String,
        dtype: String,
        reason: String,
    },

    /// An underlying arrow-rs error surfaced during conversion.
    #[error("arrow error: {0}")]
    Arrow(#[from] arrow::error::ArrowError),

    /// A `to_parquet` `compression` argument outside the four D-29 supported codecs.
    ///
    /// Carries the offending codec string so the raised Python exception names it directly (same
    /// "no silent best-effort behavior" precedent as `UnsupportedColumn`) — an unrecognized codec
    /// is never silently coerced to snappy or any other default.
    #[error("unsupported compression codec {0:?}: expected one of \"snappy\", \"zstd\", \"gzip\", \"uncompressed\"")]
    UnsupportedCodec(String),

    /// Any other conversion/runtime failure not covered by a more specific variant.
    #[error("{0}")]
    Other(String),
}

impl From<FlintError> for PyErr {
    fn from(err: FlintError) -> PyErr {
        match &err {
            FlintError::NotImplemented(_) => PyNotImplementedError::new_err(err.to_string()),
            // `flint.FlintError` (not a builtin `TypeError`) so callers get an honest, catchable
            // `flint`-owned exception naming the offending column/dtype (D-08 / RESEARCH.md
            // Pitfall 1) instead of relying on builtin exception hierarchy semantics.
            FlintError::UnsupportedColumn { .. } => PyFlintError::new_err(err.to_string()),
            // Same treatment as `UnsupportedColumn` (D-29): a named, catchable `flint`-owned
            // exception, not a builtin `ValueError` — a typo'd codec string is a user-facing
            // input-validation failure, not an internal conversion error.
            FlintError::UnsupportedCodec(_) => PyFlintError::new_err(err.to_string()),
            FlintError::Arrow(_) => PyValueError::new_err(err.to_string()),
            FlintError::Other(_) => PyValueError::new_err(err.to_string()),
        }
    }
}
