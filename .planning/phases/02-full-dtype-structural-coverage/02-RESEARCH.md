# Phase 2: Full Dtype & Structural Coverage - Research

**Researched:** 2026-07-16
**Domain:** pandas <-> Arrow dtype/structural coverage (nulls, object/string, categorical, datetime/tz/timedelta, multi-chunk) on top of an existing Rust/PyO3/arrow-rs zero-copy bridge
**Confidence:** HIGH for the empirically-reproduced findings below (all run directly against this repo's pinned pandas 3.0.3 / pyarrow 25.0.0 / arrow 59.1.0 stack, not guessed); MEDIUM for claims sourced only from web search/docs.rs without local reproduction (tagged inline)

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

**Null Handling Scope (CONV-03)**
- **D-07:** Only `pandas.ArrowDtype`-backed nullable columns are in scope for Phase 2's null support (e.g. `int64[pyarrow]` with nulls). These already carry an Arrow-compatible null bitmap internally via the existing `DtypeBackend::Arrow` path.
- **D-08:** Pandas' own nullable extension dtypes (`Int64`, `boolean`, `Float64` — capital-letter masked-array dtypes using `pd.NA`) are explicitly OUT of scope for Phase 2. Not rejected as an error necessarily, but not a locked requirement — planner should confirm current `classify_dtype` rejection behavior for these is still an honest, clear error.
- **D-09:** Plain numpy `float64` columns containing `NaN` are NOT treated as nulls. They keep going through the existing Phase 1 zero-copy numeric path completely unchanged — `NaN` round-trips as a literal float value with no Arrow null bitmap involved. This is a deliberate non-change: CONV-03's null-handling work targets only ArrowDtype-backed nullable columns, not NaN-as-missing-value semantics on the numpy path.

**String/Object Dtype Scope (CONV-04)**
- **D-10:** Both `string[pyarrow]`/ArrowDtype string columns AND legacy numpy `object`-dtype string columns are in scope. Object-dtype columns are accepted with an honest copy — introspection + conversion — reported via `copy_report()`/`RequiresCopy` (reason: no Arrow-compatible physical layout), not silently or rejected outright.
- **D-11:** When an object-dtype column is accepted, Flint MUST validate that every non-null value is a `str` (or the value is `None`/null). If any non-string value is found (int, dict, custom object, etc.), raise a clear error naming the column and the offending value's type — no best-effort `str()` coercion. This matches the project's existing explicit-rejection philosophy (no silent best-effort behavior).

**Multi-Chunk Handling Strategy (CONV-08 + DIAG-01/DIAG-02 override closure)**
- **D-12:** Phase 2 does NOT pursue genuine zero-copy multi-chunk preservation. Keep the Phase 1 approach of concatenating a multi-chunk Arrow-backed pandas column into one contiguous batch via `arrow::compute::concat` (an honest copy, not zero-copy). This is a deliberate scope decision — true zero-copy multi-chunk support would require per-column independent chunk-boundary alignment (pyarrow's `ChunkedArray` model allows different chunk boundaries per column; arrow-rs `RecordBatch` requires uniform row counts per batch) and is judged too large a lift for this phase.
- **D-13:** The actual Phase 2 fix is making `plan_column`/`ColumnConversionRecord` chunk-count-aware, so `strict=True` and `copy_report()` honestly reflect the concat copy for multi-chunk columns — closing the DIAG-01/DIAG-02 gap recorded as an override in `01-VERIFICATION.md`.
- **D-14:** This is an intentional behavior change for existing callers: once `plan_column` becomes chunk-count-aware, `from_pandas(df, strict=True)` will now RAISE `ZeroCopyRequiredError` for a multi-chunk Arrow-backed column that previously succeeded silently (because the concat is now correctly recognized as a real copy). This is the correct fix per DIAG-01's contract ("errors instead of silently falling back to a copy") — no opt-in flag (e.g. `allow_rechunk`) to bypass this under strict mode.

**Datetime/Timezone/Timedelta (CONV-06/CONV-07)**
- **D-15:** Only nanosecond-resolution pandas dtypes are in scope: `datetime64[ns]`, `datetime64[ns, tz]`, and `timedelta64[ns]`, mapping directly to Arrow `Timestamp(Nanosecond, tz)` / `Duration(Nanosecond)`. Non-ns-resolution columns (`datetime64[s]`/`[ms]`/`[us]`, possible since pandas 2.0's unit-flexible dtypes) are explicitly OUT of scope and should be rejected with a clear error naming the column and its actual resolution.
- **D-16:** Timezone-aware timestamp columns round-trip the tz string exactly as-is (e.g. `"America/New_York"`) with no internal normalization to UTC. Flint does not hand-roll DST/ambiguous-time logic — it trusts pandas'/arrow-rs's own tz handling and only surfaces an error if arrow-rs itself rejects the input.

**Categorical Round-Trip Fidelity (CONV-05)**
- **D-17:** Both the `ordered` flag and the exact category order must be preserved exactly across the round trip. Reconstructing a Table back to pandas must yield a `Categorical` with identical `ordered` and `categories` (order included) to the original — not just equivalent values. This applies even for unordered categoricals, where category definition order still matters for round-trip equality.
- **D-18:** The exact integer code width pandas chose (`int8`/`int16`/`int32`/`int64`, based on category count) must be preserved across the round trip — the reconstructed `Categorical`'s `.cat.codes.dtype` must match the source, not be normalized to one fixed width.

### Claude's Discretion
- Exact rejection error message/type for pandas nullable extension dtypes (`Int64`/`boolean`/`Float64`) and non-ns-resolution datetime/timedelta dtypes — should follow the existing `FlintError::UnsupportedColumn` pattern from Phase 1, naming the column and dtype.
- Internal representation details for preserving categorical code width and order (e.g. how `classify_dtype`/`plan_column` are extended) — implementation detail within the locked fidelity requirements above.
- Whether/how `ColumnConversionRecord`'s `reason` field gains new categories to distinguish "structural copy due to multi-chunk" from "dtype-driven copy" (e.g. object-dtype string) — API detail within D-13's locked chunk-count-awareness requirement.

### Deferred Ideas (OUT OF SCOPE)
- **Genuine zero-copy multi-chunk preservation** — explicitly deferred, not to a specific future phase.
- **Pandas nullable extension dtypes** (`Int64`/`boolean`/`Float64` masked arrays) — deferred, not scheduled to any specific future phase.
- **Non-nanosecond datetime/timedelta resolution** (`datetime64[s]`/`[ms]`/`[us]`) — deferred, not scheduled to any specific future phase.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-------------------|
| CONV-03 | Convert pandas columns with nulls (numeric) to/from Arrow Table with correct null handling | Confirmed empirically: `ArrowDtype`-backed nullable columns (`int64[pyarrow]` etc.) already carry a real Arrow validity bitmap and already pass `classify_dtype`'s existing `kind` check (`'i'`/`'u'`/`'f'`/`'b'` unchanged) — the null bitmap transmits automatically through the existing `__arrow_c_stream__` FFI import path with no new Rust code required. The actual required work is (a) confirming/fixing D-08's rejection path for masked extension dtypes (`Int64`/`boolean`/`Float64`), which this research found is **currently a raw, unattributed `AttributeError`, not an honest `FlintError`** — see Common Pitfall 1 — and (b) the DIAG-01/02 chunk-awareness fix shared with CONV-08. |
| CONV-04 | Convert object/string dtype columns to/from Arrow Table with correct value/null handling | Confirmed empirically: both `string[pyarrow]` (via `.pyarrow_dtype`/`pa.types.is_string`) and legacy `object`-dtype columns already export via `__arrow_c_stream__` — the existing `import_column_via_pandas_stream` fallback path is structurally sufficient for the *conversion* mechanics. The **validation** required by D-11 is NOT provided by pandas/pyarrow's own machinery — see Common Pitfall 2 (empirically reproduced: dict-valued and int-valued object columns convert silently with no error; mixed-type columns raise an order-dependent, uncontrolled pyarrow error, not a Flint-owned one). |
| CONV-05 | Convert categorical dtype columns to/from Arrow dictionary-encoded columns | Confirmed empirically: plain pandas `Categorical` (ordered flag, category order, and code width int8/16/32/64) round-trips correctly through `__arrow_c_stream__` -> pyarrow dictionary type on export. The **import-back-to-pandas** direction requires a `to_pandas` fix (current blanket `types_mapper=pandas.ArrowDtype` reconstructs an ArrowDtype dictionary column, not a real `Categorical` — violates D-17) — see Common Pitfall 4 with a verified fix. The **schema/Field reconstruction** in `from_pandas` also currently drops the `ordered` flag — see Common Pitfall 3. |
| CONV-06 | Convert datetime and timezone-aware timestamp columns to/from Arrow Table | Confirmed empirically: `datetime64[ns]`/`datetime64[ns, tz]` round-trip correctly through the existing generic `__arrow_c_stream__` path (kind `'M'`) — no new FFI/marshalling code needed, only `classify_dtype` extension + explicit ns-only resolution gating (D-15) — see Common Pitfall 5 for the critical pandas-3.0-specific gotcha (default resolution is no longer ns). |
| CONV-07 | Convert timedelta columns to/from Arrow Table | Same mechanism as CONV-06 (kind `'m'`), same ns-only resolution gating requirement, same pandas-3.0 default-resolution gotcha (Common Pitfall 5). |
| CONV-08 | Convert a Table with multiple chunks per column (ChunkedArray) to/from pandas | `to_pandas` direction already iterates all `batches()` (confirmed correct in `01-VERIFICATION.md`). `from_pandas` direction already concatenates via `arrow::compute::concat` (D-12, already implemented). The remaining required work is D-13's chunk-count-awareness fix for `plan_column`/`ColumnConversionRecord` — see Architecture Patterns section for the two candidate implementation strategies and their trade-offs. |
</phase_requirements>

## Summary

This phase extends an already-working, already-generic zero-copy/copy-fallback pipeline rather than building new FFI machinery. The single most important empirical finding is that **the existing `import_column_via_pandas_stream` pattern (isolate a column into a single-column DataFrame, call its `__arrow_c_stream__` export, import via `pyo3_arrow::PyTable::from_arrow_pycapsule`) already generically handles every new dtype this phase targets** — nullable `ArrowDtype` numerics, `string[pyarrow]`/legacy `object` strings, plain pandas `Categorical` (with correct category order and code width!), and `datetime64[ns]`/`datetime64[ns, tz]`/`timedelta64[ns]` — because pandas'/pyarrow's own conversion machinery does the real work.

This was NOT just verified against pyarrow's own Python-level consumer (`pa.table(df)`) — it was verified by building this repo's actual compiled `flint` extension (`uv run maturin develop`) and driving `flint.Table.from_pandas(df)` / `.to_pandas()` through the real Rust code path (`classify_dtype` was temporarily, minimally patched to route these dtypes to the existing generic fallback for this test only, then reverted via `git checkout` before this document was finalized — repo confirmed clean, extension rebuilt to its original state, and the full existing Python test suite re-confirmed green, 29/29, afterward). Every one of the following round-tripped correctly through the real compiled `flint-python`/arrow-rs code: a small ordered categorical (`int8` codes), a >255-category unordered categorical (`int16` codes, confirming code-width selection flows through unmodified), `string[pyarrow]`, legacy `object` strings with a `None`, `datetime64[ns]`, `datetime64[ns, tz]`, and `timedelta64[ns]` — all correct values, all `zero_copy=True` in `copy_report()`. This upgrades the core thesis from "verified against pandas/pyarrow, inferred for flint" to directly demonstrated against this project's own compiled code.

What is genuinely new work for this phase is **not** FFI/marshalling code, but four specific, verified gaps in the surrounding decision/validation/reconstruction logic: (1) `classify_dtype`'s `dtype.kind`-first dispatch is structurally unable to distinguish `string[pyarrow]` vs. legacy `object` vs. `Categorical` vs. rejected masked extension dtypes, because pandas overloads `dtype.kind == 'O'`/`'i'`/`'b'` across all of them — it must be restructured to check dtype *type* (`isinstance`) before falling back to `kind`; (2) accepting object-dtype columns via the generic capsule-stream path does **not** enforce D-11's "every value must be str" rule — pandas/pyarrow's own inference silently accepts dict-valued and int-valued object columns and produces order-dependent, un-owned error messages for genuinely mixed columns, so Flint must add its own explicit validation pass; (3) `from_pandas`'s schema-reconstruction loop rebuilds a fresh `Field` from only `array.data_type().clone()`, which silently drops the dictionary `ordered` flag (`Field.dict_is_ordered` lives on `Field`, not `DataType`) — a direct, verified violation risk for D-17; (4) `to_pandas`'s blanket `types_mapper=pandas.ArrowDtype` reconstructs a dictionary-typed column as an ArrowDtype dictionary column, not a real `pd.Categorical` — also a direct D-17 violation, with a verified one-line fix (a per-column-type-aware `types_mapper` callable that returns `None` for dictionary types).

A fifth, non-obvious, high-user-impact finding: **pandas 3.0 changed the default parsing resolution for `pd.to_datetime()`/`pd.to_timedelta()` from nanoseconds to microseconds** (confirmed empirically and against pandas' own 3.0.0 whatsnew). Since D-15 restricts scope to ns-only, the single most common way a pandas-3.0 user creates a datetime/timedelta column will now be *rejected* by Flint unless they explicitly `.astype('datetime64[ns]')` — this needs to be a documented, expected rejection case, not a surprise bug report.

**Primary recommendation:** Extend `classify_dtype`/`plan_column`/`ArrowKind`/`DtypeBackend` using `isinstance`-based dtype-type discrimination (not `dtype.kind` alone), continue delegating all actual data conversion to the existing generic `__arrow_c_stream__`/`arrow::compute::concat` pattern, add an explicit Python-side object-dtype content validation pass, fix `from_pandas`'s Field construction to preserve `dict_is_ordered`, and fix `to_pandas`'s `types_mapper` to special-case dictionary columns back to real `Categorical`.

## Architectural Responsibility Map

This project is a Rust/Python native-extension library, not a web application — the tiers below are ARCHITECTURE.md's own five internal components (Rust core / PyO3 Arrow wrappers / PyO3 pandas-interop / PyO3 diagnostics / Python-facing API), used in place of browser/SSR/API/CDN/DB tiers.

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Null bitmap handling (CONV-03) | PyO3 pandas-interop (`classify_dtype`/`plan_column`) | Rust core (arrow-rs `Array`/validity bitmap) | Decision logic (accept ArrowDtype-nullable, reject masked-extension) lives in the binding layer; bitmap semantics are inherited unchanged from arrow-rs's FFI import — no new Rust core code needed. |
| Object/string dtype + D-11 content validation | PyO3 pandas-interop (`pandas.rs`, new validation pass) | — | Validation must happen at the Python/PyO3 boundary (only Python object identity/type-checking can inspect each element); no Rust-core or diagnostics-layer involvement. |
| Categorical round-trip (order, `ordered`, code width) | PyO3 pandas-interop (`from_pandas` schema construction) + PyO3 Arrow wrappers (`to_pandas` type-mapper) | Rust core (arrow-rs `Field.dict_is_ordered`, `DictionaryArray`) | The *conversion* mechanism (FFI import/export of `DictionaryArray`) is already generic in arrow-rs/pyo3-arrow; the fidelity risk is entirely in how `flint-python` reconstructs `Field`/`Schema` (from_pandas) and picks a `types_mapper` (to_pandas) around that already-correct core. |
| Datetime/tz/timedelta ns-only scoping | PyO3 pandas-interop (`classify_dtype` resolution check) | Rust core (arrow-rs `Timestamp`/`Duration` types, unchanged) | Same reasoning as null handling: the FFI conversion of `Timestamp(Nanosecond, tz)`/`Duration(Nanosecond)` is already generic; the new work is purely a dtype-classification gate (reject non-ns resolutions) in the binding layer. |
| Multi-chunk diagnostics honesty (CONV-08 + DIAG-01/02 closure) | PyO3 pandas-interop (`plan_column`/`ColumnConversionRecord`) + PyO3 diagnostics (`check_strict`/`build_copy_report`, unchanged consumers) | — | `plan_column`'s decision must become chunk-count-aware; `diagnostics.rs`'s consumers need no changes since they already just read whatever `ColumnConversionRecord` says (per Phase 1's "never re-derive the decision" pattern). |

## Standard Stack

No new dependencies are required for this phase. All work extends the already-pinned, already-verified Phase 1 stack.

### Core (unchanged from Phase 1 — re-verified current at time of this research)

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|---------------|
| arrow (apache/arrow-rs) | 59.1.0 (pinned in `Cargo.toml`, confirmed) | Rust Arrow implementation: `DictionaryArray`, `TimestampArray`, `DurationArray`, `Field.dict_is_ordered`, `arrow::compute::concat` | Already the project's sole Arrow implementation (CLAUDE.md non-negotiable); this phase uses existing types (`DataType::Dictionary`, `DataType::Timestamp`, `DataType::Duration`) already reachable via the FFI import path, no upgrade needed. |
| pyo3 | 0.29.0 (pinned, confirmed) | Rust<->Python FFI | Unchanged from Phase 1. |
| pyo3-arrow | 0.19.0 (pinned, confirmed) | `PyTable::from_arrow_pycapsule`, FFI stream import (dictionary-aware per arrow-rs's C Data Interface implementation) | Unchanged from Phase 1; this phase's dictionary/temporal support rides on pyo3-arrow's existing generic FFI import, not new pyo3-arrow APIs. |
| numpy (rust-numpy) | 0.29.0 (pinned, confirmed) | Only used for the existing contiguous-numpy-numeric zero-copy borrow path (unchanged) | Phase 2's new dtypes (nulls/strings/categoricals/datetimes) all route through the *existing* `import_column_via_pandas_stream` fallback, not through `rust-numpy`'s buffer-protocol path — no new numpy-crate usage expected. |

### Dev/Test (unchanged — pinned in `pyproject.toml`, confirmed installed in this repo's environment)

| Library | Version | Purpose |
|---------|---------|---------|
| pandas | 3.0.3 (confirmed via `uv run python -c "import pandas; print(pandas.__version__)"`) | Reference/round-trip test target; several Phase 2 findings are pandas-3.0-specific (see State of the Art) |
| pyarrow | 25.0.0 (confirmed installed) | Dev/test comparison target only, never a runtime dependency (CLAUDE.md constraint, unchanged) |
| numpy | 2.5.1 (pinned) | Dev dependency, unchanged |

**Installation:** No new packages. `uv sync --dev && uv run maturin develop` (existing workflow, unchanged).

**Version verification:** `Cargo.toml` confirmed still pins `arrow = "59.1.0"`, `pyo3 = "0.29.0"` (features `extension-module`, `abi3-py311`), `pyo3-arrow = "0.19.0"`, `numpy = "0.29.0"` — no drift from Phase 1's STACK.md. `pyproject.toml`'s dev-dependency group confirmed still pins `pandas==3.0.3`, `pyarrow==25.0.0`, `numpy==2.5.1`. All versions directly read from this repo's own manifest files, not re-fetched from a registry (no version bump is proposed by this phase).

## Package Legitimacy Audit

**Not applicable — this phase introduces no new external packages.** All Rust crates and Python dev-dependencies used by Phase 2's work are the exact same set already vetted and pinned during Phase 1 (see `Cargo.toml`/`pyproject.toml`, re-confirmed above). No `gsd-tools query package-legitimacy check` run was necessary.

## Architecture Patterns

### System Architecture Diagram (Phase 2 additions layered on Phase 1's existing flow)

```
pandas.DataFrame column
        │
        ▼
┌───────────────────────────────────────────────────────────────────┐
│ classify_dtype (flint-python/src/pandas.rs)                       │
│                                                                    │
│  Step 1 (NEW — restructure): dispatch on dtype TYPE, not kind:    │
│    isinstance(dtype, pd.ArrowDtype)?          -> ArrowDtype branch│
│    isinstance(dtype, pd.CategoricalDtype)?    -> Categorical      │
│    isinstance(dtype, ExtensionDtype) (other)? -> REJECT (D-08)    │
│    plain numpy dtype?                         -> kind-based match │
│                                                                    │
│  Step 2 (per branch): resolve ArrowKind + reject out-of-scope     │
│    resolutions (D-15) using np.datetime_data(dtype) / dtype.unit  │
└───────────────────────────────────────────────────────────────────┘
        │ (dtype_backend, arrow_kind) known
        ▼
┌───────────────────────────────────────────────────────────────────┐
│ plan_column (flint-core/src/pandas_plan.rs)                        │
│  UNCHANGED matrix logic for Numeric/Bool; NEW variants added for  │
│  String/Categorical/Timestamp/Duration, all -> ZeroCopyBorrow for  │
│  DtypeBackend::Arrow, RequiresCopy for anything needing a copy     │
│  (object dtype, categorical-from-numpy, multi-chunk — see D-13)   │
└───────────────────────────────────────────────────────────────────┘
        │
        ▼
┌───────────────────────────────────────────────────────────────────┐
│ NEW (D-11): object-dtype content validation                        │
│   ONLY for legacy `object` dtype columns accepted under D-10:      │
│   iterate column values in Python, assert isinstance(v, str) or    │
│   v is None/NaN; raise FlintError::UnsupportedColumn naming the     │
│   column + offending value's type on first violation.              │
│   (pyarrow's own inference is NOT sufficient — see Pitfall 2)       │
└───────────────────────────────────────────────────────────────────┘
        │
        ▼
┌───────────────────────────────────────────────────────────────────┐
│ import_column_via_pandas_stream (UNCHANGED mechanism, reused as-is) │
│   isolate column -> __arrow_c_stream__ -> PyTable::from_arrow_     │
│   pycapsule -> batches (single: Arc clone; multi: arrow::compute:: │
│   concat) -- ALREADY generically handles Dictionary/Timestamp/     │
│   Duration/nullable-numeric/string arrays, confirmed empirically.  │
│                                                                     │
│   NEW (D-13): batches.len() must now flow back into the            │
│   ColumnConversionRecord's zero_copy/reason fields (see two         │
│   candidate strategies below), not just into the returned array.   │
└───────────────────────────────────────────────────────────────────┘
        │
        ▼
┌───────────────────────────────────────────────────────────────────┐
│ Field/Schema construction (from_pandas, NEEDS FIX)                  │
│   CURRENT: Field::new(name, array.data_type().clone(), nullable)   │
│   -- silently drops dict_is_ordered (lives on Field, not DataType)  │
│   FIX: for DataType::Dictionary columns, explicitly propagate the   │
│   source pandas dtype's `.ordered` flag via                        │
│   Field::new_dictionary(...).with_dict_is_ordered(is_ordered)       │
└───────────────────────────────────────────────────────────────────┘
        │
        ▼
   RecordBatch / flint.Table  (unchanged Table/PyCapsule machinery)
        │
        ▼ to_pandas()
┌───────────────────────────────────────────────────────────────────┐
│ to_pandas (flint-python/src/table.rs, NEEDS FIX)                    │
│   CURRENT: blanket types_mapper=pandas.ArrowDtype for ALL columns   │
│   -- reconstructs dictionary columns as ArrowDtype dictionary,      │
│   NOT a real Categorical (violates D-17, verified empirically)      │
│   FIX: types_mapper=lambda t: None if pa.types.is_dictionary(t)     │
│        else pandas.ArrowDtype(t)                                    │
│   (verified: produces a real pd.Categorical with correct ordered/   │
│    categories/codes-dtype for the dictionary column, while other    │
│    columns still get ArrowDtype as before)                          │
└───────────────────────────────────────────────────────────────────┘
        │
        ▼
   pandas.DataFrame (round-tripped)
```

### Recommended Extension Points (no new files needed)

```
crates/flint-core/src/pandas_plan.rs
├── DtypeBackend: add Categorical variant (pandas Categorical is neither
│   plain numpy ndarray nor ArrowDtype — it's its own extension array
│   with its own codes/categories split; treating it as a third backend
│   avoids overloading Numpy/Arrow semantics that don't apply to it)
├── ArrowKind: add String, Categorical, Timestamp { tz: Option<String> },
│   Duration variants
└── plan_column: extend match arms; RequiresCopy reasons follow existing
    string-message pattern

crates/flint-python/src/pandas.rs
├── classify_dtype: restructure to isinstance-first dispatch (see diagram)
├── NEW: validate_object_column_contents (D-11) — Python-side scan
├── from_pandas: NEW dict_is_ordered propagation for Dictionary DataType
└── import_column_via_pandas_stream: NEW — return (ArrayRef, usize) so the
    caller learns actual batch count for D-13's diagnostics fix

crates/flint-python/src/table.rs
└── to_pandas: NEW per-column-type-aware types_mapper (dictionary -> None)
```

### Pattern: isinstance-first dtype classification (NEW, this phase's centerpiece fix)

**What:** Before consulting `dtype.kind`, check the dtype's Python *type* via `isinstance`/attribute checks: `pd.ArrowDtype` -> use `.pyarrow_dtype` + `pa.types.is_*` predicates for unambiguous sub-classification (string/timestamp/duration/dictionary/numeric/bool); `pd.CategoricalDtype` -> handle directly via `.ordered`/`.categories`; any other `pd.api.extensions.ExtensionDtype` (masked `Int64`/`boolean`/`Float64`, non-Arrow `StringDtype`) -> reject explicitly (D-08); otherwise it's a plain numpy dtype, dispatch on `.kind` as Phase 1 already does, adding resolution checks (`np.datetime_data(dtype)`) for `'M'`/`'m'` kinds.

**When to use:** Always, for this phase's `classify_dtype` extension — this is not optional/discretionary, it's the only reliable way to distinguish the dtypes this phase must handle differently.

**Why it's necessary (empirically verified, not assumed):**
```python
# All of these report the SAME dtype.kind letter, verified against pandas 3.0.3:
pd.Series(['a','b'], dtype=object).dtype.kind                       # 'O' -- legacy object
pd.Series(['a','b'], dtype='string[pyarrow]').dtype.kind             # 'O' -- ArrowDtype string
pd.Categorical(['a','b']).dtype.kind                                 # 'O' -- Categorical
pd.array([1,2], dtype='Int64').dtype.kind                            # 'i' -- masked, OUT OF SCOPE (D-08)
pd.array([1,2], dtype='int64[pyarrow]').dtype.kind                   # 'i' -- ArrowDtype, IN SCOPE (D-07)
```
A `kind`-first `match` (Phase 1's current structure) cannot distinguish any of these pairs — `isinstance(dtype, pd.ArrowDtype)` / `isinstance(dtype, pd.CategoricalDtype)` / `isinstance(dtype, pd.api.extensions.ExtensionDtype)` must run first.

**Example (verified against this repo's pinned pandas/pyarrow):**
```python
# Source: empirically verified in this research session against pandas 3.0.3 / pyarrow 25.0.0
import pandas as pd
import pyarrow as pa

def classify(dtype):
    if isinstance(dtype, pd.ArrowDtype):
        pa_t = dtype.pyarrow_dtype
        if pa.types.is_string(pa_t) or pa.types.is_large_string(pa_t):
            return ("Arrow", "String")
        if pa.types.is_timestamp(pa_t):
            return ("Arrow", f"Timestamp(tz={pa_t.tz})")
        if pa.types.is_duration(pa_t):
            return ("Arrow", "Duration")
        if pa.types.is_integer(pa_t) or pa.types.is_floating(pa_t):
            return ("Arrow", "Numeric")
        if pa.types.is_boolean(pa_t):
            return ("Arrow", "Bool")
        return ("Arrow", "Unsupported")
    if isinstance(dtype, pd.CategoricalDtype):
        return ("Categorical", f"ordered={dtype.ordered}")
    if isinstance(dtype, pd.api.extensions.ExtensionDtype):
        # masked Int64/boolean/Float64, non-Arrow StringDtype, etc. -- D-08 out of scope
        return ("REJECT", type(dtype).__name__)
    # plain numpy dtype from here down
    kind = dtype.kind
    if kind in ("i", "u", "f"):
        return ("Numpy", "Numeric")
    if kind == "b":
        return ("Numpy", "Bool")
    if kind == "O":
        return ("Numpy", "ObjectString")  # D-10/D-11 validation required
    if kind in ("M", "m"):
        import numpy as np
        unit, _ = np.datetime_data(dtype)
        if unit != "ns":
            return ("REJECT", f"non-ns resolution: {unit}")
        return ("Numpy", "Timestamp" if kind == "M" else "Duration")
    return ("REJECT", f"unsupported kind {kind!r}")
```

### Anti-Patterns to Avoid

- **Trusting `dtype.kind` alone once ArrowDtype/Categorical/masked-extension dtypes are in play:** verified above to be genuinely ambiguous (three different dtype *kinds* of column share `kind == 'O'`, two share `kind == 'i'` with opposite in/out-of-scope status). This is not a style preference — it is the direct, verified cause of D-08's currently-broken rejection path (see Pitfall 1).
- **Relying on pyarrow's own conversion errors as D-11's validation mechanism:** verified empirically to be order-dependent (an int-then-str column raises a different exception type/message than a str-then-int column) and to silently *succeed* (no error at all) for dict-valued or all-non-str object columns. Flint must own this validation explicitly.
- **Rebuilding `Field` from only `array.data_type()` for dictionary-typed columns:** verified that `dict_is_ordered` lives on `Field`, not `DataType` — `array.data_type().clone()` cannot carry it forward under any circumstance, regardless of how carefully the rest of the pipeline is written.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|--------------|-----|
| Dictionary/categorical FFI encoding, category order, code width selection | A custom `DictionaryArray` builder or manual code-width-selection logic | The existing `import_column_via_pandas_stream` generic capsule-stream import (unchanged) | Verified empirically: pandas' own `__arrow_c_stream__` export already produces a `dictionary<values=..., indices=int8|16|32, ordered=0|1>` pyarrow type with exact category order and pandas-chosen code width preserved — arrow-rs's FFI import (used generically by pyo3-arrow) already deserializes this correctly into `DictionaryArray<Int8Type\|Int16Type\|...>`. Only the *Field-level* `ordered` flag needs an explicit propagation fix (see Pitfall 3) — the array-level encoding itself needs zero new Rust code. |
| Timestamp/timezone semantics, DST/ambiguous-time handling | Custom tz-arithmetic or UTC-normalization code | arrow-rs's own `Timestamp(Nanosecond, tz)` construction via the same generic FFI import, trusting pandas'/arrow-rs's own tz validation (D-16, already locked) | Confirmed via decisions D-16 and Phase 1's established "don't hand-roll FFI" pattern; also, this phase's own empirical test shows tz strings (`"America/New_York"`) round-trip through the existing generic path with zero special-casing already. |
| Object-dtype string *conversion* (the actual byte-copying) | Hand-written Python-object-to-Arrow-string materialization | pandas'/pyarrow's own `__arrow_c_stream__` machinery (same generic fallback path used for numpy-bool/non-contiguous-numeric copies since Phase 1) | Verified: a legacy `object`-dtype string column with a `None` value already exports cleanly to an Arrow `string` array via the existing pattern with zero new conversion code — the *only* new code needed is the pre-conversion validation pass (D-11), not the conversion itself. |
| Multi-chunk merging algorithm | A custom chunk-boundary-alignment or manual buffer-concatenation routine | `arrow::compute::concat` (already implemented per D-12, Phase 1's CR-01 fix) | Already in the codebase (`crates/flint-python/src/pandas.rs::import_column_via_pandas_stream`); Phase 2 only needs the *diagnostics* to catch up to what this already does, not new merge logic. |

**Key insight:** Every one of this phase's six requirements (CONV-03 through CONV-08) is, at the raw-conversion-mechanics level, already handled by Phase 1's generic `__arrow_c_stream__`-based fallback path — verified directly against the pinned pandas 3.0.3/pyarrow 25.0.0/arrow 59.1.0 stack in this research session, not assumed. The real Phase 2 engineering effort is entirely in the surrounding decision/validation/reconstruction layer: telling `classify_dtype` which dtypes are in/out of scope (correctly, unlike today's ambiguous `kind`-based dispatch), validating object-column contents Flint itself must own, and fixing two specific, verified metadata-loss bugs in schema (`from_pandas`) and reverse-mapping (`to_pandas`) construction.

## Common Pitfalls

### Pitfall 0 (spike-confirmed against compiled flint): all four architecture-level gaps reproduce identically through the real Rust code, not just pyarrow's own consumer

Before trusting the pyarrow-only reproductions below, this research additionally built this repo's actual `flint` extension (`uv run maturin develop`) and drove `flint.Table.from_pandas`/`.to_pandas` directly (via a temporary, reverted `classify_dtype` patch that routed new dtypes to the existing generic fallback path, exactly as production code will once `classify_dtype` is properly extended). Findings, against the REAL compiled extension:

- **Masked `Int64` still crashes with a raw `AttributeError`** (`'IntegerArray' object has no attribute 'flags'`) via the actual `flint.Table.from_pandas` call, not just a hypothetical code-read — confirms Pitfall 1 below is a live, reproducible defect, not a theoretical risk.
- **Categorical round-trips correctly for VALUES** through the real compiled extension, including the >255-category case that forces `int16` codes (`pd.Categorical(['c1','c2']*2, categories=[f'c{i}' for i in range(300)])` round-tripped with correct values and `int16` indices in the exported schema).
- **The `ordered` flag is lost by `from_pandas` itself, independent of `to_pandas`:** a source `Categorical(..., ordered=True)` was converted with `flint.Table.from_pandas(df)`, and the resulting `Table` was exported directly via `pa.table(flint_table)` (a PyCapsule export, with **no** `to_pandas`/pyarrow-`to_pandas` conversion involved at all) — the exported schema already reads `dictionary<..., ordered=0>`, i.e. the flag is wrong before `to_pandas` ever runs. This conclusively isolates Pitfall 3's root cause to `from_pandas`'s own `Field::new(..., array.data_type().clone(), ...)` schema-construction step, exactly as hypothesized from the arrow-schema API docs — not merely consistent with it.
- **Object dict-values column silently converts to `struct<k: int64>[pyarrow]`, with `zero_copy=True` and no error**, through the real compiled extension's `import_column_via_pandas_stream` fallback — confirms Pitfall 2 is a live defect in flint's actual conversion path, not just in pyarrow's default inference in isolation.
- **`string[pyarrow]`, legacy `object` strings (with `None`), `datetime64[ns]`, `datetime64[ns, tz]`, and `timedelta64[ns]` all round-tripped correctly** through the real compiled extension with correct values, confirming the "don't hand-roll — the existing generic path already works" thesis directly, not by inference from pyarrow's behavior alone.

This spike was reverted before this document was finalized (`git checkout -- crates/flint-python/src/pandas.rs`, repo confirmed clean, extension rebuilt to its committed state, full existing Python test suite re-confirmed green at 29/29). No production code in this repository was changed by this research pass — the patch existed only transiently to gather the evidence above.

---

### Pitfall 1: D-08's masked-extension-dtype rejection is currently NOT an honest error — it is a raw `AttributeError`

**What goes wrong:** Passing a pandas masked nullable extension column (`pd.array([1, 2, None], dtype='Int64')` or `dtype='boolean'`) into the current `from_pandas` crashes with a bare Python `AttributeError: 'IntegerArray' object has no attribute 'flags'`, not a clean `FlintError::UnsupportedColumn` naming the column and dtype.

**Why it happens:** `classify_dtype`'s current logic determines `arrow_kind` purely from `dtype.kind` (`'i'`/`'b'`/`'f'` all pass the numeric/bool check) and determines `dtype_backend` purely from `dtype.is_instance(arrow_dtype_type)`. A masked `Int64`/`boolean` dtype has `kind == 'i'`/`'b'` (passes) and `isinstance(dtype, pd.ArrowDtype) == False` (so it's classified `DtypeBackend::Numpy`). `from_pandas` then unconditionally calls `series.getattr("values")?.getattr("flags")?.getattr("c_contiguous")` for any `Numpy`-backed column — but `IntegerArray`/`BooleanArray` (pandas' masked-array types) have no `.flags` attribute at all, since they aren't plain numpy arrays.

**How to avoid:** Add an explicit `isinstance(dtype, pd.api.extensions.ExtensionDtype)` check (true for masked `Int64`/`boolean`/`Float64` AND for `CategoricalDtype`/`ArrowDtype`/non-Arrow `StringDtype`) *before* the `kind`-based numpy dispatch, and within that branch explicitly reject anything that isn't `ArrowDtype` or `CategoricalDtype` with `FlintError::UnsupportedColumn` naming the column and the dtype's concrete type name (`type(dtype).__name__`, e.g. `"Int64Dtype"`).

**Warning signs:** Any `.values.flags` or similar attribute-chain access performed before confirming the underlying value is actually a plain numpy `ndarray`.

**Verified:** Empirically reproduced twice in this research session — first in isolation (`pd.Series(pd.array([1,2,None,4], dtype='Int64')).values` is a `pandas.arrays.IntegerArray` with no `.flags` attribute), then directly against the actual compiled `flint` extension (`flint.Table.from_pandas(df)` on a masked-`Int64` column raises the identical raw `AttributeError`, not a `FlintError` — see Pitfall 0).

---

### Pitfall 2: D-11's object-dtype content validation is NOT provided "for free" by delegating to pyarrow's own inference

**What goes wrong:** If Flint's object-dtype handling simply routes to the existing generic `import_column_via_pandas_stream` fallback (as it already does for numpy-bool/non-contiguous-numeric copies), pyarrow's own type-inference machinery silently does whatever it wants rather than enforcing "every non-null value must be `str`":
- A dict-valued object column (`pd.Series([{'a':1}, {'b':2}], dtype=object)`) converts **silently, with no error**, into an Arrow `struct<a: int64, b: int64>` column.
- An all-int object column (`pd.Series([1, 2, 3], dtype=object)`) converts **silently, with no error**, into an Arrow `int64` column.
- A genuinely mixed-type column raises an error, but the exact exception type and message are **order-dependent**: `pd.Series(['a', 123, None], dtype=object)` raises `pyarrow.lib.ArrowTypeError: ("Expected bytes, got a 'int' object", ...)`, while `pd.Series([123, 'a', None], dtype=object)` raises a *different* exception, `pyarrow.lib.ArrowInvalid: ("Could not convert 'a' with type str: tried to convert to int64", ...)`.

**Why it happens:** pandas'/pyarrow's `__arrow_c_stream__` export infers the target Arrow type from the object column's contents rather than enforcing a caller-specified contract; type inference naturally accepts non-string Python objects that happen to be convertible to *some* Arrow type (struct, int64, etc.), and its error behavior on genuinely mixed content depends on which element it encounters first.

**How to avoid:** Add an explicit Flint-owned validation pass over the object column's values (in Python, via PyO3, before calling `import_column_via_pandas_stream`): iterate the column, and for each non-null value assert `isinstance(v, str)`; on the first violation, raise `FlintError::UnsupportedColumn` naming the column and `type(v).__name__` (matching D-11's exact wording). Do this *before* attempting the capsule-stream conversion, not as a post-hoc check of pyarrow's own error.

**Warning signs:** Any test suite for D-11 that only checks a genuinely-mixed-type column raises *some* exception — it must also check dict-valued, all-non-str, and both orderings of mixed-type columns, since pyarrow's default behavior differs across all of these.

**Verified:** All four scenarios above (dict-valued silent success, all-int silent success, two orderings of mixed-type with two different exception types) were independently reproduced in this research session against pandas 3.0.3 / pyarrow 25.0.0's own consumer; the dict-valued silent-success case was additionally reproduced directly against the actual compiled `flint` extension (`flint.Table.from_pandas` produced `zero_copy=True`, no error, `struct<k: int64>[pyarrow]` on `to_pandas` — see Pitfall 0).

---

### Pitfall 3: `from_pandas`'s schema reconstruction silently drops the categorical `ordered` flag

**What goes wrong:** `Field.dict_is_ordered` (the flag D-17 requires to survive the round trip) is a property of arrow-rs's `Field` struct, **not** of `DataType::Dictionary` itself (confirmed via arrow-schema docs: `Field::dict_is_ordered(&self) -> Option<bool>` and `Field::with_dict_is_ordered`). `from_pandas`'s current schema-construction loop (`crates/flint-python/src/pandas.rs`) builds every column's `Field` via `Field::new(&column_name_str, array.data_type().clone(), array.null_count() > 0)` — this only has access to the column's `ArrayRef`/`DataType` (from `import_column_via_pandas_stream`, which itself only returns `batches[0].column(0).clone()`, discarding the imported `RecordBatch`'s own `Schema`). Even though arrow-rs's FFI import (triggered internally by `PyTable::from_arrow_pycapsule`) *does* correctly parse pandas' exported `ARROW_FLAG_DICTIONARY_ORDERED` schema flag into the imported batch's own `Field.dict_is_ordered`, that information never reaches Flint's own rebuilt `Field`, because `from_pandas` never looks at the imported `Field` — only at the bare `ArrayRef`.

**Why it happens:** This is structurally the same class of gap pyo3-arrow itself documents needing to solve for extension types generally (storing `FieldRef` alongside `ArrayRef`, not just `Arc<dyn Array>`) — Flint's own schema-reconstruction step reintroduces exactly that problem by discarding the imported `Field` and rebuilding from `DataType` alone.

**How to avoid:** For any column whose `DataType` is `DataType::Dictionary(..)`, determine the pandas source dtype's `ordered` flag directly (`series.dtype.ordered` when `isinstance(dtype, pd.CategoricalDtype)`) and explicitly construct the `Field` via `Field::new_dictionary(name, key_type, value_type, nullable).with_dict_is_ordered(is_ordered)` rather than the generic `Field::new(..., array.data_type().clone(), ...)`. (Alternative: change `import_column_via_pandas_stream`'s single-batch fast path to return the original imported `Field` alongside the `ArrayRef`, and reuse it directly for dictionary columns — avoids re-deriving `is_ordered` from the pandas dtype a second time.)

**Verified:** `Field::dict_is_ordered`/`with_dict_is_ordered` API confirmed via `docs.rs/arrow-schema/59.1.0` (official docs). The current `pandas.rs` code path (`Field::new(&column_name_str, array.data_type().clone(), ...)`) confirmed by direct code read. **Additionally confirmed end-to-end against the compiled extension** (see Pitfall 0): a source `Categorical(ordered=True)` converted via the real `flint.Table.from_pandas` and exported directly via PyCapsule (`pa.table(flint_table)`, with no `to_pandas` call in between) already shows `ordered=0` in its schema — proving the loss happens in `from_pandas`'s own Field construction, not somewhere in the `to_pandas` direction.

---

### Pitfall 4: `to_pandas`'s blanket `types_mapper=pandas.ArrowDtype` does NOT reconstruct a real `Categorical` for dictionary-typed columns

**What goes wrong:** `Table.to_pandas`'s current implementation (`crates/flint-python/src/table.rs`) unconditionally passes `types_mapper=pandas.ArrowDtype` to pyarrow's `Table.to_pandas`. For a dictionary-typed column, this reconstructs a `pandas.ArrowDtype` column whose `str(dtype)` is `"dictionary<values=large_string, indices=int8, ordered=1>[pyarrow]"` — **not** a plain `pd.Categorical`. D-17 explicitly requires the reconstructed column to be "a `Categorical` with identical `ordered` and `categories`" — an ArrowDtype-wrapped dictionary type does not satisfy this (no `.cat.ordered`/`.cat.categories`/`.cat.codes` accessor surface at all).

**How to avoid (verified fix):** Use a per-column-type-aware `types_mapper` callable instead of the blanket `pandas.ArrowDtype` class reference:
```python
# Source: verified empirically in this research session against pandas 3.0.3 / pyarrow 25.0.0
def types_mapper(arrow_type):
    if pa.types.is_dictionary(arrow_type):
        return None  # fall through to pyarrow's own default (non-Arrow) reconstruction
    return pandas.ArrowDtype(arrow_type)
```
Confirmed: with this mapper, a dictionary column reconstructs as `dtype == 'category'`, with `.cat.ordered`, `.cat.categories` (exact order), and `.cat.codes.dtype` (exact width, e.g. `int8`) all correctly matching the pre-conversion source — while non-dictionary columns are unaffected (still get `pandas.ArrowDtype` as before, preserving Phase 1's CONV-02 behavior).

**Trade-off to flag for the planner:** pyarrow's *default* (non-ArrowDtype) reconstruction of a dictionary array does **not** appear to be zero-copy for the codes buffer (verified: the reconstructed `.cat.codes` numpy array's buffer address differs from the source pyarrow `DictionaryArray`'s indices buffer address) — this is expected and consistent with the project's own "minimal-copy is an honest fallback" posture (per STACK.md's Critical Caveat), but the `copy_report()`/diagnostics surface for `to_pandas` (which today is a no-op per Phase 1's `to_pandas` doc comment, since every output column is "already Arrow memory") may need a documented exception for dictionary columns if strict fidelity to the "no copy-vs-borrow decision on the way out" comment in `table.rs` is desired. Flagged as an **Open Question** below rather than a locked recommendation, since D-17/D-18 do not explicitly require `to_pandas` zero-copy for categoricals (only value/order/width fidelity).

---

### Pitfall 5: pandas 3.0's default datetime/timedelta parsing resolution is no longer nanoseconds

**What goes wrong:** The single most common way a pandas 3.0 user creates a datetime or timedelta column — `pd.to_datetime([...])` / `pd.to_timedelta([...])` with no explicit unit — now produces `datetime64[us]`/`timedelta64[us]` by default, not `datetime64[ns]`/`timedelta64[ns]`. Per D-15, Flint only supports ns-resolution, so this column will be **rejected** by `classify_dtype` unless the caller explicitly `.astype('datetime64[ns]')` (or constructs the array with an explicit `dtype='datetime64[ns]'` from the start).

**Why it happens:** Confirmed via pandas' own "What's new in 3.0.0" release notes (2026-01-21): "the new default resolution when parsing strings is microseconds, falling back to nanoseconds when the precision of the string requires it." This is an intentional pandas 3.0 change, not a bug, but it directly collides with D-15's ns-only scope decision.

**How to avoid:** This is not a code fix — it is a **documentation/error-message requirement**. The rejection error for a non-ns-resolution datetime/timedelta column (per D-15, using the existing `FlintError::UnsupportedColumn` pattern) should explicitly mention that pandas 3.0's default parsing no longer produces ns-resolution and suggest `.astype('datetime64[ns]')` as the fix — otherwise this will read as a confusing, unexpected failure to nearly every pandas-3.0 user who passes a "normal-looking" datetime column.

**Warning signs:** Any test suite for D-15's non-ns rejection path that only constructs the test column via explicit `dtype='datetime64[us]'` — also add a test using plain `pd.to_datetime([...])` with no explicit dtype, since that is the realistic failure mode.

**Verified:** Empirically reproduced (`pd.Series(pd.to_datetime(['2024-01-01','2024-01-02'])).dtype` == `datetime64[us]` against pandas 3.0.3) and independently confirmed against pandas' own official 3.0.0 whatsnew documentation.

---

### Pitfall 6 (carried forward, D-13/D-14 implementation choice): chunk-count-awareness has two viable strategies with different risk profiles

**What goes wrong (if not decided explicitly at plan time):** D-13 requires `plan_column`/`ColumnConversionRecord` to become chunk-count-aware, but the batch count for an `ArrowDtype`-backed column is only definitively known *after* `import_column_via_pandas_stream` runs (today's exact root cause per `01-VERIFICATION.md`). There are two ways to get chunk-count knowledge earlier or later, with different trade-offs the planner must pick between rather than leaving implicit:
- **Strategy A (pre-emptive introspection):** query the pandas column's chunk count *before* conversion via `series.array._pa_array.num_chunks` (verified this attribute exists and returns the correct count in this research session) — but this is a **private pandas attribute** (leading underscore), not a stable public API, and could break across pandas versions without warning.
- **Strategy B (post-hoc correction, recommended):** let `import_column_via_pandas_stream` return `(ArrayRef, usize)` (array + actual batch count observed), and have `from_pandas` correct the already-computed `ColumnConversionRecord`'s `zero_copy`/`reason` fields *after* the fact if `batches.len() > 1`, before `check_strict`/`build_copy_report` ever see it. This avoids relying on any private pandas API and keeps `plan_column`'s own pure-Rust matrix logic unchanged (chunk-awareness becomes a `from_pandas`-level correction step, not a new `plan_column` input).

**Recommendation:** Strategy B — it does not add a dependency on pandas private internals, and it cleanly separates "what `plan_column`'s dtype/backend/contiguity matrix says in the abstract" from "what actually happened once the real batch count was observed," which matches this project's own established pattern of never re-deriving a decision that could diverge (RESEARCH.md Pitfall 2 / apache/arrow#39194) — the correction is a refinement of the single source of truth, not a second decision-maker.

## Code Examples

### Fix for Pitfall 3 (Field `dict_is_ordered` propagation) — Rust sketch

```rust
// Source: synthesized from arrow-schema 59.1.0 docs (Field::new_dictionary,
// Field::with_dict_is_ordered) + this repo's existing pandas.rs structure.
// Not a drop-in diff -- illustrates the required API shape.
use arrow::datatypes::{DataType, Field};

fn build_field(column_name: &str, array: &dyn arrow::array::Array, is_ordered: Option<bool>) -> Field {
    match array.data_type() {
        DataType::Dictionary(key_type, value_type) => {
            Field::new_dictionary(
                column_name,
                (**key_type).clone(),
                (**value_type).clone(),
                array.null_count() > 0,
            )
            .with_dict_is_ordered(is_ordered.unwrap_or(false))
        }
        other => Field::new(column_name, other.clone(), array.null_count() > 0),
    }
}
```

### Fix for Pitfall 4 (`to_pandas` categorical reconstruction) — verified Python behavior, Rust call-site sketch

```rust
// Source: verified Python-level behavior in this research session; Rust call-site
// illustrates building the equivalent types_mapper callable via PyO3.
// crates/flint-python/src/table.rs::to_pandas, replacing the current unconditional
// `kwargs.set_item("types_mapper", arrow_dtype)?;`
let pa_types = py.import("pyarrow")?.getattr("types")?;
let types_mapper = pyo3::types::PyCFunction::new_closure(
    py,
    None,
    None,
    move |args: &Bound<'_, pyo3::types::PyTuple>, _kwargs| -> PyResult<PyObject> {
        let arrow_type = args.get_item(0)?;
        let is_dictionary: bool = pa_types.call_method1("is_dictionary", (&arrow_type,))?.extract()?;
        if is_dictionary {
            Ok(py.None())
        } else {
            Ok(arrow_dtype.call1((arrow_type,))?.unbind())
        }
    },
)?;
kwargs.set_item("types_mapper", types_mapper)?;
```

### D-11 object-column validation — Python-level shape (illustrative)

```python
# Source: illustrates the validation Flint must perform explicitly (Pitfall 2) --
# NOT sufficient to rely on pyarrow's own conversion errors for this.
for i, value in enumerate(series):
    if value is None or (isinstance(value, float) and value != value):  # NaN
        continue
    if not isinstance(value, str):
        raise FlintUnsupportedColumnError(
            column=column_name,
            dtype="object",
            reason=f"non-string value of type {type(value).__name__!r} found at row {i}",
        )
```

### Resolution-unit check for D-15 (both plain numpy and tz-aware paths — verified)

```python
# Source: verified against pandas 3.0.3 / numpy in this research session.
import numpy as np
import pandas as pd

def datetime_unit(dtype) -> str:
    if isinstance(dtype, pd.DatetimeTZDtype):
        return dtype.unit  # tz-aware: pandas ExtensionDtype exposes `.unit` directly
    # plain numpy datetime64[X]/timedelta64[X]: use numpy's own introspection,
    # NOT string-parsing str(dtype) -- np.datetime_data is the documented API.
    unit, _count = np.datetime_data(dtype)
    return unit
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|---------------|--------------------|----------------|--------|
| `pd.to_datetime()`/`pd.to_timedelta()` default to nanosecond resolution | Default resolution is now microseconds, falling back to nanoseconds only when the input string's precision requires it | pandas 3.0.0 (2026-01-21, per pandas' own whatsnew) | Directly collides with D-15's ns-only scope — the "normal" way to create a datetime/timedelta column in pandas 3.0 will now be rejected by Flint unless explicitly cast to `datetime64[ns]`. Must be a documented, expected rejection with an actionable error message (Pitfall 5), not treated as a surprising edge case. |
| `pd.Table.to_pandas(zero_copy_only=True)` as the sole way to request a zero-copy guarantee (pyarrow's own flag, documented as unreliable) | This project's own `strict=True` + `copy_report()` diagnostics (D-03/D-04, already implemented Phase 1) | Established during Phase 1, unaffected by Phase 2 | Phase 2 must extend, not compromise, this existing per-column diagnostics honesty — D-13/D-14's chunk-count-awareness fix is a continuation of this same commitment, not a new one. |

**Deprecated/outdated:**
- Relying on `dtype.kind` alone for pandas dtype classification once ArrowDtype/CategoricalDtype/masked-extension dtypes are in scope — verified this phase to be structurally ambiguous (see Pitfall 1, Architecture Patterns).

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|-----------------|
| A1 | `arrow::compute::concat` (arrow-rs 59.1.0) generically supports `DictionaryArray`, `TimestampArray` (with tz), and `DurationArray` without special-casing, consistent with its type-generic design over the `Array` trait | Don't Hand-Roll (multi-chunk merging), Common Pitfalls (D-12/D-13) | If `concat` panics or errors on one of these array types, the existing D-12 multi-chunk fallback (already used for Phase 1's numeric CR-01 fix) would need type-specific handling for the new dtypes — should be confirmed with a direct unit test early in Plan 01 rather than assumed correct from general Arrow-format knowledge. Confidence: MEDIUM (not independently reproduced in this research session against arrow-rs source/docs; general Arrow-ecosystem design knowledge only). |
| A2 | Requesting `pa.types.is_large_string`/an umbrella string-type check will be needed alongside `pa.types.is_string` since `large_string[pyarrow]` reports `is_string() == False` | Architecture Patterns (isinstance-first classification example) | If D-10's `string[pyarrow]` scope is interpreted to include `large_string[pyarrow]` too, the classification example's `is_string(pa_t) or is_large_string(pa_t)` check must actually be implemented (not just `is_string`) — confirmed via direct empirical test that `pa.types.is_string(pa.large_string()) == False`, so this is HIGH confidence as a fact, but the *scope* question ("should large_string be accepted") is Claude's Discretion, not locked by CONTEXT.md. |

**All other claims in this research were either empirically reproduced against this repo's pinned pandas 3.0.3/pyarrow 25.0.0/arrow 59.1.0 stack in this session, or cited directly from official arrow-rs/arrow-schema docs.rs pages and pandas' own release notes** — no other claim in this document should be treated as unverified training-data recall.

## Open Questions (RESOLVED)

1. **Should `to_pandas`'s diagnostics/strict-mode surface a copy signal for the dictionary-column reconstruction path (Pitfall 4's trade-off)?**
   - What we know: `to_pandas` currently treats `strict`/copy-diagnostics as a universal no-op because every output column is "already Arrow memory" (Phase 1's documented reasoning). The verified fix for D-17 (per-column `types_mapper` returning `None` for dictionary columns) causes pyarrow's own default reconstruction to run for that column, which does NOT appear to be zero-copy for the codes buffer.
   - What's unclear: whether D-17/D-18's fidelity requirement is understood by the user to implicitly also require this to be flagged in `copy_report()`, or whether "categorical values/order/width fidelity" and "copy-vs-zero-copy diagnostics" are understood as orthogonal concerns for the `to_pandas` direction specifically (CONTEXT.md's decisions are silent on this specific interaction).
   - Recommendation: raise explicitly with the user during planning/discuss-phase if not already resolved; in the absence of an explicit answer, the conservative default is to leave `to_pandas`'s `strict` parameter as documented-no-op (matching existing code comments) but ensure the categorical-reconstruction copy is at least mentioned in the PLAN.md's own test/verification notes so it isn't rediscovered as a surprise during Phase 2 verification the way DIAG-01/02 was during Phase 1's.

2. **Does `DtypeBackend::Categorical` need its own `plan_column` matrix entry, or can plain `Categorical` columns be treated identically to the existing `RequiresCopy` fallback path used for numpy-bool/non-contiguous-numeric?**
   - What we know: plain pandas `Categorical` is never `ArrowDtype`-backed (verified: `isinstance(pd.CategoricalDtype(...), pd.ArrowDtype)` is always `False`) and is never zero-copy-eligible at the numpy-buffer level (its codes+categories split has no single flat buffer matching Arrow's dictionary layout) — so its `plan_column` outcome will always be `RequiresCopy` in practice.
   - What's unclear: whether representing it as a distinct `ArrowKind::Categorical` (as sketched in Architecture Patterns) versus simply routing it through the existing generic `RequiresCopy` fallback with a categorical-specific `reason` string is the cleaner implementation — this is explicitly called out in CONTEXT.md's "Claude's Discretion" section, so the planner has latitude here.
   - Recommendation: either is compatible with D-17/D-18 as long as the `reason` string in `ColumnConversionRecord` is honest and specific (e.g. "categorical dtype has no zero-copy-eligible Arrow physical layout") — pick based on how much the planner wants `plan_column`'s pure-Rust unit tests to exercise this case directly versus leaving it as an integration-level concern.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|--------------|-----------|---------|-----------|
| Rust toolchain (`cargo`/`rustc`) | All new `flint-core`/`flint-python` code | Yes | 1.97.0 | — |
| `uv` | Dev workflow (`uv sync --dev`, `uv run maturin develop`) | Yes | 0.11.25 | — |
| `maturin` | Building the PyO3 extension | Yes | 1.14.1 (via `uv run maturin --version`) | — |
| pandas | Round-trip target for all Phase 2 dtypes | Yes | 3.0.3 (matches pinned dev-dependency) | — |
| pyarrow | Comparison/dev-test target only | Yes | 25.0.0 (matches pinned dev-dependency) | — |

**Missing dependencies with no fallback:** None — this repo's local environment already has everything needed to implement and test Phase 2 directly (confirmed by running the empirical checks in this research session against the actual installed toolchain, not assumed).

## Security Domain

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|-----------------|---------|--------------------|
| V2 Authentication | No | This is a native-extension data-interop library, not a service with authenticated users. |
| V3 Session Management | No | Same as above — no session concept exists in this domain. |
| V4 Access Control | No | Same as above. |
| V5 Input Validation | Yes | (1) Object-dtype column content validation (D-11) — every non-null value must be validated as `str` before being trusted for conversion, per Pitfall 2's findings; use explicit Rust/PyO3-side type checks, never best-effort coercion. (2) Datetime/timedelta resolution validation (D-15) — reject non-ns resolutions explicitly rather than truncating/reinterpreting. (3) Existing Phase 1 untrusted-PyCapsule validation pattern (T-01-03/T-01-04, `crates/flint-python/src/import.rs`) is unchanged by this phase but sets the precedent: never dereference a buffer/pointer derived from a Python-side value (dtype metadata, category list, tz string) without first validating it came from an expected, well-formed structure. |
| V6 Cryptography | No | No cryptographic operations in this domain. |

### Known Threat Patterns for this stack

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|------------------------|
| Malformed/adversarial object-dtype column silently coerced into an unintended Arrow type (e.g. a dict-valued column silently becoming a nested `struct`, verified in Pitfall 2) | Tampering (data silently reinterpreted in a way the caller didn't intend, undermining the "honest conversion" contract) | Explicit Flint-owned content validation (D-11) before any conversion is attempted, rejecting anything that isn't a `str`/`None`, rather than trusting pyarrow's permissive type inference. |
| Untrusted tz string passed through to arrow-rs without Flint attempting its own tz validation | Tampering / Denial of Service (if arrow-rs mishandles a malformed or absurdly large tz string) | Per D-16 (already locked): Flint does not hand-roll tz validation — it trusts pandas'/arrow-rs's own rejection behavior and surfaces arrow-rs's own error, rather than attempting a parallel, possibly-incomplete validation layer that could itself be a source of bugs. This is the correct mitigation per the project's existing "don't hand-roll" philosophy — no new Flint-side tz-parsing code should be introduced. |
| Category list with pathologically many entries or extremely long string categories driving unexpected memory use during the dictionary FFI import | Denial of Service | Out of this phase's explicit locked scope (no size/memory-limit requirement in CONTEXT.md's decisions) — recommend the planner add this as a documented known-limitation rather than new validation code, consistent with the project's "the concat/copy fallback path should be tested but its memory scaling is not itself a Phase 2 correctness requirement" framing already established for the multi-chunk copy path (D-12). |

## Project Constraints (from CLAUDE.md)

Actionable directives from `.claude/CLAUDE.md` that this phase's plan must comply with:

- **Language/format non-negotiable:** Rust core with PyO3 bindings; Arrow columnar memory format only (not a custom layout) — Phase 2 adds no new language/format surface, only extends existing `arrow-rs` type usage (`DataType::Dictionary`, `Timestamp`, `Duration`), consistent with this constraint.
- **Scope discipline:** v1 is bridge + Parquet IO only — Phase 2 stays within "bridge" scope (dtype/structural coverage of the pandas<->Arrow conversion); no compute engine, no distributed execution, no new language bindings are introduced by anything recommended in this research.
- **Tooling must remain `uv`-compatible:** confirmed no new dependencies are introduced (Package Legitimacy Audit); existing `uv sync --dev && uv run maturin develop && uv run pytest` workflow is unaffected.
- **Don't hand-roll FFI/marshalling; use PyO3-arrow's abstractions where they fit:** directly reinforced by this research's central finding — every new dtype's actual conversion mechanics should continue routing through the existing generic `__arrow_c_stream__`/`pyo3_arrow::PyTable::from_arrow_pycapsule` pattern, not new hand-written marshalling (see Don't Hand-Roll section).
- **Never treat all pandas DataFrames as zero-copy convertible; detect dtype backend and honestly label zero-copy vs. minimal-copy:** directly extended by D-07/D-09/D-10/D-12's decisions and this research's `ColumnConversionRecord`/`plan_column` chunk-awareness findings (Pitfall 6) — no recommendation in this research relabels a copying path as zero-copy.
- **pyarrow/polars/duckdb remain dev/test-only, never a runtime dependency:** unaffected — all Phase 2 work continues to rely on pyarrow only as the mechanism pandas' own `__arrow_c_stream__` export uses internally (not a Flint runtime import), consistent with the existing architecture.
- **Build `abi3` wheels; avoid raw `pyo3-ffi`:** unaffected by this phase's dtype-coverage scope; no packaging changes recommended here.

## Sources

### Primary (HIGH confidence — empirically verified in this research session)
- **Direct execution against this repo's own compiled `flint` extension** (`uv run maturin develop`, then `flint.Table.from_pandas`/`.to_pandas` via Python, with a temporary, fully-reverted `classify_dtype` patch used solely to route new dtypes through the existing generic fallback path — see Pitfall 0): confirmed masked-`Int64` raw `AttributeError`, confirmed categorical value/code-width round-trip including the >255-category int16 case, confirmed the `ordered` flag is lost specifically by `from_pandas`'s Field construction (isolated via a direct PyCapsule export with no `to_pandas` involved), confirmed the object dict-values silent-success gap, confirmed string/object/datetime/tz/timedelta round-trip correctness. Spike reverted via `git checkout`, repo confirmed clean, extension rebuilt to its committed state, full existing Python test suite re-confirmed green (29/29) afterward — no production code was altered by this research.
- Direct execution against this repo's pinned environment (`uv run python`, pandas 3.0.3, pyarrow 25.0.0, numpy 2.5.1) — dtype.kind ambiguity (object/string[pyarrow]/category all report 'O'; masked Int64/ArrowDtype int64 both report 'i'), masked-extension-dtype `.values.flags` AttributeError, object-column content validation gaps (dict-valued/int-valued/mixed-type-both-orderings), categorical round-trip via `__arrow_c_stream__` (order/ordered/code-width preserved), `to_pandas(types_mapper=...)` per-column-callable behavior fixing the categorical-reconstruction gap, datetime/tz/timedelta `__arrow_c_stream__` round-trip, pandas 3.0 default resolution change, `np.datetime_data`/`DatetimeTZDtype.unit` resolution introspection.
- Direct reads of this repo's own source: `crates/flint-core/src/pandas_plan.rs`, `crates/flint-python/src/pandas.rs`, `crates/flint-python/src/diagnostics.rs`, `crates/flint-python/src/table.rs`, `crates/flint-python/src/error.rs`, `Cargo.toml`, `pyproject.toml`.
- `.planning/phases/01-core-zero-copy-round-trip-interop/01-VERIFICATION.md` — DIAG-01/02 root-cause analysis (chunk-count-unaware `plan_column`), directly informing Pitfall 6/D-13.
- pandas official "What's new in 3.0.0" whatsnew documentation (via web search, cross-corroborated against this session's own empirical reproduction of the resolution-default change) — [pandas.pydata.org/docs/whatsnew/v3.0.0.html](https://pandas.pydata.org/docs/whatsnew/v3.0.0.html)
- [`docs.rs/arrow-schema/59.1.0/arrow_schema/struct.Field.html`](https://docs.rs/arrow-schema/59.1.0/arrow_schema/struct.Field.html) — `Field::dict_is_ordered`/`with_dict_is_ordered`/`new_dictionary` signatures (official crate docs, version-pinned to this project's exact dependency).
- [apache/arrow issue #35259](https://github.com/apache/arrow/issues/35259) — confirms plain (non-ArrowDtype) pandas `Categorical` is the reliable conversion path for dictionary-encoded Arrow columns, consistent with this phase's own empirical findings and CONTEXT.md's D-17 phrasing ("must yield a `Categorical`").

### Secondary (MEDIUM confidence — web search cross-checked against official docs, not independently reproduced against arrow-rs source in this session)
- Arrow C Data Interface `ARROW_FLAG_DICTIONARY_ORDERED` flag mechanism and arrow-rs's `TryFrom<&Field> for FFI_ArrowSchema`/reverse conversions — [docs.rs/arrow-schema FFI module](https://docs.rs/arrow-schema/latest/arrow_schema/ffi/struct.FFI_ArrowSchema.html), [Apache Arrow C Data Interface spec](https://arrow.apache.org/docs/format/CDataInterface.html)
- pyo3-arrow's documented rationale for storing `FieldRef` alongside `ArrayRef` (extension-type/Field-metadata preservation) — cross-referenced against this session's own Pitfall 3 finding, which independently arrives at the same class of gap in Flint's own code.
- `arrow::compute::concat`'s general type-genericity (Assumption A1) — general Arrow-ecosystem design knowledge, not independently confirmed against arrow-rs 59.1.0 source in this session; flagged in Assumptions Log for planner follow-up.

### Tertiary (LOW confidence)
- None — every claim in this document is either empirically reproduced (Primary) or sourced from an official docs page/release notes (Secondary), with any remaining uncertainty explicitly logged in the Assumptions Log above rather than left implicit.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — no new dependencies; existing pinned versions re-confirmed directly against this repo's `Cargo.toml`/`pyproject.toml` and installed environment.
- Architecture: HIGH — the core claim ("existing generic FFI path already handles every new dtype") and all four identified gaps (Pitfalls 1-4) were reproduced twice: once against pandas/pyarrow's own consumer, and once directly against this repo's actual compiled `flint` extension via a temporary, reverted `classify_dtype` patch (Pitfall 0) — not inferred from documentation or pyarrow-only behavior alone.
- Pitfalls: HIGH for Pitfalls 0, 1, 2, 3, 4, 5 (all directly reproduced with runnable scripts in this session; Pitfalls 1, 2, and 3's root cause were additionally confirmed against the compiled extension itself via the Pitfall 0 spike, not just pyarrow's own consumer); MEDIUM for Pitfall 6 (implementation-strategy trade-off between two valid designs, not a single verifiable fact) and Assumption A1 (`arrow::compute::concat` generic support for Dictionary/Timestamp/Duration — not exercised in the compiled-extension spike, which only tested multi-chunk numeric data; still recommend an early unit test per the Assumptions Log).

**Research date:** 2026-07-16
**Valid until:** 30 days (pandas/pyarrow/arrow-rs are all still under active development; re-verify dtype-kind/resolution-default behavior if either pandas or pyarrow's pinned version changes before Phase 2 is planned/executed)

---
*Phase: 2-full-dtype-structural-coverage*
*Research completed: 2026-07-16*
