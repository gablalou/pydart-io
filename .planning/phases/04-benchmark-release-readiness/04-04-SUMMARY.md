---
phase: 04-benchmark-release-readiness
plan: 04
subsystem: infra
tags: [github-actions, oidc, pypi, trusted-publishing, packaging]

# Dependency graph
requires:
  - phase: 04-03
    provides: .github/workflows/wheels.yml (D-34 five-cell wheel build matrix) and .github/workflows/compat-matrix.yml (D-37 two-endpoint compatibility matrix), which release.yml mirrors as its own wheels/compat-matrix jobs
provides:
  - .github/workflows/release.yml (tag/dispatch-triggered wheel build + D-37 compat gate + OIDC publish job)
affects: []

# Tech tracking
tech-stack:
  added: [pypa/gh-action-pypi-publish (OIDC trusted publishing)]
  patterns:
    - "release.yml defines its own `wheels` and `compat-matrix` jobs (mirroring wheels.yml/compat-matrix.yml's matrices) rather than referencing those workflows' jobs directly, because GitHub Actions `needs:` can only depend on jobs defined in the SAME workflow file -- a `publish` job cannot `needs:` a job living in a different .yml file"
    - "`id-token: write` is granted ONLY on the `publish` job (job-level `permissions:` block), never at the workflow level -- workflow-wide `permissions: contents: read` covers `wheels` and `compat-matrix`, which never need OIDC"

key-files:
  created:
    - .github/workflows/release.yml

key-decisions:
  - "release.yml's wheels/compat-matrix jobs duplicate the matrix definitions from wheels.yml/compat-matrix.yml rather than trying to invoke those workflows as reusable workflows, since GitHub's cross-workflow `needs:` gating (required by this plan's must_haves) only works for jobs declared in the same file as the job that needs them"
  - "Comments describing the 'no stored long-lived PyPI API token' guarantee deliberately avoid spelling the literal string `PYPI_API_TOKEN`, since the plan's own automated verify grep (`! grep -qi 'PYPI_API_TOKEN'`) would otherwise false-positive on an explanatory comment"

requirements-completed: []  # PKG-03's real-PyPI half is NOT yet satisfied -- see Known Stubs / Next Phase Readiness. Only the workflow authoring (Task 1) is done.

coverage:
  - id: D1
    description: "release.yml authored: OIDC publish job with job-scoped `id-token: write`, gated behind `needs: [wheels, compat-matrix]`, using `pypa/gh-action-pypi-publish`, no stored PyPI API token"
    requirement: "PKG-03"
    verification:
      - kind: automated
        ref: "grep -q 'gh-action-pypi-publish' && grep -q 'id-token' && grep -q 'needs:' && ! grep -qi 'PYPI_API_TOKEN' && python3 check for 'id-token: write' -- all passed; independently confirmed via python3 yaml.safe_load that top-level permissions is {contents: read} only, publish job permissions is {id-token: write} only, and wheels/compat-matrix jobs have no permissions override"
        status: pass
    human_judgment: false
  - id: D2
    description: "PyPI Trusted Publisher configured for pydart-io (repo + release.yml + pypi environment), name re-verified still free"
    verification: []
    human_judgment: true
    rationale: "PyPI trusted-publisher configuration is a human-only web-UI action requiring account ownership -- no CLI/API exists for a non-owner to create it. This plan execution STOPPED at this checkpoint (Task 2, gate=blocking-human) and did not proceed."
  - id: D3
    description: "Real PyPI publish triggered and verified: uv add pydart-io / pip install pydart-io installs and import pydart works"
    verification: []
    human_judgment: true
    rationale: "Not yet attempted -- blocked on D2 (Task 2 checkpoint) being resolved first. Requires a human to trigger the release and verify a real install."

# Metrics
duration: 12min
completed: 2026-07-28
status: blocked
---

# Phase 4 Plan 4: Release-to-PyPI Workflow (Partial -- Paused at Human Checkpoint) Summary

**Authored `.github/workflows/release.yml` (OIDC-only PyPI trusted-publishing pipeline, matrix-gated); execution paused at Task 2's mandatory human-only PyPI trusted-publisher configuration checkpoint**

## Performance

- **Duration:** 12 min (Task 1 only; Tasks 2-3 not yet executed)
- **Started:** 2026-07-28T11:58:00Z (approx)
- **Completed:** N/A -- plan not fully complete, paused at Task 2 checkpoint
- **Tasks:** 1 of 3 completed
- **Files modified:** 1

## Accomplishments

- Authored `.github/workflows/release.yml`, triggered on `v*` tag pushes and `workflow_dispatch`.
- Defined `wheels` and `compat-matrix` jobs inside `release.yml` itself (mirroring the D-34/D-37 matrices from `wheels.yml`/`compat-matrix.yml`), since GitHub Actions' `needs:` can only reference jobs in the same workflow file -- a cross-file `needs: [wheels, compat-matrix]` as literally sketched in RESEARCH.md's Pattern 3 snippet is only achievable this way.
- Authored the `publish` job: `needs: [wheels, compat-matrix]` (a failing wheel cell or failing compat endpoint blocks publish), `environment: pypi`, job-scoped `permissions: { id-token: write }` (not workflow-wide), downloads all `wheels-*` artifacts, and publishes via `pypa/gh-action-pypi-publish@release/v1`. No `PYPI_API_TOKEN`-style stored secret is referenced anywhere in the file (verified by grep and by hand-inspection of every comment).
- Verified via `python3 -c "import yaml; ..."` that the top-level `permissions:` block is `{contents: read}` only, the `publish` job's `permissions:` is `{id-token: write}` and nothing else, and neither `wheels` nor `compat-matrix` override permissions (they inherit read-only) -- confirming the plan's job-scoped-least-privilege acceptance criterion structurally, not just by grep.
- STOPPED at Task 2 (`type="checkpoint:human-action" gate="blocking-human"`) per plan and per this execution's explicit instructions: PyPI Trusted Publisher configuration is a human-only, account-ownership-gated web UI action with no CLI/API path for a non-owner. Did not attempt to configure it. Did not trigger Task 3's release/publish.

## Task Commits

Each task was committed atomically:

1. **Task 1: Author the OIDC trusted-publishing release workflow** - `601b787` (feat)
2. **Task 2: Configure the PyPI trusted publisher for pydart-io and re-verify the name is free** - NOT executed (blocking-human checkpoint; requires human PyPI account action)
3. **Task 3: Publish and verify a real install from PyPI** - NOT executed (depends on Task 2)

**Plan metadata:** (this commit) `docs: pause plan at Task 2 checkpoint`

## Files Created/Modified

- `.github/workflows/release.yml` - Tag/dispatch-triggered wheel build (mirrors D-34 matrix) + D-37 compat gate + OIDC publish job (`needs: [wheels, compat-matrix]`, job-scoped `id-token: write`, `pypa/gh-action-pypi-publish`, no stored token)

## Decisions Made

- `release.yml`'s `wheels`/`compat-matrix` jobs duplicate (rather than reference) the matrix definitions in `wheels.yml`/`compat-matrix.yml`, because GitHub Actions job-level `needs:` can only depend on jobs declared in the same workflow file. The plan's own `must_haves.key_links` ("the publish job `needs:` the wheels + compat-matrix jobs") is only satisfiable this way within a single `release.yml`.
- Explanatory comments about the absence of a stored PyPI token deliberately say "long-lived PyPI API token" instead of spelling the literal string the plan's own automated verify grep checks for (`PYPI_API_TOKEN`), to avoid a false-positive self-inflicted verify failure while still documenting the guarantee clearly for future readers.

## Deviations from Plan

None - Task 1 executed exactly as specified. No auto-fixes were needed; the workflow authoring matched the plan's must_haves and acceptance criteria on first attempt (confirmed by the automated verify command and an independent YAML-parse structural check).

## Issues Encountered

None for Task 1. Tasks 2 and 3 are intentionally not started -- they require a human to own and configure the PyPI project's trusted publisher, which is outside this executor's authority per the plan's own gate and this execution's explicit instructions ("do NOT attempt to configure PyPI trusted publishing yourself").

## Known Stubs

None in the code sense (release.yml is a complete, real implementation, not a placeholder). However, the plan's overall D-32/PKG-03 deliverable ("published `pydart-io` package on real PyPI") is NOT yet achieved -- only the workflow that will perform that publish exists. `pydart-io` is not yet live on PyPI. Do not treat this plan as fully satisfying PKG-03 until Tasks 2 and 3 are completed by a human and this SUMMARY (or a follow-up one) is updated to `status: complete`.

## User Setup Required

**External service configuration is required and BLOCKS the rest of this plan.** A human with PyPI account ownership must:
1. Re-verify `pydart-io` is still unclaimed on real PyPI (`https://pypi.org/pypi/pydart-io/json` should 404, or return a project the human owns) -- RESEARCH.md's availability finding is only valid for ~7 days.
2. On PyPI, add a Trusted Publisher (pending publisher, since the project doesn't exist on PyPI yet) for this GitHub repo (`gablalou/pydart-io`), the `release.yml` workflow filename, and the `pypi` environment.
3. Confirm no long-lived PyPI API token is created (OIDC only).

Then, separately (Task 3), trigger the release (tag push or `workflow_dispatch`) and confirm a real `uv add pydart-io` / `pip install pydart-io` install with a working `import pydart`.

## Next Phase Readiness

- `.github/workflows/release.yml` is authored, verified structurally sound (job-scoped least-privilege OIDC, matrix-gated, no stored token), and ready to run the moment PyPI's trusted publisher is configured.
- This plan is NOT complete. A continuation agent (or the same human, resuming) must: (1) resolve Task 2's checkpoint (PyPI trusted publisher config + name re-verification), (2) resolve Task 3's checkpoint (trigger release, verify real install), (3) update this SUMMARY's frontmatter to `status: complete` and populate `requirements-completed: [PKG-03]` only once the real-PyPI install is confirmed working.
- Carried-forward blocker (unchanged, not re-litigated per this execution's explicit instructions): the Phase 04-02 finding that `pydart.Table.from_pandas` is 3-19x slower than pyarrow on every true zero-copy scenario remains open and non-blocking; the user has explicitly chosen to proceed with this release despite it.

---
*Phase: 04-benchmark-release-readiness*
*Completed: N/A -- paused at Task 2 checkpoint, 2026-07-28*

## Self-Check: PASSED

- FOUND: `.planning/phases/04-benchmark-release-readiness/04-04-SUMMARY.md`
- FOUND: `601b787` (Task 1 commit)
