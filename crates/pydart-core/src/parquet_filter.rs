//! Single source-of-truth Parquet filter-comparison logic (PARQ-04/PARQ-05, D-23..D-27).
//!
//! `could_match_range` is the ONE function consumed by BOTH the row-group-level skip decision
//! (`pydart_core::parquet_io::surviving_row_groups`) and the row-level `ArrowPredicateFn` builder
//! (`pydart_core::parquet_io`'s `RowFilter` construction) -- both are built from the SAME parsed
//! `Vec<FilterExpr>` once per `from_parquet` call in `pydart-python/src/table.rs`, never re-derived
//! (RESEARCH.md Anti-Patterns; `pandas_plan.rs` single-decision-point precedent).
//!
//! This module has no `pyo3`/`pyo3-arrow` dependency (see `pydart-core`'s crate-level doc comment)
//! so the comparison matrix itself can be unit-tested without a Python interpreter attached. It
//! also has no `arrow`/`parquet`-crate dependency in its public surface -- `ScalarValue` is a
//! plain, small value representation (`i64`/`f64`/`bool`/`String`) so `could_match_range` can be
//! tested with zero Arrow array construction. Extracting a `ScalarValue` from a real Arrow
//! statistics array is `pydart_core::parquet_io`'s job, not this module's.
//!
//! ## The six-operator range-comparison contract (D-25)
//!
//! `could_match_range(op, value, min, max)` returns `true` when a row group's `[min, max]` range
//! MIGHT contain a row satisfying `column {op} value` ("keep this row group -- cannot prove it has
//! no match"), and `false` only when the range PROVABLY cannot satisfy the predicate ("safe to
//! skip"). On any doubt -- missing/`None` min or max, or an incomparable value/stat pairing -- the
//! function conservatively returns `true` (T-03-04: over-pruning silently drops matching rows,
//! which D-26 explicitly forbids; under-pruning only costs a missed IO optimization).
//!
//! | Op | Skip (`false`) when | Keep (`true`) otherwise |
//! |----|----------------------|--------------------------|
//! | `Eq` | `value < min` or `value > max` | `min <= value <= max` |
//! | `Ne` | `min == max == value` (single-valued group entirely equal to the excluded value) | every other case, including `min < value < max` |
//! | `Lt` | `min >= value` (nothing in range is `< value`) | `min < value` |
//! | `Le` | `min > value` | `min <= value` |
//! | `Gt` | `max <= value` (nothing in range is `> value`) | `max > value` |
//! | `Ge` | `max < value` | `max >= value` |
//!
//! The `!=` arm is its own explicit match arm (never derived by negating `==`'s logic) per
//! RESEARCH.md's Anti-Pattern callout: a `min < value < max` range must be conservatively KEPT
//! even though no individual row's value is provably equal to `value`.

/// The fixed D-25 set of supported filter operators. Exactly six -- no `in`/membership operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

/// A filter literal's value, parsed at the PyO3 boundary (`pydart-python/src/table.rs`) from a
/// Python `int`/`float`/`bool`/`str`. Deliberately a plain, `arrow`-crate-free representation so
/// `could_match_range` is unit-testable with no Arrow array construction.
#[derive(Debug, Clone, PartialEq)]
pub enum ScalarValue {
    Int64(i64),
    Float64(f64),
    Bool(bool),
    Utf8(String),
}

/// One AND-combined filter condition (D-23/D-24): `column {op} value`.
#[derive(Debug, Clone)]
pub struct FilterExpr {
    pub column: String,
    pub op: Op,
    pub value: ScalarValue,
}

/// Compare two `ScalarValue`s, widening `Int64`/`Float64` cross-type pairs to `f64` so a Python
/// `int` filter literal correctly compares against a `float64` column's statistics (and vice
/// versa). Returns `None` for any other mismatched-variant pairing (cannot compare) -- callers
/// must treat `None` conservatively (see `le`/`lt`/`eq` below).
fn compare(a: &ScalarValue, b: &ScalarValue) -> Option<std::cmp::Ordering> {
    match (a, b) {
        (ScalarValue::Int64(x), ScalarValue::Int64(y)) => x.partial_cmp(y),
        (ScalarValue::Float64(x), ScalarValue::Float64(y)) => x.partial_cmp(y),
        (ScalarValue::Bool(x), ScalarValue::Bool(y)) => x.partial_cmp(y),
        (ScalarValue::Utf8(x), ScalarValue::Utf8(y)) => x.partial_cmp(y),
        (ScalarValue::Int64(x), ScalarValue::Float64(y)) => (*x as f64).partial_cmp(y),
        (ScalarValue::Float64(x), ScalarValue::Int64(y)) => x.partial_cmp(&(*y as f64)),
        _ => None,
    }
}

/// `a <= b`, conservatively `true` when incomparable.
fn le(a: &ScalarValue, b: &ScalarValue) -> bool {
    !matches!(compare(a, b), Some(std::cmp::Ordering::Greater))
}

/// `a < b`, conservatively `true` when incomparable.
fn lt(a: &ScalarValue, b: &ScalarValue) -> bool {
    matches!(compare(a, b), Some(std::cmp::Ordering::Less) | None)
}

/// `a == b`, using the same cross-type-widening `compare` as `le`/`lt` (NOT derived `PartialEq`,
/// which would treat an `Int64(5)` filter literal and a `Float64(5.0)` column stat as unequal).
fn eq(a: &ScalarValue, b: &ScalarValue) -> bool {
    matches!(compare(a, b), Some(std::cmp::Ordering::Equal))
}

/// Decide whether a row group whose column statistics are `[min, max]` MIGHT contain a row
/// matching `column {op} value`. See the module doc comment's decision table.
///
/// The match on `op` is EXHAUSTIVE -- no `_ =>` wildcard arm -- so a future operator addition
/// cannot silently fall through to a wrong default.
///
/// `min`/`max` are `None` when the row group's statistics for this column are absent (e.g. an
/// all-null row group, or a physical type this project's statistics extraction does not trust,
/// such as a possibly-truncated string min/max -- see `pydart_core::parquet_io::scalar_from_array`).
/// ANY operator conservatively returns `true` (keep) when either bound is missing -- never skip on
/// absent stats (T-03-04).
pub fn could_match_range(
    op: Op,
    value: &ScalarValue,
    min: Option<&ScalarValue>,
    max: Option<&ScalarValue>,
) -> bool {
    let (min, max) = match (min, max) {
        (Some(min), Some(max)) => (min, max),
        _ => return true,
    };

    match op {
        Op::Eq => le(min, value) && le(value, max),
        // `!=` can ONLY prove "no match possible" when the entire row group is single-valued and
        // that single value equals the excluded value -- every other case (including
        // `min < value < max`) must conservatively keep the group.
        Op::Ne => !(eq(min, max) && eq(min, value)),
        Op::Lt => lt(min, value),
        Op::Le => le(min, value),
        Op::Gt => lt(value, max),
        Op::Ge => le(value, max),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gt_keeps_range_that_can_exceed_value() {
        assert!(could_match_range(
            Op::Gt,
            &ScalarValue::Int64(250),
            Some(&ScalarValue::Int64(200)),
            Some(&ScalarValue::Int64(299)),
        ));
    }

    #[test]
    fn gt_skips_range_provably_below_value() {
        assert!(!could_match_range(
            Op::Gt,
            &ScalarValue::Int64(250),
            Some(&ScalarValue::Int64(0)),
            Some(&ScalarValue::Int64(99)),
        ));
    }

    #[test]
    fn lt_skips_range_provably_at_or_above_value() {
        assert!(!could_match_range(
            Op::Lt,
            &ScalarValue::Int64(100),
            Some(&ScalarValue::Int64(100)),
            Some(&ScalarValue::Int64(199)),
        ));
    }

    #[test]
    fn ge_keeps_range_including_boundary_value() {
        assert!(could_match_range(
            Op::Ge,
            &ScalarValue::Int64(100),
            Some(&ScalarValue::Int64(100)),
            Some(&ScalarValue::Int64(199)),
        ));
    }

    #[test]
    fn le_skips_range_entirely_above_value() {
        assert!(!could_match_range(
            Op::Le,
            &ScalarValue::Int64(99),
            Some(&ScalarValue::Int64(100)),
            Some(&ScalarValue::Int64(199)),
        ));
    }

    #[test]
    fn eq_keeps_range_containing_value() {
        assert!(could_match_range(
            Op::Eq,
            &ScalarValue::Int64(150),
            Some(&ScalarValue::Int64(100)),
            Some(&ScalarValue::Int64(199)),
        ));
    }

    #[test]
    fn eq_skips_range_not_containing_value() {
        assert!(!could_match_range(
            Op::Eq,
            &ScalarValue::Int64(250),
            Some(&ScalarValue::Int64(100)),
            Some(&ScalarValue::Int64(199)),
        ));
    }

    #[test]
    fn ne_keeps_range_with_other_values() {
        assert!(could_match_range(
            Op::Ne,
            &ScalarValue::Int64(150),
            Some(&ScalarValue::Int64(100)),
            Some(&ScalarValue::Int64(199)),
        ));
    }

    #[test]
    fn ne_skips_single_valued_group_equal_to_excluded_value() {
        assert!(!could_match_range(
            Op::Ne,
            &ScalarValue::Int64(150),
            Some(&ScalarValue::Int64(150)),
            Some(&ScalarValue::Int64(150)),
        ));
    }

    #[test]
    fn ne_keeps_non_single_valued_group_starting_at_excluded_value() {
        // min == value but max != min -- NOT single-valued, so != cannot prove every row equals
        // the excluded value. Must be kept (the over-pruning-safety edge from must_haves).
        assert!(could_match_range(
            Op::Ne,
            &ScalarValue::Int64(150),
            Some(&ScalarValue::Int64(150)),
            Some(&ScalarValue::Int64(199)),
        ));
    }

    #[test]
    fn missing_min_or_max_keeps_for_every_operator() {
        let value = ScalarValue::Int64(5);
        for op in [Op::Eq, Op::Ne, Op::Lt, Op::Le, Op::Gt, Op::Ge] {
            assert!(
                could_match_range(op, &value, None, None),
                "missing min/max must conservatively keep for {op:?}"
            );
            assert!(
                could_match_range(op, &value, Some(&ScalarValue::Int64(0)), None),
                "missing max must conservatively keep for {op:?}"
            );
            assert!(
                could_match_range(op, &value, None, Some(&ScalarValue::Int64(0))),
                "missing min must conservatively keep for {op:?}"
            );
        }
    }

    #[test]
    fn cross_type_int_and_float_comparisons_widen_correctly() {
        // A Python int filter literal (Int64) compared against a float64 column's min/max
        // (Float64 stats) must widen to f64 rather than conservatively-always-keep.
        assert!(!could_match_range(
            Op::Gt,
            &ScalarValue::Int64(250),
            Some(&ScalarValue::Float64(0.0)),
            Some(&ScalarValue::Float64(99.5)),
        ));
        assert!(could_match_range(
            Op::Gt,
            &ScalarValue::Float64(250.0),
            Some(&ScalarValue::Int64(200)),
            Some(&ScalarValue::Int64(299)),
        ));
    }
}
