---
phase: 01
slug: core-zero-copy-round-trip-interop
status: verified
# threats_open = count of OPEN threats at or above workflow.security_block_on severity (the blocking gate)
threats_open: 0
asvs_level: 1
created: 2026-07-15
---

# Phase 01 — Security

> Per-phase security contract: threat register, accepted risks, and audit trail.

Register built from `<threat_model>` blocks authored at plan time in all 5 plans (01-01 through 01-05) — `register_authored_at_plan_time: true`. Verified at grep-level (ASVS L1) directly against the implementation; short-circuited per workflow rule (`threats_open: 0 AND register_authored_at_plan_time: true AND asvs_level == 1`) — no deeper auditor pass required.

Note (01-05-PLAN.md): this is an in-process backend data-conversion library with no network surface, no auth/session, no user-supplied query strings — web-app threat categories (injection/session/CSRF) do not apply. Applicable risks are memory-safety and malformed-input handling at the pandas/Arrow FFI boundary, plus supply-chain (dependency) integrity.

---

## Trust Boundaries

| Boundary | Description | Data Crossing |
|----------|-------------|---------------|
| package registries (PyPI / crates.io) → dev environment | Toolchain, maturin, and dev/test dependencies downloaded and executed during scaffold/build | Build/test tooling, not shipped runtime deps |
| pandas/numpy memory → Rust core (via pyo3-arrow) | Data buffers cross the FFI boundary during `from_pandas`/`to_pandas` | User's own DataFrame data |
| Rust `Drop` ↔ Python refcount | Releasing a borrowed buffer's owner touches Python refcounts, possibly off the main thread | Python object refcount state |
| foreign library object → flint `from_arrow` (CAP-02) | An arbitrary caller-supplied object claiming Arrow PyCapsule compliance crosses into Rust and is dereferenced | Foreign Arrow C Data Interface structs |
| flint `Table` export → external consumers | flint hands a capsule to pyarrow/Polars/DuckDB | Exported Arrow buffers |
| pandas column → Arrow C stream → Rust core | `import_column_via_pandas_stream` consumes a column's `__arrow_c_stream__` export, which may yield any number of RecordBatches | User's own DataFrame data (multi-chunk case) |
| test harness → the zero-copy claim | These tests ARE the trust boundary for the project's core credibility claim | N/A — correctness/credibility boundary |

---

## Threat Register

| Threat ID | Category | Component | Severity | Disposition | Mitigation | Status |
|-----------|----------|-----------|----------|-------------|------------|--------|
| T-01-SC (P01) | Tampering | Supply chain — dev/test dependency installs | high | mitigate | RESEARCH.md Package Legitimacy Audit cleared every package via primary-source evidence (`pip index versions`/crates.io); all Python deps are dev/test-only, none is a runtime dependency of the shipped `flint` wheel. | closed |
| T-01-01 | Tampering | `Table.from_pandas` composing `pyo3_arrow::PyTable` | medium | mitigate | Confirmed: `table.rs` composes `pyo3_arrow::PyTable` as an internal field, delegates PyCapsule dunders to it, no hand-rolled FFI or unchecked `unsafe` shortcut (grep confirms doc comments + `use pyo3_arrow::PyTable`, no bypass). | closed |
| T-01-02 | Denial of Service | `maturin develop` build step | low | accept | Build failures surface as non-zero exit; no untrusted external input drives the build beyond the audited dependency set (T-01-SC). | closed |
| T-01-03 | Tampering | `from_pandas` numpy buffer borrow — contiguity/offset check | high | mitigate | Confirmed: `plan_column` (`crates/flint-core/src/pandas_plan.rs:64-71`) routes non-contiguous numpy buffers to `RequiresCopy`, never borrows as contiguous; unit tests `plan_column_contiguous_numpy_numeric_is_zero_copy_borrow` / `plan_column_non_contiguous_numpy_numeric_requires_copy` pass. | closed |
| T-01-04 | Tampering (memory corruption) / DoS | Rust `Drop` on borrowed pandas/numpy buffer owner | high | mitigate | Confirmed: `NumpyBufferOwner<T>(Py<PyArray1<T>>)` (`pandas.rs:221-234`) relies on PyO3's own `Py<T>` `Drop` impl, which reacquires the GIL as needed before decrementing refcount — no custom `unsafe` `Drop`. | closed |
| T-01-05 | Repudiation | strict mode vs `copy_report` disagreeing with each other | medium | mitigate | Confirmed: both consume the same `plan_column`-derived record; `test_copy_report_agrees_with_strict_mode_rejection_per_column` passes. (Scope note: this threat is about mutual consistency between the two APIs, not about either being truthful relative to actual copy behavior on multi-chunk input — that separate, narrower gap is tracked and consciously deferred via the `01-VERIFICATION.md` override, not a threat-register regression.) | closed |
| T-01-06 (P03) | Repudiation (false certification) | `tests/rust/zero_copy_alloc.rs` allocation proof | high | mitigate | Confirmed: measured closure guarded with `std::hint::black_box` (lines 77, 102); sanity-check test `deliberately_copying_path_is_detected_by_the_allocation_counter` passes. | closed |
| T-01-07 (P03) | Repudiation (false certification) | `tests/python/test_zero_copy_pointer.py` reverse direction | medium | mitigate | Confirmed: targets the mechanism confirmed in 01-02-SUMMARY.md (not an assumed direct-borrow); passes. | closed |
| T-01-08 (P04) | Tampering / DoS | `from_arrow` import of untrusted foreign capsule (`import.rs`) | high | mitigate | Confirmed: delegates marshalling to pyo3-arrow's validated `FromPyObject` path, no unchecked `unsafe` shortcut added (grep: doc comment + no bypass); errors surface as `flint.FlintError` via `PyFlintError`, never a panic — `test_from_arrow_rejects_object_without_pycapsule_protocol` / `test_from_arrow_rejects_broken_stream_dunder_without_panicking` pass. | closed |
| T-01-09 (P04) | Denial of Service | Consuming a foreign `__arrow_c_stream__` more than once | medium | mitigate | Confirmed: `test_from_arrow_consumes_foreign_stream_dunder_exactly_once` passes. | closed |
| T-01-10 (P04) | Repudiation | DuckDB interop silently skipped due to unconfirmed native consumption | medium | mitigate | Confirmed: empirical spike (`_probe_duckdb_native_consumption`) records native-vs-pyarrow-intermediary path; `DUCKDB_NATIVE_CONSUMPTION == True` confirmed, never silently skipped. | closed |
| T-01-SC (P04) | Tampering | pyarrow/polars/duckdb as test dependencies | high | transfer | Dev/test-only consumers, already cleared in the P01 Package Legitimacy Audit; interop correctness relative to their own protocol behavior is upstream's responsibility. | closed |
| T-01-06 (P05) | Tampering (silent data corruption) | `import_column_via_pandas_stream` multi-batch handling — CR-01 | high | mitigate | Confirmed genuinely fixed and independently re-verified (01-VERIFICATION.md re-verification cycle): every batch is now accounted for (`arrow::compute::concat` for >1 batch); regression test `test_from_pandas_preserves_all_rows_of_multi_chunk_arrow_backed_column` passes; fresh manual reproduction (6 rows in, 6 rows out) confirmed. | closed |
| T-01-07 (P05) | Denial of Service | Concatenating a stream of many small batches into one array | low | accept | Bounded by source data the caller already holds in memory, not attacker-amplified; matches pyarrow's own combine/concat behavior. No artificial batch-count cap added. | closed |
| T-01-08 (P05) | Tampering (memory safety) | `unsafe` numpy-borrow path adjacency | low | accept | Confirmed: CR-01 fix diff (`7d0bc52`) adds zero `unsafe` blocks — `arrow::compute::concat` is safe Rust; existing `unsafe` buffer-borrow surface (`Buffer::from_custom_allocation`) untouched. | closed |
| T-01-SC (P05) | Tampering | Package installs for the CR-01 fix | n/a | accept | No new crate, no new Python dependency, no Cargo feature added — `arrow::compute::concat` ships in the already-pinned `arrow` crate's non-optional `arrow-select` component. | closed |

*Status: open · closed · open — below {block_on} threshold (non-blocking)*
*Severity: critical > high > medium > low — only open threats at or above workflow.security_block_on (currently: high) count toward threats_open*
*Disposition: mitigate (implementation required) · accept (documented risk) · transfer (third-party)*

---

## Accepted Risks Log

| Risk ID | Threat Ref | Rationale | Accepted By | Date |
|---------|------------|-----------|--------------|------|
| R-01-01 | T-01-02 | Build failures on an audited dependency set are non-blocking DoS at worst; no untrusted external input drives the build. | Claude (gsd-security-auditor, plan-time) | 2026-07-13 |
| R-01-02 | T-01-07 (P05) | Multi-batch concat cost is bounded by caller-supplied data already in memory, not attacker-amplified. | Claude (gsd-security-auditor, plan-time) | 2026-07-14 |
| R-01-03 | T-01-08 (P05) | CR-01 fix adds no new `unsafe` code; existing unsafe surface unmodified. | Claude (gsd-security-auditor, plan-time) | 2026-07-14 |
| R-01-04 | T-01-SC (P05) | No new dependency introduced by the CR-01 fix. | Claude (gsd-security-auditor, plan-time) | 2026-07-14 |
| R-01-05 | T-01-SC (P04) | pyarrow/Polars/DuckDB are dev/test-only interop targets, already cleared in the P01 Package Legitimacy Audit; their own protocol correctness is upstream's responsibility. | Claude (gsd-security-auditor, plan-time) | 2026-07-14 |

*Accepted risks do not resurface in future audit runs.*

---

## Security Audit Trail

| Audit Date | Threats Total | Closed | Open | Run By |
|------------|---------------|--------|------|--------|
| 2026-07-15 | 16 | 16 | 0 | Claude (gsd-secure-phase, grep-level ASVS L1, short-circuited per `threats_open: 0 AND register_authored_at_plan_time: true AND asvs_level == 1`) |

---

## Sign-Off

- [x] All threats have a disposition (mitigate / accept / transfer)
- [x] Accepted risks documented in Accepted Risks Log
- [x] `threats_open: 0` confirmed
- [x] `status: verified` set in frontmatter

**Approval:** verified 2026-07-15
