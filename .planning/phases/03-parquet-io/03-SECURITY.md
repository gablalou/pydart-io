---
phase: 03
slug: parquet-io
status: verified
# threats_open = count of OPEN threats at or above workflow.security_block_on severity (the blocking gate)
threats_open: 0
asvs_level: 1
created: 2026-07-24
---

# Phase 03 — Security

> Per-phase security contract: threat register, accepted risks, and audit trail.

---

## Trust Boundaries

| Boundary | Description | Data Crossing |
|----------|-------------|---------------|
| caller path arg -> from_parquet/to_parquet | Untrusted str/Path values cross into filesystem open/create calls. | File paths |
| Parquet file bytes -> read_parquet | A file at a caller-supplied path is untrusted input (may be corrupt/malformed/attacker-controlled). | Binary file contents |
| caller compression string -> build_writer_properties | Untrusted codec string drives an enum selection; an unhandled value must fail closed, not default. | Codec selector string |
| caller filter tuples / column list -> from_parquet | Untrusted operator strings and column names drive predicate construction and projection. | Filter/column specifications |
| Parquet row-group statistics -> surviving_row_groups | Statistics read from the (untrusted) file drive skip decisions; a wrong comparison silently drops matching rows. | Row-group min/max statistics |
| pandas source dtype nullability -> build_field | A column's declared nullability drives the emitted Arrow/Parquet schema; deriving it wrongly corrupts downstream schema merges. | Dtype nullability metadata |
| directory / file-list path args -> from_parquet | Untrusted directory contents and path lists drive which files are opened and concatenated. | Directory listings, path lists |
| Parquet file bytes (multiple files) -> read_parquet | Each file is untrusted; a corrupt or schema-divergent file must fail loud, not silently merge/skip. | Binary file contents (multi-file) |
| arrow-rs ArrowWriter dictionary encoding -> Parquet output | Internal writer behavior (not caller-controlled) reassigns dictionary keys; a semantic property (category order) can silently change. | Categorical/dictionary column data |

---

## Threat Register

| Threat ID | Category | Component | Severity | Disposition | Mitigation | Status |
|-----------|----------|-----------|----------|-------------|------------|--------|
| T-03-01 | Denial of Service / Tampering | `read_parquet` metadata/footer parse | high | mitigate | Every parquet-crate `Result` is `?`-propagated into `FlintError`; no `.unwrap()`/`.expect()` on any parse result from untrusted file bytes — verifier scan of `parquet_io.rs` confirmed zero such calls (the two `.expect()` present are on a length-checked-above invariant, not a parse `Result`). A corrupt footer surfaces as a caught error, never a panic. | closed |
| T-03-SC | Tampering | cargo package install (`parquet` crate) | high | accept | Only one new dependency (`parquet` 59.1.0, same apache/arrow-rs monorepo as the already-trusted `arrow` crate); RESEARCH package-legitimacy audit returned verdict OK. No further new dependency across Plans 02-04. | closed |
| T-03-03 | Tampering (silent wrong behavior) | `build_writer_properties` codec match | medium | mitigate | Exhaustive 4-arm match with explicit `FlintError::UnsupportedCodec` error arm — an unrecognized codec string cannot silently fall through to a default. Verified by `tests/python/test_parquet_compression.py` (13 tests, codec-rejection case included). | closed |
| T-03-04 | Tampering (silent wrong results) | `surviving_row_groups`/`could_match_range` row-group pruning | high | mitigate | Conservative pruning (skip only when provably no match; keep on any doubt). Independently verified by `tests/rust/parquet_row_group_pruning.rs` (isolated skip-engagement proof against a real 3-row-group file) and `tests/python/test_parquet_pushdown.py`'s 36-case six-operator boundary property test against an unfiltered-then-pandas-filter baseline. The related CR-01 code-review finding (row-level filter cast silently dropping matching rows for out-of-range integer literals) was fixed (`df26820`) and independently re-verified live by the phase verifier with an `int8[pyarrow]` probe. | closed |
| T-03-05 | Tampering (silent input drop) | operator-string parsing at PyO3 boundary | medium | mitigate | Unrecognized operator strings raise `FlintError::UnsupportedFilterOperator` (exhaustive match, no ignored tuple). Verified by `test_parquet_pushdown.py::test_unknown_operator_raises`. | closed |
| T-03-06 | Tampering (silent schema corruption) | `build_field` nullability (WR-01) | high | mitigate | Nullability sourced from the declared source dtype schema, not observed `null_count` — prevents a wrongly-non-nullable field silently breaking downstream concat/merge. Verified by `tests/python/test_wr01_nullability.py` and an explicit `concat_tables` reproduction test. | closed |
| T-03-07 | Tampering (unintended data inclusion) | directory read file selection (D-21) | medium | mitigate | Directory reads filter strictly to `.parquet`, sorted deterministically. Code review (WR-02) found the initial implementation silently swallowed `read_dir` per-entry errors and didn't filter non-file entries via `filter_map(entry.ok())`; fixed (`07c251e`) to surface errors and filter to files only. | closed |
| T-03-08 | Tampering (silent wrong merge) | cross-file schema reconciliation (D-21) | medium | mitigate | Strict schema-equality across files; first mismatch raises `ParquetSchemaMismatch` naming file + column. Code review (WR-04) found `fields_match` omitted `dict_is_ordered`, permitting an ordered-vs-unordered dictionary mismatch to bypass detection; fixed (`74047ea`). | closed |
| T-03-02 | Information Disclosure | caller-supplied path (traversal) | low | accept | Out of scope per D-20 — same trust level as Python's own `open()`/pyarrow `write_table` (no path-sandboxing). Documented caller responsibility, not a Flint-owned mitigation. | closed |
| T-03-09 | Tampering (silent semantic change) | arrow-rs `DictEncoder` category reordering on Parquet write (ordered categoricals) | medium | accept | No `WriterProperties` mitigation exists in parquet 59.1.0 given the project's arrow-rs-only constraint; pyarrow does not share this limitation (confirmed independently). User-approved at an execution-time checkpoint (2026-07-24). Regression-pinned by `test_ordered_categorical_category_order_not_guaranteed_known_gap`. Cosmetic for unordered categoricals; real `<`/sort-order risk disclosed for ordered categoricals — to be surfaced in Phase 4 release/benchmark docs if categorical fidelity is presented as a headline interop claim. | closed |

*Status: open · closed · open — below high threshold (non-blocking)*
*Severity: critical > high > medium > low — only open threats at or above workflow.security_block_on (high) count toward threats_open*
*Disposition: mitigate (implementation required) · accept (documented risk) · transfer (third-party)*

---

## Accepted Risks Log

| Risk ID | Threat Ref | Rationale | Accepted By | Date |
|---------|------------|-----------|-------------|------|
| AR-03-01 | T-03-SC | Single new dependency (`parquet` 59.1.0) from the already-trusted apache/arrow-rs monorepo; audited OK at Plan 01 research time. | Phase plan authors (03-01..03-04-PLAN.md) | 2026-07-23 |
| AR-03-02 | T-03-02 | Path traversal via caller-supplied file paths is out of scope per D-20 — Flint offers the same trust level as Python's own `open()`; not a library-owned mitigation surface. | Phase plan authors (03-01, 03-04-PLAN.md) | 2026-07-23 |
| AR-03-03 | T-03-09 | arrow-rs `ArrowWriter`/`DictEncoder` reassigns dictionary keys during Parquet encoding with no available `WriterProperties` fix in parquet 59.1.0; confirmed independent of Flint's own code via a pure arrow-rs/parquet probe, and confirmed pyarrow does not share the limitation. Accepted at an execution-time checkpoint rather than pursuing a hand-rolled Parquet column-writer (against CLAUDE.md's "Don't Hand-Roll" guidance). | User (execution-time checkpoint) | 2026-07-24 |

*Accepted risks do not resurface in future audit runs.*

---

## Security Audit Trail

| Audit Date | Threats Total | Closed | Open | Run By |
|------------|---------------|--------|------|--------|
| 2026-07-24 | 10 | 10 | 0 | Claude (gsd-secure-phase, L1 grep-depth, ASVS level 1, register authored at plan time) |

---

## Sign-Off

- [x] All threats have a disposition (mitigate / accept / transfer)
- [x] Accepted risks documented in Accepted Risks Log
- [x] `threats_open: 0` confirmed
- [x] `status: verified` set in frontmatter

**Approval:** verified 2026-07-24
