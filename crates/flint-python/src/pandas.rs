//! pandas <-> Arrow per-column conversion, driven by `flint_core::pandas_plan::plan_column`.
//!
//! Per RESEARCH.md Pattern 3 / Pitfall 2, every column's copy-vs-borrow decision is made by the
//! single `plan_column` function in `flint-core` -- this module NEVER re-implements that
//! decision, it only acts on it. `crates/flint-python/src/diagnostics.rs` (Task 2) consumes the
//! same `ColumnConversionRecord`s produced here for strict mode (D-03) and `copy_report()` (D-04),
//! so the two features can never silently disagree.
//!
//! Two conversion strategies are used per column, chosen by `plan_column`'s result:
//! - **`ZeroCopyBorrow` + `DtypeBackend::Arrow`**: the column's data is already Arrow memory
//!   (owned by pandas' own `ArrowExtensionArray`/pyarrow `ChunkedArray`). We isolate the column
//!   into a single-column `DataFrame` and import its `__arrow_c_stream__` export directly --
//!   genuinely zero-copy, no hand-rolled FFI (RESEARCH.md "Don't Hand-Roll").
//! - **`ZeroCopyBorrow` + `DtypeBackend::Numpy`**: the column is a contiguous numpy numeric
//!   array. We borrow its buffer directly via the `rust-numpy` crate and wrap it in an
//!   `arrow_buffer::Buffer` via `Buffer::from_custom_allocation`, keeping the numpy array's
//!   `Py<PyArray1<T>>` handle alive as the buffer's `Allocation` owner (T-01-03/T-01-04
//!   mitigations below).
//! - **`RequiresCopy`** (numpy bool, non-contiguous numpy, or anything unexpected): we fall back
//!   to the same single-column `__arrow_c_stream__` export used for the Arrow-backed path --
//!   pandas'/pyarrow's own conversion machinery performs the actual repacking copy, so this
//!   project never hand-writes bit-packing or generic numeric-copy logic itself.

use std::ptr::NonNull;
use std::sync::Arc;

use arrow::array::{Array, ArrayRef, PrimitiveArray};
use arrow::buffer::{Buffer, ScalarBuffer};
use arrow::datatypes::{
    DataType, Field, Float32Type, Float64Type, Int16Type, Int32Type, Int64Type, Int8Type, Schema,
    SchemaRef, UInt16Type, UInt32Type, UInt64Type, UInt8Type,
};
use arrow::record_batch::RecordBatch;
use numpy::{PyArray1, PyArrayMethods};
use pyo3::prelude::*;
use pyo3::types::{PyAnyMethods, PyCapsule, PyList};
use pyo3_arrow::PyTable;

use flint_core::pandas_plan::{plan_column, ArrowKind, ColumnPlan, DtypeBackend};

use crate::error::FlintError;

/// A single column's conversion outcome, retained on `Table` so `copy_report()` (D-04) and
/// strict-mode (D-03) -- both in `crate::diagnostics` -- report the ACTUAL decision `from_pandas`
/// made, rather than a re-derived, possibly-diverging one (T-01-05).
#[derive(Debug, Clone)]
pub struct ColumnConversionRecord {
    pub column: String,
    pub dtype: String,
    pub zero_copy: bool,
    pub reason: Option<String>,
}

impl ColumnConversionRecord {
    fn from_plan(column: String, dtype: String, plan: &ColumnPlan) -> Self {
        match plan {
            ColumnPlan::ZeroCopyBorrow => Self {
                column,
                dtype,
                zero_copy: true,
                reason: None,
            },
            ColumnPlan::RequiresCopy { reason } => Self {
                column,
                dtype,
                zero_copy: false,
                reason: Some(reason.clone()),
            },
        }
    }
}

/// The result of converting an entire pandas DataFrame: the assembled `RecordBatch` plus the
/// per-column plan that produced it.
pub struct FromPandasOutcome {
    pub batch: RecordBatch,
    pub records: Vec<ColumnConversionRecord>,
}

/// Determine a column's `(DtypeBackend, ArrowKind)` from its pandas dtype, rejecting anything
/// outside this phase's numeric/bool scope with a `FlintError::UnsupportedColumn` naming the
/// column and dtype (no silent copy/acceptance of out-of-scope dtypes, matching Plan 01's
/// established rejection behavior).
///
/// Dispatch is **isinstance-first**, driven by the pandas dtype's Python TYPE, NOT by
/// `dtype.kind` alone (RESEARCH.md "Pattern: isinstance-first dtype classification" /
/// Pitfall 1). `dtype.kind` alone cannot distinguish an `ArrowDtype` int64 column (in scope,
/// D-07) from a masked `Int64` extension column (out of scope, D-08) -- both report
/// `kind == 'i'`. The dispatch order below is load-bearing:
///
/// 1. `isinstance(dtype, pandas.ArrowDtype)` -> `DtypeBackend::Arrow`, sub-classified via
///    `pyarrow.types` predicates on `dtype.pyarrow_dtype` (not `dtype.kind`). Plan 02 added
///    `pa.types.is_string`/`is_large_string` -> `ArrowKind::String` here (D-10). Plan 04 adds
///    `pa.types.is_timestamp`/`is_duration` -> `ArrowKind::Timestamp{tz}`/`Duration` here,
///    each gated to ns resolution (D-15) -- non-ns is rejected before this branch returns.
/// 2. `isinstance(dtype, pandas.CategoricalDtype)` -> `DtypeBackend::Categorical` /
///    `ArrowKind::Categorical` (D-17/D-18, OQ2). MUST be checked here, BEFORE the generic
///    `ExtensionDtype` reject branch below -- `pandas.CategoricalDtype` IS an
///    `ExtensionDtype` (`isinstance(pd.CategoricalDtype(), pd.api.extensions.ExtensionDtype)`
///    is `True`), so appending this check after the catch-all would never be reached and
///    every `Categorical` column would be incorrectly rejected as an unsupported masked
///    extension dtype.
/// 3. `isinstance(dtype, pandas.DatetimeTZDtype)` -> `DtypeBackend::Numpy` /
///    `ArrowKind::Timestamp { tz: Some(..) }` (D-15/D-16, CONV-06). MUST be checked here,
///    BEFORE the generic `ExtensionDtype` reject branch below -- `pandas.DatetimeTZDtype` IS
///    an `ExtensionDtype` (like `CategoricalDtype` above), so placing this check after the
///    catch-all would make every tz-aware datetime64 column incorrectly rejected. Routed to
///    `DtypeBackend::Numpy` (not `Arrow`): a `DatetimeTZDtype` column is not pyarrow-backed
///    memory, so `plan_column`'s `(Numpy, Timestamp)` arm honestly reports `RequiresCopy`
///    rather than falsely claiming a zero-copy borrow. Gated to ns resolution (D-15); the tz
///    string is round-tripped exactly as-is via `str(dtype.tz)`, no UTC normalization (D-16).
/// 4. else `isinstance(dtype, pandas.api.extensions.ExtensionDtype)` -> reject. This branch
///    catches masked `Int64`/`boolean`/`Float64` (D-08) and any other non-Arrow extension
///    dtype not handled by an earlier, more specific branch above.
/// 5. else it is a plain numpy dtype -> dispatch on `dtype.kind` as Phase 1 already does. Plan
///    02 added `kind == 'O'` (object) -> `DtypeBackend::Numpy, ArrowKind::String`, whose content
///    is separately validated by `validate_object_column_contents` (D-11) before conversion.
///    Plan 04 adds `kind == 'M'`/`'m'` (plain numpy `datetime64`/`timedelta64`) ->
///    `DtypeBackend::Numpy`, `ArrowKind::Timestamp { tz: None }`/`Duration`, using
///    `np.datetime_data(dtype)` (NOT string-parsing `str(dtype)`) for the ns-resolution gate.
fn classify_dtype(
    py: Python<'_>,
    dtype: &Bound<'_, PyAny>,
    arrow_dtype_type: &Bound<'_, PyAny>,
    categorical_dtype_type: &Bound<'_, PyAny>,
    datetime_tz_dtype_type: &Bound<'_, PyAny>,
    extension_dtype_type: &Bound<'_, PyAny>,
    column_name: &str,
) -> PyResult<(DtypeBackend, ArrowKind, String)> {
    let dtype_str: String = dtype.str()?.extract()?;

    // (1) pandas.ArrowDtype -- sub-classify via pyarrow.types predicates on the wrapped
    // pyarrow DataType, never via dtype.kind (RESEARCH.md Pattern, isinstance-first dispatch).
    if dtype.is_instance(arrow_dtype_type)? {
        let pyarrow_dtype = dtype.getattr("pyarrow_dtype")?;
        let pyarrow_types = py.import("pyarrow")?.getattr("types")?;

        let is_integer: bool = pyarrow_types
            .call_method1("is_integer", (&pyarrow_dtype,))?
            .extract()?;
        let is_floating: bool = pyarrow_types
            .call_method1("is_floating", (&pyarrow_dtype,))?
            .extract()?;
        let is_boolean: bool = pyarrow_types
            .call_method1("is_boolean", (&pyarrow_dtype,))?
            .extract()?;
        let is_string: bool = pyarrow_types
            .call_method1("is_string", (&pyarrow_dtype,))?
            .extract()?;
        // Accept large_string too (Assumption A2): pa.types.is_string(pa.large_string()) is
        // False, so a plain is_string check alone would wrongly reject large_string[pyarrow].
        let is_large_string: bool = pyarrow_types
            .call_method1("is_large_string", (&pyarrow_dtype,))?
            .extract()?;
        let is_timestamp: bool = pyarrow_types
            .call_method1("is_timestamp", (&pyarrow_dtype,))?
            .extract()?;
        let is_duration: bool = pyarrow_types
            .call_method1("is_duration", (&pyarrow_dtype,))?
            .extract()?;

        let arrow_kind = if is_integer || is_floating {
            ArrowKind::Numeric
        } else if is_boolean {
            ArrowKind::Bool
        } else if is_string || is_large_string {
            ArrowKind::String
        } else if is_timestamp {
            // D-15: ns-only resolution gate, same reasoning as the DatetimeTZDtype and
            // plain-numpy branches below.
            let unit: String = pyarrow_dtype.getattr("unit")?.extract()?;
            if unit != "ns" {
                return Err(FlintError::UnsupportedColumn {
                    column: column_name.to_string(),
                    dtype: dtype_str,
                    reason: non_ns_temporal_rejection_reason(&unit),
                }
                .into());
            }
            let tz: Option<String> = pyarrow_dtype.getattr("tz")?.extract()?;
            ArrowKind::Timestamp { tz }
        } else if is_duration {
            let unit: String = pyarrow_dtype.getattr("unit")?.extract()?;
            if unit != "ns" {
                return Err(FlintError::UnsupportedColumn {
                    column: column_name.to_string(),
                    dtype: dtype_str,
                    reason: non_ns_temporal_rejection_reason(&unit),
                }
                .into());
            }
            ArrowKind::Duration
        } else {
            return Err(FlintError::UnsupportedColumn {
                column: column_name.to_string(),
                dtype: dtype_str,
                reason: "only numeric (int/uint/float), boolean, string, timestamp, and \
                         duration ArrowDtype columns are supported in this phase"
                    .to_string(),
            }
            .into());
        };

        return Ok((DtypeBackend::Arrow, arrow_kind, dtype_str));
    }

    // (2) pandas.CategoricalDtype -- MUST be checked here, BEFORE the generic ExtensionDtype
    // reject branch below (D-17/D-18, OQ2). A CategoricalDtype IS an ExtensionDtype, so this
    // check would never be reached if placed after the catch-all reject.
    if dtype.is_instance(categorical_dtype_type)? {
        return Ok((DtypeBackend::Categorical, ArrowKind::Categorical, dtype_str));
    }

    // (3) pandas.DatetimeTZDtype -- MUST be checked here, BEFORE the generic ExtensionDtype
    // reject branch below (D-15/D-16, CONV-06), for the same reason as CategoricalDtype above:
    // DatetimeTZDtype IS an ExtensionDtype, so this check would never be reached if placed
    // after the catch-all reject. Routed to DtypeBackend::Numpy (not Arrow) -- a tz-aware
    // datetime64 column is not pyarrow-backed memory, so plan_column's (Numpy, Timestamp) arm
    // honestly reports RequiresCopy. `dtype.unit` is read directly (pandas' ExtensionDtype
    // exposes it, per RESEARCH.md's verified `datetime_unit` helper) rather than
    // string-parsing `str(dtype)`. The tz string is round-tripped via `str(dtype.tz)` exactly
    // as-is (D-16: no UTC normalization -- confirmed empirically that `str(dtype.tz)` yields
    // the original zone name, e.g. "America/New_York", not a UTC-normalized form).
    if dtype.is_instance(datetime_tz_dtype_type)? {
        let unit: String = dtype.getattr("unit")?.extract()?;
        if unit != "ns" {
            return Err(FlintError::UnsupportedColumn {
                column: column_name.to_string(),
                dtype: dtype_str,
                reason: non_ns_temporal_rejection_reason(&unit),
            }
            .into());
        }
        let tz_obj = dtype.getattr("tz")?;
        let tz_str: String = tz_obj.str()?.extract()?;
        return Ok((
            DtypeBackend::Numpy,
            ArrowKind::Timestamp { tz: Some(tz_str) },
            dtype_str,
        ));
    }

    // (4) Any other pandas ExtensionDtype (masked Int64/boolean/Float64) is rejected here,
    // honestly, BEFORE the plain-numpy `.values.flags` access in `from_pandas` is ever reached
    // (D-08 / Pitfall 1: masked extension arrays like `IntegerArray`/`BooleanArray` have no
    // `.flags` attribute and previously crashed with a raw AttributeError instead of a clean
    // FlintError).
    if dtype.is_instance(extension_dtype_type)? {
        let dtype_type_name: String = dtype.get_type().name()?.extract()?;
        return Err(FlintError::UnsupportedColumn {
            column: column_name.to_string(),
            dtype: dtype_str,
            reason: format!(
                "pandas masked/extension dtype {dtype_type_name} is not supported in this \
                 phase (pandas masked nullable extension dtypes such as Int64/boolean/Float64 \
                 are out of scope; use an ArrowDtype-backed column, e.g. \
                 dtype=\"int64[pyarrow]\", for nullable numeric support)"
            ),
        }
        .into());
    }

    // (5) Plain numpy dtype -- dispatch on `.kind` exactly as Phase 1 does. Object ('O') maps to
    // ArrowKind::String -- its content is validated separately by
    // `validate_object_column_contents` (D-11), called from `from_pandas` before any
    // conversion is attempted for this (Numpy, String) case. 'M' (datetime64) / 'm'
    // (timedelta64) use `np.datetime_data(dtype)` (NOT string-parsing `str(dtype)`, per
    // RESEARCH.md's verified `datetime_unit` helper) for the ns-resolution gate (D-15).
    let kind: String = dtype.getattr("kind")?.extract()?;
    let arrow_kind = match kind.as_str() {
        "b" => ArrowKind::Bool,
        "i" | "u" | "f" => ArrowKind::Numeric,
        "O" => ArrowKind::String,
        "M" | "m" => {
            let numpy = py.import("numpy")?;
            let datetime_data = numpy.call_method1("datetime_data", (dtype,))?;
            let unit: String = datetime_data.get_item(0)?.extract()?;
            if unit != "ns" {
                return Err(FlintError::UnsupportedColumn {
                    column: column_name.to_string(),
                    dtype: dtype_str,
                    reason: non_ns_temporal_rejection_reason(&unit),
                }
                .into());
            }
            if kind == "M" {
                ArrowKind::Timestamp { tz: None }
            } else {
                ArrowKind::Duration
            }
        }
        _ => {
            return Err(FlintError::UnsupportedColumn {
                column: column_name.to_string(),
                dtype: dtype_str,
                reason: "only numeric (int/uint/float), boolean, object/string, datetime64, \
                         and timedelta64 columns are supported in this phase"
                    .to_string(),
            }
            .into());
        }
    };

    Ok((DtypeBackend::Numpy, arrow_kind, dtype_str))
}

/// Build the actionable rejection reason for a non-nanosecond-resolution datetime/timedelta
/// column (D-15 / RESEARCH.md Pitfall 5).
///
/// pandas 3.0 changed `pd.to_datetime()`/`pd.to_timedelta()`'s default parsing resolution from
/// nanoseconds to microseconds -- the single most common way a pandas-3.0 user builds a
/// datetime/timedelta column (with no explicit `dtype=`) now yields `us` resolution, which
/// Flint rejects per D-15's ns-only scope. The message explicitly names this pandas-3.0
/// behavior change and suggests the `.astype('datetime64[ns]')` fix, rather than reading as a
/// confusing, unexplained failure.
fn non_ns_temporal_rejection_reason(actual_unit: &str) -> String {
    format!(
        "resolution {actual_unit:?} is not supported; only nanosecond ('ns') resolution \
         datetime64/timedelta64 columns are supported in this phase. Note: pandas 3.0 changed \
         the default parsing resolution of pd.to_datetime()/pd.to_timedelta() from nanoseconds \
         to microseconds, so a column built without an explicit dtype may now be {actual_unit:?} \
         resolution -- cast it explicitly, e.g. .astype('datetime64[ns]') (or the timedelta64 \
         equivalent), before calling flint.Table.from_pandas"
    )
}

/// Convert an entire pandas DataFrame into a `RecordBatch`, driving every column's
/// copy-vs-borrow decision through `plan_column` (the single source of truth).
pub fn from_pandas(py: Python<'_>, df: &Bound<'_, PyAny>) -> PyResult<FromPandasOutcome> {
    let pandas = py.import("pandas")?;
    let arrow_dtype_type = pandas.getattr("ArrowDtype")?;
    let categorical_dtype_type = pandas.getattr("CategoricalDtype")?;
    let datetime_tz_dtype_type = pandas.getattr("DatetimeTZDtype")?;
    let extension_dtype_type = pandas
        .getattr("api")?
        .getattr("extensions")?
        .getattr("ExtensionDtype")?;

    let columns = df.getattr("columns")?;
    let mut fields = Vec::new();
    let mut arrays: Vec<ArrayRef> = Vec::new();
    let mut records = Vec::new();

    for column_name in columns.try_iter()? {
        let column_name = column_name?;
        let column_name_str: String = column_name.str()?.extract()?;
        let series = df.get_item(&column_name)?;
        let dtype = series.getattr("dtype")?;

        let (dtype_backend, arrow_kind, dtype_str) = classify_dtype(
            py,
            &dtype,
            &arrow_dtype_type,
            &categorical_dtype_type,
            &datetime_tz_dtype_type,
            &extension_dtype_type,
            &column_name_str,
        )?;

        let is_contiguous = match dtype_backend {
            // Irrelevant for Arrow-backed columns -- plan_column ignores it in that branch.
            DtypeBackend::Arrow => true,
            DtypeBackend::Numpy => {
                let values = series.getattr("values")?;
                values
                    .getattr("flags")?
                    .getattr("c_contiguous")?
                    .extract::<bool>()?
            }
            // A Categorical's codes+categories split array never has a `.values.flags`
            // access path meaningful to this check -- it always routes through the generic
            // __arrow_c_stream__ fallback below, never the numpy-buffer borrow path, so
            // contiguity is irrelevant here exactly as it is for the Arrow arm above.
            DtypeBackend::Categorical => true,
        };

        // `ArrowKind` no longer derives `Copy` (Task 1: `Timestamp` carries an owned `String`),
        // so `arrow_kind` is cloned into `plan_column` here and borrowed (`&arrow_kind`) at its
        // two remaining use sites below, rather than moved by value at each site.
        let plan = plan_column(dtype_backend, arrow_kind.clone(), is_contiguous);
        records.push(ColumnConversionRecord::from_plan(
            column_name_str.clone(),
            dtype_str,
            &plan,
        ));

        // D-11 / RESEARCH.md Pitfall 2: a numpy object-dtype string column's content is NOT
        // safe to trust to pyarrow's own type inference (it silently accepts dict-valued and
        // int-valued columns, and raises order-dependent, non-Flint-owned errors on genuinely
        // mixed columns). Run this Flint-owned validation pass BEFORE any conversion is
        // attempted for exactly this (Numpy, String) case. The ArrowDtype string case does NOT
        // get this validation -- its physical layout is already a typed Arrow string buffer.
        if matches!((dtype_backend, &arrow_kind), (DtypeBackend::Numpy, ArrowKind::String)) {
            validate_object_column_contents(&series, &column_name_str)?;
        }

        // D-31/WR-01: `is_nullable` is threaded from the DECLARED source nullability (the
        // stream-imported schema's `Field::is_nullable()`, or a hard-coded `false` for the
        // numpy-buffer fast path below), NEVER derived from `array.null_count() > 0` -- see
        // `build_field`'s doc comment for the full rationale and the concrete WR-01 failure
        // this fixes.
        let (array, is_nullable) = match (&plan, dtype_backend, &arrow_kind) {
            (ColumnPlan::ZeroCopyBorrow, DtypeBackend::Numpy, ArrowKind::Numeric) => {
                // This dtype family (contiguous numpy numeric) cannot represent nulls -- keep
                // hard-coded `false` unchanged (RESEARCH.md explicit call-out, does NOT go
                // through import_column_via_pandas_stream at all).
                (borrow_numpy_numeric_column(&series, &column_name_str)?, false)
            }
            _ => {
                let (array, observed_batch_count, declared_nullable) =
                    import_column_via_pandas_stream(py, df, &column_name)?;
                // D-13 / RESEARCH.md Pitfall 6 Strategy B: plan_column's a-priori ColumnPlan
                // (already pushed into `records` above) has no chunk-count visibility -- it is
                // computed from dtype backend + arrow kind + contiguity alone, before this
                // stream-import call runs. When the stream actually yielded more than one
                // RecordBatch, `import_column_via_pandas_stream` just performed a genuine
                // `arrow::compute::concat` copy (see its doc comment) that the pre-computed
                // record does not yet reflect. Correct that column's record here, in place,
                // BEFORE `from_pandas` returns -- so `check_strict`/`build_copy_report`
                // (diagnostics.rs, unchanged) see the corrected, honest record rather than the
                // stale `ZeroCopyBorrow` prediction (closes the DIAG-01/DIAG-02 override from
                // 01-VERIFICATION.md). A single-chunk column (`observed_batch_count <= 1`,
                // including the empty-column 0-batch case) is left untouched.
                if observed_batch_count > 1 {
                    if let Some(record) = records.last_mut() {
                        record.zero_copy = false;
                        record.reason = Some(format!(
                            "column arrived as {observed_batch_count} Arrow chunks (e.g. from a \
                             pd.concat of Arrow-backed frames) and was concatenated into one \
                             contiguous buffer via arrow::compute::concat, which is a copy, not \
                             a zero-copy borrow"
                        ));
                    }
                }
                (array, declared_nullable)
            }
        };

        // D-17 / Pitfall 3: a dictionary-typed column's `ordered` flag lives on arrow-schema's
        // `Field` (`Field::dict_is_ordered`), NOT on `DataType::Dictionary` itself -- it must be
        // sourced from the pandas source dtype's own `.ordered` attribute (only meaningful for a
        // DtypeBackend::Categorical column) and propagated explicitly via `Field::new_dictionary`
        // + `with_dict_is_ordered`, rather than the generic `Field::new(..,
        // array.data_type().clone(), ..)` this used to unconditionally use for every column
        // (which silently dropped `ordered` for every categorical, confirmed empirically in
        // RESEARCH.md Pitfall 3 via a direct PyCapsule export with no `to_pandas` involved).
        let is_ordered = if matches!(dtype_backend, DtypeBackend::Categorical) {
            Some(dtype.getattr("ordered")?.extract::<bool>()?)
        } else {
            None
        };
        fields.push(build_field(
            &column_name_str,
            array.as_ref(),
            is_ordered,
            is_nullable,
        ));
        arrays.push(array);
    }

    let schema: SchemaRef = Arc::new(Schema::new(fields));
    let batch = RecordBatch::try_new(schema, arrays).map_err(FlintError::from)?;

    Ok(FromPandasOutcome { batch, records })
}

/// Build a column's `Field`, propagating the dictionary `ordered` flag (D-17 / Pitfall 3) for
/// `DataType::Dictionary`-typed columns and the DECLARED source nullability (D-31/WR-01) for
/// every column.
///
/// `Field::dict_is_ordered` lives on `Field`, not `DataType` -- `array.data_type().clone()`
/// alone can never carry it forward, so any `DataType::Dictionary` column MUST be constructed
/// via `Field::new_dictionary(..).with_dict_is_ordered(..)` instead of the generic
/// `Field::new(.., array.data_type().clone(), ..)` every other column uses unchanged.
/// `is_ordered` is `None` for any non-categorical column (defaults to `false`, matching
/// arrow-rs's own default) and `Some(dtype.ordered)` for a genuine pandas `Categorical` column
/// (sourced from the pandas source dtype in `from_pandas`, not re-derived here).
///
/// **D-31/WR-01 (02-REVIEW.md WR-01 fix):** `is_nullable` MUST be the column's DECLARED source
/// nullability (threaded in from `from_pandas`'s call site -- either
/// `import_column_via_pandas_stream`'s returned schema nullability, or a hard-coded `false` for
/// the `borrow_numpy_numeric_column` fast path), NEVER `array.null_count() > 0`. Deriving
/// nullability from the CURRENT batch's observed null count conflates "this batch happens to
/// have zero nulls right now" with "this column's type cannot hold nulls" -- a nullable
/// `int64[pyarrow]` column with zero nulls would otherwise round-trip as a `not null` Flint
/// schema field, breaking `pyarrow.concat_tables` against a genuinely-nullable sibling batch
/// (the exact reproduction in 02-REVIEW.md). Because pyarrow's `__arrow_c_stream__` export marks
/// EVERY column `nullable=True` (verified empirically, RESEARCH.md Summary/A5), this uniformly
/// broadens every stream-imported column to `nullable=True` -- an intentional, documented,
/// permissive/safe broadening (never breaks `concat_tables` the way a wrongly-`non-nullable`
/// field does), not a regression.
fn build_field(column_name: &str, array: &dyn Array, is_ordered: Option<bool>, is_nullable: bool) -> Field {
    match array.data_type() {
        DataType::Dictionary(key_type, value_type) => Field::new_dictionary(
            column_name,
            (**key_type).clone(),
            (**value_type).clone(),
            is_nullable,
        )
        .with_dict_is_ordered(is_ordered.unwrap_or(false)),
        other => Field::new(column_name, other.clone(), is_nullable),
    }
}

/// Import a single column's data as an Arrow array by isolating it into a single-column
/// DataFrame and consuming its `__arrow_c_stream__` PyCapsule export.
///
/// Used for `DtypeBackend::Arrow` columns (already Arrow memory -- genuinely zero-copy) AND as
/// the `RequiresCopy` fallback for numpy columns (numpy bool, non-contiguous numeric): in that
/// case pandas'/pyarrow's own conversion machinery performs the actual copy, so this project
/// never hand-writes bit-packing or generic numeric-copy logic (RESEARCH.md "Don't Hand-Roll").
///
/// A column's `__arrow_c_stream__` export may yield more than one `RecordBatch` (e.g. a
/// `pd.concat` of two Arrow-backed frames produces a 2-chunk `ChunkedArray`, never auto-rechunked
/// by pandas/pyarrow). Every batch is accounted for: a single-batch stream returns that batch's
/// column directly (an `Arc<dyn Array>` clone -- genuinely zero-copy, no allocation), while a
/// multi-batch stream is concatenated into one contiguous array via `arrow::compute::concat` (an
/// honest copy -- a multi-chunk column was never one contiguous buffer to begin with, so this does
/// not regress the single-chunk zero-copy path). Silently returning only the first batch would
/// truncate every row after it (CR-01).
///
/// A genuinely empty (0-row) column's stream yields ZERO record batches (confirmed empirically
/// for an empty `object`-dtype column, CONV-04's flagged empty-column edge case) even though the
/// stream's schema is still available -- this is a valid, common case (an empty DataFrame column
/// is not a malformed stream), so it is handled by constructing a genuinely empty array of the
/// column's Arrow type directly from the schema, rather than treated as an error.
///
/// Returns `(array, observed_batch_count, declared_is_nullable)`. `observed_batch_count` (D-13 /
/// RESEARCH.md Pitfall 6 Strategy B): the caller (`from_pandas`) uses the observed batch count to
/// correct that column's already-computed `ColumnConversionRecord` post-hoc when
/// `observed_batch_count > 1` -- this function itself does not know about
/// `ColumnConversionRecord`/diagnostics at all, it only surfaces the count it already has on
/// hand from the branch it took. `declared_is_nullable` (D-31/WR-01) is
/// `schema.field(0).is_nullable()` -- the DECLARED nullability pyarrow's own
/// `__arrow_c_stream__` export puts on the schema, captured here (before it would otherwise sit
/// unused) and threaded by the caller into `build_field`, replacing the previous
/// `array.null_count() > 0` derivation. This is read from the schema once, up front, so it is
/// available identically on the empty-batch, single-batch, and multi-batch paths below.
fn import_column_via_pandas_stream(
    py: Python<'_>,
    df: &Bound<'_, PyAny>,
    column_name: &Bound<'_, PyAny>,
) -> PyResult<(ArrayRef, usize, bool)> {
    let single_column_selector = PyList::new(py, [column_name])?;
    let single_column_df = df.get_item(single_column_selector)?;
    let capsule: Bound<'_, PyCapsule> = single_column_df
        .call_method0("__arrow_c_stream__")?
        .extract()?;
    let py_table = PyTable::from_arrow_pycapsule(&capsule)?;
    let (batches, schema) = py_table.into_inner();
    let declared_is_nullable = schema.field(0).is_nullable();

    if batches.is_empty() {
        let data_type = schema.field(0).data_type();
        return Ok((arrow::array::new_empty_array(data_type), 0, declared_is_nullable));
    }

    if batches.len() == 1 {
        return Ok((batches[0].column(0).clone(), 1, declared_is_nullable));
    }

    let columns: Vec<ArrayRef> = batches.iter().map(|b| b.column(0).clone()).collect();
    let column_refs: Vec<&dyn Array> = columns.iter().map(|c| c.as_ref()).collect();
    let concatenated = arrow::compute::concat(&column_refs).map_err(FlintError::from)?;
    Ok((concatenated, batches.len(), declared_is_nullable))
}

/// Flint-owned content validation for a legacy numpy object-dtype column (D-11 / RESEARCH.md
/// Pitfall 2).
///
/// pandas'/pyarrow's own `__arrow_c_stream__` export infers the target Arrow type from an
/// object column's contents rather than enforcing any caller-specified contract: a dict-valued
/// column silently converts to a nested `struct`, an all-int column silently converts to
/// `int64`, and a genuinely mixed-type column raises an order-dependent, non-Flint-owned
/// exception (a different pyarrow exception type depending on which non-str element is
/// encountered first). None of that is acceptable for this project's "honest conversion"
/// contract, so this function iterates the column's values in Python BEFORE
/// `import_column_via_pandas_stream` is ever called for a `(DtypeBackend::Numpy,
/// ArrowKind::String)` column, and rejects the first non-`None`/non-`NaN` value that is not a
/// `str` with a `FlintError::UnsupportedColumn` naming the column, dtype "object", and the
/// offending value's `type(v).__name__` plus its row index.
fn validate_object_column_contents(series: &Bound<'_, PyAny>, column_name: &str) -> PyResult<()> {
    for (i, value) in series.try_iter()?.enumerate() {
        let value = value?;
        if value.is_none() {
            continue;
        }
        // NaN (a float) is also treated as a missing value here, matching D-09's existing
        // "NaN is not treated as an error" posture for numeric columns.
        if let Ok(as_float) = value.extract::<f64>() {
            if as_float.is_nan() {
                continue;
            }
        }
        if !value.is_instance_of::<pyo3::types::PyString>() {
            let value_type_name: String = value.get_type().name()?.extract()?;
            return Err(FlintError::UnsupportedColumn {
                column: column_name.to_string(),
                dtype: "object".to_string(),
                reason: format!(
                    "non-string value of type {value_type_name:?} found at row {i}; object-dtype \
                     columns must contain only str values (and None/NaN) -- Flint does not rely \
                     on pyarrow's own type inference for this, which would silently accept \
                     dict-valued or int-valued object columns instead of rejecting them"
                ),
            }
            .into());
        }
    }
    Ok(())
}

/// Newtype around a borrowed numpy array's `Py<PyArray1<T>>` handle, used solely as the
/// `arrow_buffer::alloc::Allocation` owner for a zero-copy Arrow buffer.
///
/// Dropping this drops the underlying `Py<PyArray1<T>>`, whose own `Drop` impl (PyO3's `Py<T>`)
/// already reacquires the GIL as needed before decrementing the Python refcount -- satisfying the
/// T-01-04 threat mitigation (GIL-safe release of a borrowed buffer's owner) without any custom
/// `unsafe` `Drop` code here. The explicit `Send`/`Sync`/`RefUnwindSafe` impls below make that
/// safety property visible to `Allocation`'s supertrait bounds regardless of `Py<T>`'s own
/// (already-satisfied) auto-trait derivation.
#[allow(dead_code)] // held only for its `Drop` side effect (keeps the numpy array alive)
struct NumpyBufferOwner<T>(Py<PyArray1<T>>);

// SAFETY: `Py<T>` is unconditionally `Send`/`Sync` in PyO3 -- its reference count is atomic and
// its `Drop` impl is GIL-independent (see module doc comment above).
unsafe impl<T> Send for NumpyBufferOwner<T> {}
unsafe impl<T> Sync for NumpyBufferOwner<T> {}
impl<T> std::panic::RefUnwindSafe for NumpyBufferOwner<T> {}

/// Borrow a contiguous numpy numeric column's buffer directly into an Arrow `PrimitiveArray`,
/// with NO data copy.
///
/// # Security (T-01-03)
/// The caller (`from_pandas` above) only reaches this function when `plan_column` has already
/// resolved to `ZeroCopyBorrow` for a `(Numpy, Numeric)` column, which requires
/// `is_contiguous == true` (checked via numpy's own `ndarray.flags.c_contiguous`, RESEARCH.md
/// Security Domain). This function additionally requires `PyReadonlyArray::as_slice()` to
/// succeed (which itself independently verifies C-contiguity) before treating the buffer as
/// borrowable -- a non-contiguous or offset buffer is never read as if it were a simple flat
/// buffer, preventing an out-of-bounds read.
fn borrow_numpy_numeric_column(series: &Bound<'_, PyAny>, column_name: &str) -> PyResult<ArrayRef> {
    let values = series.getattr("values")?;
    let dtype_name: String = values.getattr("dtype")?.getattr("name")?.extract()?;

    macro_rules! borrow {
        ($rust_ty:ty, $arrow_ty:ty) => {{
            let typed = values.cast::<PyArray1<$rust_ty>>().map_err(|_| {
                FlintError::Other(format!(
                    "column {column_name:?}: expected a 1-D numpy array of {}",
                    stringify!($rust_ty)
                ))
            })?;
            let readonly = typed
                .try_readonly()
                .map_err(|e| FlintError::Other(format!("column {column_name:?}: {e}")))?;
            let slice = readonly.as_slice().map_err(|e| {
                FlintError::Other(format!(
                    "column {column_name:?}: numpy buffer is not contiguous ({e})"
                ))
            })?;
            let len = slice.len();
            let byte_len = len * std::mem::size_of::<$rust_ty>();
            let ptr = NonNull::new(slice.as_ptr() as *mut u8).ok_or_else(|| {
                FlintError::Other(format!(
                    "column {column_name:?}: numpy buffer pointer is null"
                ))
            })?;
            let owner: Arc<NumpyBufferOwner<$rust_ty>> =
                Arc::new(NumpyBufferOwner(typed.clone().unbind()));
            // SAFETY: `ptr`/`byte_len` describe exactly the contiguous numpy buffer validated by
            // `readonly.as_slice()` above; `owner` keeps the numpy array alive for as long as
            // this Arrow buffer lives (T-01-03/T-01-04 mitigations, see doc comments above).
            let buffer = unsafe { Buffer::from_custom_allocation(ptr, byte_len, owner) };
            let scalar_buffer = ScalarBuffer::<$rust_ty>::new(buffer, 0, len);
            Arc::new(PrimitiveArray::<$arrow_ty>::new(scalar_buffer, None)) as ArrayRef
        }};
    }

    let array = match dtype_name.as_str() {
        "int8" => borrow!(i8, Int8Type),
        "int16" => borrow!(i16, Int16Type),
        "int32" => borrow!(i32, Int32Type),
        "int64" => borrow!(i64, Int64Type),
        "uint8" => borrow!(u8, UInt8Type),
        "uint16" => borrow!(u16, UInt16Type),
        "uint32" => borrow!(u32, UInt32Type),
        "uint64" => borrow!(u64, UInt64Type),
        "float32" => borrow!(f32, Float32Type),
        "float64" => borrow!(f64, Float64Type),
        other => {
            return Err(FlintError::Other(format!(
                "column {column_name:?}: unsupported numpy numeric dtype {other:?}"
            ))
            .into())
        }
    };

    Ok(array)
}
