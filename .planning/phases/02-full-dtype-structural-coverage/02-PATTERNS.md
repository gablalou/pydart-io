# Phase 2: Full Dtype & Structural Coverage - Pattern Map

**Mapped:** 2026-07-16
**Files analyzed:** 4 (all existing files being EXTENDED, not new files)
**Analogs found:** 4 / 4 (self-analogous — this phase extends the same files whose existing code is the pattern to follow)

**Note on phase shape:** Unlike a greenfield phase, Phase 2 adds no new source files. Every "pattern assignment" below is a same-file extension: the analog for each change is the *existing code in the same file*, since RESEARCH.md's core finding is "extend the existing decision matrix / existing generic FFI fallback, do not build new machinery." Excerpts are taken directly from the current file contents (already read in full) at the paths below.

## File Classification

| File to Modify | Role | Data Flow | Closest Analog | Match Quality |
|---|---|---|---|---|
| `crates/flint-core/src/pandas_plan.rs` | model / decision-matrix (pure Rust, no pyo3) | transform (pure function, no IO) | itself — existing `DtypeBackend`/`ArrowKind`/`ColumnPlan`/`plan_column` | exact (extend enum variants + match arms) |
| `crates/flint-python/src/pandas.rs` | service / conversion driver (PyO3 boundary) | request-response (per-column classify + convert) | itself — existing `classify_dtype`, `from_pandas`, `import_column_via_pandas_stream` | exact (restructure classify_dtype dispatch order; extend match arms) |
| `crates/flint-python/src/table.rs` | controller / pyclass API surface | request-response (`to_pandas`, `from_pandas` PyO3 methods) | itself — existing `to_pandas` method's `types_mapper` construction | exact (swap static `types_mapper` value for per-column-type-aware callable) |
| `crates/flint-python/src/diagnostics.rs` | service / diagnostics consumer | request-response (reads `ColumnConversionRecord`s) | itself — existing `check_strict`/`build_copy_report` | exact (NO changes expected — consumes whatever `ColumnConversionRecord`s say; confirm no-op) |
| `crates/flint-python/src/error.rs` | utility / error boundary | transform (Rust error -> PyErr) | itself — existing `FlintError` enum + `From<FlintError> for PyErr` | exact (reuse `UnsupportedColumn` variant as-is; no new variant needed per D-conventions) |

## Pattern Assignments

### `crates/flint-core/src/pandas_plan.rs` (model, transform)

**Analog:** itself, current `DtypeBackend`/`ArrowKind`/`ColumnPlan`/`plan_column` (lines 13-80)

**Current enum shape to extend** (lines 13-32):
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DtypeBackend {
    Arrow,
    Numpy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ArrowKind {
    Numeric,
    Bool,
}
```
Per RESEARCH.md's "Recommended Extension Points": add `DtypeBackend::Categorical` (pandas `Categorical` is neither plain numpy nor ArrowDtype — its own backend) and `ArrowKind::{String, Categorical, Timestamp { tz: Option<String> }, Duration}`.

**Core matrix pattern to follow** (lines 64-79) — every new backend/kind pair must be added as an explicit match arm with a `reason: String` following the existing string style (a full sentence explaining WHY a copy is needed, not just a label):
```rust
pub fn plan_column(dtype_backend: DtypeBackend, arrow_kind: ArrowKind, is_contiguous: bool) -> ColumnPlan {
    match (dtype_backend, arrow_kind) {
        (DtypeBackend::Arrow, ArrowKind::Numeric) => ColumnPlan::ZeroCopyBorrow,
        (DtypeBackend::Arrow, ArrowKind::Bool) => ColumnPlan::ZeroCopyBorrow,
        (DtypeBackend::Numpy, ArrowKind::Numeric) if is_contiguous => ColumnPlan::ZeroCopyBorrow,
        (DtypeBackend::Numpy, ArrowKind::Numeric) => ColumnPlan::RequiresCopy {
            reason: "numpy buffer is not contiguous ...".to_string(),
        },
        (DtypeBackend::Numpy, ArrowKind::Bool) => ColumnPlan::RequiresCopy {
            reason: "numpy bool is stored as 1 byte per element ...".to_string(),
        },
    }
}
```
D-13's chunk-count-awareness does NOT belong in this matrix per RESEARCH.md Pitfall 6 (Strategy B, recommended): `plan_column`'s pure input/output shape stays unchanged; chunk-count correction happens as a post-hoc adjustment in `from_pandas` (pandas.rs), not as a new parameter here. Do not add a `chunk_count` parameter to `plan_column`.

**Testing pattern** (lines 82-138): every new `(backend, kind)` combination needs a `#[test]` function following the existing naming convention `plan_column_<backend>_<kind>_is_<result>` / `plan_column_<backend>_<kind>_requires_copy`, asserting via `assert_eq!`/`assert!(matches!(...))`.

---

### `crates/flint-python/src/pandas.rs` (service, request-response)

**Analog:** itself, current `classify_dtype` (lines 84-114) and `from_pandas` (lines 116-174) and `import_column_via_pandas_stream` (lines 192-219)

**Current classify_dtype pattern to restructure** (lines 84-114) — currently kind-first, must become isinstance-first per RESEARCH.md's centerpiece fix:
```rust
fn classify_dtype(
    dtype: &Bound<'_, PyAny>,
    arrow_dtype_type: &Bound<'_, PyAny>,
    column_name: &str,
) -> PyResult<(DtypeBackend, ArrowKind, String)> {
    let dtype_str: String = dtype.str()?.extract()?;
    let kind: String = dtype.getattr("kind")?.extract()?;

    let arrow_kind = match kind.as_str() {
        "b" => ArrowKind::Bool,
        "i" | "u" | "f" => ArrowKind::Numeric,
        _ => {
            return Err(FlintError::UnsupportedColumn {
                column: column_name.to_string(),
                dtype: dtype_str,
                reason: "only numeric (int/uint/float) and boolean columns are supported in this phase".to_string(),
            }.into());
        }
    };

    let dtype_backend = if dtype.is_instance(arrow_dtype_type)? {
        DtypeBackend::Arrow
    } else {
        DtypeBackend::Numpy
    };

    Ok((dtype_backend, arrow_kind, dtype_str))
}
```
**Rejection error pattern to reuse verbatim** (same lines): every new out-of-scope dtype (D-08 masked extension types, D-15 non-ns temporal) must raise `FlintError::UnsupportedColumn { column, dtype, reason }` — same struct, same call site style, just a different `reason` string. RESEARCH.md's diagram gives the exact isinstance-first dispatch order to implement (checked already against pandas 3.0.3/pyarrow 25.0.0 in RESEARCH.md's Code Examples section, "Pattern: isinstance-first dtype classification").

**Current from_pandas per-column loop to extend** (lines 118-174) — the `is_contiguous` branch (lines 136-146) and `plan`-driven array-selection `match` (lines 155-160) are the two places new `(DtypeBackend, ArrowKind)` combinations plug in:
```rust
let is_contiguous = match dtype_backend {
    DtypeBackend::Arrow => true,
    DtypeBackend::Numpy => {
        let values = series.getattr("values")?;
        values.getattr("flags")?.getattr("c_contiguous")?.extract::<bool>()?
    }
};

let plan = plan_column(dtype_backend, arrow_kind, is_contiguous);
records.push(ColumnConversionRecord::from_plan(column_name_str.clone(), dtype_str, &plan));

let array = match (&plan, dtype_backend, arrow_kind) {
    (ColumnPlan::ZeroCopyBorrow, DtypeBackend::Numpy, ArrowKind::Numeric) => {
        borrow_numpy_numeric_column(&series, &column_name_str)?
    }
    _ => import_column_via_pandas_stream(py, df, &column_name)?,
};
```
Pitfall 1 fix (D-08): the new `DtypeBackend::Categorical`/new `ArrowKind` variants must NOT reach the `values.getattr("flags")` call — that line only makes sense for genuine numpy `ndarray`s (masked `Int64`/`boolean` crash there with `AttributeError`, confirmed in RESEARCH.md). Guard this branch so it is only reached for `DtypeBackend::Numpy` AND a plain-numpy-compatible kind.

**Field construction bug to fix** (lines 162-166) — currently drops `dict_is_ordered` for dictionary columns (RESEARCH.md Pitfall 3), verified root cause:
```rust
fields.push(Field::new(
    &column_name_str,
    array.data_type().clone(),
    array.null_count() > 0,
));
```
Fix per RESEARCH.md Code Examples section (verbatim Rust sketch, "Fix for Pitfall 3"):
```rust
use arrow::datatypes::{DataType, Field};

fn build_field(column_name: &str, array: &dyn arrow::array::Array, is_ordered: Option<bool>) -> Field {
    match array.data_type() {
        DataType::Dictionary(key_type, value_type) => {
            Field::new_dictionary(column_name, (**key_type).clone(), (**value_type).clone(), array.null_count() > 0)
                .with_dict_is_ordered(is_ordered.unwrap_or(false))
        }
        other => Field::new(column_name, other.clone(), array.null_count() > 0),
    }
}
```
`is_ordered` must be sourced from `series.dtype.ordered` when `isinstance(dtype, pd.CategoricalDtype)`, per RESEARCH.md Pitfall 3's "How to avoid."

**Fallback conversion mechanism — DO NOT MODIFY** (lines 192-219): `import_column_via_pandas_stream` already generically handles Dictionary/Timestamp/Duration/nullable-numeric/string arrays per RESEARCH.md's empirical verification (Pitfall 0). The only required change here is its *return signature*, per D-13/Pitfall 6 Strategy B: change `-> PyResult<ArrayRef>` to `-> PyResult<(ArrayRef, usize)>` (array + `batches.len()`), so `from_pandas` can correct `ColumnConversionRecord.zero_copy`/`reason` after the fact when `batches.len() > 1`, without touching `plan_column`'s pure matrix. The single-batch (line 211-213) and multi-batch concat (lines 215-218) logic itself is unchanged — only the batch count needs to additionally flow back to the caller.

**Error construction pattern to reuse** (`crates/flint-python/src/error.rs`, lines 24-29): every rejection continues to use the existing `FlintError::UnsupportedColumn { column, dtype, reason }` variant — no new `FlintError` variant is needed for D-08/D-15 rejections (this is the "Claude's Discretion" item resolved by reuse, matching the existing pattern exactly). New copy `reason` strings (object-dtype, multi-chunk) also reuse the existing `ColumnPlan::RequiresCopy { reason: String }` shape — no schema change needed unless the planner decides `ColumnConversionRecord.reason` needs a structured category (CONTEXT.md leaves this to discretion; simplest-compatible choice is to keep it a free-text `String` and just vary the sentence, consistent with current `plan_column` reasons which are also free text).

**NEW validation pass needed (D-11, no existing analog — net-new function in same file):** a Python-side scan over object-dtype column values, following the existing function style (small, single-purpose, `PyResult<T>`-returning free function like `borrow_numpy_numeric_column`), iterating `series` values via PyO3 and raising `FlintError::UnsupportedColumn` naming `type(v).__name__` on the first non-str/non-null value. Model its signature/error style directly on `borrow_numpy_numeric_column` (lines 250-308): same `(series: &Bound<'_, PyAny>, column_name: &str) -> PyResult<()>` shape, same `FlintError::Other`/`FlintError::UnsupportedColumn` construction style.

---

### `crates/flint-python/src/table.rs` (controller, request-response)

**Analog:** itself, current `to_pandas` method (lines 125-142)

**Current blanket types_mapper to replace** (lines 136-140):
```rust
let pandas = py.import("pandas")?;
let arrow_dtype = pandas.getattr("ArrowDtype")?;
let kwargs = PyDict::new(py);
kwargs.set_item("types_mapper", arrow_dtype)?;
let df = pa_table.call_method("to_pandas", (), Some(&kwargs))?;
```
Fix per RESEARCH.md Pitfall 4 + Code Examples ("Fix for Pitfall 4"), using a `PyCFunction::new_closure` in place of the static `arrow_dtype` class reference:
```rust
let pa_types = py.import("pyarrow")?.getattr("types")?;
let types_mapper = pyo3::types::PyCFunction::new_closure(
    py, None, None,
    move |args: &Bound<'_, pyo3::types::PyTuple>, _kwargs| -> PyResult<PyObject> {
        let arrow_type = args.get_item(0)?;
        let is_dictionary: bool = pa_types.call_method1("is_dictionary", (&arrow_type,))?.extract()?;
        if is_dictionary {
            Ok(py.None())  // fall through to pyarrow's own default (Categorical) reconstruction
        } else {
            // else branch: pandas.ArrowDtype(arrow_type), same as current unconditional behavior
        }
    },
)?;
kwargs.set_item("types_mapper", types_mapper)?;
```
Everything else in `to_pandas` (lines 126-135, 141-142) — batch collection, `PyTable::try_new`, `into_pyarrow`, empty-table error — is unchanged; only the `types_mapper` construction changes. The doc comment above the method (lines 15-28, 109-124) will need a follow-up update noting the dictionary-column exception to the "unconditionally zero-copy" claim (RESEARCH.md Pitfall 4's flagged Open Question re: `.cat.codes` buffer not being zero-copy for the default/non-ArrowDtype dictionary reconstruction path).

**Everything else in this file (from_pandas classmethod, PyCapsule dunders, buffer_address, copy_report) — NO changes expected**, since `from_pandas`'s strict-mode/copy_report machinery already just reads `ColumnConversionRecord`s produced by `pandas::from_pandas` (lines 86-107, 218-220) — Phase 2's changes flow through unchanged per the "single source of truth" pattern already established.

---

### `crates/flint-python/src/diagnostics.rs` (service, request-response)

**Analog:** itself, `check_strict` (lines 40-52) and `build_copy_report` (lines 57-76) — expected to require NO code changes.

Both functions consume `&[ColumnConversionRecord]` generically (matching on `record.zero_copy`/`record.reason`, lines 41-49, 67-72) — they have no dtype-specific logic at all. D-13's chunk-count-awareness fix flowing through `ColumnConversionRecord.zero_copy`/`.reason` (corrected in `pandas.rs`) requires zero changes here, confirmed by RESEARCH.md's Architectural Responsibility Map ("diagnostics.rs's consumers need no changes since they already just read whatever ColumnConversionRecord says"). Planner should still list this file as "verify unchanged" in a plan's test/verification section, not skip it.

---

## Shared Patterns

### Error construction (`FlintError::UnsupportedColumn`)
**Source:** `crates/flint-python/src/error.rs` lines 24-29, 44
**Apply to:** every new rejection path (D-08 masked extension dtypes, D-15 non-ns temporal, D-11 object-content validation)
```rust
#[error("column {column:?} (dtype={dtype}) is not supported: {reason}")]
UnsupportedColumn {
    column: String,
    dtype: String,
    reason: String,
},
// ...
FlintError::UnsupportedColumn { .. } => PyTypeError::new_err(err.to_string()),
```
No new `FlintError` variant needed for these rejections — reuse this one with varied `reason` text, consistent with D-08/D-15 both being "confirm/extend the existing rejection pattern" per CONTEXT.md's Claude's Discretion section.

### Single source-of-truth decision flow
**Source:** `crates/flint-core/src/pandas_plan.rs` module doc (lines 1-11) + `crates/flint-python/src/pandas.rs` module doc (lines 1-23)
**Apply to:** all new dtype/backend combinations — every copy-vs-borrow decision must be added to `plan_column`'s match arms (pandas_plan.rs) and consumed, never re-derived, by `pandas.rs`/`diagnostics.rs`/`table.rs`. This is the load-bearing architectural constraint for this entire phase (RESEARCH.md Pitfall 2 / apache/arrow#39194) — any planner-proposed file change that computes a copy-vs-borrow decision outside `plan_column` should be flagged as a deviation.

### Delegating actual data conversion to `import_column_via_pandas_stream`
**Source:** `crates/flint-python/src/pandas.rs` lines 176-219 (doc comment + implementation)
**Apply to:** string/object, categorical, timestamp, duration, and multi-chunk conversion — RESEARCH.md's empirically-verified finding is that this existing function ALREADY handles every new dtype's actual FFI marshalling correctly; new code should extend its *return value* (batch count) and its *caller's* handling (validation, Field construction), never its internal `__arrow_c_stream__`/`concat` mechanism.

## No Analog Found

None — this phase modifies only existing, already-read files; every change point has a directly corresponding existing pattern in the same file to extend.

## Metadata

**Analog search scope:** `crates/flint-core/src/`, `crates/flint-python/src/` (pandas_plan.rs, pandas.rs, table.rs, diagnostics.rs, error.rs) — the full set of files named in RESEARCH.md's Recommended Extension Points and CONTEXT.md's Reusable Assets.
**Files scanned:** 5 (all fully read, no re-reads)
**Pattern extraction date:** 2026-07-16
