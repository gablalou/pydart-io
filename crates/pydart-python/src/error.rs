//! Centralized error boundary: one Rust error enum, one `impl From<PydartError> for PyErr`.
//!
//! Per RESEARCH.md's "Don't Hand-Roll" table and 01-PATTERNS.md's "Shared Patterns" section, Rust
//! errors are never converted to `PyErr` ad hoc scattered across conversion code — every Rust
//! error in this crate flows through `PydartError` and this single `From` impl. This is what makes
//! D-03's "clear exception naming the offending column/dtype" achievable consistently, and gives
//! later plans (strict mode, diagnostics) one place to extend.

use pyo3::exceptions::{PyNotImplementedError, PyValueError};
use pyo3::PyErr;
use thiserror::Error;

use crate::diagnostics::PyPydartError;

/// All errors raised by the Rust side of the `pydart` extension.
#[derive(Debug, Error)]
pub enum PydartError {
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

    /// A `from_parquet` `filters=[(column, operator, value), ...]` tuple whose operator string is
    /// outside the fixed D-25 six-operator set.
    ///
    /// Carries both the offending column and operator string so the raised Python exception names
    /// them directly (same "no silent best-effort behavior" precedent as `UnsupportedColumn`/
    /// `UnsupportedCodec`) -- an unrecognized operator is never silently dropped/ignored, which
    /// would otherwise return too many rows (T-03-05).
    #[error("unsupported filter operator {operator:?} on column {column:?}: expected one of \"==\", \"!=\", \"<\", \"<=\", \">\", \">=\"")]
    UnsupportedFilterOperator { column: String, operator: String },

    /// A `from_parquet` multi-file/directory read (D-21) where two files' Arrow schemas
    /// disagree.
    ///
    /// Carries the first file, the first mismatched file, and the first differing column name
    /// so the raised exception is directly actionable -- `from_parquet` NEVER silently
    /// unions/best-effort-merges divergent schemas across files (T-03-08).
    #[error("schema mismatch across Parquet files: {first_file:?} and {other_file:?} disagree on column {column:?}")]
    ParquetSchemaMismatch {
        first_file: String,
        other_file: String,
        column: String,
    },

    /// A `from_parquet` file (single, list-element, or directory-discovered) that failed to
    /// open/parse.
    ///
    /// Carries the offending path and the underlying reason (e.g. an `io::Error`/
    /// `ParquetError` message) so a missing file, non-Parquet file, or corrupt file surfaces as
    /// a named, catchable failure naming the exact path -- never a silent skip or a generic,
    /// unattributed error.
    #[error("failed to read Parquet file {path:?}: {reason}")]
    ParquetReadError { path: String, reason: String },

    /// A `from_parquet` directory/list `path` argument that resolves to ZERO files (an empty
    /// directory with no `.parquet` entries, or an empty path list), or a malformed `path`
    /// argument shape (not a `str`/`Path`/list of `str`/`Path`).
    ///
    /// Same "no silent best-effort" input-validation family as `UnsupportedCodec`/
    /// `UnsupportedFilterOperator`/`ParquetSchemaMismatch` (D-21 empty edge) -- `from_parquet`
    /// NEVER silently returns an empty `Table` for this case.
    #[error("{0}")]
    InvalidParquetPathArgument(String),

    /// Any other conversion/runtime failure not covered by a more specific variant.
    #[error("{0}")]
    Other(String),
}

impl From<PydartError> for PyErr {
    fn from(err: PydartError) -> PyErr {
        match &err {
            PydartError::NotImplemented(_) => PyNotImplementedError::new_err(err.to_string()),
            // `pydart.PydartError` (not a builtin `TypeError`) so callers get an honest, catchable
            // `pydart`-owned exception naming the offending column/dtype (D-08 / RESEARCH.md
            // Pitfall 1) instead of relying on builtin exception hierarchy semantics.
            PydartError::UnsupportedColumn { .. } => PyPydartError::new_err(err.to_string()),
            // Same treatment as `UnsupportedColumn` (D-29): a named, catchable `pydart`-owned
            // exception, not a builtin `ValueError` — a typo'd codec string is a user-facing
            // input-validation failure, not an internal conversion error.
            PydartError::UnsupportedCodec(_) => PyPydartError::new_err(err.to_string()),
            // Same treatment as UnsupportedCodec (D-25): a named, catchable pydart-owned exception
            // naming the offending column/operator, never a builtin exception.
            PydartError::UnsupportedFilterOperator { .. } => PyPydartError::new_err(err.to_string()),
            // D-21: a named, catchable pydart-owned exception naming both files and the
            // mismatched column -- never a silent union/merge.
            PydartError::ParquetSchemaMismatch { .. } => PyPydartError::new_err(err.to_string()),
            // Same treatment as PydartError::Arrow/Other: wraps an underlying IO/parse failure
            // rather than a caller-input-validation failure.
            PydartError::ParquetReadError { .. } => PyValueError::new_err(err.to_string()),
            // Same treatment as UnsupportedCodec/UnsupportedFilterOperator/ParquetSchemaMismatch:
            // a named, catchable pydart-owned exception, not a builtin ValueError -- an empty
            // directory/list is a caller-input-validation failure, not a wrapped IO/parse error.
            PydartError::InvalidParquetPathArgument(_) => PyPydartError::new_err(err.to_string()),
            PydartError::Arrow(_) => PyValueError::new_err(err.to_string()),
            PydartError::Other(_) => PyValueError::new_err(err.to_string()),
        }
    }
}
