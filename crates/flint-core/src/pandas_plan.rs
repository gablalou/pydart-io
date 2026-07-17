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
    /// `pandas.CategoricalDtype`-backed: neither plain numpy nor `ArrowDtype` -- a pandas
    /// `Categorical` stores its own split codes+categories extension array, never a flat
    /// numpy buffer and never pyarrow's own `ArrowExtensionArray` (`isinstance(CategoricalDtype,
    /// pandas.ArrowDtype)` is always `False`). See D-17/D-18 and RESEARCH.md Open Question 2.
    Categorical,
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
    /// String/text data: covers both Arrow-backed `string[pyarrow]`/`large_string[pyarrow]`
    /// columns (already Arrow memory) and legacy numpy `object`-dtype columns of Python `str`
    /// values (no Arrow-compatible physical layout, requires a copy). See D-10/D-11 and
    /// RESEARCH.md Pitfall 2 for the content-validation requirement specific to the numpy-object
    /// case, enforced by `crates/flint-python/src/pandas.rs`'s
    /// `validate_object_column_contents`, not by this matrix.
    String,
    /// pandas `Categorical` (ordered or unordered). Always paired with
    /// `DtypeBackend::Categorical` -- see OQ2 (RESEARCH.md Open Question 2): modeled as its own
    /// variant (not folded into the generic `RequiresCopy` fallback) so `plan_column`'s
    /// pure-Rust unit tests exercise the categorical copy decision directly and the reason
    /// string is categorical-specific (D-17/D-18).
    Categorical,
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
/// | Arrow   | String  | (n/a)      | `ZeroCopyBorrow`|
/// | Numpy   | String  | (n/a)      | `RequiresCopy`  |
/// | Categorical | Categorical | (n/a) | `RequiresCopy` |
///
/// `is_contiguous` is ignored for `DtypeBackend::Arrow` columns (already Arrow's own memory,
/// always zero-copy-borrowable at this phase's scope), for `Numpy`+`Bool` (bit-packing means
/// a numpy bool column always requires a copy, regardless of contiguity), and for
/// `DtypeBackend::Categorical` (a categorical's split codes+categories representation has no
/// single flat Arrow-compatible buffer to borrow, regardless of contiguity -- OQ2).
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
        (DtypeBackend::Arrow, ArrowKind::String) => ColumnPlan::ZeroCopyBorrow,
        (DtypeBackend::Numpy, ArrowKind::String) => ColumnPlan::RequiresCopy {
            reason: "numpy object-dtype string column stores boxed Python str pointers with no \
                     contiguous Arrow-compatible UTF-8 buffer; materializing an Arrow string \
                     array requires a copy"
                .to_string(),
        },
        (DtypeBackend::Categorical, ArrowKind::Categorical) => ColumnPlan::RequiresCopy {
            reason: "pandas Categorical stores a split codes+categories representation with no \
                     single flat Arrow-compatible buffer; dictionary-encoding it into an Arrow \
                     DictionaryArray requires a copy (OQ2)"
                .to_string(),
        },
        // Any other (backend, kind) pairing is unreachable in practice -- classify_dtype only
        // ever pairs DtypeBackend::Categorical with ArrowKind::Categorical, and vice versa --
        // but the match must stay exhaustive as new variants are added across the phase.
        (DtypeBackend::Arrow, ArrowKind::Categorical)
        | (DtypeBackend::Numpy, ArrowKind::Categorical)
        | (DtypeBackend::Categorical, ArrowKind::Numeric)
        | (DtypeBackend::Categorical, ArrowKind::Bool)
        | (DtypeBackend::Categorical, ArrowKind::String) => ColumnPlan::RequiresCopy {
            reason: "unexpected dtype backend/kind pairing; defaulting to a safe copy rather \
                     than an unreachable panic"
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

    #[test]
    fn plan_column_arrow_string_is_zero_copy_borrow() {
        assert_eq!(
            plan_column(DtypeBackend::Arrow, ArrowKind::String, true),
            ColumnPlan::ZeroCopyBorrow
        );
        // is_contiguous is irrelevant for Arrow-backed columns.
        assert_eq!(
            plan_column(DtypeBackend::Arrow, ArrowKind::String, false),
            ColumnPlan::ZeroCopyBorrow
        );
    }

    #[test]
    fn plan_column_numpy_string_requires_copy() {
        assert!(matches!(
            plan_column(DtypeBackend::Numpy, ArrowKind::String, true),
            ColumnPlan::RequiresCopy { .. }
        ));
        assert!(matches!(
            plan_column(DtypeBackend::Numpy, ArrowKind::String, false),
            ColumnPlan::RequiresCopy { .. }
        ));
    }

    #[test]
    fn plan_column_categorical_requires_copy() {
        // is_contiguous is irrelevant for Categorical -- neither value changes the outcome.
        for is_contiguous in [true, false] {
            let plan = plan_column(DtypeBackend::Categorical, ArrowKind::Categorical, is_contiguous);
            match plan {
                ColumnPlan::RequiresCopy { reason } => {
                    assert!(
                        reason.contains("Categorical") || reason.contains("categorical"),
                        "expected a categorical-specific reason, got: {reason:?}"
                    );
                }
                other => panic!("expected RequiresCopy for Categorical, got {other:?}"),
            }
        }
    }
}
