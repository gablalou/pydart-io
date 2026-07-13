# Pitfalls Research

**Domain:** Rust-backed Python library for zero-copy pandas <-> Arrow Table conversion + Parquet IO (pyarrow alternative, not a query engine)
**Researched:** 2026-07-13
**Confidence:** HIGH for FFI memory-safety, packaging/ABI, and API-design findings (official PyO3/Arrow/NumPy docs + GitHub issue trackers, cross-checked against a comparable real-world project, `arro3`/`pyo3-arrow`). MEDIUM for benchmarking-methodology specifics (task-derived, not found in a single authoritative post-mortem — treat as informed best practice, not verified case study).

## Critical Pitfalls

### Pitfall 1: Buffer lifetime mismatch between Rust ownership and Python's reference-counted GC

**What goes wrong:**
A Rust-owned buffer is exposed to Python (e.g. as a NumPy array or Arrow buffer) without anything keeping the Rust allocation alive for as long as Python holds a reference to it. Python's refcounting/GC has no visibility into Rust's ownership graph, so it can decide the buffer is unreferenced and drop it while a NumPy view or another Python object still points at the same memory — a classic use-after-free that manifests as segfaults or silently corrupted data, often only under GC pressure or in long-running processes (so it doesn't show up in dev testing).

**Why it happens:**
Rust's ownership model assumes a single, statically-checked owner; Python's model assumes anything reachable from a live reference stays alive via refcounting. When you export a Rust `Vec<u8>`/`Buffer` as a Python-visible buffer (via the buffer protocol or a raw pointer wrapped in a capsule), you must manually bridge the two models — e.g. wrap the buffer in a `PyCapsule` or attach it as `base`/owner of the NumPy array. Skipping this (or getting the capsule destructor wrong) is the single most common mistake in Rust/Python zero-copy bridges.

**How to avoid:**
- Always attach the owning Rust allocation to the Python-visible object via a capsule/`base` reference (e.g. `numpy::PyArray::from_owned_array` patterns, or a `PyCapsule` whose destructor drops the Rust `Arc`) so refcounting on the Python side keeps the Rust memory alive.
- Prefer reference-counted Rust buffers (`Arc<Buffer>`, matching `arrow-rs`'s own `Buffer` type, which is already `Arc`-backed) over raw pointers with manual lifetime bookkeeping.
- Never expose a pointer into a buffer that could be reallocated/dropped from Rust-side code paths while a Python reference to it might still be live (e.g. resizing a `Vec` in place while a slice view is exported).
- Use PyO3's `PyBuffer<T>` (RAII, lifetime-bound to the Python object) rather than hand-rolled buffer exposure when implementing the buffer protocol.

**Warning signs:**
- Segfaults or corrupted values that only appear after many iterations, under `gc.collect()`, or under memory pressure — not on first call.
- Any code path storing a raw pointer/length pair on the Rust side and separately handing a NumPy array/memoryview to Python without an explicit owner object.
- Miri/ASan failures during Rust unit tests that don't reproduce in normal Python usage (indicates the unsafe boundary itself is unsound, independent of Python's GC).

**Phase to address:**
FFI/core-bridge implementation phase (the phase that builds the pandas<->Arrow conversion core) — this must be solved before any other feature is layered on top, since it's a foundational memory-safety property, not a later polish item.

---

### Pitfall 2: GIL deadlocks and re-entrancy hazards when releasing/reacquiring across threads

**What goes wrong:**
Code that does long-running Rust work while holding the GIL blocks all Python threads (killing the "faster than pyarrow" pitch for multi-threaded workloads). Conversely, code that releases the GIL (`Python::allow_threads`/`detach`) and then needs to call back into Python (e.g. to raise an exception, access a `PyObject`, or initialize a `OnceLock` that itself touches the interpreter) can deadlock if two threads end up waiting on each other — one holding the GIL waiting on a Rust-side lock, another holding the Rust-side lock waiting to reacquire the GIL.

**Why it happens:**
PyO3's GIL guards must be acquired/released in strict LIFO order; violating this can force an unrecoverable panic. Lazy-initialization primitives that internally call into Python (e.g. `import`) while under a `OnceLock` can create a circular wait between "waiting for GIL" and "waiting for the lock." This is subtle because it only manifests under concurrency (multi-threaded conversion, Parquet reads from multiple threads, etc.), which is exactly the scenario this project is being built to be fast at.

**How to avoid:**
- Wrap any CPU-bound Rust conversion/Parquet work (the whole point of using Rust here) in `Python::allow_threads` / `detach` so the GIL is released for the duration, and reacquire it only at the boundary where Python objects are touched again.
- Never hold a Rust-side mutex across a point where the GIL might be reacquired by another thread; if a Rust lock and the GIL are both needed, always acquire the GIL first, Rust lock second, and release in reverse order.
- Use PyO3's `PyOnceLock` for any lazy static that touches the Python API, instead of `std::sync::OnceLock`, since it is deadlock-aware.
- Add a concurrency test (multiple threads calling the conversion function simultaneously, including one that triggers a Python exception mid-conversion) to CI — this class of bug does not show up in single-threaded tests.

**Warning signs:**
- Hangs (not crashes) under concurrent load that don't reproduce single-threaded.
- Any `unsafe`/lazy-static code that calls `Python::import` or similar from inside a lock's initializer.
- Benchmarks showing no speedup (or a slowdown) under multi-threaded Python usage — often a sign the GIL is being held the whole time rather than released.

**Phase to address:**
Core FFI/bridge phase for the release-GIL discipline; a dedicated concurrency-stress-test task (can be part of the same phase's verification step, or a follow-up hardening phase before v1 release) to catch deadlocks before they ship.

---

### Pitfall 3: "Zero-copy" claims that silently copy — the object dtype / string column trap

**What goes wrong:**
The library advertises zero-copy pandas<->Arrow conversion, but the moment a DataFrame has an `object`-dtype column (which is pandas' default for strings, mixed types, or anything non-primitive), true zero-copy is structurally impossible — pandas stores a NumPy array of PyObject pointers, which has no Arrow-compatible physical layout at all. Any conversion path that "supports" object columns is doing a hidden copy (and often a slow one, since it has to introspect each Python object). If this isn't surfaced to the user, benchmark and correctness claims quietly become false for the most common pandas column type in practice.

**Why it happens:**
Developers built and benchmarked the happy path (numeric columns, no nulls) and only later discover that real-world pandas DataFrames are full of `object` columns (strings, categoricals loaded as strings, mixed nulls). Zero-copy is only physically possible for a narrow set of cases: numeric/float/timestamp types stored in one contiguous buffer, with no nulls (Arrow uses a separate null bitmap that pandas' legacy block manager doesn't have an equivalent for) — everything else needs at least one buffer materialization.

**How to avoid:**
- Explicitly enumerate and document exactly which pandas dtypes achieve zero-copy (numeric/float/bool without nulls; and separately, `ArrowDtype`-backed columns which are already Arrow-backed and can be truly zero-copy in both directions) versus which always copy (`object` dtype, columns with nulls in a nullable-incompatible layout, categoricals in some representations).
- Provide a real API (not just documentation) to detect the zero-copy-eligible case, mirroring the lesson from pyarrow's own `zero_copy_only=True` flag — but learn from pyarrow's mistake (Pitfall 5 below): make the check strict enough that "zero-copy succeeded" is actually true, and cheap enough to call before doing real work.
- In benchmarks and README claims, always state "zero-copy for numeric/non-null/Arrow-backed columns; falls back to an explicit, measured copy otherwise" rather than a blanket "zero-copy" claim.
- Recommend (and make it easy for) users to adopt pandas' `ArrowDtype`-backed columns end-to-end (`pd.ArrowDtype`) for the strings/timestamps case, since that's the one path where pandas' physical layout is already Arrow's layout.

**Warning signs:**
- Any conversion function that accepts arbitrary DataFrames without dtype introspection and always returns success — that's a sign a copy is happening silently.
- Benchmark suite that only tests numeric columns (see Pitfall 6) — masks the fact that the common case (mixed-type real-world data) doesn't get the advertised speedup.
- Memory profiling showing 2x peak RSS during conversion (both pandas and Arrow copies alive simultaneously) — the classic worst case even pyarrow's own docs call out.

**Phase to address:**
Core conversion phase — dtype-eligibility detection must be a first-class, tested code path, not an afterthought. Benchmark suite phase must include object/string columns explicitly (not just as a "does it work" test but as a "here is the honest, measured cost" test).

---

### Pitfall 4: Misaligned buffers and endianness assumptions silently breaking zero-copy or portability

**What goes wrong:**
Two related but distinct failures: (1) the Arrow C Data Interface only *recommends* 64-byte buffer alignment (matching AVX-512 SIMD width) — it does not require it, and compliant consumers must tolerate arbitrary alignment. A NumPy array that has been sliced (nonzero byte offset) or otherwise isn't aligned the way your Rust code assumes can force a realigning copy that quietly defeats zero-copy, or worse, cause unaligned-access UB if Rust code assumes alignment it doesn't verify. (2) Arrow's canonical in-memory layout is little-endian; if this library (or a future cross-machine/Parquet-interchange feature) ever encounters big-endian source data, "zero-copy" is not achievable without a byte-swap, which is itself a copy — treating "zero-copy" as architecture-independent is incorrect.

**Why it happens:**
Developers test on their own (little-endian x86/ARM) dev machine with freshly allocated, naturally-aligned arrays, and never exercise the edge cases: a pandas Series produced via slicing/`.iloc`/`.copy(deep=False)` (which can have a non-zero offset or non-default strides), or a byte-order edge case that basically never comes up on commodity hardware today but is part of the Arrow spec.

**How to avoid:**
- Never assume incoming NumPy/pandas buffers are 64-byte aligned or zero-offset; check alignment/offset explicitly in the Rust FFI layer and take the (documented, measured) copy path when it doesn't hold, rather than relying on assumed alignment for `unsafe` SIMD or pointer-cast code.
- Validate `dtype.byteorder` (or Arrow schema endianness field) on ingestion and explicitly copy-and-swap for the (rare) non-native-endian case rather than silently misinterpreting bytes.
- Add a property-based/fuzz test that constructs sliced, strided, and offset pandas Series/arrays and asserts conversion correctness (not just the happy-path contiguous array).

**Warning signs:**
- Correctness bugs that only show up with sliced/viewed DataFrames (e.g. `df.iloc[10:]`) rather than freshly loaded ones.
- Any `unsafe` Rust code casting a raw pointer to a SIMD-aligned type without an explicit alignment check first.
- No test in the suite that passes a non-contiguous or offset array through the conversion path.

**Phase to address:**
Core FFI/bridge phase — alignment/offset/endianness handling belongs in the same hardening pass as Pitfall 1 (buffer lifetime), since both are "unsafe boundary correctness" concerns that are easy to get right on the happy path and wrong on realistic inputs.

---

### Pitfall 5: API design that lets a strict "zero-copy or fail" mode be effectively useless, or silently falls back to a copy with no signal

**What goes wrong:**
pyarrow's own `zero_copy_only=True` flag on `Table.to_pandas()` is a well-documented cautionary tale: multiple long-standing GitHub issues show it failing even in cases that should intuitively be zero-copy-eligible (e.g. `to_pandas(zero_copy_only=True)` essentially never succeeds for chunked/nullable data), which makes the flag nearly useless for its stated purpose. Meanwhile the *default* (non-strict) path silently copies whenever it needs to, with no signal to the caller that their "zero-copy" expectation wasn't met — users only discover this via memory profiling, not via the API itself.

**Why it happens:**
It's easy to bolt a boolean flag onto an existing conversion function without actually restructuring the implementation to make the zero-copy-eligible path first-class and independently testable. The strict-mode check ends up being conservative/buggy (rejects cases that actually could be zero-copy) while the default path has no observability into whether it took the cheap or expensive branch.

**How to avoid:**
- Design the zero-copy/copy distinction as a first-class return-time signal from day one (e.g. return an explicit "was this zero-copy?" indicator, or emit a debug-level log/counter), not a strict-mode flag bolted on later.
- Make the "would this be zero-copy" check match reality exactly — test it against every dtype/null/chunking combination you claim to support before shipping the flag, learning from pyarrow's issue tracker rather than repeating the same gap.
- Keep the library's API vocabulary close to pyarrow's where there's no reason to diverge (e.g. similar method names/semantics for `to_pandas`/`from_pandas`-equivalent conversions) so pyarrow users can adopt this library with minimal relearning — deviating from established conventions without a clear reason is an adoption tax, not a differentiator.
- Provide a clear, actionable error message when a copy is unavoidable (name the specific column and dtype that forced it), rather than either silently succeeding-with-copy or a generic "cannot convert" error.

**Warning signs:**
- A strict "zero-copy required" mode that fails on inputs you internally know are zero-copy-eligible — a sign the check logic doesn't match the actual conversion code path.
- User bug reports of the form "why is this fast/why is this using 2x memory" — a sign there's no visibility into which path was taken.
- API method names/parameter conventions that differ from pyarrow's for no principled reason, creating friction for the target audience (pyarrow users evaluating this as an alternative).

**Phase to address:**
Core conversion API design phase (early, before the API surface is locked in) — this is much cheaper to get right before v1 ships than to redesign after adoption starts, per pyarrow's own multi-year-open issues on this exact topic.

---

### Pitfall 6: Benchmark claims that don't survive scrutiny ("faster than pyarrow" on cherry-picked cases)

**What goes wrong:**
A benchmark suite that only measures the narrow case where zero-copy is actually achievable (numeric, non-null, single-chunk, already-Arrow-backed data) will show a dramatic, true-but-misleading speedup, because that's precisely the case pyarrow itself already handles efficiently or near-optimally — real differentiation claims need to also cover the cases that actually happen in practice (object/string columns, chunked/multi-batch tables, data with nulls), where the story is more nuanced. Separately, wall-clock-only benchmarks that ignore peak memory (RSS) hide the "2x memory during conversion" failure mode entirely, which matters as much as speed for a library whose core value proposition is reduced overhead.

**Why it happens:**
It's tempting to run the benchmark on the same data the feature was developed and tested against (numeric, in-memory, freshly generated), and to report the biggest available speedup number, especially before an external audience is scrutinizing the claim. Additionally, Python benchmarking has cold-start effects (import time, first-call JIT/compilation-like effects in PyO3/Rust codegen are less of an issue than in general Python since Rust is AOT-compiled, but allocator warm-up, page-fault costs on first touch of large buffers, and OS file-cache state for Parquet reads are all real confounders) that make a single untuned timing loop unreliable.

**How to avoid:**
- Benchmark across a matrix of realistic shapes: numeric-only, mixed numeric+string, high column count, high row count, with and without nulls, and both single- and multi-chunk/multi-batch inputs — report each cell of the matrix rather than one headline number.
- Report both throughput (time) and peak memory (RSS delta) for every benchmark, especially for the "how much does this cost when zero-copy isn't possible" cases — that's the honest place to differentiate on measured overhead, not just cherry-picked speed.
- Use a proper benchmarking harness with warmup iterations and statistical repeats (e.g. Rust's `criterion` for the Rust-side kernels, `pytest-benchmark` or repeated-trial timing for the Python-facing API) rather than single-shot `time.time()` measurements, and control for disk/OS page cache state explicitly for the Parquet IO benchmarks (cold read vs warm read numbers are both meaningful but must be reported as different things).
- Publish the benchmark methodology and raw data alongside the "faster than pyarrow" claim so it survives community scrutiny (this domain's audience — data engineers evaluating a pyarrow alternative — will re-run benchmarks and call out cherry-picking quickly).

**Warning signs:**
- A benchmark suite with only one or two data shapes, all numeric, all non-null.
- Headline claims with no reported memory numbers, only timing.
- Benchmarks that don't distinguish cold-cache vs warm-cache Parquet reads.

**Phase to address:**
Dedicated benchmark-suite phase (explicitly listed as an active requirement in PROJECT.md) — this phase's own success criteria should require the realistic-shape matrix and memory reporting, not just a speed number, since the project's stated reason to exist depends on this claim being defensible.

---

### Pitfall 7: Packaging/ABI failures that only surface for a subset of users' environments

**What goes wrong:**
Wheels built and tested locally work fine for the maintainer but silently fail (segfault, `undefined symbol`, or import error) for a subset of downstream users, because of ABI mismatches that don't manifest at build time: (a) NumPy's ABI is forward-compatible but not backward-compatible — a wheel built against NumPy 2.x works with NumPy 1.26, but a wheel built against NumPy 1.x can crash (a documented real case: a NumPy C-API slot going `NULL` under NumPy 2.0, causing segfaults) when the user has NumPy 2.x installed; (b) mixing a manylinux1-built pyarrow with a from-source package built against the C++11 (`cxx11`) ABI on the same system produces `undefined symbol` errors; (c) glibc/manylinux tag mismatches (Rust 1.64+ requires glibc >= 2.17, i.e. at minimum `manylinux2014`) can silently produce a wheel that PyPI accepts but which fails to import on older Linux distros if the manylinux tag doesn't actually match the compiled binary's requirements.

**Why it happens:**
CI usually builds and tests against one recent version of each dependency (latest NumPy, latest pyarrow) and the exact same OS/glibc as the runner, so the mismatch only appears in the field, for users on an older or a differently-built stack — a class of bug that's invisible until the user base is diverse enough to hit it.

**How to avoid:**
- Build wheels against the *oldest* supported NumPy ABI you want to support (NumPy's guidance: build against the lowest NumPy version you need to support, since the ABI is forward-compatible), not against whatever is newest at CI-run time.
- Use `maturin`'s built-in `auditwheel`-equivalent compliance checking and target `manylinux2014` (or newer, deliberately chosen, not whatever the default happens to produce) explicitly, and verify the resulting tag against the actual glibc symbol versions used.
- Test the built wheel against a matrix of pandas/pyarrow/numpy versions (oldest-supported and newest at time of release) in CI, not just "latest" — this project's core interop point is exactly the boundary where cross-version breakage happens.
- If linking against or interoperating with pyarrow's C++ objects at the ABI level (versus only via the Arrow C Data Interface, which is C-ABI and doesn't have this problem), be explicit about which pyarrow build (cxx11 vs old ABI) is required, or avoid C++-level linkage entirely and stick to the C Data Interface / PyCapsule protocol precisely to sidestep this class of bug.

**Warning signs:**
- Any user bug report that only reproduces on their machine/CI, not the maintainer's.
- CI matrix testing only "latest" versions of numpy/pandas/pyarrow rather than a supported range.
- Wheels built with the default/whatever manylinux tag `maturin` picks without an explicit, deliberate minimum-glibc decision.

**Phase to address:**
Packaging/distribution phase (should be its own phase or a clearly-scoped part of the release phase) — must include an explicit multi-version compatibility test matrix as an acceptance criterion, not just "wheel builds successfully."

---

## Technical Debt Patterns

| Shortcut | Immediate Benefit | Long-term Cost | When Acceptable |
|----------|-------------------|-----------------|------------------|
| Support only the numeric/non-null zero-copy path in v1, defer object/string dtype handling | Ships faster, benchmark story looks great | Most real DataFrames have string/object columns — early adopters hit the copy fallback immediately and the "faster than pyarrow" claim looks cherry-picked | Acceptable for v1 IF explicitly documented as a known limitation with a clear error message, not silently |
| Skip the multi-version pandas/pyarrow/numpy CI matrix, test only latest | Simpler CI, faster iteration during early development | Field failures (ABI segfaults, import errors) that are expensive to diagnose after release | Only acceptable pre-first-release; must be added before any public wheel is published |
| Use raw pointers across the FFI boundary instead of `Arc`/capsule-based ownership | Slightly less boilerplate initially | Use-after-free bugs that are extremely hard to reproduce/debug once shipped | Never acceptable for the core conversion path |
| Report only wall-clock speed in early benchmarks, add memory profiling later | Faster to get a "look how fast" number out | Credibility risk when a community member profiles memory and finds a 2x RSS spike the README didn't mention | Acceptable only for internal/dev-loop benchmarks, never for public claims |

## Integration Gotchas

| Integration | Common Mistake | Correct Approach |
|-------------|----------------|-------------------|
| pyarrow interop | Assuming any `pyarrow.Table`/`Array` can be zero-copy imported without checking chunking/null layout | Use the Arrow C Data Interface / PyCapsule protocol (`__arrow_c_array__`/`__arrow_c_stream__`) explicitly and validate structure before claiming zero-copy |
| pandas interop | Treating all pandas dtypes uniformly in the conversion path | Branch explicitly on dtype family (numpy-backed numeric vs `object` vs `ArrowDtype`-backed) with different, honestly-labeled code paths |
| Polars/DuckDB interop | Assuming this library's Arrow representation is automatically compatible with every consumer without testing against them | Test round-trips against actual Polars/DuckDB versions in CI, since "Arrow-compatible" in principle still has real-world edge cases (schema metadata, dictionary encoding) |
| Parquet IO | Assuming schema/dtype fidelity is automatic across write-then-read round trips | Explicitly test round-trip fidelity for nullable columns, nested types, and dictionary-encoded columns — these are where Parquet's and Arrow's representations diverge most |

## Performance Traps

| Trap | Symptoms | Prevention | When It Breaks |
|------|----------|------------|-----------------|
| Holding the GIL during large conversions/Parquet reads | No speedup under concurrent Python usage, or actively slower than single-threaded pyarrow under load | Release the GIL (`allow_threads`/`detach`) around all Rust-side heavy work | As soon as callers try multi-threaded usage, which is a realistic case for a "faster" claim |
| Treating every conversion as potentially zero-copy without a fast eligibility check | Overhead of introspecting a DataFrame outweighs savings for small DataFrames | Make the eligibility check itself cheap (dtype/null-count metadata lookup, not a data scan) | Noticeable at high call-frequency, low-row-count workloads (e.g. streaming small batches) |
| Ignoring peak memory during the "copy fallback" path | Users hit OOM on large DataFrames despite "zero-copy" branding | Document and, where possible, stream/chunk the fallback-copy path rather than materializing the whole DataFrame twice | Breaks at DataFrame sizes approaching available RAM, i.e. exactly the scale where users need this library most |

## Security Mistakes

| Mistake | Risk | Prevention |
|---------|------|------------|
| Trusting `unsafe` pointer arithmetic on buffer lengths/offsets received from Python (or a Parquet file) without bounds validation | Out-of-bounds read/write, memory corruption from a malformed/malicious Parquet file or crafted DataFrame | Validate all lengths/offsets against the actual buffer size before any `unsafe` pointer dereference; fuzz-test the Parquet reader against malformed files |
| Accepting arbitrary Arrow C Data Interface capsules from any Python object claiming to implement the protocol without validating the exported schema/array pointers | A malicious or buggy library could hand this library invalid pointers, causing a crash or worse | Validate capsule contents (null-checks, schema-consistency checks) before dereferencing, same discipline as parsing untrusted input |

## UX Pitfalls

| Pitfall | User Impact | Better Approach |
|---------|-------------|-------------------|
| Generic "conversion failed" error with no indication of which column/dtype caused it | User can't tell whether it's a real bug or an expected copy-required case, erodes trust in the library | Name the specific column and dtype in the error/warning, and distinguish "copied (informational)" from "failed (actionable)" |
| API method/parameter names that diverge from pyarrow's without explanation | pyarrow users (the target audience) have to relearn conventions for no clear benefit, raising the adoption bar | Mirror pyarrow's naming/semantics wherever there's no principled reason to differ; document deviations explicitly when they do exist |
| No visibility into whether a given call was zero-copy or a fallback copy | Users can't verify the core value proposition themselves, undermining the "faster than pyarrow" trust story | Provide an explicit signal (return metadata, debug log, or a `--verbose`/introspection API) reporting zero-copy vs copy per call |

## "Looks Done But Isn't" Checklist

- [ ] **Zero-copy pandas -> Arrow conversion:** Often only tested on numeric/non-null columns — verify against object/string dtype columns, nullable columns, and sliced/offset DataFrames explicitly.
- [ ] **Zero-copy Arrow -> pandas conversion:** Often missing null-bitmap handling — verify with columns containing nulls, not just fully-dense columns.
- [ ] **Parquet round-trip:** Often missing dictionary-encoding and nested-type (struct/list) fidelity checks — verify schema and values are identical after write-then-read for these cases, not just flat primitive columns.
- [ ] **Cross-platform wheels:** Often only tested on the CI runner's own OS/glibc/dependency versions — verify against the oldest supported NumPy/pandas/pyarrow combination and, ideally, an older Linux distro's glibc.
- [ ] **Benchmark suite:** Often only reports speed on the best-case data shape — verify memory (RSS) is reported alongside speed, and that mixed/object-dtype/nullable cases are included, not just the numeric happy path.
- [ ] **Concurrency safety:** Often only tested single-threaded — verify with a multi-threaded stress test that exercises GIL release/reacquire and exception paths concurrently.

## Recovery Strategies

| Pitfall | Recovery Cost | Recovery Steps |
|---------|----------------|-----------------|
| Use-after-free / buffer lifetime bug found post-release | HIGH | Requires auditing every `unsafe` FFI boundary for ownership transfer, likely a point release with a documented CVE-style advisory if it shipped; add Miri/ASan to CI going forward |
| GIL deadlock found post-release | MEDIUM | Usually isolatable to a specific code path (identify via thread dump / py-spy on a hung process); fix by correcting lock-acquisition order or moving to `PyOnceLock`; add a concurrency regression test |
| Benchmark claims publicly challenged as unfair/cherry-picked | MEDIUM | Publish an updated, methodology-transparent benchmark suite with the missing cases (memory, mixed dtypes); credibility recovery takes longer than the technical fix |
| ABI incompatibility reported by users on older stacks | LOW-MEDIUM | Rebuild wheels against the oldest supported NumPy/pyarrow version, expand the CI compatibility matrix, ship a patch release |
| API design mismatch with pyarrow conventions causing adoption friction | MEDIUM | Can require a breaking API change/deprecation cycle if discovered after users have integrated against it — cheaper to get right pre-v1 |

## Pitfall-to-Phase Mapping

| Pitfall | Prevention Phase | Verification |
|---------|-------------------|----------------|
| Buffer lifetime / use-after-free (Pitfall 1) | Core FFI/bridge phase | Miri/ASan on the Rust unsafe boundary; long-running/GC-pressure stress test in Python |
| GIL deadlocks (Pitfall 2) | Core FFI/bridge phase | Multi-threaded concurrency test hitting the conversion path from several threads simultaneously, including one triggering a Python exception mid-call |
| Silent copies on object/string columns (Pitfall 3) | Core conversion phase | Dtype-matrix test asserting zero-copy vs copy-with-signal for every supported pandas dtype, including object/string |
| Buffer alignment / endianness (Pitfall 4) | Core FFI/bridge phase | Fuzz/property test with sliced, strided, offset arrays; explicit byte-order handling test (can be a documented known-limitation if big-endian is out of scope, but must be stated, not silently wrong) |
| API design / silent-fallback signaling (Pitfall 5) | Core conversion API design phase (before v1 API freeze) | Review API surface against pyarrow's conventions; verify every copy-fallback path returns/logs a signal, not just success |
| Benchmark rigor (Pitfall 6) | Dedicated benchmark-suite phase | Benchmark matrix (numeric/mixed/nullable/chunked x speed/memory) reviewed before any public "faster than pyarrow" claim is published |
| Packaging/ABI (Pitfall 7) | Packaging/distribution phase | CI matrix across oldest-to-newest supported numpy/pandas/pyarrow versions and multiple manylinux targets; wheel import-smoke-test on each |

## Sources

- [PyO3 Memory management guide](https://pyo3.rs/v0.20.0/memory) — HIGH (official docs)
- [PyO3 Free-threading support guide](https://pyo3.rs/v0.28.3/free-threading) — HIGH (official docs)
- [PyO3 `Python::with_gil` deadlock discussion #3089](https://github.com/PyO3/pyo3/discussions/3089) — HIGH (maintainer discussion, primary source)
- [PyO3 FAQ & Troubleshooting](https://pyo3.rs/main/faq) — HIGH (official docs)
- [Apache Arrow: Pandas Integration docs](https://arrow.apache.org/docs/python/pandas.html) — HIGH (official docs)
- [Apache Arrow: The Arrow C data interface spec](https://arrow.apache.org/docs/format/CDataInterface.html) — HIGH (official spec)
- [Apache Arrow Columnar Format spec (buffer alignment rationale)](https://arrow.apache.org/docs/format/Columnar.html) — HIGH (official spec)
- [`pa.Table.to_pandas(zero_copy_only=True)` never succeeds, apache/arrow#39194](https://github.com/apache/arrow/issues/39194) — HIGH (primary-source bug report)
- [Non zero-copy of `pa.table.to_pandas()` for simple case, apache/arrow#38644](https://github.com/apache/arrow/issues/38644) — HIGH (primary-source bug report)
- [`to_numpy(zero_copy_only=True)` fails with Binary data, pola-rs/polars#12232](https://github.com/pola-rs/polars/issues/12232) — HIGH (primary-source bug report, cross-checked pattern)
- [NumPy: For downstream package authors (ABI guidance)](https://numpy.org/devdocs/dev/depending_on_numpy.html) — HIGH (official docs)
- [NumPy 2.0.0 Release Notes](https://numpy.org/devdocs/release/2.0.0-notes.html) — HIGH (official docs)
- [PyO3/rust-numpy: Support for Numpy 2, issue #409](https://github.com/PyO3/rust-numpy/issues/409) — HIGH (primary-source issue, slot-82 segfault case)
- [How we build Apache Arrow's manylinux wheels — Uwe Korn's blog](https://uwekorn.com/2019/09/15/how-we-build-apache-arrows-manylinux-wheels.html) — MEDIUM (maintainer blog, not official spec, but domain-authoritative author)
- [Maturin distribution guide](https://www.maturin.rs/distribution.html) — HIGH (official docs)
- [pypackaging-native: Depending on packages for which an ABI matters](https://pypackaging-native.github.io/key-issues/abi/) — MEDIUM (community-maintained but widely-cited reference)
- [arro3 GitHub repository / README (Kyle Barron)](https://github.com/kylebarron/arro3) — HIGH (directly comparable real-world project in this exact niche — minimal Rust-backed Arrow library for Python)
- [pyo3-arrow on crates.io](https://crates.io/crates/pyo3-arrow) — HIGH (official crate docs, direct analog for this project's FFI layer)
- [0-copy your PyArrow array to rust — Niklas Molin, Medium](https://medium.com/@niklas.molin/0-copy-you-pyarrow-array-to-rust-23b138cb5bf2) — MEDIUM (practitioner blog post)
- Benchmarking-methodology pitfalls (Pitfall 6) — MEDIUM confidence overall: derived from the task's own domain checklist plus general benchmarking best practice, not a single authoritative Arrow/pyarrow-specific post-mortem found during research. Treat the specific matrix/memory-reporting recommendations as informed best practice to apply, not a verified case study.

---
*Pitfalls research for: Rust+Python zero-copy Arrow/pandas interop library*
*Researched: 2026-07-13*
