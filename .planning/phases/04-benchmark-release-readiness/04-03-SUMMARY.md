---
phase: 04-benchmark-release-readiness
plan: 03
subsystem: infra
tags: [github-actions, maturin, ci, wheels, packaging, uv]

# Dependency graph
requires:
  - phase: 04-01
    provides: pyproject.toml packaging config floor (pydart-io name, requires-python >=3.11, numpy range >=2.3,<2.6)
provides:
  - .github/workflows/wheels.yml (D-34 five-cell wheel build matrix, maturin-action, artifact upload)
  - .github/workflows/ci.yml (cargo test + maturin develop + pytest on push/PR)
  - .github/workflows/compat-matrix.yml (D-37 two-endpoint numpy/pandas compatibility matrix)
  - Public GitHub repo gablalou/pydart-io hosting the project and running these workflows
affects: [04-04 (release/publish plan will consume wheels.yml artifacts and add a PyPI publish job)]

# Tech tracking
tech-stack:
  added: [PyO3/maturin-action@v1, actions/upload-artifact@v4, GitHub Actions]
  patterns:
    - "wheels.yml declares (OS runner, target triple) as strategy.matrix.include cells rather than a cross-product matrix, so each cell can carry cell-specific settings (manylinux version, runner label) independently"
    - "ci.yml always runs `uv run maturin develop` immediately before `uv run pytest` -- bare `cargo test` never rebuilds the installed PyO3 extension, so Rust-side regressions would otherwise be invisible to the Python test suite (carried forward from the Phase 2 gate-tooling fix)"
    - "compat-matrix.yml uses a two-endpoint include list (oldest + newest), not a full version cross-product, per D-37/RESEARCH.md OQ3"

key-files:
  created:
    - .github/workflows/wheels.yml
    - .github/workflows/ci.yml
    - .github/workflows/compat-matrix.yml
  modified:
    - .github/workflows/wheels.yml (post-authoring fix: macos-13 -> macos-15-intel runner label)

key-decisions:
  - "GitHub Actions' macos-13 runner image was retired in Dec 2025 (confirmed via GitHub API job inspection showing the x86_64-apple-darwin job stuck queued for 1h13m with no runner ever assigned, and independently via GitHub's own changelog); fixed by switching the x86_64-apple-darwin cell to the current Intel replacement label macos-15-intel"
  - "Repo created public on GitHub (gablalou/pydart-io) specifically so the aarch64 Linux wheel-build cell gets the free native ubuntu-24.04-arm runner, avoiding a QEMU cross-compile fallback"

requirements-completed: [PKG-01, PKG-02, PKG-03]

coverage:
  - id: D1
    description: "Local host-platform wheel builds via maturin and installs cleanly via uv pip install into a fresh venv; import pydart succeeds"
    requirement: "PKG-03"
    verification:
      - kind: other
        ref: "uv run maturin build --release --out dist; uv pip install into fresh venv; python -c 'import pydart' (Task 1, executed locally)"
        status: pass
    human_judgment: false
  - id: D2
    description: "wheels.yml declares all five D-34 cells (linux x86_64, linux aarch64, macOS x86_64, macOS arm64, windows x86_64) and all five build successfully on GitHub Actions with artifacts uploaded"
    requirement: "PKG-01"
    verification:
      - kind: e2e
        ref: "GitHub Actions run 30349901732 (workflow_dispatch on commit fdeca01): aarch64-apple-darwin (3m25s), x86_64-unknown-linux-gnu (3m59s), aarch64-unknown-linux-gnu (3m47s), x86_64-apple-darwin (7m54s, macos-15-intel), x86_64-pc-windows-msvc (7m7s) -- all 5 succeeded, all 5 artifacts uploaded"
        status: pass
    human_judgment: true
    rationale: "Confirming a cross-platform CI matrix is genuinely green (not just authored correctly) requires a human to review the actual GitHub Actions run results, since local verification cannot execute macOS/Windows/ARM runners"
  - id: D3
    description: "ci.yml runs cargo test + maturin develop + pytest (in that order) on push/PR and passes"
    verification:
      - kind: e2e
        ref: "GitHub Actions run 30349901156 (push on commit fdeca01): success"
        status: pass
    human_judgment: true
    rationale: "Confirming the CI workflow is green on the real push event requires human review of the GitHub Actions run"
  - id: D4
    description: "compat-matrix.yml declares and passes both D-37 endpoints: oldest {py3.11, numpy 2.3.0, pandas 3.0.0} and newest {py3.12, numpy 2.5.1, pandas 3.0.5}"
    requirement: "PKG-02"
    verification:
      - kind: e2e
        ref: "GitHub Actions run 30349901256 (push on commit fdeca01): both endpoint cells succeeded"
        status: pass
    human_judgment: true
    rationale: "Confirming the compatibility matrix passed on both real endpoint cells requires human review of the GitHub Actions run"

# Metrics
duration: 94min
completed: 2026-07-28
status: complete
---

# Phase 4 Plan 3: GitHub Actions Packaging & CI Pipeline Summary

**Full D-34 wheel matrix (5 cells), CI workflow, and D-37 compat matrix authored, pushed to a new public GitHub repo, and confirmed green -- including diagnosing and fixing a retired macos-13 runner image mid-flight**

## Performance

- **Duration:** 94 min
- **Started:** 2026-07-28T16:39:12+08:00
- **Completed:** 2026-07-28T18:13:21+08:00
- **Tasks:** 3
- **Files modified:** 3 (plus 1 post-checkpoint fix commit to wheels.yml)

## Accomplishments

- Removed stale pre-rename `flint`-named build artifacts (`python/flint`, `target/**/*flint*`) before any local build, per RESEARCH.md's Runtime State Inventory.
- Built a wheel locally for the host platform via `uv run maturin build --release --out dist`, proved it installs cleanly via `uv pip install` into a fresh venv, and confirmed `import pydart` succeeds (PKG-03 local half).
- Authored `.github/workflows/wheels.yml`: a `PyO3/maturin-action@v1` build job with a five-cell `strategy.matrix.include` covering every D-34 target (linux x86_64, linux aarch64 on native `ubuntu-24.04-arm`, macOS x86_64, macOS arm64, windows x86_64), `manylinux: "2014"` on the linux cells, and per-cell `actions/upload-artifact@v4` (PKG-01).
- Authored `.github/workflows/ci.yml`: installs uv, runs `uv sync --dev`, `uv run maturin develop`, `cargo test --workspace`, and `uv run pytest` on push/PR, with `maturin develop` explicitly ordered before `pytest`.
- Authored `.github/workflows/compat-matrix.yml`: a two-endpoint D-37 matrix (oldest {py3.11, numpy 2.3.0, pandas 3.0.0}; newest {py3.12, numpy 2.5.1, pandas 3.0.5}), each cell running `maturin develop` then `pytest`.
- Created the public GitHub repo `gablalou/pydart-io` (required for the free native `ubuntu-24.04-arm` runner) and pushed all Plan 03 commits.
- Diagnosed and fixed a real CI infrastructure failure: the `x86_64-apple-darwin` cell on `macos-13` never got a runner assigned and sat queued for 1h13m. Confirmed via GitHub API job inspection (no runner ever picked up the job) and via web search that GitHub retired the `macos-13` image in Dec 2025. Fixed by switching that cell's runner label to `macos-15-intel`, the current Intel-architecture replacement.
- Re-ran all three workflows on the fixed commit and confirmed all green: wheels.yml (all 5 D-34 cells succeeded, all 5 artifacts uploaded), ci.yml (success), compat-matrix.yml (both endpoint cells succeeded).

## Task Commits

Each task was committed atomically:

1. **Task 1: Preflight cleanup + build and install one wheel + author the D-34 wheel matrix** - `717af47` (feat)
2. **Task 2: Author the CI test workflow and the numpy/pandas compatibility matrix** - `be17975` (feat)
3. **Task 3: Confirm the wheel matrix and compat matrix are green on GitHub** - checkpoint task; repo creation/push performed by the human, then a real infrastructure fault (retired `macos-13` runner) was found and fixed at `fdeca01` (fix), after which all three workflows were re-run and confirmed green. Also `185f101` (docs: session-pause commit recorded at the original checkpoint stop).

**Plan metadata:** (this commit) `docs: complete plan`

## Files Created/Modified

- `.github/workflows/wheels.yml` - Five-cell D-34 wheel build matrix via maturin-action, per-cell artifact upload; `x86_64-apple-darwin` cell's runner later corrected from `macos-13` to `macos-15-intel`
- `.github/workflows/ci.yml` - cargo test + maturin develop + pytest on push/PR, correctly ordered
- `.github/workflows/compat-matrix.yml` - D-37 two-endpoint numpy/pandas compatibility matrix

## Decisions Made

- GitHub Actions' `macos-13` runner image was retired by GitHub in Dec 2025. Confirmed two ways: (1) primary evidence -- `gh run view`/job inspection on the actual stuck run showed the `x86_64-apple-darwin` job queued for 1h13m with no runner ever assigned (not a slow build, a runner that would never appear); (2) corroborating evidence -- GitHub's own runner-images changelog documents the `macos-13` image's retirement in Dec 2025. Fixed by switching to `macos-15-intel`, the current Intel-architecture equivalent, in `.github/workflows/wheels.yml` (commit `fdeca01`).
- Repo created as **public** on GitHub (`gablalou/pydart-io`) specifically to get the free native `ubuntu-24.04-arm` runner for the aarch64 Linux wheel-build cell, avoiding a QEMU cross-compile fallback that a private repo would have required.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixed wheels.yml's x86_64-apple-darwin cell targeting a retired GitHub-hosted runner image**
- **Found during:** Task 3 (post-push verification checkpoint)
- **Issue:** The plan specified `macos-13` for the macOS x86_64 (Intel) wheel-build cell (matching RESEARCH.md's YAML skeleton, authored before this runner's retirement was public knowledge). After the repo was pushed and `wheels.yml` triggered, the `x86_64-apple-darwin` job sat in queued state for over an hour with no runner ever assigned, while the other four D-34 cells built and completed normally within minutes. `gh run view`/job-level inspection confirmed no runner picked up the job (not a slow build -- an unavailable runner image), and a web search corroborated that GitHub retired the `macos-13` image in December 2025.
- **Fix:** Replaced `macos-13` with `macos-15-intel` (the current GitHub-hosted Intel macOS runner label) for the `x86_64-apple-darwin` matrix cell in `.github/workflows/wheels.yml`.
- **Files modified:** `.github/workflows/wheels.yml`
- **Verification:** Re-triggered `wheels.yml` via `workflow_dispatch` on the fixed commit; the `x86_64-apple-darwin` cell completed successfully in 7m54s, alongside all four other D-34 cells.
- **Committed in:** `fdeca01` ("fix(04-03): replace retired macos-13 runner with macos-15-intel")

---

**Total deviations:** 1 auto-fixed (1 bug fix, Rule 1)
**Impact on plan:** Necessary infrastructure fix to unblock the D-34 wheel matrix; no scope creep, no change to the plan's declared PKG-01/PKG-02/PKG-03 objectives.

## Issues Encountered

- The `x86_64-apple-darwin` wheel-build job appeared to hang (queued 1h13m) after the initial push. Root-caused to GitHub's Dec 2025 retirement of the `macos-13` runner image rather than a build failure -- resolved via the auto-fix documented above. All other cells and workflows ran and passed normally on the first attempt.

## User Setup Required

None - no external service configuration beyond the GitHub repo creation/push already documented in the plan's `user_setup` block (Task 3), which the human completed as part of resolving the Task 3 checkpoint.

## Next Phase Readiness

- `wheels.yml` produces verified, working wheel artifacts for all five D-34 cells -- ready for Plan 04-04 to add a publish job (e.g. PyPI Trusted Publishing via `id-token: write`, scoped to that job only per T-04-02) that consumes these artifacts on tag push.
- `ci.yml` and `compat-matrix.yml` are live and green on the public repo, giving ongoing regression coverage across the numpy/pandas D-37 version floor and ceiling for any future change.
- No blockers carried into Plan 04-04 from this plan. The unresolved, human-signed-off performance finding from 04-02 (pydart 3-43x slower than pyarrow on most axes) remains a separate, already-documented blocker in STATE.md and is out of this plan's scope.

---
*Phase: 04-benchmark-release-readiness*
*Completed: 2026-07-28*

## Self-Check: PASSED

- FOUND: `.planning/phases/04-benchmark-release-readiness/04-03-SUMMARY.md`
- FOUND: `717af47` (Task 1 commit)
- FOUND: `be17975` (Task 2 commit)
- FOUND: `fdeca01` (Task 3 fix commit)
- FOUND: `185f101` (session-pause commit at original checkpoint)
