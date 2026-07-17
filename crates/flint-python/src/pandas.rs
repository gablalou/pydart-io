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
    Field, Float32Type, Float64Type, Int16Type, Int32Type, Int64Type, Int8Type, Schema,
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
///    `pa.types.is_string`/`is_large_string` -> `ArrowKind::String` here (D-10).
///    <!-- EXTENSION POINT: Plan 04 (timestamp/duration[pyarrow]) inserts its `pa.types.is_*`
///    sub-kind checks HERE, before the Numeric/Bool/String fallthrough below rejects anything
///    else in this branch. -->
/// 2. `isinstance(dtype, pandas.CategoricalDtype)` -> `DtypeBackend::Categorical` /
///    `ArrowKind::Categorical` (D-17/D-18, OQ2). MUST be checked here, BEFORE the generic
///    `ExtensionDtype` reject branch below -- `pandas.CategoricalDtype` IS an
///    `ExtensionDtype` (`isinstance(pd.CategoricalDtype(), pd.api.extensions.ExtensionDtype)`
///    is `True`), so appending this check after the catch-all would never be reached and
///    every `Categorical` column would be incorrectly rejected as an unsupported masked
///    extension dtype.
/// 3. else `isinstance(dtype, pandas.api.extensions.ExtensionDtype)` -> reject. This branch
///    catches masked `Int64`/`boolean`/`Float64` (D-08) and, for now, also
///    `DatetimeTZDtype` (non-Arrow extension dtypes).
///    <!-- EXTENSION POINT: Plan 04 (DatetimeTZDtype-if-applicable) MUST insert its specific
///    isinstance check ABOVE this generic reject branch, exactly as this comment says --
///    inserting below here would never be reached. -->
/// 4. else it is a plain numpy dtype -> dispatch on `dtype.kind` as Phase 1 already does. Plan
///    02 added `kind == 'O'` (object) -> `DtypeBackend::Numpy, ArrowKind::String`, whose content
///    is separately validated by `validate_object_column_contents` (D-11) before conversion.
fn classify_dtype(
    py: Python<'_>,
    dtype: &Bound<'_, PyAny>,
    arrow_dtype_type: &Bound<'_, PyAny>,
    categorical_dtype_type: &Bound<'_, PyAny>,
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

        let arrow_kind = if is_integer || is_floating {
            ArrowKind::Numeric
        } else if is_boolean {
            ArrowKind::Bool
        } else if is_string || is_large_string {
            ArrowKind::String
            // EXTENSION POINT: Plan 04 adds pa.types.is_timestamp -> ArrowKind::Timestamp{tz},
            // is_duration -> ArrowKind::Duration sub-kinds here, before the final reject
            // fallthrough below.
        } else {
            return Err(FlintError::UnsupportedColumn {
                column: column_name.to_string(),
                dtype: dtype_str,
                reason: "only numeric (int/uint/float), boolean, and string ArrowDtype columns \
                         are supported in this phase"
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

    // (3) Any other pandas ExtensionDtype (masked Int64/boolean/Float64, and for now also
    // DatetimeTZDtype which a later plan will intercept ABOVE this branch) is rejected here,
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

    // (4) Plain numpy dtype -- dispatch on `.kind` exactly as Phase 1 does. Temporal ('M'/'m')
    // kinds remain rejected here for now; Plan 04 adds them. Object ('O') maps to
    // ArrowKind::String -- its content is validated separately by
    // `validate_object_column_contents` (D-11), called from `from_pandas` before any
    // conversion is attempted for this (Numpy, String) case.
    let kind: String = dtype.getattr("kind")?.extract()?;
    let arrow_kind = match kind.as_str() {
        "b" => ArrowKind::Bool,
        "i" | "u" | "f" => ArrowKind::Numeric,
        "O" => ArrowKind::String,
        _ => {
            return Err(FlintError::UnsupportedColumn {
                column: column_name.to_string(),
                dtype: dtype_str,
                reason: "only numeric (int/uint/float), boolean, and object/string columns are \
                         supported in this phase"
                    .to_string(),
            }
            .into());
        }
    };

    Ok((DtypeBackend::Numpy, arrow_kind, dtype_str))
}

/// Convert an entire pandas DataFrame into a `RecordBatch`, driving every column's
/// copy-vs-borrow decision through `plan_column` (the single source of truth).
pub fn from_pandas(py: Python<'_>, df: &Bound<'_, PyAny>) -> PyResult<FromPandasOutcome> {
    let pandas = py.import("pandas")?;
    let arrow_dtype_type = pandas.getattr("ArrowDtype")?;
    let categorical_dtype_type = pandas.getattr("CategoricalDtype")?;
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

        let plan = plan_column(dtype_backend, arrow_kind, is_contiguous);
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
        if matches!((dtype_backend, arrow_kind), (DtypeBackend::Numpy, ArrowKind::String)) {
            validate_object_column_contents(&series, &column_name_str)?;
        }

        let array = match (&plan, dtype_backend, arrow_kind) {
            (ColumnPlan::ZeroCopyBorrow, DtypeBackend::Numpy, ArrowKind::Numeric) => {
                borrow_numpy_numeric_column(&series, &column_name_str)?
            }
            _ => import_column_via_pandas_stream(py, df, &column_name)?,
        };

        fields.push(Field::new(
            &column_name_str,
            array.data_type().clone(),
            array.null_count() > 0,
        ));
        arrays.push(array);
    }

    let schema: SchemaRef = Arc::new(Schema::new(fields));
    let batch = RecordBatch::try_new(schema, arrays).map_err(FlintError::from)?;

    Ok(FromPandasOutcome { batch, records })
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
fn import_column_via_pandas_stream(
    py: Python<'_>,
    df: &Bound<'_, PyAny>,
    column_name: &Bound<'_, PyAny>,
) -> PyResult<ArrayRef> {
    let single_column_selector = PyList::new(py, [column_name])?;
    let single_column_df = df.get_item(single_column_selector)?;
    let capsule: Bound<'_, PyCapsule> = single_column_df
        .call_method0("__arrow_c_stream__")?
        .extract()?;
    let py_table = PyTable::from_arrow_pycapsule(&capsule)?;
    let (batches, schema) = py_table.into_inner();

    if batches.is_empty() {
        let data_type = schema.field(0).data_type();
        return Ok(arrow::array::new_empty_array(data_type));
    }

    if batches.len() == 1 {
        return Ok(batches[0].column(0).clone());
    }

    let columns: Vec<ArrayRef> = batches.iter().map(|b| b.column(0).clone()).collect();
    let column_refs: Vec<&dyn Array> = columns.iter().map(|c| c.as_ref()).collect();
    let concatenated = arrow::compute::concat(&column_refs).map_err(FlintError::from)?;
    Ok(concatenated)
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
