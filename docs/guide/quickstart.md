<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Quickstart

**English** | [Русский](quickstart.ru.md)

This page gets you from a downloaded zip to a running Nova program in a
few minutes, then to a slightly bigger example that shows the two things
that make Nova different: effects in function signatures, and structured
concurrency without `async`/`await`.

Nova is a compiled language: `nova build` turns a `.nv` file into C, then
into a native binary via your system C compiler. There is no interpreter
(`nova run` is intentionally unsupported) — `nova build` and `nova test`
are the two commands you'll use.

## Install (Windows x64)

1. Download `nova-v0.1.0-windows-x64.zip` from the
   [GitHub Releases page](https://github.com/nv-lang/nova/releases) and
   unzip it anywhere, e.g. `C:\nova`.
2. In a PowerShell session, from the folder you unzipped into, **dot-source**
   the setup script (the leading `. ` matters — without it the environment
   variables vanish when the script exits):

   ```powershell
   . .\setup-env.ps1
   ```

   This sets five environment variables (`NOVA_STD_PATH`, `NOVA_CG_INCLUDE`,
   `NOVA_RT_DIR`, `NOVA_GC_LIB_DIR`, `NOVA_GC_INCLUDE_DIR`) that tell
   `nova.exe` where to find the standard library and the C runtime when it
   isn't running from inside the Nova monorepo, and it adds the folder to
   `PATH` for the current session. To make this permanent, add the folder
   to your `PATH` and set the same five variables via
   *Settings → Environment Variables* (or `setx`).

3. Check it worked:

   ```powershell
   nova --version
   # nova 0.1.0
   ```

4. You also need a C compiler on your machine — Nova compiles to C, not
   directly to machine code. MSVC (Visual Studio Build Tools) is detected
   automatically via `vcvars64.bat`; Clang or GCC also work via
   `--toolchain`.

### Install (Linux)

There is no prebuilt Linux archive for v0.1.0 yet — build from source.
See [docs/guide/linux-build.md](linux-build.md) for the full recipe (Debian/Ubuntu
packages, Rust toolchain, `git submodule update`, build, smoke test);
it's a five-minute `TL;DR` at the top of that page.

## Hello, Nova

Every Nova project needs its own `nova.toml` so the compiler knows the
package root. Create a folder with two files:

`nova.toml`:

```toml
[package]
name = "hello"
version = "0.1.0"
```

`hello.nv`:

```nova
module hello

fn main() {
    println("Hello, Nova!")
}
```

The module name (`hello`) has to match the package name for a `.nv` file
sitting directly at the project root — that's a Nova convention, not a
typo.

Build and run:

```powershell
nova build hello.nv
.\hello.exe
# Hello, Nova!
```

`nova build` compiles `hello.nv` all the way to `hello.exe` in one step
(Nova → C → native binary). There's also `nova check` (type-check only,
no C compiler needed) and `nova test` (runs in-file `test { ... }` blocks).

## A slightly bigger example: effects + concurrency

The one-liner above doesn't show what makes Nova interesting. This does —
it's the actual `examples/mini_aggregator.nv` file from the Nova
repository, ~30 lines, no networking, no UI:

```nova
module nova_examples.mini_aggregator

import std.time.duration

const BUDGET_MS int = 120   // total budget for the whole run, ms

// One "source": waits its own latency (simulated network call) and
// reports on a channel. A spawn that misses the shared deadline is
// genuinely cancelled — not left running in the background with its
// result thrown away.
fn probe(latency_ms int, deadline Monotonic) Time -> str {
    ro { tx, rx } = Channel[bool].new(1)
    // A `with Fail[T]` handler runs IN THE FIBER of the failing operation,
    // not the installing scope's fiber (see spec/decisions/06-concurrency.md
    // D441) — a bare `mut` flag captured there is a data race under M:N.
    // `AtomicBool` is the synchronized alternative.
    mut timed_out = AtomicBool.new(false)
    with Fail[TimeoutError] = |_e| { timed_out.store(true) } {
        supervised(deadline: deadline) {
            spawn {
                Time.sleep(latency_ms)
                ro _ = tx.try_send(true)
            }
        }
    }
    if timed_out.load() {
        "cancelled"
    } else {
        match rx.try_recv() {
            Some(_) => "done"
            None    => "cancelled"
        }
    }
}

// Fan-out: all sources start AT ONCE, results collected into []str.
fn fan_out(latencies []int, deadline Monotonic) Time -> []str {
    ro outcomes = parallel for i int in 0..latencies.len() {
        probe(latencies[i], deadline)
    }
    outcomes
}

fn main() Time {
    ro latencies []int = [20, 40, 60, 80, 300, 800]   // ms; last two miss the budget
    ro t0 Monotonic = Monotonic.now()
    ro deadline Monotonic = t0 + BUDGET_MS.to_millis()
    ro outcomes = fan_out(latencies, deadline)
    mut done = 0
    mut cancelled = 0
    for i int in 0..outcomes.len() {
        if outcomes[i] == "done" { done = done + 1 } else { cancelled = cancelled + 1 }
    }
    ro now Monotonic = Monotonic.now()
    ro wall = now.elapsed_since(t0)
    println("done=${done} cancelled=${cancelled} wall=${wall.millis()}ms")
}
```

Build and run it from a checkout of the Nova repository (it lives in
`examples/`, alongside a `nova.toml` that already declares the workspace):

```powershell
cd examples
nova build mini_aggregator.nv -o mini_agg
.\mini_agg
# done=3 cancelled=3 wall=155ms
```

(Note: with an explicit `-o name`, the output file is exactly `name` — no
`.exe` is appended, even though it's a normal PE binary on Windows. Without
`-o`, `nova build hello.nv` names the output `hello.exe` after the input
file.)

The exact `done`/`cancelled` split and `wall` time are timing-dependent —
the important part is structural: six sources start together, the two
slowest ones (300ms, 800ms) never get to finish because they exceed the
120ms shared budget, and they are actually cancelled (`supervised(deadline:)`)
rather than left running after their result is discarded. Note what's
**not** here: no `async fn`, no `.await`, no `Future<T>` in the return
type — `Time` in the function signature is the only marker that this code
touches the clock, and `spawn`/`parallel for`/`supervised` give you
structured concurrency without a separate "async" dialect of the language.

## Where to go next

- [spec/overview.ru.md](../../spec/overview.ru.md) — main ideas, what's borrowed
  from where, tooling overview.
- [examples/flagship/aggregator](../../examples/flagship/aggregator) — the
  full-sized version of the example above: a real HTTP server (via the
  `http` package), a web UI with a waterfall visualization, and the same
  `Net Time Emit` effect signature checked by the compiler
  (`--strict-effects`). Comes with its own Dockerfile.
- [spec/decisions/](../../spec/decisions/) — the design decision log (D-numbers),
  the authoritative source for Nova syntax and semantics — every language
  feature traces back to a decision here.
- [docs/dev/test-conventions.md](../dev/test-conventions.md) — how `nova test` works,
  `EXPECT_*` markers, CLI flags.
- [docs/guide/linux-build.md](linux-build.md) — building from source on Linux/WSL2.
