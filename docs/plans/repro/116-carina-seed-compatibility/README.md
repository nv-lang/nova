# Task 116 — Carina seed compatibility evidence

This directory is a permanent, receipt-only evidence bundle for task 116. It is
not a language fixture, a specification, or an acceptance claim. The raw
artifacts under `receipts/` are retained byte-for-byte; the exact published
seed ZIP/EXE, generated C bytes, native EXE bytes, vcpkg libraries, and cache
binaries are intentionally not copied into Git. This receipt-only retention
choice was relayed on the owner's behalf through the orchestrator. Their
observed sizes and SHA-256 values remain in the receipts and `manifest.json`. Once the session
scratch is gone, byte-level revalidation of those excluded files is no longer
possible from this bundle alone.

## Offline commands

From the repository root:

```sh
python docs/plans/repro/116-carina-seed-compatibility/harness.py verify
python docs/plans/repro/116-carina-seed-compatibility/harness.py self-test
```

The harness uses only the Python standard library. It reads and hashes the
bundled receipts and, where present, the matching source/native input files in
this checkout. It does **not** spawn a process, invoke Oracle, run Carina, run
the native compiler, or start a build. `self-test` exercises named refusals
against in-memory mutated evidence only.

Historical `launch.json` files intentionally retain the exact Windows paths
and arguments observed during the runs. The harness treats those fields as
receipt data; it never dereferences them or uses them to launch anything.

## Evidence map

- `manifest.json` lists byte-exact bundled receipts, sizes, hashes, fixed input
  pins, the current task-scope source hash, and artifacts excluded by the
  owner’s retention decision.
- `receipts/baseline/` preserves the direct diagnostic receipt for the pinned
  seed. Its C stdout is represented by hash only.
- `receipts/manifests/` preserves the 565-file source/driver manifest, native
  runtime/libuv/toolchain inventory, prior cache baseline, and exact launch
  plans.
- `receipts/build/first-cross-volume/` records the first permitted driver
  attempt: seed child exit 0, parent exit 1 on Windows cross-volume rename
  error 17. The temporary EXE bytes are not included and were not executed.
- `receipts/build/same-volume/` records the one authorized successful
  same-volume publication and its audits. The generated C and published PE
  bytes are not included; their hashes are recorded.
- `receipts/smoke/` preserves the three CLI cases’ raw stdout/stderr/status,
  launch metadata, matcher probes, and post-smoke audit.
- `receipts/audits/` preserves independent preflight, build, smoke, cache, and
  scoped task-092 receipts, including the post-smoke rehash of 1551 pinned
  task-092 inputs (`missing=[]`, `changed=[]`). That comparison does not claim
  every byte of the dirty task-092 worktree.
- `report.md` gives the criterion-by-criterion result and the remaining gaps.

## Boundaries

The CLI smoke is limited to usage, `check stdout_hello.nv`, and checking one
absent absolute `.nv` path. It is not D-conformance, B2, fixed-point proof,
release permission, or task acceptance. The previously recorded cross-volume
publication finding (#1895), child-capture failure-path finding (#1896), and
separate inherited-environment provenance gap (#1897) remain open. Their
Carina 0.3 impact was not evaluated in task 116; there is no separate decision
on their release acceptability. STOP 092 is unchanged. No CI, commit, or further
native/source stage is authorized by this bundle.
