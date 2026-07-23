# Phase 3: Parquet IO - Research

**Researched:** 2026-07-23
**Domain:** Native Rust Parquet read/write (apache/arrow-rs `parquet` crate) bound into `flint.Table` via PyO3, with row-group statistics-driven predicate pushdown and full dtype-fidelity round-trip
**Confidence:** HIGH for the `parquet` crate's core write/read/filter API surface (docs.rs + crates.io, cross-checked against multiple pages) and for the WR-01 fix mechanism (direct codebase reading); MEDIUM for the exact row-group-pruning idiom (no single canonical non-DataFusion example found, synthesized from `StatisticsConverter` + `with_row_groups` docs) and for pandas/pyarrow historical-issue claims (WebSearch-sourced, not Context7/curated-doc backed in this environment).

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

**API Surface (PARQ-01/PARQ-02)**
- D-19: `Table.from_parquet(...)` (classmethod) / `table.to_parquet(...)` (instance method) — same `from_X`/`to_X` naming pattern as D-01/D-02. No module-level `flint.read_parquet`/`write_parquet`.
- D-20: `str`/`pathlib.Path` only — no file-like objects (`io.BytesIO`, open handles) in v1.
- D-21: `Table.from_parquet` also supports reading multiple files as one `Table` (list of paths or a directory path). Exact mechanism left to discretion (see below).
- D-22: `table.to_parquet` overwrites the target file silently if it exists — no `overwrite=True` guard flag.

**Predicate Pushdown & Projection API (PARQ-04/PARQ-05)**
- D-23: Read-side filtering uses pyarrow-style tuple filters: `filters=[("col", ">", 5), ("col2", "==", "x")]` — flat list of `(column, operator, value)` tuples. No general expression evaluator.
- D-24: Multiple filter conditions combine with AND only (no OR, no DNF/nested lists).
- D-25: Supported operators in v1: `==`, `!=`, `<`, `<=`, `>`, `>=`. No `in`/membership operator.
- D-26: "Predicate pushdown" means BOTH row-group-level skipping (via written row-group statistics, PARQ-04) AND exact row-level filtering of rows within surviving row groups (via arrow-rs's `RowFilter`/`ArrowPredicate`). The `Table` returned by `from_parquet(..., filters=...)` contains ONLY matching rows — no false positives requiring a second filter pass by the caller.
- D-27: Column projection (`columns=[...]`) and `filters` are independent, combinable parameters on `Table.from_parquet`.

**Compression & Row-Group Defaults (PARQ-02/PARQ-03)**
- D-28: Default compression codec when unspecified on `to_parquet` is **snappy** (matches pyarrow's default).
- D-29: Exactly four codecs supported: snappy, zstd, gzip, uncompressed. No lz4/brotli/lzo/lz4_raw even though arrow-rs's parquet crate supports them for free.
- D-30: Default row-group size is a **row-count threshold** (~1,048,576 rows/group, matching pyarrow's default), not byte-size. `to_parquet` accepts `row_group_size` (row-count) to override.

**WR-01 Nullability Fix (carried forward from 02-REVIEW.md, direct PARQ-06 dependency)**
- D-31: Fix WR-01 as part of Phase 3, not deferred. `build_field` in `crates/flint-python/src/pandas.rs` currently derives Arrow field nullability from the current batch's observed `null_count() > 0` rather than the source pandas dtype's declared nullability. A nullable `int64[pyarrow]` column with zero nulls round-trips as a `not null` Flint schema field, which threatens PARQ-06 schema fidelity and breaks `pyarrow.concat_tables`-style schema merges.

### Claude's Discretion
- Exact multi-file/directory read mechanism for D-21 (explicit `List[str|Path]` param vs directory auto-discovery vs both) and the schema-mismatch policy across files (strict error vs best-effort union).
- Exact fix mechanism for WR-01 (D-31) — whether the source dtype's declared nullability is threaded through at `classify_dtype`/`plan_column` time, or derived from a schema already available at a different pipeline point. **Research finding below identifies a concrete, low-risk mechanism — see "WR-01 Fix Mechanism" section.**
- Exact row-group statistics written (min/max only vs min/max + null-count + distinct-count) beyond what PARQ-04 requires — follow arrow-rs parquet crate defaults unless a concrete reason emerges not to.
- Whether categorical/dictionary columns get forced dictionary-encoding in the Parquet writer or rely on the writer's own heuristics.
- Internal implementation of row-level exact filtering (D-26) — e.g. building `ArrowPredicateFn` closures per operator from the fixed D-25 operator set.

### Deferred Ideas (OUT OF SCOPE)
- WR-02 (numpy Copy-on-Write zero-copy-borrow guarantee gap) — not this phase's domain (concerns the numpy buffer-borrow path from Phase 1, not Parquet IO).
- `in`/membership filter operator — deferred from v1's fixed operator set (D-25).
- OR / disjunctive-normal-form filter combination — deferred; D-24 locks AND-only for v1.
- File-like object / in-memory buffer support for Parquet read/write — deferred; D-20 locks path/str-only for v1.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| PARQ-01 | User can read a Parquet file into a Table | `ParquetRecordBatchReaderBuilder::try_new` + `.build()` + `RecordBatch` iteration (Standard Stack, Code Examples: Read) |
| PARQ-02 | User can write a Table to a Parquet file with a chosen compression codec | `ArrowWriter::try_new(writer, schema, Some(WriterProperties))` + `Compression` enum (Standard Stack, Code Examples: Write) |
| PARQ-03 | User can configure row-group size on write | `WriterPropertiesBuilder::set_max_row_group_size` (Standard Stack, Common Pitfalls: Row-Group Size API Naming) |
| PARQ-04 | Written Parquet files include row-group statistics enabling predicate pushdown on read | Written automatically by `ArrowWriter` (column-level min/max/null-count stats are on-by-default); read side uses `StatisticsConverter` + `ArrowReaderBuilder::with_row_groups` (Architecture Patterns: Pattern 2) |
| PARQ-05 | User can apply column projection and predicate pushdown when reading Parquet | `ProjectionMask::columns` + `RowFilter`/`ArrowPredicateFn` (Architecture Patterns: Pattern 3) |
| PARQ-06 | Parquet round-trip preserves logical types correctly (tz-aware timestamps, categorical/dictionary encoding) | Arrow schema embedding (`ARROW:schema` metadata key, on by default) + WR-01 fix (Common Pitfalls: Pitfall 1, Pitfall 2; WR-01 Fix Mechanism section) |
</phase_requirements>

## Summary

Flint already has everything a Parquet IO layer needs to compose against: a pyo3-free `flint-core` crate for logic that doesn't touch Python, a `flint-python` crate whose `Table` composes `pyo3_arrow::PyTable`, and an established "one error enum, one decision function, no re-derived logic in two places" discipline that this phase's plan should extend rather than reinvent. The `parquet` crate (apache/arrow-rs, pinned to the same `59.1.0` as `arrow` per this project's lockstep convention) is the correct, sole dependency to add — it is the same crate family already vendored, its `arrow` feature (default-on) gives direct `RecordBatch <-> Parquet` conversion, and its default feature set already includes snappy/gzip/zstd support (D-29's exact four codecs, with no extra Cargo features required beyond the plain `parquet = "59.1.0"` line).

Three API surfaces map directly onto the six locked requirements: `ArrowWriter` (write side: `WriterProperties::builder().set_compression(..).set_max_row_group_size(..).build()`, `write(&batch)`, `close()`), `ParquetRecordBatchReaderBuilder`/`ArrowReaderBuilder` (read side: `with_projection(ProjectionMask)`, `with_row_filter(RowFilter)`, `with_row_groups(Vec<usize>)`), and the crate's own row-group `Statistics`/`StatisticsConverter` machinery for the row-group-skip half of D-26's predicate pushdown. The most important non-obvious finding is that arrow-rs's `ArrowWriter` embeds the full Arrow schema into Parquet file metadata by default (the `ARROW:schema` key, via `ArrowWriterOptions` — disabled only by an explicit, unused-here `with_skip_arrow_metadata()`), and the reader consults this hint by default. This is the actual mechanism — not a Parquet-native logical type — by which PARQ-06's hardest cases (an exact IANA tz name like `"America/New_York"`, and a genuine `DataType::Dictionary` reconstruction rather than plain-value decoding) survive a round trip. Sources on this point were not fully consistent during research (one WebSearch summary claimed dictionary types are "NOT preserved by default on read," while a direct WebFetch of the crate's own module docs states the embedded schema hint IS consulted by default) — the direct-fetch, crate-authored source is judged more authoritative and is what this document follows, but given PARQ-06 is this phase's most consequential fidelity requirement, **this must be treated as a mandatory Wave-0 verification gate, not an assumption the plan can skip**: the first task touching Parquet read/write should include an explicit round-trip test asserting `DataType::Dictionary` (with `dict_is_ordered` intact) and an exact tz string survive write-then-read, before any dependent task is built on top of the assumption that this "just works." See Common Pitfalls 1-2 for the exact test shape.

The one genuine correctness bug this phase must fix (WR-01/D-31) is diagnosable from the existing codebase, and this research empirically verified the fix's mechanism and blast radius (not merely assumed it): `build_field` in `crates/flint-python/src/pandas.rs` derives nullability from `array.null_count() > 0` instead of from any declared source nullability. `import_column_via_pandas_stream` (the same function already used for every Arrow-backed and stream-fallback column) already has a correctly-declared nullability sitting unused in the schema it destructures from `PyTable::into_inner()`. An empirical check against the pinned pandas 3.0.3/pyarrow 25.0.0 (`pa.RecordBatchReader._import_from_c_capsule(df[[col]].__arrow_c_stream__()).schema.field(0).nullable`) confirms `[VERIFIED: empirical check against pinned pandas 3.0.3/pyarrow 25.0.0, this session]` that pyarrow's `__arrow_c_stream__` export declares **every** column's field as `nullable=True` — not only `ArrowDtype`-backed ones, but also a plain non-nullable numpy `int64` column with no nulls. This means sourcing nullability from the stream schema is **not a surgical, WR-01-only fix** — it broadens every stream-imported column (numpy bool, object/string, datetime/timedelta, non-contiguous numeric, categorical, and Arrow-backed columns alike) to `nullable=True` uniformly, in addition to fixing WR-01's specific ArrowDtype case. This broadening is in the safe/permissive direction (a nullable-but-actually-dense field is never a `pyarrow.concat_tables` compatibility hazard the way a wrongly-non-nullable field is — WR-01's actual failure mode), and no existing Phase 1-2 test asserts `nullable=False` on any field (confirmed via `grep -rn nullable tests/python/`), so this is a low-risk broadening, not a regression — but the planner should record it as an intentional, understood side effect of the chosen fix mechanism, not discover it as a surprise during verification. The direct numpy-buffer-borrow fast path (`borrow_numpy_numeric_column`, used only for contiguous numpy numeric `ZeroCopyBorrow` columns) does NOT go through `import_column_via_pandas_stream` at all and is unaffected — it should remain hard-coded `nullable=false`, which is correct since that dtype family cannot represent nulls.

**Primary recommendation:** Add `parquet = "59.1.0"` (default features, no extra flags) to `flint-core`'s `Cargo.toml`; implement `Table::from_parquet`/`to_parquet` as thin `#[pymethods]` in `flint-python/src/table.rs` delegating to new pyo3-free logic in `flint-core/src/parquet_io.rs`; fix WR-01 by threading the imported stream's `Field::is_nullable()` through `import_column_via_pandas_stream`'s return value into `build_field` for every stream-imported column (accepting the verified, safe broadening to uniform `nullable=True` for that code path), while leaving `borrow_numpy_numeric_column`'s hard-coded `nullable=false` unchanged.

## Project Constraints (from CLAUDE.md)

`./.claude/CLAUDE.md` exists and is authoritative for this project (same authority as a locked CONTEXT.md decision). Directives directly relevant to this phase's plan:

- **Rust + PyO3 core is non-negotiable.** Parquet IO must be implemented as Rust (`flint-core`/`flint-python`) with a PyO3 binding surface — never a Python-side Parquet implementation, never a shell-out to an external Parquet CLI tool.
- **Arrow columnar format only, not a custom layout.** The `parquet` crate's `arrow` feature (`RecordBatch <-> Parquet`) is the correct integration point — do not build a bespoke row-oriented or custom-columnar intermediate representation.
- **v1 scope is bridge + Parquet IO only.** No compute engine (this directly constrains D-23/D-24/D-25's fixed six-operator, AND-only, no-DNF filter design — do not build a general expression evaluator even if it seems easy), no distributed/out-of-core execution (`object_store`/remote Parquet reads are explicitly out of scope, matching D-20's local-filesystem-only decision), no other-language bindings.
- **`uv`-compatible tooling.** Any new Python-facing test/dev dependency must install and run under `uv add`/`uv pip install`; the existing `build_command`/`test_command` (`cargo build --workspace`; `cargo test --workspace && uv run maturin develop && uv run pytest tests/python -q`, per `.planning/config.json`) must keep working unchanged — this phase adds a Rust dependency only, no new Python dependency, so no `pyproject.toml` change is anticipated.
- **`arrow`/`parquet` lockstep version pinning.** `parquet` MUST be pinned to the exact same `59.1.0` as the already-pinned `arrow` crate (both released from the same `apache/arrow-rs` monorepo) — never use a caret/range version for either.
- **PyO3 high-level API only, never raw `pyo3-ffi`.** Any new `unsafe` code introduced for this phase (there should be very little — Parquet IO is almost entirely safe-Rust `parquet`/`arrow` crate calls) must stay within PyO3's `#[pyclass]`/`#[pyfunction]`/`Bound`/`Py<T>` safe wrapper model, consistent with Phases 1-2's existing `unsafe` boundary discipline (`borrow_numpy_numeric_column`, `NumpyBufferOwner`).
- **No manual filesystem sandboxing/access-control layer** — path validation is limited to type/existence/extension checks (D-20's `str`/`Path`-only scope), not a security boundary; matches the CLAUDE.md's overall "leaner, lower-level interop layer" positioning, not a full-featured IO framework.
- **GSD workflow enforcement.** Direct file edits outside a GSD command are disallowed per `./.claude/CLAUDE.md` — this phase's plan should be executed via `/gsd-execute-phase`, not ad hoc edits.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Parquet file read (bytes -> RecordBatch) | API / Backend (Rust core, `flint-core`) | — | Pure IO + Arrow decode logic, no Python object touched; belongs in the pyo3-free crate per the established `flint-core`/`flint-python` split |
| Parquet file write (RecordBatch -> bytes) | API / Backend (Rust core, `flint-core`) | — | Same rationale as read; `WriterProperties` construction from primitive `(codec, row_group_size)` args needs no `pyo3` types |
| Path/multi-file argument parsing (`str`/`Path`/`List`/directory) | Frontend Server equivalent (`flint-python` PyO3 boundary) | — | `pathlib.Path`/`str` extraction and directory globbing are inherently Python-object-facing; must live in `flint-python`, calling into `flint-core` once resolved to a `Vec<PathBuf>` |
| Filter tuple parsing (`[("col", ">", 5), ...]`) | Frontend Server equivalent (`flint-python` PyO3 boundary) | API / Backend | Parsing a Python list-of-tuples is PyO3-facing (`flint-python`); translating the parsed, typed representation into `RowFilter`/`ArrowPredicateFn` closures and doing row-group-statistics comparisons is pure Rust logic (`flint-core`), mirroring the existing `plan_column` single-decision-point pattern |
| Row-group statistics-based skip decision | API / Backend (`flint-core`) | — | `StatisticsConverter` + comparison against the parsed filter value is pure Rust/Arrow logic with no Python dependency |
| Error surfacing (unsupported operator, schema mismatch across files, codec string) | API / Backend boundary (`flint-python/src/error.rs`) | — | Must extend the existing single `FlintError` enum / single `impl From<FlintError> for PyErr`, per the established "no silent best-effort behavior, named specific errors" pattern |
| Schema/dtype fidelity (tz names, dictionary reconstruction) | Database / Storage (Parquet file + embedded Arrow schema metadata) | API / Backend | The fidelity mechanism (the `ARROW:schema` metadata key) lives in the file itself, written/read by the `parquet` crate's own default behavior — Flint's job is to not disable it, not to reimplement it |

## Standard Stack

### Core

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `parquet` (apache/arrow-rs) | 59.1.0 [VERIFIED: crates.io/docs.rs — confirmed released 2026-07-07, same monorepo/lockstep version as the already-pinned `arrow` 59.1.0] | Native Rust Parquet reader/writer with `RecordBatch <-> Parquet` conversion via its `arrow` feature | Already the recommended stack choice in `.planning/research/STACK.md`; same monorepo as `arrow`, so version compatibility is automatic; provides `ArrowWriter`, `ArrowReaderBuilder`/`ParquetRecordBatchReaderBuilder`, `RowFilter`/`ArrowPredicate`, and row-group `Statistics`/`StatisticsConverter` needed for every locked decision in this phase |

### Supporting

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| (none new) | — | — | No additional crates are required. `parquet`'s default feature set already includes `arrow` (the `RecordBatch` interop feature, default-on) plus `snap`/`flate2`/`zstd` (the compression codecs needed for D-29's exact snappy/gzip/zstd/uncompressed set) [CITED: docs.rs/crate/parquet feature list] — no extra `features = [...]` line is needed in `Cargo.toml` beyond the bare version pin. |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Manual row-group statistics comparison via `StatisticsConverter` | DataFusion's `PruningPredicate` (built on the same underlying parquet-crate statistics APIs) | DataFusion's pruning predicate is a much higher-level, expression-tree-based abstraction meant for a full query engine — pulling in `datafusion` as a dependency directly contradicts PROJECT.md's "no compute engine" scope boundary and would add a large, unnecessary dependency tree for six fixed comparison operators |
| `parquet`'s native `arrow` feature RecordBatch API | `polars`'/`duckdb`'s Parquet readers (via their Rust crates) | Both are full query-engine dependencies, same "no compute engine" scope violation as DataFusion above; `parquet` is the correct minimal-dependency choice |

**Installation:**
```bash
# crates/flint-core/Cargo.toml
# [dependencies]
# parquet = "59.1.0"
cargo add -p flint-core parquet@59.1.0
```

**Version verification:** `parquet` 59.1.0 confirmed live on crates.io/docs.rs, released 2026-07-07, same release lockstep as the already-pinned `arrow` 59.1.0 (both published from the `apache/arrow-rs` monorepo). [VERIFIED: docs.rs/crate/parquet/59.1.0]

## Package Legitimacy Audit

| Package | Registry | Age | Downloads | Source Repo | Verdict | Disposition |
|---------|----------|-----|-----------|--------------|---------|-------------|
| `parquet` | crates.io | ~8 years (published 2018-04-01) | ~1.01M/week | github.com/apache/arrow-rs | OK | Approved |

**Packages removed due to [SLOP] verdict:** none
**Packages flagged as suspicious [SUS]:** none

`parquet` was verified via the `package-legitimacy check` seam (`gsd-tools query package-legitimacy check --ecosystem crates parquet`), which returned `verdict: OK` with `publishedAt: 2018-04-01`, `weeklyDownloads: 1010018`, `repoUrl: github.com/apache/arrow-rs`. This is the same crate family (`apache/arrow-rs`) already a dependency of this project via `arrow`, discovered originally through the project's own `.planning/research/STACK.md` (itself sourced from a direct crates.io registry API fetch, not WebSearch) — tag: `[VERIFIED: crates.io registry + package-legitimacy seam]`. No install-time postinstall-script risk applies (Rust crates have no npm-style postinstall hooks).

## Architecture Patterns

### System Architecture Diagram

```
                    Python caller
                          |
          Table.from_parquet(path|paths|dir,           Table.to_parquet(path,
            columns=[...], filters=[...])                compression=.., row_group_size=..)
                          |                                          |
                          v                                          v
        +-----------------------------------+     +-----------------------------------+
        | flint-python/src/table.rs          |     | flint-python/src/table.rs          |
        | - resolve path/List[Path]/dir      |     | - str/Path -> PathBuf              |
        |   -> Vec<PathBuf>                  |     | - compression string -> Compression|
        | - parse filter tuples -> typed     |     |   enum (validate against D-29 set) |
        |   FilterExpr list (flint-core type)|     | - row_group_size -> WriterProperties|
        +-----------------------------------+     +-----------------------------------+
                          |                                          |
                          v                                          v
        +-----------------------------------+     +-----------------------------------+
        | flint-core/src/parquet_io.rs        |     | flint-core/src/parquet_io.rs        |
        | read_parquet(paths, projection,     |     | write_parquet(batch, path,          |
        |   filters) -> RecordBatch           |     |   compression, row_group_size)      |
        |                                     |     |                                     |
        | 1. Open file(s), read ParquetMetaData|    | 1. Build WriterProperties           |
        | 2. For each row group: StatisticsConv|    |    (compression, max_row_group_size)|
        |    -> min/max vs each filter's value |    | 2. ArrowWriter::try_new(file,        |
        |    -> skip row group if provably no  |    |    schema, Some(props))              |
        |    match (row-group-level pushdown)  |    | 3. writer.write(&batch) per input   |
        | 3. ArrowReaderBuilder                |    |    batch                             |
        |    .with_row_groups(surviving)       |    | 4. writer.close() -- flushes row     |
        |    .with_projection(ProjectionMask)  |    |    group stats + embeds Arrow schema |
        |    .with_row_filter(RowFilter of     |    |    metadata (ARROW:schema, default-on)|
        |    ArrowPredicateFn per D-25 operator)|    +-----------------------------------+
        | 4. Concatenate RecordBatches across  |                    |
        |    row groups / multiple files       |                    v
        +-----------------------------------+          Parquet file on local disk
                          |                        (row-group stats + embedded Arrow
                          v                          schema metadata written by default)
                  Arrow RecordBatch(es)
                          |
                          v
              flint.Table (wraps pyo3_arrow::PyTable,
               same construction path as from_pandas)
```

### Recommended Project Structure

```
crates/
├── flint-core/
│   └── src/
│       ├── parquet_io.rs      # NEW: pyo3-free read/write/filter/stats logic
│       ├── parquet_filter.rs  # NEW (or a module inside parquet_io.rs): typed
│       │                      #   FilterExpr { column, op, value } + row-group
│       │                      #   statistics comparison + ArrowPredicateFn builder
│       ├── pandas_plan.rs     # existing, unchanged
│       └── table.rs           # existing, unchanged
└── flint-python/
    └── src/
        ├── table.rs            # ADD: from_parquet/to_parquet #[pymethods],
        │                       #   delegating to flint_core::parquet_io
        ├── pandas.rs           # MODIFY: WR-01 fix in build_field /
        │                       #   import_column_via_pandas_stream
        └── error.rs            # ADD: FlintError variants for Parquet-specific
                                 #   failures (unsupported operator, unsupported
                                 #   codec string, cross-file schema mismatch)
```

### Pattern 1: Write path — `ArrowWriter` with explicit `WriterProperties`

**What:** Configure compression codec and row-group size via `WriterPropertiesBuilder`, then stream `RecordBatch`es through `ArrowWriter::write`, finishing with `close()`.
**When to use:** Every `table.to_parquet(...)` call (PARQ-02, PARQ-03).
**Example:**
```rust
// Source: https://docs.rs/parquet/latest/parquet/arrow/arrow_writer/struct.ArrowWriter.html
// Source: https://docs.rs/parquet/latest/parquet/file/properties/struct.WriterProperties.html
use parquet::arrow::ArrowWriter;
use parquet::file::properties::WriterProperties;
use parquet::basic::{Compression, ZstdLevel, GzipLevel};

fn build_writer_properties(codec: &str, row_group_size: usize) -> Result<WriterProperties, FlintError> {
    let compression = match codec {
        "snappy" => Compression::SNAPPY,
        "zstd" => Compression::ZSTD(ZstdLevel::default()), // parameterized variant -- see Pitfall 3
        "gzip" => Compression::GZIP(GzipLevel::default()),
        "uncompressed" => Compression::UNCOMPRESSED,
        other => return Err(FlintError::UnsupportedCodec(other.to_string())), // D-29: reject anything else
    };
    Ok(WriterProperties::builder()
        .set_compression(compression)
        .set_max_row_group_size(row_group_size) // D-30: row-count threshold, not bytes
        .build())
}

fn write_parquet(batch: &RecordBatch, path: &Path, props: WriterProperties) -> Result<(), FlintError> {
    let file = std::fs::File::create(path)?; // D-22: overwrites silently, matches std::fs::File::create semantics
    let mut writer = ArrowWriter::try_new(file, batch.schema(), Some(props))?;
    writer.write(batch)?;
    writer.close()?; // flushes final row group, writes footer + row-group statistics + embedded Arrow schema (default-on)
    Ok(())
}
```

### Pattern 2: Read path — row-group statistics pruning via `StatisticsConverter` + `with_row_groups`

**What:** Before decoding any row group's data pages, use the crate's `StatisticsConverter` to extract each row group's min/max for a filtered column, compare against the filter's literal value, and pass only the surviving row-group indices to `ArrowReaderBuilder::with_row_groups`.
**When to use:** PARQ-04's "written files enable predicate pushdown on read" and the row-group-skip half of D-26.
**Example:**
```rust
// Source: https://docs.rs/parquet/latest/parquet/arrow/arrow_reader/statistics/struct.StatisticsConverter.html
// Source: https://docs.rs/parquet/latest/parquet/arrow/arrow_reader/struct.ArrowReaderBuilder.html
use parquet::arrow::arrow_reader::{ArrowReaderBuilder, ParquetRecordBatchReaderBuilder, statistics::StatisticsConverter};
use parquet::file::reader::ChunkReader;

fn surviving_row_groups(
    metadata: &parquet::file::metadata::ParquetMetaData,
    arrow_schema: &Schema,
    parquet_schema: &parquet::schema::types::SchemaDescriptor,
    filters: &[FilterExpr], // this project's typed representation of D-23/D-25 filters
) -> Result<Vec<usize>, FlintError> {
    let mut keep: Vec<bool> = vec![true; metadata.num_row_groups()];
    for filter in filters {
        let converter = StatisticsConverter::try_new(&filter.column, arrow_schema, parquet_schema)?;
        let mins = converter.row_group_mins(metadata.row_groups().iter())?;
        let maxes = converter.row_group_maxes(metadata.row_groups().iter())?;
        for (i, k) in keep.iter_mut().enumerate() {
            // Compare filter.value against (mins[i], maxes[i]) per filter.op; if the
            // predicate CANNOT be true for any row in this row group's [min, max] range,
            // set *k = false. Conservative: on any doubt (e.g. null stats), keep the group.
            if !filter.could_match_range(mins.as_ref(), maxes.as_ref(), i) {
                *k = false;
            }
        }
    }
    Ok(keep.iter().enumerate().filter_map(|(i, k)| k.then_some(i)).collect())
}
```
This is the manual, non-DataFusion idiom the crate's own docs point to (`StatisticsConverter` + hand-rolled range comparison) — there is no single "give me a pruned reader" one-liner in the bare `parquet` crate; that orchestration is this project's own code, analogous to `plan_column` being this project's own single-decision-point function rather than something the ecosystem hands you for free. `[CITED: docs.rs StatisticsConverter, ArrowReaderBuilder]`, `[ASSUMED: exact "could_match_range" comparison logic is this project's own code, not a crate API — no canonical example of the full comparison loop was found during this research pass; validate the six-operator (`==`,`!=`,`<`,`<=`,`>`,`>=`) comparison logic against `min`/`max` carefully during implementation, especially for the `!=` operator, which can only skip a row group when `min == max == value` (a single-valued row group entirely equal to the excluded value) — every other case must conservatively keep the group.]`

### Pattern 3: Read path — column projection + exact row-level filtering

**What:** `ProjectionMask::columns(&schema_descr, [...])` restricts which columns are physically decoded; `RowFilter::new(vec![Box::new(ArrowPredicateFn::new(projection, closure))])` performs exact row-level filtering on the rows within the surviving row groups the crate does decode.
**When to use:** PARQ-05 (`columns=[...]` and `filters=[...]`, independently combinable per D-27) and the row-level-exact half of D-26.
**Example:**
```rust
// Source: https://docs.rs/parquet/latest/parquet/arrow/arrow_reader/struct.ArrowPredicateFn.html
use parquet::arrow::{ProjectionMask, arrow_reader::{ArrowPredicateFn, RowFilter}};
use arrow::compute::kernels::cmp::gt;
use arrow::array::{Int64Array, AsArray};
use arrow::datatypes::Int64Type;

let filter_projection = ProjectionMask::columns(&parquet_schema_descr, ["amount"]);
let predicate = ArrowPredicateFn::new(filter_projection, move |batch: RecordBatch| {
    let column = batch.column(0).as_primitive::<Int64Type>();
    let scalar = Int64Array::new_scalar(5);
    gt(column, &scalar) // D-25 operator ">" -- one such closure per operator, built generically
});
let row_filter = RowFilter::new(vec![Box::new(predicate)]);

let output_projection = ProjectionMask::columns(&parquet_schema_descr, requested_columns_iter);
let mut builder = ParquetRecordBatchReaderBuilder::try_new(file)?
    .with_row_groups(surviving_row_group_indices) // Pattern 2's output
    .with_projection(output_projection)           // D-27: independent of filters
    .with_row_filter(row_filter);                  // D-26: exact row-level filtering
let reader = builder.build()?;
```
`[VERIFIED: docs.rs ArrowPredicateFn/RowFilter/ArrowReaderBuilder — method signatures fetched directly]`. Note the `RowFilter`'s own `ProjectionMask` (columns needed to *evaluate* the predicate) is a separate mask from the *output* `ProjectionMask` passed to `with_projection` (columns the caller actually wants back) — the crate handles decoding the filter-evaluation columns even if they are not in the output projection, per its documented execution model ("Once all predicates have been evaluated, the final RowSelection is applied to the top-level ProjectionMask to produce the final output RecordBatch").

### Anti-Patterns to Avoid
- **Re-implementing filter/statistics logic in two places:** exactly RESEARCH.md's (project-init) Pitfall 2 precedent (`plan_column`) — the row-group-skip decision (Pattern 2) and the row-level filter closures (Pattern 3) must both be derived from the SAME parsed `FilterExpr` list, built once per `from_parquet` call, not re-parsed or re-derived independently for the two mechanisms.
- **Disabling or overriding the embedded Arrow schema hint:** never call `ArrowWriterOptions::with_skip_arrow_metadata()` on write or `ArrowReaderOptions::with_schema(..)` on read unless there is a specific, tested reason — doing either silently breaks PARQ-06's tz-name and dictionary-type round-trip fidelity (see Common Pitfalls below).
- **Treating a `!=` filter's row-group skip like `<`/`>`/`<=`/`>=`:** a not-equal predicate can only ever prove a row group unnecessary when the group is provably single-valued and equal to the excluded value (`min == max == value`); any `min < value < max` range must be conservatively kept even though every individual row's value could theoretically differ from `value`.
- **Silently accepting an unsupported codec string or filter operator:** per the project's established "no silent best-effort behavior, named specific errors" pattern (extend `FlintError`, do not `.unwrap_or(default)` a typo'd codec name to snappy).

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Parquet file format encoding/decoding (page layout, Thrift metadata, footer) | A custom Parquet reader/writer | `parquet` crate's `ArrowWriter`/`ParquetRecordBatchReaderBuilder` | This is exactly what the crate exists for; arrow-rs's own Thrift metadata parser is 3-9x faster than the previous implementation (per `.planning/research/STACK.md`) — reimplementing any part of this is pure regression risk with zero benefit |
| Arrow schema <-> Parquet logical type mapping (tz name preservation, dictionary reconstruction) | Custom metadata-embedding/extraction logic | The crate's built-in `ARROW:schema` metadata-key mechanism (on by default) | Already solves exactly PARQ-06's hardest cases (see Architecture Patterns note); building a parallel custom scheme would duplicate the crate's own solution and risk disagreeing with it on files written by other Arrow-ecosystem tools (pyarrow, Polars) |
| Row-group statistics extraction (min/max/null-count parsing from Parquet's Thrift-encoded stats) | Manual Thrift stat-blob parsing | `StatisticsConverter` (`row_group_mins`/`row_group_maxes`/`row_group_null_counts`) | The crate already converts these into typed Arrow arrays; hand-parsing Parquet's internal statistics encoding (which varies by physical type, e.g. truncated string stats) is exactly the kind of "deceptively complex" problem this project's "Don't Hand-Roll" ethos exists to avoid |

**Key insight:** Every one of this phase's six requirements maps onto an existing, well-tested `parquet` crate API — the only genuinely new Flint-owned code is (1) parsing this project's specific tuple-filter/path/codec-string API surface into the crate's typed inputs, and (2) the WR-01 nullability fix, which is a bug in code that already exists, not new logic this phase must invent.

## Runtime State Inventory

Not applicable — this phase is a greenfield feature addition (new Parquet IO capability + one bug fix), not a rename/refactor/migration. No stored data, live service config, OS-registered state, secrets, or build artifacts carry over from a prior naming scheme in a way this phase's scope touches.

## Common Pitfalls

### Pitfall 1: Categorical/dictionary round-trip silently degrading to plain values

**What goes wrong:** A `DataType::Dictionary`-typed Arrow column written to Parquet can come back as a plain (non-dictionary) array on read — the Parquet format itself has no schema-level "dictionary" logical type (dictionary encoding is an internal, transparent physical *page encoding* optimization, not a logical type the reader must preserve). pyarrow's own long-standing issue #1688 documents this exact failure mode for the C++/Python implementation: "an Arrow Dictionary type is written out as its value type when saved to Parquet," and historically required an explicit `read_dictionary=[...]` opt-in at read time to get `pandas.Categorical` back.

**Why it happens:** Conflating "Parquet physically dictionary-encodes this column's data pages" (an internal storage optimization, decided per-column by the writer/encoder, invisible to the logical schema) with "the reader will reconstruct an Arrow `DataType::Dictionary` array" (a logical-type decision that depends entirely on whether the reader has and trusts a schema hint saying so).

**How to avoid:** arrow-rs's `ArrowWriter` embeds the *full Arrow schema* (including `DataType::Dictionary` field types) into the Parquet file's own key-value metadata by default (the `ARROW:schema` key, via IPC-format + base64 encoding — the same convention `pyarrow`/arrow-cpp uses, per docs.rs). On read, `ArrowReaderBuilder` consults this hint by default and reconstructs `DictionaryArray` columns without any extra configuration — **as long as the write path never calls `ArrowWriterOptions::with_skip_arrow_metadata()`, and the read path never calls `ArrowReaderOptions::with_schema(..)` to override it.** Verify this holds via an explicit round-trip test: write a `Categorical`-sourced `Table` (dictionary-encoded, per Phase 2's D-17/D-18), read it back, and assert `DataType::Dictionary` (with the correct key/value types and `dict_is_ordered` flag) survives, not a plain `Utf8`/`Int32` array.

**Warning signs:** A round-trip test that only checks *values* match (would pass even if dictionary-ness is lost) rather than asserting the returned `Table`'s `Field::data_type()` is still `DataType::Dictionary(..)`. Any code path that calls `with_skip_arrow_metadata()` or `ArrowReaderOptions::with_schema()` without an explicit, documented reason.

**Phase to address:** This phase (PARQ-06). Verification: an explicit round-trip test asserting `DataType::Dictionary` (not just decoded values) survives write-then-read, plus a `dict_is_ordered` assertion mirroring Phase 2's existing categorical-ordering test.

`[CITED: github.com/apache/arrow/issues/1688 (pyarrow, historical); CITED: docs.rs/parquet/arrow (ARROW:schema embedding mechanism, current arrow-rs behavior)]`

---

### Pitfall 2: tz-aware timestamp round-trip losing the exact zone name

**What goes wrong:** Parquet's native `TIMESTAMP` logical type only carries an `isAdjustedToUTC` boolean (plus a time unit) — it has NO field for an IANA zone name like `"America/New_York"`. A timestamp column written by a tool that doesn't embed Arrow schema metadata (or read by a tool that ignores it) round-trips with only a UTC-vs-not-UTC signal, losing the exact original zone string. STATE.md flags pyarrow issues #35259/#1688 as specifically worth checking here.

**Why it happens:** Same root cause as Pitfall 1 — Parquet's own logical type system is a lowest-common-denominator format-level spec; the exact tz name is an Arrow-level concept preserved only via the same `ARROW:schema` embedded-metadata mechanism, not the Parquet spec itself.

**How to avoid:** arrow-rs's own conversion sets `isAdjustedToUTC = true` whenever an Arrow timestamp has a non-empty tz (Arrow's own in-memory representation of a tz-aware timestamp is already a UTC instant), and — exactly as with dictionaries — the *exact* tz string is recovered from the embedded Arrow schema hint on read, not derived from the Parquet type itself. This means a Flint-internal round-trip (write with this phase's `ArrowWriter`, read with this phase's own reader) preserves the tz string exactly (D-16's "no UTC normalization, round-trip the tz string exactly as-is" carries through Parquet unchanged), but a file received from a non-Arrow-embedding external tool would NOT have the original zone name recoverable — only the UTC-instant values and the `isAdjustedToUTC` flag remain. Document this as an explicit, expected limitation for cross-tool Parquet files (not a Flint bug), and write the round-trip test as "written and read by this project's own code," matching PARQ-06's literal wording ("a Parquet round-trip preserves...").

**Warning signs:** A round-trip test that only checks the UTC-instant epoch values match (would pass even with a normalized/lost tz name) rather than asserting the returned column's `DataType::Timestamp(_, Some(tz))` string is byte-identical to the original.

**Phase to address:** This phase (PARQ-06). Verification: an explicit round-trip test using a non-UTC, non-trivial zone name (e.g. `"America/New_York"`, mirroring Phase 2's own D-16 test fixture) asserting the exact tz string, not just numeric equivalence.

`[CITED: docs.rs/parquet/arrow — isAdjustedToUTC/schema-embedding mechanism; ASSUMED: the specific pyarrow issue numbers #35259/#1688 flagged in STATE.md as needing verification — #1688 was confirmed to concern the *categorical/dictionary* case (Pitfall 1 above, not timestamps); #35259 could not be located/confirmed via WebSearch in this research pass — treat the STATE.md reference to "#35259 timezone timestamp" as unverified and re-check directly against the apache/arrow issue tracker if a tz round-trip anomaly is observed during implementation]`

---

### Pitfall 3: `Compression::ZSTD`/`GZIP` are parameterized enum variants, not unit variants

**What goes wrong:** Naively writing `Compression::ZSTD` or `Compression::GZIP` (as if they were simple unit variants like `Compression::SNAPPY`) fails to compile — both require a level parameter (`ZstdLevel`/`GzipLevel`, each with its own fallible constructor, e.g. `ZstdLevel::try_new(level: i32)`).

**Why it happens:** The `Compression` enum mixes unit variants (`UNCOMPRESSED`, `SNAPPY`, `LZO`, `LZ4`, `LZ4_RAW`) with parameterized ones (`GZIP(GzipLevel)`, `BROTLI(BrotliLevel)`, `ZSTD(ZstdLevel)`) — easy to miss if only skimming an example that happens to use `SNAPPY`.

**How to avoid:** Use each level type's `::default()` (both `GzipLevel`/`ZstdLevel` implement sensible library defaults) unless PARQ-02 is later extended to expose a user-facing compression-level parameter (it is not, per D-29's locked scope — codec choice only, no level tuning). Map the four D-29 codec strings to exactly: `"snappy"` -> `Compression::SNAPPY`, `"zstd"` -> `Compression::ZSTD(ZstdLevel::default())`, `"gzip"` -> `Compression::GZIP(GzipLevel::default())`, `"uncompressed"` -> `Compression::UNCOMPRESSED`.

**Warning signs:** A compile error on `Compression::ZSTD` / `Compression::GZIP` used without a parameter — this is a compile-time-caught mistake, not a runtime one, so it will not silently ship wrong, but budget for it in the write-path task's first implementation pass.

**Phase to address:** This phase (PARQ-02), Write pattern implementation.

`[VERIFIED: docs.rs/parquet/latest/parquet/basic/enum.Compression.html — exact enum definition fetched directly]`

---

### Pitfall 4: Row-group size setter naming/deprecation across parquet-rs versions

**What goes wrong:** Some `parquet` crate versions expose `set_max_row_group_size` while others additionally offer (or have renamed toward) `set_max_row_group_row_count` alongside a separate `set_max_row_group_bytes` — using the wrong/deprecated name for the pinned `59.1.0` version produces a compile error or (worse, if only a deprecation warning) silently keeps using the row-count semantics while a reviewer assumes byte-based config was intended.

**Why it happens:** The crate has evolved its row-group-size configuration surface over major versions (adding byte-based limits alongside the original row-count limit) — training-data/WebSearch results surfaced both names without a version-pinned example in this research pass.

**How to avoid:** At implementation time, confirm the exact method name available on the pinned `parquet = "59.1.0"` (`cargo doc --open -p parquet` or `docs.rs/parquet/59.1.0/parquet/file/properties/struct.WriterPropertiesBuilder.html` for that exact version) before writing the `build_writer_properties` function — D-30 requires row-count semantics specifically (~1,048,576 rows/group default), so whichever setter name is current for `59.1.0`, ensure it is the row-count variant, not a byte-size one.

**Warning signs:** A compile error naming a method that doesn't exist on `WriterPropertiesBuilder` for the pinned version; a test verifying "N rows always produces exactly `ceil(N / row_group_size)` row groups" failing because a byte-based limit was accidentally configured instead.

**Phase to address:** This phase (PARQ-03), Write pattern implementation — first task, confirm the exact setter name against the pinned crate version's own docs before writing calling code.

`[ASSUMED: exact setter name available on parquet 59.1.0 specifically — WebSearch results showed both `set_max_row_group_size` (docs.rs "latest", possibly a different point-version) and a newer `set_max_row_group_row_count`/`set_max_row_group_bytes` split without a version-pinned confirmation; verify directly against the pinned version at implementation time, not from this research]`

## Code Examples

Verified patterns from official sources (also see Architecture Patterns above for the fuller write/read/filter examples):

### Constructing an `ArrowWriter` and closing it (returns row-group-carrying metadata)
```rust
// Source: https://docs.rs/parquet/latest/parquet/arrow/arrow_writer/struct.ArrowWriter.html
pub fn try_new(writer: W, arrow_schema: SchemaRef, props: Option<WriterProperties>) -> Result<Self>
pub fn write(&mut self, batch: &RecordBatch) -> Result<()>
pub fn close(self) -> Result<ParquetMetaData> // finalizes footer + row-group stats + embedded Arrow schema
```

### Constructing a reader and iterating batches
```rust
// Source: https://docs.rs/parquet/latest/parquet/arrow/arrow_reader/struct.ArrowReaderBuilder.html
let mut builder = ParquetRecordBatchReaderBuilder::try_new(file)?;
assert_eq!(builder.metadata().num_row_groups(), 1);
let mut reader = builder.build()?;
while let Some(batch) = reader.next().transpose()? {
    println!("Read {} rows", batch.num_rows());
}
```

### Constructing a `ProjectionMask` from column names
```rust
// Source: https://arrow.apache.org/rust/parquet/arrow/struct.ProjectionMask.html
let mask = ProjectionMask::columns(&parquet_schema_descr, ["a", "c"]);
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|---------------|--------|
| pyarrow's pre-2024 raw `_export_to_c`/`_import_from_c` pointer-integer FFI (not relevant to writing Parquet itself, but relevant to any pandas-facing surface this phase touches indirectly via `Table`) | Arrow PyCapsule Interface (already adopted by this project in Phases 1-2, unaffected by this phase) | 2023-2024 | N/A to Parquet IO directly — noted only because `Table`'s existing PyCapsule dunders are unaffected/untouched by this phase |
| pandas pre-3.0: `__arrow_c_array__`/`__arrow_c_stream__` export-only, no import counterpart | pandas 3.0 added `DataFrame.from_arrow`/`Series.from_arrow`, which DO import via the PyCapsule protocol | pandas 3.0 (2025) | Tangential to this phase (Parquet IO doesn't touch pandas at all — `Table.from_parquet`/`to_parquet` operate on the Arrow core directly) but corrects `.planning/research/STACK.md`'s now-stale "pandas only exports, no import support" claim, which was accurate for pre-3.0 pandas but not for the pinned pandas 3.0.3. Not actionable for this phase; flagged for awareness only. `[CITED: pandas.pydata.org/docs/reference/api/pandas.DataFrame.from_arrow.html]` |

**Deprecated/outdated:**
- The claim in `.planning/research/STACK.md`'s Version Compatibility table that "pandas only exports via `__arrow_c_*__` (no import support as of current pandas 3.0.x)" is out of date for the pinned pandas 3.0.3 — `DataFrame.from_arrow`/`Series.from_arrow` (added in pandas 3.0) now import via the same protocol. Not relevant to this phase's actual scope (Parquet IO doesn't route through pandas), but should be corrected in STACK.md if a future phase relies on the stale claim.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | The exact `WriterPropertiesBuilder` row-group-size setter name (`set_max_row_group_size` vs a newer `set_max_row_group_row_count`) available on the pinned `parquet = "59.1.0"` | Common Pitfalls, Pitfall 4 | Compile error caught immediately at implementation time (low risk) — but budget a few minutes to check the pinned version's own docs rather than copy a possibly-version-mismatched example verbatim |
| A2 | STATE.md's flagged pyarrow issue "#35259" concerns tz-aware timestamp Parquet round-trip specifically | Common Pitfalls, Pitfall 2 | Could not be confirmed/located via WebSearch in this research pass; the underlying tz-round-trip mechanism (embedded Arrow schema) is independently confirmed from docs.rs regardless of this specific issue number, so the risk is limited to "STATE.md's specific citation may be a mis-numbered/mis-transcribed issue reference," not to the correctness of the recommended mitigation |
| A3 | The exact row-group statistics comparison logic for the `!=` operator (only skippable when `min == max == value`) and for the other five D-25 operators against `StatisticsConverter`'s min/max arrays | Architecture Patterns, Pattern 2 | No canonical bare-`parquet`-crate (non-DataFusion) example of the full comparison loop was found; if implemented incorrectly (e.g. treating `!=` like `==`'s inverse for ranges), the risk is either over-pruning (silently dropping matching rows — a correctness bug D-26 explicitly forbids) or under-pruning (correct results, just no IO savings) — over-pruning is the dangerous direction and should be caught by a property test comparing filtered-read results against an unfiltered-read-then-Python-filter baseline |
| A4 | `parquet` crate's default features (no explicit `features = [...]`) are sufficient for D-29's exact four codecs without pulling in unwanted extras (lz4/brotli/lzo are also default-on but simply unused, not a problem) | Standard Stack | Low risk — even if default features include codecs beyond D-29's four, this only means slightly more compiled code, not a functional or scope problem; D-29's restriction is enforced by Flint's own codec-string validation (reject anything not in the four), not by Cargo feature gating |
| A5 | ~~Sourcing nullability from the imported stream's schema affects only the ArrowDtype WR-01 case~~ — **RESOLVED, no longer an open assumption**: empirically verified this session (see Summary) that pyarrow's `__arrow_c_stream__` export marks every column nullable=True, so the WR-01 fix broadens nullability uniformly across all stream-imported columns, not just the ArrowDtype case. Retained here as a record of what was checked, not as an outstanding risk. | Summary | None — verified, and the broadening direction is confirmed safe (permissive, matches no existing test's `nullable=False` assertion) |
| A6 | The dictionary/tz round-trip preservation mechanism (`ARROW:schema` embedded metadata, default-on) works exactly as the crate's own module docs describe, despite a conflicting WebSearch summary claiming dictionary types require an explicit schema-hint override to preserve | Architecture Patterns note; Common Pitfalls 1-2 | This is PARQ-06's single most consequential fact and was NOT independently re-verified against the actual pinned `parquet` 59.1.0 crate in this research session (no local round-trip spike was run) — treat as the Summary now states: a **mandatory Wave-0 verification gate** for the first Parquet read/write task, not a settled fact the plan can build on unchecked. If the round-trip spike shows dictionary-ness or the exact tz string is lost, the planner must revisit whether an explicit schema-hint mechanism needs to be added to the write/read path. |

## Open Questions

1. **Exact multi-file schema-mismatch policy (D-21 discretion)**
   - What we know: D-21 locks "multi-file/directory read produces one `Table`" as in-scope; the exact policy for schema mismatches across files is explicitly left to planner/implementer discretion.
   - What's unclear: whether arrow-rs offers a ready-made "read N Parquet files with schema reconciliation" helper, or whether this project must implement file-by-file `try_new` + manual `arrow::compute::concat`/`Schema::try_merge` itself (the same "single-decision-point" pattern used elsewhere in this codebase).
   - Recommendation: default to a strict-match-required policy (reject with a named `FlintError` citing the first mismatched file and column) as the simpler, safer v1 default, consistent with this project's "no silent best-effort behavior" pattern elsewhere (D-11's object-dtype validation, D-15's non-ns rejection) — plan a follow-up/discretion note for the planner to confirm this reading of "left to discretion" matches user intent, since it wasn't explicitly re-confirmed in CONTEXT.md beyond "left to discretion."

2. **Row-group statistics comparison correctness for all six D-25 operators**
   - What we know: `StatisticsConverter` provides typed min/max/null-count Arrow arrays per row group; the crate provides no built-in comparison-against-a-literal helper.
   - What's unclear: the precise, tested comparison logic for each operator against a `(min, max)` range, especially edge cases (all-null row group with no exact min/max stats; a range column with `min == max`).
   - Recommendation: implement as pure, independently unit-testable Rust functions (one per operator, or one parameterized function over an `Op` enum) in `flint-core`, mirroring `plan_column`'s existing pyo3-free, exhaustively-unit-tested style — write property/table-driven tests covering `min < value < max` (must keep), `value < min` and `value > max` for `>`/`>=`/`<`/`<=` (must skip when provably impossible), and the `!=` single-value-equals-value case specifically.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|--------------|-----------|---------|----------|
| Rust toolchain (`rustc`/`cargo`) | Building `flint-core`/`flint-python` with the new `parquet` dependency | Yes | rustc/cargo 1.97.0 [VERIFIED: `rustc --version`/`cargo --version`, this session] | — |
| `uv` | `test_command`'s `uv run maturin develop && uv run pytest tests/python -q` step | Yes | 0.11.25 [VERIFIED: `uv --version`, this session] | — |
| Python 3 | Test suite / `maturin develop` target interpreter | Yes | 3.12.3 [VERIFIED: `python3 --version`, this session] | — |
| `parquet` crate (new dependency this phase) | All six PARQ requirements | Not yet added to `Cargo.toml` — resolved via `cargo build` once added, same as any new crates.io dependency | 59.1.0 confirmed available on crates.io [VERIFIED: docs.rs/crate/parquet/59.1.0] | — |

**Missing dependencies with no fallback:** none — all required tooling (Rust toolchain, `uv`, Python) is already present and was already required by Phases 1-2; this phase introduces exactly one new dependency (`parquet`), which is a plain crates.io crate resolved automatically by `cargo build --workspace` and requires no separate installation step, system package, or service.

**Missing dependencies with fallback:** none.

## Security Domain

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|----------------|---------|-------------------|
| V2 Authentication | No | Not applicable — local file IO library, no auth boundary |
| V3 Session Management | No | Not applicable |
| V4 Access Control | No | Not applicable — file access control is the OS filesystem's job; this library does not add its own access-control layer (matches D-20's local-filesystem-only scope) |
| V5 Input Validation | Yes | Path/filter/codec argument validation at the PyO3 boundary: reject unsupported codec strings (D-29), unsupported filter operators (D-25), and non-`str`/`Path` path arguments (D-20) with named `FlintError` variants — no silent coercion. Malformed/malicious Parquet file input (corrupt footer, out-of-range offsets) must be handled as a recoverable `Result::Err` from the `parquet` crate's own parsing, not an `unwrap()`/panic, since a Parquet file is untrusted input the moment it comes from a path the caller supplied (could be attacker-controlled in a multi-tenant context, even if this project's own threat model is primarily "trusted local files") |
| V6 Cryptography | No | Not applicable — no Parquet encryption feature is in scope for this phase (the `parquet` crate's `encryption` feature is not enabled; D-20/D-22 do not mention encrypted Parquet support) |

### Known Threat Patterns for this stack

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|-----------------------|
| Malformed/corrupt Parquet file causing a panic or out-of-bounds read during metadata/footer parsing | Denial of Service / Tampering | Rely on the `parquet` crate's own `Result`-returning API (`try_new`, `.build()`) — never call `.unwrap()` on a parse result from file bytes not produced by this project's own writer in the same process; surface parse errors as a named `FlintError` variant, matching the project's existing "no silent best-effort behavior, named specific errors" pattern (RESEARCH.md project-init Security Mistakes: "Trusting unsafe pointer arithmetic on buffer lengths/offsets received from...a Parquet file without bounds validation") |
| Path traversal via a caller-supplied path string (`../../etc/passwd`-style) | Tampering / Information Disclosure | Out of this phase's explicit threat model (D-20 scopes to local-filesystem `str`/`Path` args, same trust level as any local file-IO library — e.g. Python's own `open()` has the same property); document as a caller responsibility, not a Flint-owned mitigation, consistent with pyarrow's own `read_table`/`write_table` having no path-sandboxing of their own |
| Directory-read (D-21) silently including unintended files (e.g. a `.tmp`/hidden file in a directory glob) | Tampering (unintended data included) | If directory auto-discovery is the chosen D-21 mechanism (planner/implementer discretion), filter strictly to a `.parquet` extension (or files also written by this crate) rather than "every file in the directory" — an explicit, named error for a non-Parquet file encountered, not a silent skip or silent inclusion-as-garbage |

## Sources

### Primary (HIGH confidence)
- `docs.rs/parquet/latest/parquet/arrow/arrow_writer/struct.ArrowWriter.html` — ArrowWriter constructor/write/close signatures
- `docs.rs/parquet/latest/parquet/file/properties/struct.WriterProperties.html` — WriterPropertiesBuilder compression/row-group config
- `docs.rs/parquet/latest/parquet/arrow/arrow_reader/struct.ArrowReaderBuilder.html` — reader construction, `with_projection`/`with_row_filter`/`with_row_groups`/`with_row_selection`
- `docs.rs/parquet/latest/parquet/arrow/arrow_reader/struct.RowFilter.html` — RowFilter construction and execution model
- `docs.rs/parquet/latest/parquet/arrow/arrow_reader/struct.ArrowPredicateFn.html` — ArrowPredicateFn closure signature and example
- `docs.rs/parquet/latest/parquet/basic/enum.Compression.html` — exact Compression enum variants (parameterized vs unit)
- `docs.rs/parquet/latest/parquet/arrow/arrow_reader/statistics/struct.StatisticsConverter.html` — statistics extraction method signatures
- `docs.rs/parquet/latest/parquet/arrow/index.html` — ARROW:schema embedding/restoration mechanism, `ArrowWriterOptions::with_skip_arrow_metadata`, `ArrowReaderOptions::with_schema` override, Parquet-schema-takes-precedence caveat
- `docs.rs/crate/parquet/59.1.0` — version/release-date confirmation
- `gsd-tools query package-legitimacy check --ecosystem crates parquet` — registry legitimacy verdict (OK, 2018 publish date, ~1.01M weekly downloads, github.com/apache/arrow-rs)
- Direct reading of `crates/flint-python/src/pandas.rs`, `crates/flint-python/src/table.rs`, `crates/flint-python/src/error.rs`, `crates/flint-core/src/table.rs`, `crates/flint-core/src/pandas_plan.rs` (this repository) — WR-01 root cause and fix mechanism, existing architecture to extend

### Secondary (MEDIUM confidence)
- `github.com/apache/arrow/issues/1688` (pyarrow) — categorical/dictionary-loses-on-Parquet-round-trip historical issue, cross-checked against current arrow-rs `ARROW:schema` mechanism
- `pandas.pydata.org/docs/reference/api/pandas.DataFrame.from_arrow.html` — pandas 3.0 `from_arrow` import-side PyCapsule support (tangential to this phase)
- `github.com/apache/arrow-rs/pull/1180`, `discussions/4674`, `pull/2335` — arrow-rs dictionary-preservation and RowFilter API history (WebSearch-summarized, not directly fetched page-by-page)

### Tertiary (LOW confidence)
- STATE.md's specific citation of pyarrow issue "#35259" for tz-aware timestamp round-trip — could not be located/confirmed via WebSearch in this research pass; flagged in Assumptions Log (A2) rather than treated as verified
- Exact `set_max_row_group_size`-vs-`set_max_row_group_row_count` naming for the precise pinned `59.1.0` version (WebSearch results mixed multiple crate-version eras) — flagged in Common Pitfalls (Pitfall 4) and Assumptions Log (A1) for implementation-time re-verification

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — `parquet` crate choice and version were already locked by project-init STACK.md and independently re-confirmed live against docs.rs/crates.io and the package-legitimacy seam this session
- Architecture: HIGH for the three core patterns (write/row-group-pruning/row-filter+projection), all backed by directly-fetched docs.rs pages with exact method signatures; MEDIUM for the specific row-group statistics comparison idiom (Pattern 2), since no single canonical bare-crate example exists — this is synthesized from real APIs (`StatisticsConverter`, `with_row_groups`) but the orchestration is this project's own code to write and test
- Pitfalls: HIGH for the dictionary/tz round-trip mechanism (directly confirmed against docs.rs's own description of `ARROW:schema` embedding) and the `Compression` enum shape (directly fetched); MEDIUM for the two specific STATE.md-flagged pyarrow issue numbers, one of which (#1688) was confirmed relevant and one (#35259) could not be located in this pass

**Research date:** 2026-07-23
**Valid until:** 30 days (stable, actively-maintained crate on a monthly release cadence; re-verify the exact `WriterPropertiesBuilder` row-group-size setter name and any newer arrow-rs releases if planning is delayed past that window)
