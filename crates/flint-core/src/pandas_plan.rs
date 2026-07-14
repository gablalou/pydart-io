//! Single source-of-truth per-column pandas<->Arrow conversion decision.
//!
//! `plan_column` is the ONE function that both the `from_pandas`/`to_pandas` conversion path
//! (`crates/flint-python/src/pandas.rs`) and the strict-mode/`copy_report()` diagnostics surface
//! (`crates/flint-python/src/diagnostics.rs`) consume. Per RESEARCH.md Pitfall 2
//! (apache/arrow#39194), the decision MUST be made per-column and MUST be the same decision for
//! both features -- never implement this matrix twice, and never gate strict mode with a
//! whole-table try/catch.
//!
//! This module has no `pyo3`/`pyo3-arrow` dependency (see `flint-core`'s crate-level doc comment)
//! so the matrix itself can be unit-tested without a Python interpreter attached.

/// Which memory layout backs a pandas column's dtype.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DtypeBackend {
    /// `pandas.ArrowDtype`-backed: the column's data is already stored as an Arrow array
    /// (pyarrow `ChunkedArray`) inside pandas' own `ArrowExtensionArray`.
    Arrow,
    /// Default numpy-backed: the column's data is a plain numpy `ndarray`.
    Numpy,
}

/// The logical Arrow type category a column's values fall into, at this phase's scope
/// (numeric or boolean only -- nulls/strings/categoricals/datetimes are Phase 2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ArrowKind {
    /// Any integer or floating-point numeric type (int8..int64, uint8..uint64, float32/float64).
    Numeric,
    /// Boolean. Called out as its own variant (not folded into `Numeric`) because numpy packs
    /// bool at 1 byte/element while Arrow packs it at 1 bit/element -- see RESEARCH.md Pitfall 1.
    Bool,
}

/// The result of planning a single column's conversion: can it be borrowed as-is (zero-copy),
/// or does it require an actual data copy (and if so, why)?
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ColumnPlan {
    /// The column's existing buffer can be borrowed directly with no data copy.
    ZeroCopyBorrow,
    /// Converting this column requires an actual copy of the data buffer.
    RequiresCopy {
        /// Human-readable explanation of why a copy is required, surfaced verbatim in
        /// `ZeroCopyRequiredError` messages (D-03) and `ColumnCopyStatus.reason` (D-04).
        reason: String,
    },
}

/// Decide how a single pandas column should be converted, given its dtype backend, logical
/// Arrow type, and (for numpy-backed columns) whether its underlying buffer is contiguous.
///
/// This is the locked decision matrix from RESEARCH.md Pattern 3 / 01-PATTERNS.md:
///
/// | Backend | Kind    | Contiguous | Plan            |
/// |---------|---------|------------|-----------------|
/// | Arrow   | Numeric | (n/a)      | `ZeroCopyBorrow`|
/// | Arrow   | Bool    | (n/a)      | `ZeroCopyBorrow`|
/// | Numpy   | Numeric | true       | `ZeroCopyBorrow`|
/// | Numpy   | Numeric | false      | `RequiresCopy`  |
/// | Numpy   | Bool    | (n/a)      | `RequiresCopy`  |
///
/// `is_contiguous` is ignored for `DtypeBackend::Arrow` columns (already Arrow's own memory,
/// always zero-copy-borrowable at this phase's scope) and for `Numpy`+`Bool` (bit-packing means
/// a numpy bool column always requires a copy, regardless of contiguity).
pub fn plan_column(dtype_backend: DtypeBackend, arrow_kind: ArrowKind, is_contiguous: bool) -> ColumnPlan {
    match (dtype_backend, arrow_kind) {
        (DtypeBackend::Arrow, ArrowKind::Numeric) => ColumnPlan::ZeroCopyBorrow,
        (DtypeBackend::Arrow, ArrowKind::Bool) => ColumnPlan::ZeroCopyBorrow,
        (DtypeBackend::Numpy, ArrowKind::Numeric) if is_contiguous => ColumnPlan::ZeroCopyBorrow,
        (DtypeBackend::Numpy, ArrowKind::Numeric) => ColumnPlan::RequiresCopy {
            reason: "numpy buffer is not contiguous (or has a non-standard stride); cannot be \
                     borrowed as a flat contiguous buffer without risking an out-of-bounds read"
                .to_string(),
        },
        (DtypeBackend::Numpy, ArrowKind::Bool) => ColumnPlan::RequiresCopy {
            reason: "numpy bool is stored as 1 byte per element while Arrow bool is bit-packed \
                     at 1 bit per element; converting requires a repacking copy"
                .to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_column_arrow_numeric_is_zero_copy_borrow() {
        assert_eq!(
            plan_column(DtypeBackend::Arrow, ArrowKind::Numeric, true),
            ColumnPlan::ZeroCopyBorrow
        );
        // is_contiguous is irrelevant for Arrow-backed columns.
        assert_eq!(
            plan_column(DtypeBackend::Arrow, ArrowKind::Numeric, false),
            ColumnPlan::ZeroCopyBorrow
        );
    }

    #[test]
    fn plan_column_arrow_bool_is_zero_copy_borrow() {
        assert_eq!(
            plan_column(DtypeBackend::Arrow, ArrowKind::Bool, true),
            ColumnPlan::ZeroCopyBorrow
        );
        assert_eq!(
            plan_column(DtypeBackend::Arrow, ArrowKind::Bool, false),
            ColumnPlan::ZeroCopyBorrow
        );
    }

    #[test]
    fn plan_column_numpy_bool_requires_copy() {
        assert!(matches!(
            plan_column(DtypeBackend::Numpy, ArrowKind::Bool, true),
            ColumnPlan::RequiresCopy { .. }
        ));
        assert!(matches!(
            plan_column(DtypeBackend::Numpy, ArrowKind::Bool, false),
            ColumnPlan::RequiresCopy { .. }
        ));
    }

    #[test]
    fn plan_column_contiguous_numpy_numeric_is_zero_copy_borrow() {
        assert_eq!(
            plan_column(DtypeBackend::Numpy, ArrowKind::Numeric, true),
            ColumnPlan::ZeroCopyBorrow
        );
    }

    #[test]
    fn plan_column_non_contiguous_numpy_numeric_requires_copy() {
        assert!(matches!(
            plan_column(DtypeBackend::Numpy, ArrowKind::Numeric, false),
            ColumnPlan::RequiresCopy { .. }
        ));
    }
}
