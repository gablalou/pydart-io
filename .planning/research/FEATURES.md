# Feature Research

**Domain:** Rust-backed Python library for zero-copy pandas <-> Arrow interop + Parquet IO (lean pyarrow alternative, not a query engine)
**Researched:** 2026-07-13
**Confidence:** MEDIUM (cross-checked against Apache Arrow official docs, pandas official docs, and multiple independent community/GitHub sources; no single-source claims presented as fact)

## Feature Landscape

### Table Stakes (Users Expect These)

Features users assume exist. Missing these = product feels incomplete or actively unusable for the stated purpose (pyarrow alternative for interop).

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| DataFrame -> Table conversion (`from_pandas`-equivalent) | This is the core value proposition; without it there's no product | MEDIUM | Must handle mixed dtypes per-column; each column is its own copy/no-copy decision (see zero-copy matrix below) |
| Table -> DataFrame conversion (`to_pandas`-equivalent) | Round-trip is assumed; a one-way bridge is not a bridge | MEDIUM | Harder direction than the reverse — must decide null representation, block consolidation, dtype backend (`ArrowDtype` vs numpy vs nullable) |
| Null/missing-value handling across all dtypes | Real-world data always has nulls; silent data corruption on nulls is disqualifying | HIGH | Arrow uses a validity bitmap; numpy has no null concept for int/bool. This is the single most common correctness bug in Arrow bridge libraries (see PITFALLS) |
| dtype mapping table: int8-64, uint8-64, float32/64, bool, object/string, categorical, datetime64[ns] (+ tz), timedelta | Users' real DataFrames use this full set; a partial mapping means "doesn't work with my data" | HIGH | Datetime timezone handling and categorical dictionary mapping are the two trickiest sub-cases |
| ChunkedArray / multi-chunk Table support | Arrow Tables from Parquet reads, Polars, DuckDB etc. are routinely multi-chunk; refusing them makes the library unusable as an interop layer | MEDIUM | Multi-chunk always forces a copy on the way into pandas today (pandas requires a single contiguous buffer) — this must be handled correctly, not just documented away |
| Schema preservation on round-trip (column names, order, nested/logical types, pandas index) | Users expect `df -> table -> df` to reproduce the original frame faithfully | MEDIUM | pyarrow does this via embedded `pandas_metadata` in the Arrow schema; equivalent metadata handling is required for round-trip fidelity |
| Explicit zero-copy-or-error mode (a `zero_copy_only`-style flag) | Power users doing this specifically for performance need a way to *guarantee* no silent copy occurred, not just hope | LOW | Directly serves the project's core value ("provably faster"); also doubles as a debugging/benchmarking tool |
| Arrow PyCapsule Interface support (`__arrow_c_array__`, `__arrow_c_schema__`, `__arrow_c_stream__`) | This is the 2024+ standard mechanism Polars (v1.3+), DuckDB, pandas (2.2+, export-only), Ibis and others use for zero-copy handoff without a hard pyarrow dependency | MEDIUM | Table stakes, not a differentiator — omitting it means the library can't credibly claim "interoperates with the existing Arrow ecosystem" per PROJECT.md's own stated goal |
| Parquet read/write | Explicitly required by PROJECT.md | MEDIUM-HIGH | See Parquet-specific table stakes below |
| Basic error messages that name the failing column/dtype | Data engineers debugging a failed conversion need to know *which* column broke, not just "conversion failed" | LOW | Cheap to build, expensive in goodwill if skipped |

### Parquet-Specific Table Stakes

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| Compression codec support: snappy (default), zstd, gzip, uncompressed, (lz4/brotli nice-to-have) | Snappy is the ubiquitous default; zstd is increasingly preferred for storage-bound workloads. A Parquet writer offering only one codec is a non-starter | LOW-MEDIUM | pyarrow implements essentially all standard codecs except LZO; matching snappy+zstd+gzip+uncompressed covers the overwhelming majority of real usage |
| Row group configuration (size/count control on write) | Row group size is the primary lever for read-time pruning and write-time memory/parallelism tradeoffs; hardcoding it is a real limitation | LOW | Expose as a write-time parameter (e.g. target rows per group) |
| Row-group statistics (min/max, null counts) | These are what predicate pushdown is built on; without them, "reads a Parquet file" is much less useful than "reads it efficiently" | MEDIUM | Written into the file footer at write time; must be correct or downstream engines (DuckDB, Polars) silently skip data incorrectly |
| Predicate pushdown at read time (row-group pruning via footer statistics) | This is what makes Parquet reads fast at scale; a reader that always scans every row group defeats the point of using Parquet | MEDIUM-HIGH | Row-group-level pruning (skip whole groups using footer stats) is achievable at moderate complexity; page-level pushdown is a stretch goal, not table stakes |
| Column projection / column pushdown at read time | Reading only requested columns (not the whole row) is Parquet's headline benefit over row-oriented formats | LOW-MEDIUM | Natural consequence of Parquet's columnar layout — should be close to "free" given a competent reader implementation |
| Dictionary encoding (read + write) | Standard Parquet encoding for low-cardinality columns; also the natural counterpart to pandas categorical <-> Arrow dictionary array mapping | MEDIUM | Ties directly to the categorical dtype mapping table-stakes item above |
| Correct Arrow-logical-type <-> Parquet-physical-type round-trip (including timestamps with timezone, decimal, nested types if in scope) | Silent type coercion on Parquet round-trip (e.g. losing timezone info) is a classic, trust-destroying bug | HIGH | This is where "leaner than pyarrow" must not mean "less correct than pyarrow" |
| Reading/writing multi-file datasets with consistent schema | Real pipelines write partitioned Parquet directories, not single files | MEDIUM | Schema *evolution* across files with genuinely differing schemas (see below) is explicitly a step beyond this and should be scoped carefully |

### Differentiators (Competitive Advantage)

Features that set the product apart from pyarrow specifically for the interop use case. Must map back to PROJECT.md's Core Value (measurably faster, zero-copy, lean).

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| Small install/import footprint | pyarrow's wheel is very large (historically 150-250MB combined with numpy dependency; conda-forge now ships split `pyarrow-core`/`pyarrow`/`pyarrow-all` packages specifically because of this pain) and has a non-trivial import time. A Rust-native, narrowly-scoped binary can plausibly be an order of magnitude smaller and faster to import | MEDIUM | This is a concrete, benchmarkable claim (binary size + `import` time) that directly supports "leaner alternative" positioning |
| Measurably faster conversion, with a public benchmark suite vs pyarrow | This is the stated Core Value in PROJECT.md — without hard numbers, the project "has no reason to exist" per its own framing | MEDIUM-HIGH | Must benchmark across the realistic dtype matrix, not just the easy all-numeric-no-nulls case, or the numbers will be misleading and get challenged in the community |
| First-class, zero-copy-by-default handling of `pandas.ArrowDtype` columns | This is the one case where *true* zero-copy in both directions is achievable today, and pyarrow itself still round-trips through extra metadata/type-mapper ceremony. Making this the smooth, default path (rather than an opt-in edge case) is a real differentiator | LOW-MEDIUM | Directly enables the "genuinely zero-copy" marketing claim for the growing population of `dtype_backend="pyarrow"` users |
| Explicit, queryable "did this copy?" API/diagnostics per column | Goes beyond a single global `zero_copy_only` flag — report per-column why a copy happened (dtype mismatch, nulls present, multi-chunk, non-contiguous, object dtype) | LOW-MEDIUM | Turns "it's probably faster" into an inspectable, debuggable guarantee; strong fit for a library whose whole pitch is performance transparency |
| PyCapsule-native API surface (accept/return anything implementing `__arrow_c_*__`, not just pyarrow objects) | Lets the library exchange data with Polars/DuckDB directly without ever requiring pyarrow as a dependency — a genuine "leaner" story, since pyarrow itself is one of the things users are trying to avoid installing | MEDIUM | This is both a differentiator and infrastructure for table stakes; treat the *acceptance* of foreign capsule objects as the differentiator layer on top of the table-stakes *support* of the protocol |
| Fast dictionary/categorical round-trip tuned for the common case | Categorical <-> dictionary has known pyarrow rough edges (metadata bugs, `string[pyarrow]`-as-category failures reported against pyarrow) | MEDIUM | An opportunity to be *more correct*, not just faster, in a documented pyarrow weak spot |
| Narrow, well-documented API surface | pyarrow's Python API is enormous (compute, dataset, flight, filesystems, etc.); a bridge-only library can have a tiny, learnable surface as a selling point in itself | LOW | This is as much a positioning/documentation exercise as an engineering one |

### Anti-Features (Commonly Requested, Often Problematic)

Features that seem like natural additions but would violate the project's stated scope discipline.

| Feature | Why Requested | Why Problematic | Alternative |
|---------|---------------|------------------|-------------|
| Compute kernels (filter, groupby, join, aggregation, sort) | Once users have a Table, they'll ask "why can't I just filter it here?" | This is squarely query-engine territory (Polars' job); building it turns the project into a second, weaker Polars and destroys the "lean, focused" positioning that is the whole point | Convert to Polars/DuckDB/pandas for any compute; this library's job ends at handing over the data |
| Distributed / out-of-core execution | Users with big data will ask for chunked/streaming or cluster support | Massive scope and engineering-effort increase for a "single-machine bridge" library; conflicts directly with PROJECT.md's stated constraint | Users needing this should reach for Dask, Ray, Spark, or DuckDB's out-of-core engine, feeding results through this library's bridge if needed |
| Becoming its own DataFrame type / lazy expression API | Natural creeping temptation for any bridge library — "just add a `.filter()` and now we're a mini-Polars" | This is scope creep by a different name than compute kernels but with the same effect: it turns a bridge into a competing engine, contradicting "narrower/lower-level than Polars" (Key Decision in PROJECT.md) | Stay strictly a conversion + IO layer; any user-facing manipulation happens in pandas, Polars, or DuckDB, not here |
| Multi-language bindings (R, Node, etc.) | Rust core "could" support this cheaply in principle, so it's an easy ask | Splits focus and testing/support burden away from the Python interop use case that is the actual goal for v1 | Explicitly deferred per PROJECT.md; revisit only if the Rust core is stable and there's demonstrated demand |
| CSV/JSON/other file format IO | "You already read Parquet, why not CSV?" is a natural, low-effort-sounding ask | Dilutes focus from getting the Parquet path (and the zero-copy bridge) fully correct and fast first; CSV/JSON also have much messier type-inference problems than Parquet's typed schema | Users convert CSV/JSON via pandas/pyarrow/Polars today; only reconsider after Parquet path is solid, per PROJECT.md |
| Automatic multi-file Parquet schema merging (Spark-style `mergeSchema`) | Feels like a natural extension of "supports Parquet datasets" | Schema-evolution merge logic is genuinely complex (type widening rules, column reordering, conflict resolution) and pyarrow itself does not do this automatically today — building it well is a project unto itself | Support reading a fixed, consistent schema across a partitioned dataset (table stakes); require users to pass an explicit unified schema for anything beyond that, rather than guessing |
| A `bool8`-style custom extension type to "fix" the boolean zero-copy gap | Tempting because Arrow's 1-bit-packed booleans vs numpy's 1-byte-per-bool layout is a genuine, annoying zero-copy gap | Inventing a custom extension type undermines the "Arrow-compatible, interoperates with the ecosystem" constraint — other Arrow consumers won't recognize a bespoke type without extra glue | Document the boolean copy as an accepted, unavoidable cost (like the official `bool8` extension-type discussion in the Arrow project itself, which remains optional/experimental); do not invent a competing convention |

## Feature Dependencies

```
DataFrame -> Table conversion
    └──requires──> dtype mapping table (int/float/string/categorical/datetime/bool)
                       └──requires──> null/validity-bitmap handling

Table -> DataFrame conversion
    └──requires──> dtype mapping table (reverse direction)
    └──requires──> ChunkedArray / multi-chunk handling (single-chunk consolidation)
    └──requires──> schema/metadata preservation (pandas_metadata equivalent)

Zero-copy guarantee mode (`zero_copy_only`-style flag)
    └──requires──> DataFrame -> Table conversion
    └──requires──> Table -> DataFrame conversion
    └──enhances──> Benchmark suite (makes "is this actually zero-copy" testable, not just assumed)

Per-column "did this copy?" diagnostics API
    └──requires──> Zero-copy guarantee mode
    └──enhances──> Benchmark suite credibility

PyCapsule Interface support (export + accept foreign objects)
    └──requires──> DataFrame <-> Table conversion (needs a Table representation to export/import)
    └──enhances──> Ecosystem interop claim (Polars, DuckDB, Ibis without hard pyarrow dependency)

Parquet read/write
    └──requires──> Arrow schema mapping (same dtype table as conversion, applied to Parquet logical types)
    └──requires──> Compression codec support
    └──requires──> Row-group statistics (write time) ──enables──> Predicate pushdown (read time)
    └──requires──> Column projection support

Multi-file dataset reading (consistent schema)
    └──requires──> Parquet read
    └──precedes──> Schema-evolution / merge support (deliberately NOT v1, see anti-features)

Benchmark suite vs pyarrow
    └──requires──> DataFrame -> Table conversion (both directions working)
    └──requires──> Zero-copy guarantee mode (to prove, not just claim, the copy behavior)
    └──requires──> Parquet read/write (for IO-side benchmarks)

Compute kernels [ANTI-FEATURE] ──conflicts──> Narrow/lean positioning differentiator
Distributed execution [ANTI-FEATURE] ──conflicts──> Single-machine constraint (PROJECT.md)
```

### Dependency Notes

- **DataFrame <-> Table conversion requires the dtype mapping table:** you cannot ship a partial converter — half-supporting dtypes just relocates the "doesn't work with my data" complaint from pyarrow to this library.
- **Null handling is a prerequisite baked into the dtype table, not a separate feature:** every dtype's zero-copy-vs-copy answer changes depending on whether nulls are present (see the zero-copy matrix below), so it must be designed in from the start, not bolted on.
- **Zero-copy guarantee mode enhances the benchmark suite:** the benchmark suite's central claim ("measurably faster") is only trustworthy if the library can *prove*, column by column, when it actually avoided a copy — otherwise "zero-copy" is marketing, not measurement.
- **PyCapsule support requires conversion to exist first, but is architecturally a thin export/import layer over the same Table representation** — it should not require a second data model.
- **Predicate pushdown requires row-group statistics to exist at write time:** a reader cannot prune row groups whose files were never written with min/max stats, so both must land together (or the reader must tolerate files without stats, which is worth an explicit compatibility decision).
- **Schema-evolution/merge conflicts with staying in scope:** it's tempting to build once multi-file dataset reading exists, but treat it as a hard boundary — support *consistent* multi-file schemas as table stakes, defer true merge-on-conflict logic.

## MVP Definition

### Launch With (v1)

Minimum viable product — matches PROJECT.md's "Active" requirements almost exactly; this is the credible core, not padding.

- [ ] DataFrame -> Table conversion covering the full realistic dtype set (int/uint/float variants, bool, object/string, `string[pyarrow]`, categorical, datetime64[ns] incl. tz, timedelta) with correct null handling — without this, there is no product
- [ ] Table -> DataFrame conversion (reverse direction), including multi-chunk consolidation and schema/metadata round-trip fidelity — a one-directional bridge fails the "bridge" framing
- [ ] Explicit zero-copy-or-error mode + per-column copy diagnostics — this is what makes the performance claim provable rather than asserted
- [ ] Arrow PyCapsule Interface support (both export and import) — required to credibly claim ecosystem interoperability with Polars/DuckDB, and is now the standard mechanism, not an extra
- [ ] Parquet read/write with: snappy + zstd (+ gzip, uncompressed) compression, configurable row-group size, row-group statistics on write, predicate pushdown (row-group pruning) and column projection on read — the minimum for "reads/writes Parquet files" to mean something at scale, not just "opens a toy file"
- [ ] Benchmark suite vs pyarrow covering conversion speed *and* memory, across the realistic dtype matrix (not just the easy all-numeric-no-null case) — required to validate Core Value per PROJECT.md

### Add After Validation (v1.x)

Features to add once the core bridge + Parquet path is proven and adopted.

- [ ] Additional Parquet compression codecs (lz4, brotli) — add once core snappy/zstd/gzip path is stable and users ask
- [ ] Consistent multi-file/partitioned dataset reading — add once single-file Parquet path is solid and users hit multi-file use cases
- [ ] Page-level (not just row-group-level) predicate pushdown — a performance refinement once basic pushdown is proven, not a launch blocker
- [ ] Deeper diagnostics/tracing hooks for the copy-detection API (e.g. structured logging, integration with profilers) — valuable once the basic diagnostics API has real users giving feedback

### Future Consideration (v2+)

Features to defer until the interop + Parquet niche has established product-market fit — and some that may never be in scope per PROJECT.md.

- [ ] Schema-evolution/merge across genuinely differing multi-file schemas — significant complexity, defer until there's clear demand and the simpler consistent-schema case is battle-tested
- [ ] Additional file formats (CSV/JSON) — explicitly deferred in PROJECT.md until the Parquet path is solid
- [ ] Multi-language bindings (R, Node) — explicitly deferred in PROJECT.md; revisit only after the Rust core is proven stable
- [ ] Compute kernels / distributed execution — not deferred, *excluded* per PROJECT.md's stated scope; would only be reconsidered as a distinct future milestone with its own positioning, not an incremental add to this library

## Feature Prioritization Matrix

| Feature | User Value | Implementation Cost | Priority |
|---------|------------|---------------------|----------|
| DataFrame -> Table conversion (full dtype set + nulls) | HIGH | HIGH | P1 |
| Table -> DataFrame conversion (full dtype set + nulls) | HIGH | HIGH | P1 |
| Zero-copy guarantee mode + per-column diagnostics | HIGH | MEDIUM | P1 |
| Arrow PyCapsule Interface (export + accept) | HIGH | MEDIUM | P1 |
| Parquet read/write (codecs, row groups, stats, pushdown, projection) | HIGH | HIGH | P1 |
| Benchmark suite vs pyarrow | HIGH | MEDIUM | P1 |
| Small install/import footprint as marketed differentiator | MEDIUM | LOW (mostly a consequence of scope discipline) | P2 |
| Additional codecs (lz4/brotli) | LOW-MEDIUM | LOW | P2 |
| Consistent multi-file dataset reading | MEDIUM | MEDIUM | P2 |
| Page-level predicate pushdown | LOW | MEDIUM-HIGH | P3 |
| Schema-evolution/merge | LOW-MEDIUM | HIGH | P3 |
| CSV/JSON IO | LOW (for this project's target user) | MEDIUM | P3 |
| Multi-language bindings | LOW (v1 audience is Python) | HIGH | P3 |
| Compute kernels / distributed execution | N/A — out of scope | N/A | Excluded |

**Priority key:**
- P1: Must have for launch
- P2: Should have, add when possible
- P3: Nice to have, future consideration

## The Zero-Copy Truth Table (pandas dtype x direction)

This is the load-bearing technical reference for both the requirements definition and the benchmark suite — every claim below is cross-checked against Apache Arrow's and pandas' own documentation plus corroborating community/issue-tracker sources.

| pandas column type | Direction | Zero-copy? | Reason |
|---|---|---|---|
| numpy int/float, **no nulls**, single chunk | df -> Table / Table -> df | **True zero-copy** | Arrow can wrap the existing numpy buffer directly (matching memory layout); this is the textbook case both pyarrow and this project's Rust core should optimize for |
| numpy int/float, **with nulls** | df -> Table / Table -> df | **Copy required** | pandas' legacy numpy-backed columns have no validity-bitmap concept; pandas historically represents missing ints as upcast `float64` + `NaN`, which Arrow must materialize/reconcile via a copy. Mapping to pandas' nullable `Int64`/`Float64` dtypes instead avoids the upcast but still requires constructing the validity bitmap |
| numpy `bool` | df -> Table / Table -> df | **Always a copy, either direction** | Arrow bit-packs booleans (1 bit/value); numpy uses 1 byte/bool. Layouts are fundamentally incompatible without a copy or an explicit extension type (e.g. the still-experimental `bool8`), which this project should not invent (see anti-features) |
| `object` dtype (Python strings, mixed types) | df -> Table / Table -> df | **Always a copy** | numpy object arrays hold pointers to Python objects, not contiguous UTF-8; Arrow strings need a values buffer + offsets buffer. This is the single most common real-world copy trigger, since "object dtype full of strings" is extremely common in the wild |
| `string[pyarrow]` / pandas `ArrowDtype` string columns | df -> Table / Table -> df | **True zero-copy** | The column is already backed by an Arrow array under the hood; conversion is a handoff, not a transformation. This is the standout case to make effortless and default-fast |
| `pandas.ArrowDtype` columns generally (any type, pandas 2.0+, `dtype_backend="pyarrow"`) | df -> Table / Table -> df | **True zero-copy, both directions** | This is the headline "true zero-copy" scenario — the data is already Arrow-formatted in memory. Should be the primary path this library optimizes and markets |
| `category` (pandas Categorical) | df -> Table / Table -> df | **Near-zero-copy (codes + dictionary handoff)**, but with known correctness edge cases | Maps to Arrow's dictionary-encoded array (codes buffer + dictionary values). Watch the documented pyarrow rough edge where `string[pyarrow]`-backed categories can trip up Parquet round-trips |
| `datetime64[ns]` (no timezone) | df -> Table / Table -> df | **Zero-copy when Arrow timestamp unit matches** | Straightforward buffer-compatible case when units align; mismatched units (e.g. differing resolution) force a cast/copy |
| `datetime64[ns, tz]` (timezone-aware) | df -> Table / Table -> df | **Conditional** | Timezone metadata must round-trip correctly; the underlying buffer can still be zero-copy but this is a correctness-sensitive path worth explicit test coverage, not just a happy-path assumption |
| Any `ChunkedArray` with **multiple chunks** (Table -> df direction) | Table -> df | **Copy required** | pandas requires a single contiguous buffer per column; multiple Arrow chunks (common after Parquet reads, filters, or concatenation from other engines) must be consolidated into one buffer first. This is unavoidable given pandas' current internal representation, not a library-implementation shortcoming |

**Practical implication for the library's public API:** offer a genuinely useful "zero-copy-or-tell-me-why" mode rather than a binary "zero-copy-only, else raise" mode alone. Given how many common, everyday cases (any nulls, any object-dtype string column, any boolean column, any multi-chunk table) force a copy, a mode that only ever succeeds or throws would fail for most real DataFrames. The differentiator is *knowing and reporting* exactly why a copy happened, per column — turning an unavoidable limitation of the pandas/Arrow memory model into a transparency feature instead of a surprise.

## Competitor Feature Analysis

| Feature | pyarrow | Polars | Our Approach |
|---------|---------|--------|--------------|
| pandas <-> Arrow conversion | Full-featured, but general-purpose and carries the weight of pyarrow's entire compute/dataset/filesystem/flight surface; large import footprint | Not the framing — Polars converts to/from pandas/Arrow as a side door into its own engine, not as its core purpose | Purpose-built, narrow API whose only job is this conversion — smaller surface, faster import, benchmarked explicitly against pyarrow for this one task |
| PyCapsule Interface | Supported (producer side well-established) | Supported since v1.3+ (both directions) | Support both export and *accept* of foreign PyCapsule objects, so this library can be a drop-in intermediary between any two PyCapsule-compliant tools without requiring pyarrow at all |
| Parquet IO | Full-featured, mature, industry standard (arrow-rs/arrow-cpp based) — the de facto correctness bar to match | Full Parquet support as part of its query engine, generally very fast | Match pyarrow's core Parquet correctness (compression, row groups, stats, pushdown, projection) without matching its entire dataset/filesystem API surface — narrower but not weaker on the parts it does cover |
| Compute kernels | Extensive (`pyarrow.compute`) | Extensive — this is Polars' core identity | **Explicitly excluded.** Users needing compute stay on pandas/Polars/DuckDB; this library only moves data between them |
| Package size / import time | Large (historically 150-250MB combined with numpy; split packages exist on conda-forge specifically to mitigate this) | Large but positioned as "you get a full engine for that size" | Lean by design — the size/speed differential is the marketed advantage, not an afterthought |

## Sources

- [Pandas Integration — Apache Arrow v24.0.0](https://arrow.apache.org/docs/python/pandas.html) — official docs, zero-copy conditions (numeric/no-nulls/single-chunk), pandas_metadata round-trip
- [PyArrow Functionality — pandas 3.0.4 documentation](https://pandas.pydata.org/docs/user_guide/pyarrow.html) — official pandas docs on `dtype_backend="pyarrow"` and `ArrowDtype`
- [pandas.ArrowDtype — pandas 3.0.3 documentation](https://pandas.pydata.org/docs/reference/api/pandas.ArrowDtype.html)
- [Extending PyArrow — Apache Arrow v24.0.0](https://arrow.apache.org/docs/python/extending_types.html)
- [[Python] Non zero-copy of pa.table.to_pandas() for simple case · Issue #38644 · apache/arrow](https://github.com/apache/arrow/issues/38644)
- [Pandas Corrupting PyArrow Integer Object Nulls · Issue #23786 · pandas-dev/pandas](https://github.com/pandas-dev/pandas/issues/23786) — nullable int/float64 upcast-on-null behavior
- [ARROW-2135 — NaN silently casted to int64 — ASF Jira](https://issues.apache.org/jira/browse/ARROW-2135)
- [The Arrow PyCapsule Interface — Apache Arrow v24.0.0](https://arrow.apache.org/docs/format/CDataInterface/PyCapsuleInterface.html) — official protocol spec (`__arrow_c_schema__`, `__arrow_c_array__`, `__arrow_c_stream__`)
- [Arrow PyCapsule Interface support · Issue #12530 · pola-rs/polars](https://github.com/pola-rs/polars/issues/12530) — Polars adoption (v1.3+)
- [Feature Request: Support Arrow PyCapsule Interface & remove pyarrow dependency · duckdb/duckdb Discussion #10716](https://github.com/duckdb/duckdb/discussions/10716)
- [Arrow producer/consumer — Polars user guide](https://docs.pola.rs/user-guide/misc/arrow/)
- [Universal dataframe support with the Arrow PyCapsule Interface + Narwhals — Quansight Labs](https://labs.quansight.org/blog/narwhals-pycapsule) — ecosystem adoption breadth (pandas 2.2+ export-only, Ibis, etc.)
- [pyarrow.BooleanArray — Apache Arrow v24.0.0](https://arrow.apache.org/docs/python/generated/pyarrow.BooleanArray.html) — bit-packed boolean layout
- [numpy lacks memory and speed efficiency for Booleans · Issue #14821 · numpy/numpy](https://github.com/numpy/numpy/issues/14821)
- [Re: [DISCUSS] 8-bit Boolean Canonical Extension Type — Arrow dev mailing list](https://www.mail-archive.com/dev@arrow.apache.org/msg32278.html) — `bool8` extension type status (experimental/optional)
- [pyarrow.ChunkedArray — Apache Arrow v24.0.0](https://arrow.apache.org/docs/python/generated/pyarrow.ChunkedArray.html) — multi-chunk copy requirement, `combine_chunks()`
- [Understanding Predicate Pushdown at the Row-Group Level in Parquet with PyArrow and Python — Peter Hoffmann](https://peter-hoffmann.com/2020/understand-predicate-pushdown-on-rowgroup-level-in-parquet-with-pyarrow-and-python.html)
- [Tabular Datasets — Apache Arrow v24.0.0](https://arrow.apache.org/docs/python/dataset.html) — official docs on `pyarrow.dataset` pushdown/projection
- [Querying Parquet with Millisecond Latency — Apache Arrow blog](https://arrow.apache.org/blog/2022/12/26/querying-parquet-with-millisecond-latency/)
- [Snappy vs Zstd for Parquet in Pyarrow — Levi Sands](https://ldsands.github.io/blog/2019/12/17/snappy-vs-zstd-for-parquet-in-pyarrow/) — codec tradeoff data, corroborated by multiple independent sources
- [python - read multiple parquets that have different schema? · Issue #35569 · apache/arrow](https://github.com/apache/arrow/issues/35569) — pyarrow does not auto-merge schemas (no Spark-style `mergeSchema`)
- [pyarrow.parquet.ParquetDataset — Apache Arrow v24.0.0](https://arrow.apache.org/docs/python/generated/pyarrow.parquet.ParquetDataset.html)
- [Possible to read categoricals back into Pandas from Parquet using Pyarrow? · Issue #1688 · apache/arrow](https://github.com/apache/arrow/issues/1688)
- [[Python] Pyarrow table conversion from pandas fails for categorical fields with arrow dtypes · Issue #35259 · apache/arrow](https://github.com/apache/arrow/issues/35259) — documented pyarrow rough edge (differentiator opportunity)
- [Trimming down pyarrow's conda footprint (Part 1 of X) — Uwe Korn](https://uwekorn.com/2020/09/08/trimming-down-pyarrow-conda-1-of-x.html) and [Part 2](https://uwekorn.com/2020/10/28/trimming-down-pyarrow-conda-2-of-x.html) — pyarrow package size history, split-package mitigation on conda-forge
- [pyarrow · PyPI](https://pypi.org/project/pyarrow/) — current package footprint
- [What's new in 3.0.0 — pandas 3.0.4 documentation](https://pandas.pydata.org/docs/whatsnew/v3.0.0.html) — pandas 3.0 string dtype defaults
- [String dtype: known differences and performance considerations · Issue #63105 · pandas-dev/pandas](https://github.com/pandas-dev/pandas/issues/63105) — `string[pyarrow]` vs object/python storage performance
- Project context: `.planning/PROJECT.md` (Flint) — scope, constraints, and Core Value used to frame table stakes vs differentiators vs anti-features

---
*Feature research for: Rust-backed zero-copy pandas <-> Arrow bridge + Parquet IO library (pyarrow alternative)*
*Researched: 2026-07-13*
