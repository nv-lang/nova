# AGENTS.md

> Instructions for AI agents and coding assistants working in this repository.
> Think of this as a README for agents. Human contributors: see [README.md](README.md) and [CONTRIBUTING.md](CONTRIBUTING.md).
>
> **Read this whole file before touching anything.** It is short, and half the
> rules here are the kind whose violation only shows up in a forty-minute gate
> run on someone else's machine.
>
> **New here?** How development actually works — plan-driven dev, the worktree
> model, the daily loop — is in [docs/dev/dev-workflow.md](docs/dev/dev-workflow.md)
> (Russian). Project state, architecture, where to go next:
> [docs/dev/read-project.md](docs/dev/read-project.md). The reasoning
> behind every rule below, and what each of the 34 guards stops you doing:
> [docs/dev/rules-for-agents.md](docs/dev/rules-for-agents.md).

## Rules — what you may not do here

This section is the single home of the prohibitions. The root
[CLAUDE.md](CLAUDE.md) only orders you to read this file: it is loaded into a
Claude Code session automatically, this one is not, so the order has to live
there and the rules have to live here. Nothing is duplicated — two copies drift,
and you would read the stale one.

Every line below is enforced by a guard: breaking it reddens the authoritative
gate rather than passing quietly.

**Git**

* `git add` **by filename only**. Never `-A`, `.`, `-u`, `git commit -a` — they
  sweep up another session's uncommitted files; observed three times in one day.
* **Every commit names its scope**, because the index may be someone else's — on
  2026-08-23 a commit of one file took 49 in a shared worktree. Flags before `--`,
  paths after it:
  `git -C <tree> commit -s -F <message-file> --only -- <file1> <file2>`.
  A new file needs `git add <name>` first (`--only` only sees paths git knows).
  Enforced by `scripts/claude-hooks/guard-git.py`, which also refuses a flag
  placed after `--` (there git reads everything as a pathspec). Need the whole
  index — say so in the command: `# index-verified: <reason>`.
* Never `git stash`: worktrees share one `.git`, and what you hide surfaces in
  someone else's tree.
* Never touch `git config user.*` — authorship is the owner's, by hand. (349
  commits once went out under the wrong name this way.)
* No `Co-Authored-By` trailers.
* Never `git push --force`; never rewrite history (`rebase`, `filter-branch`)
  without permission — other windows' worktrees sit on those commits.
* Tags in the `nova` repository: owner only.
* Satellite package repositories (`nova-polaris`, `nova-http`, `nova-tls`,
  `nova-socks`, `nova-compress`, `nova-bignum`, ...) are the opposite case:
  merging, pushing **and tagging** them is the integrator's own call, no
  separate word needed. A package is pulled by `version = "0.1"`, so the
  resolver picks the newest matching **tag** -- a fix sitting on the
  package's `main` is invisible to every consumer until it is tagged, and
  the clean-tree build stays red. Stopping to ask is what makes the release
  wait (observed 2026-08-16). Push the tag to all three mirrors and verify
  with `git ls-remote` that they carry the same object.

**Language**

* Commit messages in **English** — the repository is public and mirrored to
  three hosts.
* `nova.toml` / `nova.lock.toml` in **English** — they ship inside package
  repositories.
* Doc comments in `.nv` and diagnostic texts in **English**.
* Reports to the owner and `docs/dev/` in **Russian**.

**Where you work**

* `main` belongs to the integrator. You work in **your own branch in your own
  worktree**, and the integrator merges.
* Worktrees live **beside the repository** — under the directory that holds the
  main working copy, never inside the repository itself, and never on a system
  drive with no room on it. The permitted root is *derived*, not written down:
  it is the parent of the main working copy (`NOVA_WORKTREE_ROOT` overrides),
  and `scripts/guards/check-worktree-location.sh` enforces it. Two measured
  reasons: a system drive that fills up turns a build failure into thirty fake
  test failures, and a worktree inside the repo gets swept into every grep and
  reddens guards on someone else's snapshot.

**Changing the language**

* **Order of precedence, when two sources disagree:** the specification (`spec/decisions/` — D-blocks are normative) → the conventions in `docs/dev/` → the compiler (`nova check <file>`) → whatever you remember about how languages usually work. Your memory is LAST, and it is not a tiebreaker. A contradiction BETWEEN levels is not yours to resolve: report it to the owner with both places quoted. Half of this was already here — «the compiler is the authority, not your memory» — but the order among spec, conventions and compiler was not, so an agent meeting a contradiction picked one. This is the single home of that order; `/explain` («Правило поиска ответа») points here rather than repeating it.
* **The spec is written BEFORE the implementation.** A language-changing merge
  without a D-block in `spec/decisions/` and its overview page does not get
  pushed.
* **Do not pick a D-block number yourself.** "Take the next free one" only works
  with a single writer; with two windows it collides — it did. Ask the
  integrator.
* Do not invent Nova syntax. **The compiler is the authority, not your
  memory**: write the code, then run `nova check <file>`. Every retracted
  form has a diagnostic that names the canonical replacement in the message
  itself — `let` → `ro`/`mut` (D184), `readonly` → `ro`, `as_*` → bare
  nouns (D410), `external fn` → `extern "nova" fn`, `null` → `Option`,
  trailing commas in multi-line `match` arms → nothing (D452). There are
  about sixty such diagnostics; asking the compiler costs seconds and is
  always current, while any hand-written list of them starts drifting the
  day it is written.
* When the compiler and your recollection disagree, the compiler wins — but
  if you think the compiler is wrong, say so and stop. Do not work around it.

**Defects and tests**

* Found a defect → **file it** in `docs/plans/221.1-bug-sweep.md` in the same
  merge, with a priority, a CLASS, and the carrier caveat. A marker in code with
  no entry is invisible debt.
* Entry numbers are assigned by the integrator; you write `№TBD`.
* Fix the **class**, not the carrier. "The failing test passes now" is not
  acceptance.
* Never weaken or delete a test to make it pass — the test is authoritative.
* A new `E_*`/`W_*` needs a negative fixture; a negative fixture needs a
  line-pinned `nova:expect` marker.
* **Prove it both ways**: break your own condition, watch the fixture redden,
  restore it, watch it pass. A fixture that would be green without your fix
  proves nothing.

**Gates**

* Only the integrator runs the mega-CU and the full `nova test`. Yours are
  targeted: your fixture, your subdirectory, your package.

**Staying alive**

* The watchdog kills a window that produces no output for ten minutes. Do not
  background something you then wait for, print a line before a long command,
  and split long runs.

## What is Nova

Nova is a systems programming language with algebraic effects, structured concurrency, and optional contracts. Side effects are visible in function signatures (`Db Net Fail`), enabling local code review and handler-based testing without mocks. See [README.md](README.md) for a full overview.

## Build

```sh
# Build the nova CLI (main entry point for everything)
cd nova-cli && cargo build --release && cd ..

# The resulting binary:
# nova-cli/target/release/nova   (Windows: nova.exe)

# Build compiler internals only (no CLI wrapper)
cd compiler-codegen && cargo build && cd ..
```

After any change to Rust sources in `compiler-codegen/` or `nova-cli/`, rebuild before running tests.

## Test

```sh
# Full test suite (C-codegen pipeline). A PATH IS REQUIRED (Plan 172.6) — a bare
# `nova test` exits with "error: nova test requires at least one path".
# Pass BOTH live suites explicitly (spec_tests alone silently skips std):
nova-cli/target/release/nova test spec_tests std

# Targeted: run only tests matching a substring
nova-cli/target/release/nova test spec_tests --filter syntax/closure

# Single-file debug (no parallelism, keeps build artifacts)
./compiler-codegen/target/debug/nova-codegen test-build spec_tests/conformance/standalone/<fixture>.nv \
    --toolchain clang --keep-artifacts

# Interpreter pipeline (no C compilation)
./compiler-codegen/target/debug/nova-codegen test-interp spec_tests/conformance/standalone/<fixture>.nv
```

Common flags for `nova test`:

| Flag | Effect |
|---|---|
| `--filter <substr>` | Run only matching tests |
| `--mode release` | Compile with `-O3 -flto` |
| `--toolchain clang\|msvc\|gcc` | Force toolchain (default: auto) |
| `--timeout <secs>` | Per-test timeout (default: 60) |
| `--rerun-failed` | Re-run only previously failed tests |
| `--format json\|junit` | Machine-readable output |

Full test guide: [docs/dev/test-conventions.md](docs/dev/test-conventions.md).

## Repository structure

```
nova/
├── nova-cli/            # User-facing CLI: nova build/run/test/check/doc
├── compiler-codegen/    # Rust compiler: parser, type-checker, C-backend codegen, runtime
│   └── nova_rt/         # C runtime: effects, fibers, GC, libuv scheduler
├── spec_tests/          # THE authoritative corpus: conformance/ (+neg/, standalone/), soundness/, strict_effects/
├── nova_tests/          # NOT tests: CI inputs only (contracts/, doc/fixtures/)
├── nova_tests.old/      # FROZEN ARCHIVE — nothing runs it, never add
├── std/                 # Nova standard library source (tests: std/src/<module>/*_test.nv, peer files)
├── spec/                # Language specification
│   ├── decisions/       # Design decisions (D-blocks) — READ BEFORE CHANGING SEMANTICS
│   └── effects.md       # Effect system intro
├── docs/                # Developer guides
│   ├── dev/test-conventions.md   # Test authoring and EXPECT markers
│   └── dev/simplifications.md    # Running list of removed complexity
├── editors/             # Syntax highlighting plugins (VSCode, Vim, Emacs, Sublime)
└── examples/            # Nova code examples
```

## Design decisions — read before changing syntax or semantics

Nova's design is recorded in **D-blocks** in [spec/decisions/](spec/decisions/). Before adding a new construct or changing existing behavior:

1. Search `spec/decisions/` for relevant D-blocks.
2. Check `spec/decisions/history/rejected.md` — the idea may have been considered and rejected.
3. If the change contradicts an existing D-block, open an issue first.

**Never invent Nova syntax by analogy with other languages.** The spec is the ground truth.

## Writing tests

**Test files live in `spec_tests/conformance/` (language, diagnostics in `neg/`, runtime in `standalone/`) or next to the std module as `std/src/<module>/*_test.nv`.** These are the ONLY two places tests live — a test lives where it is run. `nova_tests.old/` is a FROZEN ARCHIVE: nothing runs it, never add to it. (Registry 221.1 #455: this file used to say the opposite and taught agents to write into the frozen corpus.)

```nova
// EXPECT_STDOUT hello
fn main() Io -> () => print("hello")
```

Error tests declare the expected failure with an `EXPECT_*` marker, matched as a substring against the first ~30 lines:

```nova
// EXPECT_COMPILE_ERROR type mismatch
```

(No colon after the marker name — the colon would become part of the matched substring; the corpus never uses that form.)

Other markers: `EXPECT_RUNTIME_PANIC`, `EXPECT_EXIT` / `EXPECT_EXIT_CODE`, `EXPECT_STDOUT`, `EXPECT_STDERR`, `EXPECT_TIMEOUT`, `EXPECT_COMPILE_WARNING`. The runner classifies a test by its marker (not by folder or filename suffix), so `neg/` and `_neg` are human signals only. Full list: [docs/dev/test-conventions.md](docs/dev/test-conventions.md).

A test file for a new feature `X` goes in `spec_tests/conformance/` (language semantics; negatives in `neg/`, runtime in `standalone/`) or as a peer file next to the std module, `std/src/<module>/X_test.nv`. `SOUNDNESS_REGRESSION` is not a marker the runner recognizes — it is a counter tracked only in `contracts-z3.yml`.

Full marker reference: [docs/dev/test-conventions.md](docs/dev/test-conventions.md).

## Followup markers (`[M-…]`)

Deferred work is tracked with `[M-<kebab-name>]` markers in docs and code comments.

- **Plan-bound** markers (followups of a specific plan) live in that plan's **Followups** section in `docs/plans/<plan>.md`.
- **Floating** markers (cross-cutting, not owned by any plan) — the *open* ones are listed in [docs/plans/backlog-followups.md](docs/plans/backlog-followups.md), the curated **OPEN-view** (what is still live and actionable).
- [docs/dev/simplifications.md](docs/dev/simplifications.md) is the append-only **history log** of all markers/simplifications — *not* a status view. It records that a marker existed; it does not tell you whether it is still open.

**Lifecycle:**

1. Create a floating marker → add a row to `backlog-followups.md` **and** log the change in `simplifications.md` (house style).
2. Resolve it → **remove the row** from `backlog-followups.md` (the history stays in `simplifications.md` and the commit). Keep the OPEN-view short — only live items.
3. When a marker grows into its own plan → move it to that plan's Followups and drop it from the backlog.
4. Before starting work in a subsystem, scan `backlog-followups.md` for relevant open items.

## Contribution rules

- **DCO sign-off required on every commit that arrives through a pull request** —
  CI enforces it there, and only there:
  ```sh
  git commit -s -m "your message"
  ```
  **Correction 2026-08-17 (found by window 274, verified by the integrator).**
  This line used to read "on every commit — CI enforces this", and both halves
  were false. The workflow triggers on `push: [main]` and `pull_request`; the
  project works by direct pushes to `main`, so the DCO check never ran once —
  and **none of the last twenty commits on `main` carries a sign-off**. The
  first thing that ever asked was window 274's PR #4, which went red on it.
  A rule that announces an enforcement it does not have is worse than no rule:
  it buys the feeling of a guarantee at the price of the guarantee.
  History is NOT rewritten (122 commits on the 274 branch alone, and `main`
  predates the rule wholesale); pull requests are squash-merged with a
  sign-off, which is where the DCO actually protects anything — external
  contribution.
- **`git add` specific files only** — never `git add .` or `git add -A`. Multiple agents may work in parallel worktrees.
- **One commit per logical task.** Multiple tasks → multiple commits.
- **No `Co-Authored-By: <AI tool>` trailers** in commit messages. A repo hook strips them automatically — do **not** add the trailer by hand (and no need to check for it manually; the hook removes it on commit).
- **Language convention.** Commit messages — subject AND body — are **English**, since 2026-08-09 (the repository is public and mirrored to three hosts; the history is read from outside). Enforced by `scripts/guards/check-commit-language.sh`: Cyrillic in a message after the cutover commit reddens the gate. Internal dev docs (`docs/dev/`) and reports to the owner stay Russian — see the Rules section above, which is the single home of this rule.
- **License:** code is `MIT OR Apache-2.0`; docs are `CC-BY-4.0`. See [LICENSE-MIT](LICENSE-MIT), [LICENSE-APACHE](LICENSE-APACHE).

## Key reference files

| File | What it covers |
|---|---|
| [docs/dev/dev-workflow.md](docs/dev/dev-workflow.md) | **How development works** — sources of truth, plan-driven dev, worktrees, the daily loop, operational rules |
| [spec/decisions/README.md](spec/decisions/README.md) | Index of all D-blocks |
| [docs/plans/README.md](docs/plans/README.md) | Index of all plans |
| [docs/plans/backlog-followups.md](docs/plans/backlog-followups.md) | Registry of floating `[M-…]` followup markers **not** tied to a plan (codegen / perf / debug-info backlog). Plan-bound markers live in their plan's Followups section. |
| [docs/dev/test-conventions.md](docs/dev/test-conventions.md) | EXPECT markers, test runner flags |
| [docs/dev/gate-guard-conventions.md](docs/dev/gate-guard-conventions.md) | **Writing gates and guards** — what a check may cost, tiers, the time budget |
| [docs/dev/module-conventions.md](docs/dev/module-conventions.md) | **Designing any Nova module (std/app/third-party) + C integration** — effect-family architecture (mockable plumbing + type-method facade), value/must-consume types, structured `Result` errors, byte-first, the `extern "C"` `ffi.nv` layer (CStr vs `(*u8,len)`, errno, value-records), `#cfg` platform-split. (`extern "nova"`/runtime park-wake/`#stable` are std-runtime-only — marked in §Применимость.) Complements [ffi-cookbook.md](docs/guide/ffi-cookbook.md) (FFI mechanics) and [nv-coding-style.md](docs/dev/nv-coding-style.md) (`.nv` style). |
| [docs/dev/simplifications.md](docs/dev/simplifications.md) | History of removed complexity |
| [compiler-codegen/README.md](compiler-codegen/README.md) | Compiler internals, build options |
| [docs/guide/nova-cli.md](docs/guide/nova-cli.md) | CLI command reference |
