<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/2.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
The version covers the `nova` command line and its exit codes, the Nova
language, the standard library API, the project file `nova.toml` and the
runtime behaviour of compiled programs. This file follows the Oracle compiler
line (`oracle-vX.Y.Z` tags); the Carina compiler line has its own versions and
its own notes in `docs/dev/`.

## [Unreleased]

## [0.1.7] - 2026-11-24

Patch release over 0.1.6. It fixes lost wake-ups, premature garbage collection
and connection resets in the runtime, and closes the remaining holes in
function-type checking. Oracle compiles to C and then to a native binary; there
is no interpreter. This is an early alpha: the language surface and APIs may
change, and compatibility is not guaranteed.

This is the first release recorded in this file. Releases up to 0.1.6 are
described in their own notes, [0.1.6](docs/dev/oracle-release-notes-v0.1.6.md),
[0.1.5](docs/dev/oracle-release-notes-v0.1.5.md) and
[0.1.0](docs/dev/oracle-release-notes-v0.1.0.md), and on the release pages;
those entries are not rewritten.

### Removed

- The opt-in runtime diagnostic switches no longer exist, and setting them has
  no effect: `NOVA_DIAG_791`, `NOVA_DIAG_656`, `NOVA_DIAG_M456`,
  `NOVA_DIAG_P259`, `NOVA_DIAG_P259_DL`, `NOVA_DIAG_ITEM19_R3`,
  `NOVA_DIAG_DEADLINE_OVERRUN`, `NOVA_TRACE_BUSY_YIELD_SLEEP`,
  `NOVA_GC_PAUSE_DIAG`, `NOVA_GC_SLOT_PROBE`, `NOVA_TIMER_METRICS`,
  `NOVA_WATCHDOG_WORKER` and `NOVA_SPAWN_POOL_DIAG`. The hang detector stays:
  when a wait is too long, the runtime dumps its state once. The delay is set
  with `NOVA_WATCHDOG_DUMP_SECS` (`0` turns the dump off).

### Fixed

Runtime:

- A fiber waiting on a channel, a timer or `select` could sleep forever. This
  happened when the wake-up arrived at the moment the fiber was committing to
  park, and also when a `select` arm or a file operation completed at that
  moment. Wake-ups now survive the park, and completion is checked again before
  the fiber sleeps.
- On Windows, the garbage collector did not scan the native stack of a thread
  that has no fiber arena yet: the driver thread, and a worker before its first
  fiber. An object kept alive only by such a stack could be freed while still
  in use, which showed as a crash under load. Proved on a C harness that drives
  the collector directly: the finalizer of such an object ran early in 60 of 60
  runs before the fix and in 0 of 60 runs after it.
- A thread already registered with the collector was still de-registered on
  exit, which the collector's contract forbids. On the same harness this
  crashed in 10 of 10 runs; de-registration now happens only after a successful
  registration.
- A program with nested `supervised` scopes could hang at exit: fibers parked
  in an inner scope were not cancelled on the way out.
- On Windows, closing a TCP stream while unread data was still arriving made
  the system reset the connection, and the peer lost the answer it was waiting
  for (`connection reset by peer`). Streams are now shut down and drained
  before they close. In one conformance fixture run 396 times, the reset
  happened 11 times before the fix and never after it.
- A fiber resumed while one of its sleeps was being closed could release state
  that the timer callback was still reading.
- The pipe of a closed child process, kept alive by a reference in the program,
  could hit a different child process on a late close, through a recycled
  handle address.

Compiler:

- A function value was accepted for a slot whose protocol-typed parameters or
  effect rows did not match, and the mismatch only showed at run time (for
  example a `fn(mut ActEnv, …)` value in a `fn(ReadEnv, …)` slot). Such
  mismatches are now refused: `E_FN_PROTO_METHOD_PARAM_MISMATCH`,
  `E_FN_PROTO_METHOD_RETURN_MISMATCH`, `E_FN_PROTO_METHOD_MUT_MISMATCH`,
  `E_FN_PROTO_SLOT_CONCRETE_DEMAND`, `E_FN_EFFECT_ROW_MISSING` and
  `E_FN_PROTO_METHOD_EFFECT_MISSING`
  ([specification, D497](spec/decisions/02-types.md#d497)). A program that
  compiled only because of this hole is now rejected.
- `(h.f)()` now calls the function when the field `f` has an alias or a newtype
  over a function type as its type. Before, the build failed in the generated C
  or the call went to a wrong address. This holds for heap records and for
  value records.
- An array literal used directly as the payload of a sum variant was read back
  as raw bytes when the element type differed: `Bytes([1, 2, 3])` of bytes came
  back as `1 0 0`. The elements are now converted one by one.
- `spawn(|| …)` built and exited with code 0 but never ran the body. A closure
  inside `spawn` that captured a variable broke the C build, and a captured
  mutable variable was copied instead of shared.
- Declaring a name again with a value of another type inside a nested block
  broke the build after the block.

Tools:

- A test killed by a timeout in `nova test` now dies together with its child
  processes, and the wait after the kill is bounded. Before, the descendants
  survived and a run could hang.

### Known issues

Each item names the [defect registry](docs/plans/221.1-bug-sweep.md) row that
tracks it.

- A fixed array is not accepted where a slice is expected, also when a function
  returns `[N]T` and the result is passed straight to a `[]T` parameter. The
  compiler refuses with `E7301`. Copy the elements into a new slice (row 1983).
- Function types are still compared permissively in some places. A closure
  literal, a `never` tail, a generic type parameter or an erased `any` piece of
  a function type is not checked against the slot. For example, a closure whose
  parameter type differs from the slot's is not refused by the compiler, and
  the build fails later in the generated C
  ([specification, D497](spec/decisions/02-types.md#d497), item 7). Annotate
  such values with concrete types (row 1915).
- `forbid … { … }` does not stop a call through a function value and has no
  run-time barrier, so a forbidden effect can run inside the block
  (row 1916). Do not rely on `forbid` as an enforcement boundary.
- `--strict-effects` does not report a method call on a value when the method
  has effects that the caller does not declare, nor a function value with extra
  effects passed into a slot (row 1917).
- `Time.now()` without `import std.time.duration`: `nova check` passes, and
  `nova build` stops with an internal compiler error (`E_CODEGEN_TYPE_UNKNOWN`)
  that names the missing import. Add the import (row 1918).
- A user type whose name equals an internal type of the standard library (for
  example `Slot`) breaks the build of the standard library itself, and the
  errors point into its files. Rename the type (row 1030).
- A one-line `match` arm whose body is an assignment is refused with
  `E_MATCH_ARM_SEP_MISSING`, even when the arms are on separate lines. Put
  braces around the body (row 1984).
- `nova test --filter <name>` that selects no test prints `PASS: 0 FAIL: 0` and
  exits with code 0. Check the count in the summary (row 1040).
- Twelve tests of the release source fail on Windows with the Clang toolchain:
  `spec_tests/p270/f2_display_baseline`,
  `spec_tests/soundness/assert_static_unverified_warn`,
  `spec_tests/soundness/assume_trust_introduced_warn`,
  `spec_tests/soundness/int_arith_no_overflow_positive`,
  `spec_tests/soundness/loop_compound_assign_w2402`,
  `spec_tests/soundness/loop_cond_assign_w2402`,
  `spec_tests/soundness/loop_in_let_w2402`,
  `spec_tests/soundness/ovf_unbounded_panic_neg`,
  `spec_tests/soundness/recursive_no_decreases_warn`,
  `spec_tests/conformance/standalone/carina_text_backtick_tag`,
  `std/src/encoding/serde/decode_errors_test` and
  `std/src/os/n1850_probe/env_bytes_str_literal_test`.
- With the MSVC toolchain, `p1418_exit_process_immediate` does not compile
  (`C2099: initializer is not a constant`). The Clang toolchain builds it.
- Tests that measure time can fail or time out when the machine is under load.
  `timeout_application_level2_t3_8` failed this way once on one machine in a
  run of 229 tests and never in 72 000 measured sleeps (row 2026).

[Unreleased]: https://github.com/nv-lang/nova/compare/oracle-v0.1.7...HEAD
[0.1.7]: https://github.com/nv-lang/nova/compare/oracle-v0.1.6...oracle-v0.1.7
