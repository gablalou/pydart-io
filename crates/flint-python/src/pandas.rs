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
///    `pyarrow.types` predicates on `dtype.pyarrow_dtype` (not `dtype.kind`).
///    <!-- EXTENSION POINT: Plans 02 (string[pyarrow]) / 04 (timestamp/duration[pyarrow])
///    insert their `pa.types.is_*` sub-kind checks HERE, before the Numeric/Bool fallthrough
///    below rejects anything else in this branch. -->
/// 2. else `isinstance(dtype, pandas.api.extensions.ExtensionDtype)` -> reject. This branch
///    catches masked `Int64`/`boolean`/`Float64` (D-08) and, for now, also
///    `CategoricalDtype`/`DatetimeTZDtype` (non-Arrow extension dtypes).
///    <!-- EXTENSION POINT: Plan 03 (categorical) / Plan 04 (DatetimeTZDtype-if-applicable)
///    MUST insert their specific isinstance checks ABOVE this generic reject branch, exactly
///    as this comment says -- inserting below here would never be reached. -->
/// 3. else it is a plain numpy dtype -> dispatch on `dtype.kind` as Phase 1 already does.
fn classify_dtype(
    py: Python<'_>,
    dtype: &Bound<'_, PyAny>,
    arrow_dtype_type: &Bound<'_, PyAny>,
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

        let arrow_kind = if is_integer || is_floating {
            ArrowKind::Numeric
        } else if is_boolean {
            ArrowKind::Bool
            // EXTENSION POINT: Plans 02/04 add pa.types.is_string/is_large_string ->
            // ArrowKind::String, is_timestamp -> ArrowKind::Timestamp{tz}, is_duration ->
            // ArrowKind::Duration sub-kinds here, before the final reject fallthrough below.
        } else {
            return Err(FlintError::UnsupportedColumn {
                column: column_name.to_string(),
                dtype: dtype_str,
                reason: "only numeric (int/uint/float) and boolean ArrowDtype columns are \
                         supported in this phase"
                    .to_string(),
            }
            .into());
        };

        return Ok((DtypeBackend::Arrow, arrow_kind, dtype_str));
    }

    // (2) Any other pandas ExtensionDtype (masked Int64/boolean/Float64, and for now also
    // CategoricalDtype/DatetimeTZDtype which later plans will intercept ABOVE this branch)
    // is rejected here, honestly, BEFORE the plain-numpy `.values.flags` access in
    // `from_pandas` is ever reached (D-08 / Pitfall 1: masked extension arrays like
    // `IntegerArray`/`BooleanArray` have no `.flags` attribute and previously crashed with a
    // raw AttributeError instead of a clean FlintError).
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

    // (3) Plain numpy dtype -- dispatch on `.kind` exactly as Phase 1 does. Object ('O') and
    // temporal ('M'/'m') kinds remain rejected here for now; Plans 02/04 add them.
    let kind: String = dtype.getattr("kind")?.extract()?;
    let arrow_kind = match kind.as_str() {
        "b" => ArrowKind::Bool,
        "i" | "u" | "f" => ArrowKind::Numeric,
        _ => {
            return Err(FlintError::UnsupportedColumn {
                column: column_name.to_string(),
                dtype: dtype_str,
                reason: "only numeric (int/uint/float) and boolean columns are supported in this \
                         phase"
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
        };

        let plan = plan_column(dtype_backend, arrow_kind, is_contiguous);
        records.push(ColumnConversionRecord::from_plan(
            column_name_str.clone(),
            dtype_str,
            &plan,
        ));

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
    let batches = py_table.batches();

    if batches.is_empty() {
        return Err(
            FlintError::Other("column stream produced no record batches".to_string()).into(),
        );
    }

    if batches.len() == 1 {
        return Ok(batches[0].column(0).clone());
    }

    let columns: Vec<ArrayRef> = batches.iter().map(|b| b.column(0).clone()).collect();
    let column_refs: Vec<&dyn Array> = columns.iter().map(|c| c.as_ref()).collect();
    let concatenated = arrow::compute::concat(&column_refs).map_err(FlintError::from)?;
    Ok(concatenated)
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
