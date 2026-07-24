---
status: testing
phase: 03-parquet-io
source: [03-VERIFICATION.md]
started: 2026-07-24T06:40:46Z
updated: 2026-07-24T06:40:46Z
---

## Current Test

number: 1
name: Write-interruption durability (process kill / disk full mid-to_parquet)
expected: |
  A partial/truncated Parquet file may be left on disk — `std::fs::File::create` truncates the
  target up front, and Flint provides no atomic-write/temp-then-rename guarantee. This is the
  disclosed, accepted design (all four plans' threat models treat this as caller responsibility),
  not a code defect. Confirm this matches your operational expectations before shipping to users
  who might rely on write atomicity.
awaiting: user response

## Tests

### 1. Write-interruption durability (process kill / disk full mid-to_parquet)
expected: |
  Kill the Python process (or simulate a full disk) partway through a `to_parquet()` call on a
  large `Table`, then inspect the target file. A partial/truncated Parquet file may be left on
  disk — no atomic-write/temp-then-rename guarantee exists. This is disclosed, accepted design,
  not a code defect.
result: [pending]

### 2. Concurrent-write races on the same path
expected: |
  Run two `to_parquet(same_path)` calls concurrently (two processes/threads) and confirm the
  resulting file matches only one writer's data (last-writer-wins, no corruption from an
  interleaved write). Undefined/OS-dependent outcome — Flint does not synchronize writers to the
  same path. Concurrent reads of a file that is not simultaneously being written are safe
  (verified structurally: no shared mutable state in the Parquet read path).
result: [pending]

## Summary

total: 2
passed: 0
issues: 0
pending: 2
skipped: 0
blocked: 0

## Gaps
