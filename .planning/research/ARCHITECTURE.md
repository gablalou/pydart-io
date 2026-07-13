# Architecture Research

**Domain:** Rust-backed Python library for zero-copy(ish) pandas <-> Arrow conversion + Parquet IO
**Researched:** 2026-07-13
**Confidence:** MEDIUM (official docs for the FFI/PyCapsule mechanics are HIGH-trust primary sources; project-structure conclusions are synthesized from public repo structure via web search, not independently re-verified against every source line)

## The Load-Bearing Fact First

There are **two boundaries** in this system, and they behave completely differently. Conflating them is the single biggest way this project's roadmap could go wrong:

1. **Arrow (Rust) <-> Arrow (Python/pyarrow/polars/duckdb)** — via the Arrow C Data Interface / PyCapsule protocol. This boundary is **cleanly, reliably zero-copy**: it's pointers + a release callback, full stop.
2. **pandas DataFrame <-> Arrow Table** — this is **copy-sometimes, not zero-copy**. Zero-copy only holds for:
   - Numeric/timestamp dtypes, **no nulls**, single contiguous NumPy block (buffer-protocol view), OR
   - Arrow-backed pandas dtypes (`pd.ArrowDtype`, e.g. `"int64[pyarrow]"`) where pandas' block manager already stores an Arrow buffer internally.

   Everything else — object-dtype strings, booleans (NumPy packs 1 byte/bool, Arrow packs 1 bit/bool), any nulls in a numeric column, multi-chunk ChunkedArrays (pandas requires contiguity) — **requires a copy or a transform**, not a reinterpret-cast. `pyarrow.Array.to_numpy(zero_copy_only=True)` exists specifically to raise loudly when this happens rather than copy silently.

This project's Core Value claim ("zero-copy or as close as physically possible... must be provably faster") is only honest if the architecture treats boundary 2 as a **distinct, harder component** from boundary 1 — not as "the same binding logic, just facing a different Python object." PROJECT.md already hedges this correctly ("or minimal-copy"); the architecture should make that hedge structural, not incidental.

## Standard Architecture

### System Overview

```
┌──────────────────────────────────────────────────────────────────────┐
│                     Python-facing public API surface                  │
│   flint.Table, flint.from_pandas(df), flint.read_parquet(path), ...   │
├──────────────────────────────────────────────────────────────────────┤
│                    PyO3 binding layer (thin, Rust)                    │
│  ┌───────────────┐ ┌───────────────┐ ┌───────────────┐               │
│  │ PyArray/PyTable│ │ PandasInterop │ │ ParquetIO glue │               │
│  │ (FromPyObject, │ │ (block-manager│ │ (pyfunctions   │               │
│  │  IntoPy,       │ │  walk, dtype  │ │  wrapping      │               │
│  │  __arrow_c_*__)│ │  decisions)   │ │  arrow-rs      │               │
│  │                │ │               │ │  parquet crate)│               │
│  └───────┬────────┘ └───────┬───────┘ └───────┬────────┘               │
├──────────┴──────────────────┴─────────────────┴───────────────────────┤
│                     Rust core (Arrow memory + Parquet)                 │
│  ┌─────────────────────────────────────────────────────────────────┐  │
│  │  arrow-rs: Arc<dyn Array>, RecordBatch, Schema/Field, buffers    │  │
│  │  arrow-rs `parquet` crate: reader/writer, encoding, compression  │  │
│  └─────────────────────────────────────────────────────────────────┘  │
├──────────────────────────────────────────────────────────────────────┤
│         External boundary: Arrow C Data Interface / PyCapsule          │
│   __arrow_c_array__ / __arrow_c_stream__ / __arrow_c_schema__          │
│   ←→ pyarrow, polars, duckdb, nanoarrow (zero-copy, no pyarrow dep)    │
├──────────────────────────────────────────────────────────────────────┤
│         External boundary: pandas buffer protocol / block manager      │
│   NumPy arrays (buffer protocol) ←→ pandas BlockManager               │
│   (copy-sometimes: depends on dtype, nulls, contiguity — see above)    │
└──────────────────────────────────────────────────────────────────────┘
```

### Component Responsibilities

| Component | Responsibility | Typical Implementation |
|-----------|----------------|-------------------------|
| Rust core (Arrow) | Owns Arrow memory format: arrays, buffers, validity bitmaps, schema/field/datatype, RecordBatch, chunked Table | `arrow-rs` crate (`Arc<dyn Array>`, `ArrayData`, `Schema`, `Field`) — do not reimplement Arrow itself |
| Rust core (Parquet) | Read/write Parquet files against the same in-memory Arrow representation | `arrow-rs`'s `parquet` crate (`ArrowWriter`, `ParquetRecordBatchReader`) — sibling to the Arrow core, not layered through pandas |
| PyO3 binding layer — Arrow wrappers | Wrap Rust Arrow types in `#[pyclass]` structs; implement `FromPyObject`/`IntoPy`; implement the PyCapsule dunder methods so any Arrow-aware Python library can consume/produce this library's objects with zero copy | Follow `pyo3-arrow`'s pattern: store `Arc<dyn Array>` + `FieldRef` together per wrapper struct (preserves metadata/extension types across FFI) |
| PyO3 binding layer — pandas interop | Own the copy-vs-zero-copy *decision* per column: walk pandas' BlockManager, map dtype -> Arrow type, decide buffer-protocol view vs. materialize-and-copy, handle null bitmap construction | This is new code, not something you get for free from `pyo3-arrow` or arrow-rs — it's the hardest, most bespoke part of this project |
| PyO3 binding layer — Parquet glue | Thin `#[pyfunction]`s exposing read/write, wrapping arrow-rs parquet reader/writer, returning this library's Table type | Mirrors how arrow-rs exposes `PyArrowType<T>`-wrapped functions |
| Python-facing public API | Ergonomic surface: `Table`, `from_pandas`, `to_pandas`, `read_parquet`, `write_parquet`; hides PyO3/Rust details; matches enough of pyarrow's shape that it's a drop-in-ish alternative | Thin Python module (or pure `#[pymodule]` exposure with docstrings) — avoid putting logic here |

## Recommended Project Structure

```
flint/
├── Cargo.toml                  # workspace root (or single crate for v1 — see rationale)
├── crates/
│   ├── flint-core/             # pure Rust: Arrow array/table/schema wrappers, Parquet IO
│   │   └── src/
│   │       ├── array.rs        # thin wrappers over arrow-rs Array types if needed
│   │       ├── table.rs        # RecordBatch/chunked-table representation
│   │       └── parquet.rs      # read/write against arrow-rs `parquet` crate
│   └── flint-python/           # PyO3 binding crate (the only crate depending on pyo3)
│       └── src/
│           ├── array.rs        # PyArray: Arc<dyn Array> + FieldRef, __arrow_c_array__
│           ├── table.rs        # PyTable: __arrow_c_stream__, chunked table
│           ├── schema.rs       # PySchema/PyField: __arrow_c_schema__
│           ├── pandas.rs       # pandas <-> Arrow conversion decision logic (the hard part)
│           ├── parquet.rs      # #[pyfunction] read_parquet/write_parquet
│           └── lib.rs          # #[pymodule] entry point
├── python/
│   └── flint/
│       ├── __init__.py         # thin re-export / ergonomic wrappers over the compiled extension
│       └── py.typed
├── pyproject.toml              # maturin build backend
└── tests/
    ├── rust/                   # unit tests in flint-core (no Python needed)
    └── python/                 # round-trip tests incl. zero-copy assertions (no-alloc / pointer identity)
```

### Structure Rationale

- **Two crates, not twenty:** Polars' 20+ crate workspace with multiple CPU-feature "runtime" variants is scale-appropriate for a full query engine, not for this project. For a v1 interop+Parquet library, follow **arro3's** shape instead: a small Rust core crate plus a thin PyO3 binding crate. A single crate with a `python` feature flag is also acceptable for v1 if the pure-Rust core has no other consumers yet — don't over-engineer the workspace before there's a second consumer of `flint-core`.
- **`flint-core` has zero PyO3 dependency:** keeps the Arrow/Parquet logic testable in pure Rust (`cargo test`, no Python interpreter needed) and keeps the door open for future non-Python bindings even though that's out of scope for v1.
- **`pandas.rs` is its own module, not folded into `array.rs`:** the pandas boundary has fundamentally different rules (copy-sometimes) from the Arrow-to-Arrow boundary (always zero-copy). Mixing them in one file invites accidentally treating pandas conversions as unconditionally cheap.
- **`maturin` as build backend:** the de facto standard for PyO3 extension packaging; both pyo3-arrow/arro3 and polars use it.

## Architectural Patterns

### Pattern 1: Arrow PyCapsule Interface for the Arrow<->Arrow boundary

**What:** Implement `__arrow_c_schema__`, `__arrow_c_array__`, `__arrow_c_stream__` on your Python-facing wrapper classes. These return `PyCapsule` objects wrapping C Data Interface structs (`ArrowSchema`, `ArrowArray`, `ArrowArrayStream`), with a capsule destructor that calls the struct's `release` callback if not already null.
**When to use:** Always, for the Rust-Arrow <-> Python-Arrow-ecosystem boundary (pyarrow, polars, duckdb, nanoarrow). This is the modern, dependency-free standard (formalized ~2023-2024) and should be the **primary** mechanism — not the older `pyarrow`-specific `_export_to_c`/`_import_from_c` C ABI, which requires pyarrow to be installed as a hard dependency. Keep the arrow-rs `pyarrow` cargo feature (`FromPyArrow`/`IntoPyArrow`/`PyArrowType`) as a documented *optional* legacy interop path for consumers who are still pyarrow-only, not as the primary design.
**Trade-offs:** Zero implementation cost for consumers (they just call `pa.array(your_obj)` or equivalent and it works via protocol detection) but you own correct release-callback semantics — get this wrong and you leak or double-free.

**Example (Rust, via `pyo3-arrow`-style wrapper):**
```rust
#[pyclass]
struct PyArray {
    array: Arc<dyn Array>,
    field: FieldRef,
}

#[pymethods]
impl PyArray {
    fn __arrow_c_array__(&self, py: Python, requested_schema: Option<PyObject>)
        -> PyResult<(PyObject, PyObject)> {
        // Export self.array/self.field as FFI_ArrowArray + FFI_ArrowSchema,
        // wrap each in a PyCapsule with the correct capsule name and a
        // destructor that calls the release callback if non-null.
    }
}
```

### Pattern 2: Buffer-protocol input for NumPy, decision logic for pandas

**What:** For plain NumPy arrays, PyO3's buffer-protocol support (as `pyo3-arrow` does) gives zero-copy import "for free" for contiguous numeric buffers with no nulls — a NumPy array literally is a flat buffer, so wrapping it as an Arrow array of the same primitive type is a pointer capture, not a copy. For pandas DataFrames, you must walk the **BlockManager** column by column, and for *each column* pick one of: (a) reinterpret the underlying NumPy block as a zero-copy Arrow array, (b) construct a validity bitmap alongside a zero-copy data buffer (nulls present, still no data copy), or (c) fully materialize/copy (object-dtype strings, mixed types, non-contiguous blocks).
**When to use:** (a)/(b) whenever dtype + contiguity allow it; (c) is unavoidable for `object` columns and should be treated as an explicit, benchmarked-and-documented fallback, not a silent behavior.
**Trade-offs:** This is where most of the engineering effort and most of the "provably faster than pyarrow" benchmark work will live — pyarrow already does the easy 90% (a); differentiation has to come from being leaner/faster on the same cases, not from claiming to solve the hard 10% for free.

### Pattern 3: PyArrowType-style newtype wrapper for Parquet functions

**What:** Wrap Rust-native return/argument types (e.g. `Arc<dyn Array>`, `RecordBatch`) in a single newtype (arrow-rs calls theirs `PyArrowType<T>`) that implements `FromPyObject`/`IntoPy` once, so every `#[pyfunction]` (like `read_parquet`, `write_parquet`) gets automatic conversion without repeating FFI glue per function.
**When to use:** Any `#[pyfunction]` boundary crossing between Python and the Rust core.
**Trade-offs:** Slight indirection, but avoids duplicating FFI code across every function signature.

## Data Flow

### Flow 1: Rust-built Arrow array -> Python (the clean path)

```
Rust: Arc<dyn Array> + FieldRef constructed in flint-core / flint-python
    ↓ (wrap in PyArray/PyTable, no copy)
PyO3 boundary: __arrow_c_array__ called by consumer (pyarrow.array(x), pl.from_arrow(x), etc.)
    ↓ (export FFI_ArrowArray + FFI_ArrowSchema into PyCapsules; capsule destructor
       holds the release callback so cleanup happens even if never consumed)
Consumer library imports via C Data Interface -> zero-copy Arrow object in pyarrow/polars/duckdb
```
Ownership: the exported `FFI_ArrowArray.release` callback must keep the backing `Arc<dyn Array>` alive until the consumer calls release. This is a plain refcount bump on export — no GIL subtlety here because no Python object is being borrowed, only the Rust allocation.

### Flow 2: pandas DataFrame -> flint Table (the boundary that needs care)

```
Python: df (pandas DataFrame, BlockManager owns NumPy blocks)
    ↓ from_pandas(df) called
PyO3 binding (pandas.rs): for each column —
    - numeric, no nulls, contiguous → borrow NumPy buffer via buffer protocol (zero-copy)
      the Arrow array's release/drop path now HOLDS A REFERENCE TO THE PYTHON OBJECT
      (the NumPy array / underlying pandas block) to keep it alive
    - numeric with nulls → allocate a validity bitmap (small copy), still borrow the data buffer
    - object dtype (strings, mixed) → materialize a genuine copy into a new Arrow buffer
    ↓
Rust core: assemble RecordBatch / Table from resulting per-column Arrow arrays
```
**Ownership hazard (the concrete use-after-free/double-free trap):** when a column is borrowed zero-copy from a NumPy/pandas buffer, the Arrow array's `release` callback ends up holding a reference to the *Python* object backing that buffer (to keep it alive as long as the Arrow array exists). That callback **must decrement the Python refcount under the GIL**. If this Rust-side drop happens on a background thread without holding the GIL (e.g. inside a `Drop` impl that fires during a non-Python-aware Rust computation, or via `py.allow_threads`), you get an unsound refcount operation — the concrete way this project could produce use-after-free or a segfault under load. Any zero-copy-from-pandas wrapper must guarantee its `Drop`/release path reacquires the GIL (`Python::with_gil`) before touching the borrowed Python object's refcount.

### Flow 3: flint Table -> pandas DataFrame (reverse of Flow 2)

```
Rust: Arc<dyn Array> data + validity bitmap
    ↓ to_pandas() called
PyO3 binding (pandas.rs): per column —
    - no nulls, primitive type → expose as NumPy array via buffer protocol referencing
      the same Rust-owned buffer (zero-copy view; Rust-owned memory must outlive the
      NumPy array — typically solved by wrapping the buffer's Arc in a PyCapsule that
      NumPy's array holds a reference to, so Rust memory isn't freed while NumPy still
      points at it)
    - nulls present → materialize (pandas' object/masked-array null representation
      doesn't map to Arrow's validity bitmap without transformation)
    ↓
pandas: assemble DataFrame from resulting per-column arrays via BlockManager
```

### Flow 4: Parquet IO (sibling to pandas interop, not layered through it)

```
Rust core: arrow-rs `parquet` crate reads/writes directly against RecordBatch/Table
    ↓ read_parquet(path) -> flint Table (Arrow-native, zero pandas involvement)
    ↓ optionally: Table.to_pandas() (Flow 3) if the user wants a DataFrame
```
Keeping Parquet IO independent of pandas means Parquet correctness/perf work is not gated on the harder pandas-interop work, and vice versa — they can be built and tested in parallel once the Arrow core exists.

## Scaling Considerations

| Scale | Architecture Adjustments |
|-------|---------------------------|
| Single array / small table (dev, unit tests) | Single-crate structure is fine; focus on correctness of the zero-copy paths |
| Wide tables, many columns, large Parquet files | Ensure Parquet reads support column projection / row-group filtering (arrow-rs supports this) so IO doesn't force full-file materialization |
| Very large in-memory tables | Chunked Table (multiple RecordBatches / `__arrow_c_stream__`) rather than one giant contiguous array — avoids one large reallocation and matches how Arrow ecosystems represent big data by convention |

### Scaling Priorities

1. **First bottleneck:** naive pandas conversion falling back to full copies for any column with nulls or object dtype — mitigate by making the copy-vs-zero-copy decision explicit and benchmarked per dtype, not assumed.
2. **Second bottleneck:** Parquet IO materializing entire files into memory — mitigate early by using arrow-rs's streaming/batched reader API rather than a "read whole file" convenience function only.

## Anti-Patterns

### Anti-Pattern 1: Treating "the binding layer" as one undifferentiated blob

**What people do:** Put pandas conversion, Arrow-to-Arrow FFI, and Parquet glue all in one `lib.rs` or one conceptual "PyO3 layer."
**Why it's wrong:** The Arrow<->Arrow boundary is a solved, mechanical, always-zero-copy problem (PyCapsule protocol). The pandas boundary is a genuinely hard, copy-sometimes engineering problem. Blurring them leads to either overclaiming zero-copy for pandas (breaks the benchmark story) or under-optimizing the Arrow-to-Arrow path by over-engineering it defensively.
**Do this instead:** Separate modules/crates as described above; test and benchmark them separately.

### Anti-Pattern 2: Copying the arrow-rs legacy `pyarrow` C-ABI pattern as the primary interop mechanism

**What people do:** Depend on `pyarrow`'s `_import_from_c`/`_export_to_c` methods directly (the older arrow-rs `pyarrow` feature) as the main way to talk to Python.
**Why it's wrong:** Forces a hard dependency on pyarrow being installed, which undercuts a "leaner alternative to pyarrow" positioning, and duplicates work the newer, dependency-free PyCapsule protocol already solves generically.
**Do this instead:** Implement the `__arrow_c_*__` dunder methods (PyCapsule protocol) as the primary interop surface; keep the pyarrow-specific path only as an optional compatibility shim if a consumer's version of pyarrow predates PyCapsule support.

### Anti-Pattern 3: Polars-scale workspace fragmentation for a v1 interop library

**What people do:** Pre-emptively split into a dozen crates and multiple compiled "runtime" variants (Polars' pattern) because "that's what the successful project does."
**Why it's wrong:** Polars' structure exists to serve a full query engine with many optional feature combinations and CPU-target variants; replicating it for a narrower interop+Parquet library adds build complexity with no corresponding payoff, and slows early iteration.
**Do this instead:** Start with `flint-core` + `flint-python` (or one crate with a feature flag). Split further only when a second consumer of the core actually exists.

## Integration Points

### External Services / Ecosystem

| Library | Integration Pattern | Notes |
|---------|----------------------|-------|
| pyarrow | Consumes/produces via `__arrow_c_array__`/`__arrow_c_stream__` (no import dependency); optional legacy `_import_from_c`/`_export_to_c` shim | Do not require pyarrow as a runtime dependency — that defeats "leaner than pyarrow" positioning |
| polars | Consumes/produces via PyCapsule protocol (`pl.from_arrow`, etc.) | polars is the reference implementation of "Rust + Python + Arrow, fast," but scoped as a query engine — study its Arrow-boundary code, not its query-engine code |
| duckdb | Consumes via PyCapsule protocol | Confirmed via arrow.apache.org discussion threads that duckdb moved toward PyCapsule support to drop its pyarrow dependency — validates this as the ecosystem-standard direction |
| nanoarrow | Reference/lightweight implementation of the C Data Interface + PyCapsule protocol in C, useful as a spec-compliance reference and for testing interop | Good source for edge-case testing (e.g. does your capsule destructor actually get called under GC pressure) |
| arrow-rs (`arrow`, `parquet` crates) | Direct dependency — this *is* the Rust core, not something to reimplement | Pin a specific arrow-rs version; track its release notes since C Data Interface / Parquet APIs still evolve |

### Internal Boundaries

| Boundary | Communication | Notes |
|----------|----------------|-------|
| `flint-core` (pure Rust) <-> `flint-python` (PyO3) | Direct Rust function calls / `Arc` sharing | No serialization; `flint-python` should be the only crate with a `pyo3` dependency |
| PyO3 Arrow wrappers <-> pandas-interop module | Both operate on the same `Arc<dyn Array>` types, but pandas-interop is the only place with block-manager/dtype-mapping logic | Keep pandas-interop callable independently (unit-testable without a real DataFrame if possible) so its copy/no-copy decisions can be tested in isolation |
| Parquet glue <-> Rust core | Direct calls into arrow-rs `parquet` crate against `RecordBatch`/Table | No dependency on the pandas-interop module — Parquet IO must work standalone |

## Suggested Build Order

This order is dependency-forced, not arbitrary — each stage below is a prerequisite for the next, and the roadmap should reflect that the Arrow core + stream support is a **serial bottleneck** that everything else depends on.

1. **Toolchain + single-array round trip.** Get maturin/PyO3 building, and implement one primitive type (e.g. `Int64Array`) round-tripping Rust -> Python -> Rust via the C Data Interface / PyCapsule protocol. Write a test that *proves* zero-copy (pointer identity across the boundary, or an allocation-counting test) — this becomes the pattern/template for everything else, and the proof-of-zero-copy test becomes the template for the eventual benchmark suite.
2. **Type coverage expansion.** Add validity bitmaps (nulls), variable-length types (string/binary), and bit-packed booleans. Each of these has different FFI/buffer shape than the plain-primitive case from step 1.
3. **Schema/Field/DataType wrappers.** Needed before RecordBatch/Table can express multi-column data with named, typed columns.
4. **RecordBatch -> chunked Table -> `__arrow_c_stream__`.** Multi-column, multi-batch representation; this is what most consumers (polars, duckdb) actually expect to receive, not a bare array.
5. **Two parallel tracks, once the Arrow core (steps 1-4) is stable:**
   - **(a) pandas-interop layer:** `from_pandas`/`to_pandas`, implementing the copy-vs-zero-copy decision tree per dtype described in Flow 2/3 above.
   - **(b) Parquet IO:** `read_parquet`/`write_parquet` against arrow-rs's `parquet` crate, independent of pandas.
   These can genuinely be built/tested by different phases or even in parallel because neither depends on the other — both depend only on the Arrow core.
6. **Benchmark suite vs. pyarrow.** Requires (5a) and (5b) to exist to be meaningful; this is also where the "provably faster" claim gets tested empirically, including whether the copy-fallback cases in pandas-interop are actually competitive or need further optimization.

## Sources

- [The Arrow PyCapsule Interface — Apache Arrow docs](https://arrow.apache.org/docs/format/CDataInterface/PyCapsuleInterface.html) — MEDIUM confidence (official spec doc, cross-checked)
- [arrow_pyarrow — Rust (arrow-rs docs)](https://arrow.apache.org/rust/arrow_pyarrow/index.html) — MEDIUM confidence (official crate docs, cross-checked)
- [pyo3-arrow — docs.rs](https://docs.rs/pyo3-arrow/latest/pyo3_arrow/) — MEDIUM confidence
- [arro3 (kylebarron) — GitHub](https://github.com/kylebarron/arro3) — MEDIUM confidence
- [Apache Arrow issue #39195 — Promote PyCapsule Protocol usage](https://github.com/apache/arrow/issues/39195) — MEDIUM confidence
- [duckdb discussion #10716 — PyCapsule Interface support / drop pyarrow dependency](https://github.com/duckdb/duckdb/discussions/10716) — MEDIUM confidence
- [Polars crate organization — DeepWiki](https://deepwiki.com/pola-rs/polars/1.2-crate-organization) — MEDIUM confidence (secondary source, not primary Polars docs; directionally consistent with public repo structure)
- [pyo3-polars — GitHub](https://github.com/pola-rs/pyo3-polars) — MEDIUM confidence
- [PyO3 Memory Management guide](https://pyo3.rs/v0.22.5/memory) — MEDIUM confidence (official docs)
- [Pandas Integration — Apache Arrow docs](https://arrow.apache.org/docs/python/pandas.html) — MEDIUM confidence (official docs)

---
*Architecture research for: Rust-backed Python Arrow interop + Parquet IO library*
*Researched: 2026-07-13*
