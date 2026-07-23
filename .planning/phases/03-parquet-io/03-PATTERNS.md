# Phase 3: Parquet IO - Pattern Map

**Mapped:** 2026-07-23
**Files analyzed:** 7 (2 new modules, 1 new module-or-submodule, 3 modified files, 1 Cargo.toml x2)
**Analogs found:** 7 / 7 (all files have a strong same-crate analog; this phase is additive to an established two-crate architecture, not greenfield)

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|---|---|---|---|---|
| `crates/flint-core/src/parquet_io.rs` (NEW) | service (pyo3-free core logic) | file-I/O (read/write) + batch | `crates/flint-core/src/pandas_plan.rs` | role-match (single-decision-point pattern, exact) |
| `crates/flint-core/src/parquet_filter.rs` (NEW, or submodule of above) | service (predicate/decision logic) | transform (row-group stats -> keep/skip decision) | `crates/flint-core/src/pandas_plan.rs` (`plan_column`) | exact (same "one function, one decision, unit-testable, no pyo3" shape) |
| `crates/flint-python/src/table.rs` (MODIFY: add `from_parquet`/`to_parquet` `#[pymethods]`) | controller (PyO3 boundary) | request-response (classmethod/instance method wrapping file I/O) | `crates/flint-python/src/table.rs` itself — `from_pandas`/`to_pandas` `#[pymethods]` (lines 91-191) | exact |
| `crates/flint-python/src/pandas.rs` (MODIFY: WR-01 fix in `build_field`) | utility (field/schema construction) | transform | `crates/flint-python/src/pandas.rs` itself (lines 454-475, 505-531) | exact (same file, same function, direct bug fix) |
| `crates/flint-python/src/error.rs` (MODIFY: add Parquet-specific `FlintError` variants) | utility (error boundary) | transform (Rust error -> PyErr) | `crates/flint-python/src/error.rs` itself (`UnsupportedColumn` variant + match arm) | exact |
| `crates/flint-core/Cargo.toml` (MODIFY: add `parquet = "59.1.0"`) | config | — | `crates/flint-core/Cargo.toml`'s existing `arrow = "59.1.0"` line | exact |
| `tests/rust/*.rs` (NEW: Wave-0 dictionary/tz round-trip spike) | test | request-response (round-trip assertion) | `tests/rust/concat_generic_arrays.rs` / `tests/rust/zero_copy_alloc.rs` | exact (same repo-root `tests/rust/` convention, `[[test]]` entry in `flint-core/Cargo.toml`) |

## Pattern Assignments

### `crates/flint-core/src/parquet_io.rs` (service, file-I/O)

**Analog:** `crates/flint-core/src/pandas_plan.rs` (whole file, ~322 lines, already read in full)

**Module doc-comment pattern** (lines 1-11 of pandas_plan.rs):
```rust
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
```
Copy this doc-comment shape directly: state the single-decision-point discipline this new module must follow (row-group-skip decision and row-level `ArrowPredicateFn` must derive from the SAME parsed `FilterExpr` list — RESEARCH.md Anti-Patterns), and state explicitly that this module has no `pyo3` dependency so it is unit-testable without a Python interpreter (matches `flint-core`'s crate-level pyo3-free contract, confirmed by `crates/flint-core/Cargo.toml`'s dependency list containing only `arrow`).

**Core pattern — write path** (RESEARCH.md Pattern 1, already Rust-verified against docs.rs):
```rust
use parquet::arrow::ArrowWriter;
use parquet::file::properties::WriterProperties;
use parquet::basic::{Compression, ZstdLevel, GzipLevel};

fn build_writer_properties(codec: &str, row_group_size: usize) -> Result<WriterProperties, FlintError> {
    let compression = match codec {
        "snappy" => Compression::SNAPPY,
        "zstd" => Compression::ZSTD(ZstdLevel::default()),
        "gzip" => Compression::GZIP(GzipLevel::default()),
        "uncompressed" => Compression::UNCOMPRESSED,
        other => return Err(FlintError::UnsupportedCodec(other.to_string())), // D-29
    };
    Ok(WriterProperties::builder()
        .set_compression(compression)
        .set_max_row_group_size(row_group_size) // D-30; verify exact setter name against pinned 59.1.0 first (Pitfall 4)
        .build())
}
```

**Return-type / error pattern:** mirror `FromPandasOutcome`'s shape (a plain struct returned by value, no `pyo3` types inside it) — see `pandas.rs`'s `FromPandasOutcome { batch, records }` (referenced by `pandas_plan.rs` doc comment) — for `read_parquet`'s return value (e.g. `RecordBatch` or `Vec<RecordBatch>` + any diagnostics), so the PyO3 boundary in `table.rs` does the final `PyTable::try_new` construction exactly like `from_pandas` does today (see `table.rs` lines 99-112 below).

---

### `crates/flint-core/src/parquet_filter.rs` (service, transform)

**Analog:** `crates/flint-core/src/pandas_plan.rs`'s `plan_column` function + its exhaustive match + its `#[cfg(test)]` unit-test module (lines 110-321)

**Core pattern — one function, one enum, exhaustive match, table-driven unit tests:**
```rust
// plan_column's shape to mirror for the filter-comparison function:
pub fn plan_column(dtype_backend: DtypeBackend, arrow_kind: ArrowKind, is_contiguous: bool) -> ColumnPlan {
    match (dtype_backend, arrow_kind) {
        (DtypeBackend::Arrow, ArrowKind::Numeric) => ColumnPlan::ZeroCopyBorrow,
        // ... exhaustive, no wildcard `_ =>` catch-all that could silently mask a new variant
        (DtypeBackend::Numpy, ArrowKind::Numeric) => ColumnPlan::RequiresCopy {
            reason: "...".to_string(),
        },
        // unreachable-in-practice pairings still explicitly listed and returned as a safe default,
        // never a `panic!`/`unreachable!()`
        (DtypeBackend::Arrow, ArrowKind::Categorical) | ... => ColumnPlan::RequiresCopy { reason: "..." },
    }
}
```
Apply this exact discipline to the per-operator row-group min/max comparison function (RESEARCH.md Open Question 2 / Pattern 2): one `fn could_match_range(op: Op, value: &ScalarValue, min: Option<&Scalar>, max: Option<&Scalar>) -> bool` (or similarly named), exhaustively matched over the fixed D-25 six-operator `Op` enum, each arm unit-tested with the same table-driven style as `pandas_plan.rs`'s `#[cfg(test)] mod tests` block below it (one `#[test] fn plan_column_<case>_is_<expected>()` per case; see lines 173-320 for the exact naming/assertion convention — `assert_eq!`/`assert!(matches!(...))`). Pay special attention to the `!=` operator per RESEARCH.md Pitfall/Anti-Pattern: only skippable when `min == max == value`, unlike the other five operators' straightforward range check — write this as its own explicit match arm, not derived by negating `==`'s logic.

**Doc-comment for the "why doesn't the crate give this to me" framing** (mirrors `pandas_plan.rs` lines 79-109's locked-decision-matrix table-in-doc-comment style): document the six-operator/range-comparison table directly in this function's doc comment the same way `plan_column`'s doc comment contains its own locked decision matrix as a markdown table — this is the established convention for "here is the exhaustive contract, in prose, right above the code."

---

### `crates/flint-python/src/table.rs` — add `from_parquet`/`to_parquet` (controller, request-response)

**Analog:** the same file's existing `from_pandas`/`to_pandas` `#[pymethods]` (lines 91-191, already fully read)

**Imports pattern** (lines 35-42):
```rust
use arrow::array::Array;
use pyo3::prelude::*;
use pyo3::types::{PyCapsule, PyCFunction, PyDict, PyTuple, PyType};
use pyo3_arrow::PyTable;

use crate::diagnostics;
use crate::error::FlintError;
use crate::pandas::{self, ColumnConversionRecord};
```
For Parquet, add `use crate::parquet as flint_parquet;` (a new thin `flint-python/src/parquet.rs` PyO3-facing module, or inline path-parsing directly in `table.rs` if kept small) and `use flint_core::parquet_io;` analogous to how `pandas.rs`'s `flint_core::pandas_plan::plan_column` is consumed today.

**Classmethod pattern — `from_parquet` should mirror `from_pandas`** (lines 91-112):
```rust
#[classmethod]
#[pyo3(signature = (df, strict=false))]
fn from_pandas(
    _cls: &Bound<'_, PyType>,
    py: Python<'_>,
    df: &Bound<'_, PyAny>,
    strict: bool,
) -> PyResult<Self> {
    let outcome = pandas::from_pandas(py, df)?;

    if strict {
        diagnostics::check_strict(&outcome.records)?;
    }

    let schema = outcome.batch.schema();
    let py_table = PyTable::try_new(vec![outcome.batch], schema)?;

    Ok(Self {
        inner: Py::new(py, py_table)?,
        column_reports: outcome.records,
    })
}
```
`Table::from_parquet(cls, path_or_paths, columns=None, filters=None)` should follow this identical shape: parse the Python-facing args (path/List[Path]/dir string, column list, filter tuple list) at the PyO3 boundary, delegate the actual file I/O + row-group pruning + row filtering entirely to a pyo3-free `flint_core::parquet_io::read_parquet(...)` call, then build the final `Table` via the same `PyTable::try_new(vec![batch(es)], schema)` + `Py::new(py, py_table)?` construction used here. `column_reports: Vec::new()` (matching `from_pytable`'s empty-report convention at lines 64-69) since Parquet-read columns were not produced by `from_pandas`'s per-column decision process.

**Instance method pattern — `to_parquet` should mirror `to_pandas`'s batch-gathering step** (lines 149-157):
```rust
fn to_pandas(&self, py: Python<'_>, strict: bool) -> PyResult<Py<PyAny>> {
    let _ = strict;
    let batches = self.inner.bind(py).get().batches().to_vec();
    let schema = batches.first().map(|batch| batch.schema()).ok_or_else(|| {
        FlintError::Other("cannot reconstruct a pandas DataFrame from an empty Table".to_string())
    })?;
    let owned_table = PyTable::try_new(batches, schema)?;
    ...
}
```
`to_parquet(&self, py, path, compression="snappy", row_group_size=1_048_576)` should gather `self.inner.bind(py).get().batches()` the same way, then delegate to `flint_core::parquet_io::write_parquet(&batches, path, compression, row_group_size)` — the empty-table `Other("cannot ...")` error-message convention should be reused verbatim in style for an empty-table write, if that's a case worth guarding.

**Error-path pattern for path/arg validation:** see `buffer_address`'s pattern (lines 240-262) for how an out-of-range/invalid-argument condition is turned into a `FlintError::Other(format!(...))` and converted via `.into()` — reuse this shape for e.g. "path does not exist" / "not a `.parquet` file in directory glob" validation errors, though prefer the more specific new `FlintError` variants described below over `Other` where the reason is Parquet-specific.

---

### `crates/flint-python/src/pandas.rs` — WR-01 fix in `build_field` (utility, transform)

**Analog:** the same file, same function (lines 454-475), plus its caller context at lines 440-451 and `import_column_via_pandas_stream` (lines 505-531)

**Current (buggy) code** (lines 464-475):
```rust
fn build_field(column_name: &str, array: &dyn Array, is_ordered: Option<bool>) -> Field {
    match array.data_type() {
        DataType::Dictionary(key_type, value_type) => Field::new_dictionary(
            column_name,
            (**key_type).clone(),
            (**value_type).clone(),
            array.null_count() > 0,
        )
        .with_dict_is_ordered(is_ordered.unwrap_or(false)),
        other => Field::new(column_name, other.clone(), array.null_count() > 0),
    }
}
```
Per RESEARCH.md's verified finding (Summary, A5): the fix should thread the ALREADY-AVAILABLE declared nullability from `import_column_via_pandas_stream`'s returned schema (line 516: `let (batches, schema) = py_table.into_inner();` — `schema.field(0).is_nullable()` is sitting unused there) through to `build_field`'s call site (line 444: `fields.push(build_field(&column_name_str, array.as_ref(), is_ordered));`), changing `build_field`'s signature to accept an explicit `is_nullable: bool` parameter instead of deriving it from `array.null_count() > 0`. The `borrow_numpy_numeric_column` fast path (does NOT go through `import_column_via_pandas_stream`) must keep passing a hard-coded `false` unchanged, per RESEARCH.md's explicit call-out. This is a same-file, same-function, minimal-diff fix — no new file, no new pattern needed beyond what's already in `pandas.rs`.

---

### `crates/flint-python/src/error.rs` — add Parquet `FlintError` variants (utility, transform)

**Analog:** the same file's existing `UnsupportedColumn` variant + match arm (lines 22-31, 49)

**Pattern to copy exactly:**
```rust
#[derive(Debug, Error)]
pub enum FlintError {
    ...
    /// A pandas column's dtype is outside this phase's supported numeric happy path.
    #[error("column {column:?} (dtype={dtype}) is not supported: {reason}")]
    UnsupportedColumn {
        column: String,
        dtype: String,
        reason: String,
    },
    ...
}

impl From<FlintError> for PyErr {
    fn from(err: FlintError) -> PyErr {
        match &err {
            ...
            FlintError::UnsupportedColumn { .. } => PyFlintError::new_err(err.to_string()),
            ...
        }
    }
}
```
Add variants following this exact carries-the-offending-value shape:
```rust
#[error("unsupported compression codec {0:?}: expected one of snappy, zstd, gzip, uncompressed")]
UnsupportedCodec(String),

#[error("unsupported filter operator {operator:?} on column {column:?}: expected one of ==, !=, <, <=, >, >=")]
UnsupportedFilterOperator { column: String, operator: String },

#[error("schema mismatch across Parquet files: {first_file:?} and {other_file:?} disagree on column {column:?}")]
ParquetSchemaMismatch { first_file: String, other_file: String, column: String },

#[error("failed to read Parquet file {path:?}: {reason}")]
ParquetReadError { path: String, reason: String },
```
Route `UnsupportedCodec`/`UnsupportedFilterOperator`/`ParquetSchemaMismatch` through `PyFlintError::new_err` (same treatment as `UnsupportedColumn` — a named, catchable `flint`-owned exception, not a generic builtin) and `ParquetReadError` through `PyValueError::new_err` (same treatment as `FlintError::Arrow`/`Other`, since it wraps an underlying I/O/parse failure rather than a caller-input-validation failure) — decide the exact PyErr type per variant using this existing precedent, don't invent a third pattern.

---

### `crates/flint-core/Cargo.toml` — add `parquet` dependency (config)

**Analog:** the same file's existing `arrow = "59.1.0"` line

**Exact pattern to copy:**
```toml
[dependencies]
arrow = "59.1.0"
parquet = "59.1.0"
```
Lockstep-pin `parquet` to the exact same `59.1.0` string (no `^`/range) as the existing `arrow = "59.1.0"` line, per CLAUDE.md's Version Compatibility table and RESEARCH.md's explicit instruction. No extra `features = [...]` needed (RESEARCH.md Assumptions Log A4: default features already include `arrow` + snappy/gzip/zstd).

---

### `tests/rust/*.rs` — Wave-0 dictionary/tz round-trip spike (test, request-response)

**Analog:** `tests/rust/concat_generic_arrays.rs` and `tests/rust/zero_copy_alloc.rs` (both referenced via explicit `[[test]]` entries in `crates/flint-core/Cargo.toml`, not discovered automatically)

**Pattern to copy** (from `flint-core/Cargo.toml`, already read in full):
```toml
[[test]]
name = "concat_generic_arrays"
path = "../../tests/rust/concat_generic_arrays.rs"
```
Add a new `[[test]]` entry (e.g. `name = "parquet_dictionary_tz_roundtrip"`, `path = "../../tests/rust/parquet_dictionary_tz_roundtrip.rs"`) using this exact repo-root `tests/rust/` convention — this is the established location for a Rust-only correctness probe that doesn't need a Python interpreter, exactly matching RESEARCH.md's "mandatory Wave-0 verification gate" recommendation for PARQ-06's dictionary/tz-preservation assumption (A6). Mirror `concat_generic_arrays.rs`'s framing (a probe written specifically to de-risk an assumption before dependent tasks build on it, per that file's own doc comment referencing "Assumption A1 probe").

---

## Shared Patterns

### Single source-of-truth decision function (no re-derived logic in two places)
**Source:** `crates/flint-core/src/pandas_plan.rs`'s `plan_column` (whole file)
**Apply to:** `parquet_io.rs`/`parquet_filter.rs` — the row-group-skip decision (Pattern 2) and the row-level `ArrowPredicateFn` closures (Pattern 3) MUST both consume the exact same parsed `FilterExpr` list built once per `from_parquet` call. This is this project's single most important cross-cutting convention (RESEARCH.md explicitly cites this file as the precedent, and the project's own Anti-Patterns section repeats it for this phase specifically).

### Named, specific errors — no silent best-effort coercion
**Source:** `crates/flint-python/src/error.rs` (`UnsupportedColumn` variant + match arm), reinforced by `pandas.rs`'s `validate_object_column_contents` (rejects first non-str value by name/index rather than silently coercing)
**Apply to:** unsupported codec strings (D-29), unsupported filter operators (D-25), cross-file schema mismatches (D-21 discretion — RESEARCH.md recommends strict-match-required as the v1 default), and malformed/corrupt Parquet file input (must surface as a `Result::Err`-derived `FlintError`, never `.unwrap()` on a `parquet` crate parse result).

### pyo3-free core / PyO3-facing boundary split
**Source:** `crates/flint-core/` (no `pyo3` dependency in its `Cargo.toml`) vs `crates/flint-python/` (all `pyo3`/`pyo3-arrow` dependencies)
**Apply to:** all new Parquet IO code — `parquet_io.rs`/`parquet_filter.rs` live in `flint-core` and take/return only `arrow`/`parquet`-crate types (`RecordBatch`, `PathBuf`, primitive `str`/`usize` args); all `Path`/`str`/`List`/directory-glob/Python-tuple-filter parsing happens in `flint-python/src/table.rs` before delegating in.

### PyTable construction as the final step of any `Table`-producing method
**Source:** `crates/flint-python/src/table.rs`'s `from_pandas` (lines 105-111) and `from_pytable` (lines 64-69)
**Apply to:** `from_parquet`'s final step — `PyTable::try_new(batches, schema)` then `Py::new(py, py_table)?`, with `column_reports: Vec::new()` for the Parquet-read case (matching `from_pytable`'s empty-report convention, since Parquet reads don't go through `from_pandas`'s per-column plan).

## No Analog Found

None — every file this phase touches or creates has a strong, directly-applicable analog already in the codebase (this is an additive phase within an already-established two-crate architecture, not a new architectural pattern).

## Metadata

**Analog search scope:** `crates/flint-core/src/` (table.rs, pandas_plan.rs, lib.rs), `crates/flint-python/src/` (table.rs, pandas.rs, error.rs, lib.rs, import.rs, diagnostics.rs), `crates/*/Cargo.toml`, `tests/rust/`
**Files scanned:** 11 `.rs` files (via `find`), both crate `Cargo.toml` files, 4 read in full (`table.rs` x2, `error.rs`, `pandas_plan.rs`), 1 partially read (`pandas.rs` lines 440-560)
**Pattern extraction date:** 2026-07-23
