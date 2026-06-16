# Plan 156 — slow-test-lane demo fixtures

These `.nv` files are end-to-end fixtures for the Plan 156 slow-test lane in
the test runner (`nova-codegen test-all`). Each file declares a **distinct
module** (`plan156.<name>`) so they are standalone test entries, not
folder-module peers.

## Slow classification

A test file is "slow" iff its stem ends in the `_slow` suffix (see
`is_slow_file_stem` in `compiler-codegen/src/test_runner.rs`). The `_slow`
suffix is peeled *before* the `_test` and OS suffixes; the canonical stem
layout is `<core>[_<os>][_test][_slow]`. Classification is purely by name —
the trivial bodies here are intentional; nothing actually runs slowly.

## Lane modes

| File                     | default (Exclude) | `--include-slow` | `--slow-only` |
|--------------------------|:-----------------:|:----------------:|:-------------:|
| `lane_normal.nv`         | run               | run              | skip          |
| `notslow.nv` (edge)      | run               | run              | skip          |
| `lane_big_slow.nv`       | skip              | run              | run           |
| `combo_windows_slow.nv`  | skip              | run (Windows)    | run (Windows) |

- **Default** (`nova test`): `*_slow.nv` files are skipped entirely without
  reading their bodies.
- **`--include-slow`**: run normal tests **and** `*_slow.nv`.
- **`--slow-only`**: run **only** `*_slow.nv`.
- **Edge case**: `notslow.nv` ends with the substring `slow` but not the
  `_slow` suffix, so it stays a normal test.
- **OS × slow combo**: `combo_windows_slow.nv` peels `_slow` to leave
  `combo_windows`, whose `_windows` OS-suffix additionally gates it to the
  Windows target. On non-Windows targets it is excluded even with slow
  enabled.
- The slow lane composes with `--filter` (substring on display name).
