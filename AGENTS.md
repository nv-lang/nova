# AGENTS.md

> Instructions for AI agents working in this repository. Humans: [README.md](README.md), [CONTRIBUTING.md](CONTRIBUTING.md).
> **Read this whole file before touching anything.** How development works (plans, worktrees, the daily loop):
> [docs/dev/dev-workflow.md](docs/dev/dev-workflow.md) (Russian). Project state and where to go next:
> [docs/dev/read-project.md](docs/dev/read-project.md). **The reasoning, measurements and incident history behind every
> rule below, and which rules are held by a hook or guard, live in
> [docs/dev/rules-for-agents.md](docs/dev/rules-for-agents.md)** (§12 maps rule -> mechanism; its "AGENTS.md history"
> appendix keeps what was cut from here). Guard counts are NOT written anywhere by hand: run
> `bash scripts/guards/check-rules-page-complete.sh .` and read its line.

## Rules — what you may not do here

The single home of the prohibitions (root [CLAUDE.md](CLAUDE.md) only orders you to read this file). Not every line has a guard.

**Git**

* `git add` **by filename only** — never `-A`, `.`, `-u`, `git commit -a` (they sweep up another session's files).
* **Every commit names its scope** (the index may be someone else's): flags before `--`, paths after:
  `git -C <tree> commit -s -F <message-file> --only -- <file1> <file2>`. A new file needs `git add <name>` first.
  Enforced by `scripts/claude-hooks/guard-git.py` (it also refuses a flag after `--`). Whole index needed: say so
  in the command, `# index-verified: <reason>`.
* Never `git stash` (worktrees share one `.git`).
* **Secrets are unreadable by the environment**, not by good behaviour: `.claude/settings.json` `permissions.deny`
  blocks env files, keys, certificates, `git reset --hard`, `git clean -fd`. The list is not copied here. Never route
  around it from a shell. A leaked secret is leaked forever: rotate it.
* Never touch `git config user.*` (authorship is the owner's). No `Co-Authored-By` trailers (a hook refuses them).
* Never `git push --force`; never rewrite history (`rebase`, `filter-branch`) without permission.
* Tags in the `nova` repository: owner only. Satellite package repos (`nova-polaris`, `nova-http`, `nova-tls`,
  `nova-socks`, `nova-compress`, `nova-bignum`, ...): merging, pushing **and tagging** is the integrator's own call
  (consumers see only the newest **tag**); push the tag to all three mirrors, verify with `git ls-remote`.

**Время**

* **В OpenCode время ставит не агент** (слово владельца 2026-10-06): во вкладке claude-code — провайдер
  (timeStamp) перед каждым ответом, у прочих провайдеров — плагин `nova-env`. Агент время сам НЕ пишет и `date`
  для доклада не зовёт: своя метка рядом с провайдерской давала две разные. Хук `show-local-time` во вкладке
  (`OPENCODE_SESSION_ID`) молчит.
* **Только в окне Claude Code без OpenCode:** сообщение человеку и письмо соседнему окну начинается с МЕСТНОГО
  времени из `date "+%H:%M"` перед КАЖДЫМ сообщением, не из головы (план 290 п.6; хук подаёт `ВРЕМЯ СЕЙЧАС`).
* Гейт печатает время, ярус и ожидаемую длительность сам (из `scripts/guards/gate-budget.baseline`).

**Language**

* Commit messages, `nova.toml` / `nova.lock.toml`, doc comments in `.nv`, diagnostic texts: **English** (public repo,
  three mirrors; Cyrillic in a commit message reddens `check-commit-language.sh`).
* Reports to the owner and `docs/dev/`: **Russian**.

**Where you work**

* You work in **your own branch in your own worktree**. The task's **acceptor** (not its author) merges it into
  `main` by the acceptance steps; a commit straight to `main`: the integrator only. The acceptor lands with ONE
  script, `scripts/tools/land-task.sh <N> <full tip sha>` (via `peer_watch`, under `peer_task merge`); the full path
  and the fallback are in [.claude/commands/integrator.md](.claude/commands/integrator.md), «Путь приёмщика».
* Your own branch MAY be pushed to `origin` (never `--force`, never `main`, never a tag; the mirrors carry `main`).
* **Temporary files and scratch directories go in your session's scratchpad, NOWHERE else** — never at a drive root,
  beside the repository, or in the parent of the working copy. Needs to outlive the session? It goes into the
  repository under a named path, in a commit.
* Worktrees live in **`worktrees/` beside the repository** (`<parent of the main copy>/worktrees`,
  `NOVA_WORKTREE_DIR` overrides), never inside it, never on a system drive with no room; enforced by
  `scripts/guards/check-worktree-location.sh`.

**Changing the language**

* **Precedence when sources disagree:** specification (`spec/decisions/`, D-blocks are normative) -> conventions in
  `docs/dev/` -> the compiler (`nova check <file>`) -> your memory (last, never a tiebreaker). A contradiction
  BETWEEN levels is not yours to resolve: report it to the owner with both places quoted.
* **The spec is written BEFORE the implementation:** a language-changing merge without a D-block in
  `spec/decisions/` and its overview page is not pushed.
* **Do not pick a D-block number yourself** — ask the integrator.
* Do not invent Nova syntax: write the code, run `nova check <file>`; every retracted form has a diagnostic naming
  the canonical replacement. Compiler and recollection disagree -> the compiler wins; if you think it is wrong, say
  so and stop, do not work around it.

**Defects and tests**

* Found a defect -> **file it** in `docs/plans/221.1-bug-sweep.md` in the same merge: priority, CLASS, the carrier
  caveat. Entry number: you write `№TBD`, the integrator assigns it. A marker in code with no entry is invisible debt.
* Fix the **class**, not the carrier; "the failing test passes now" is not acceptance.
* Never weaken or delete a test to make it pass. A new `E_*`/`W_*` needs a negative fixture with a line-pinned
  `nova:expect` marker.
* **Prove it both ways:** break your condition, watch the fixture redden, restore, watch it pass.

**Gates**

* Only the integrator runs the mega-CU and the full `nova test`. Yours are targeted: your fixture, subdirectory, package.

**Staying alive**

* The watchdog kills a window with no output for ten minutes: never background something you then wait for, print
  a line before a long command, split long runs.

## What is Nova

A systems language with algebraic effects, structured concurrency and optional contracts: effects are visible in
signatures (`Db Net Fail`). Compiles to C, then native (no VM, no interpreter); Boehm GC by default, `consume`
for deterministic cleanup. v0.1.0. Satellite packages (`nova-http`, `nova-tls`, ...) live in their own repos,
pulled via `nova.lock.toml`.

## Technology stack

* **Compiler** (`compiler-codegen/`, crate `nova-codegen`): Rust 1.85+, parser, type-checker, C backend, C runtime
  `nova_rt/` (effects, fibers, Boehm GC, libuv M:N scheduler); optional SMT: `cargo build --release --features z3-backend` in `nova-cli/` (libz3/vcpkg, `docs/guide/z3-setup.md`).
* **CLI** (`nova-cli/`, crate `nova`): `check` / `build` / `test` / `doc` / `lint` / `regen-runtime` / `test-build`.
  `nova run` (interpreter) is unsupported — use `nova build` or `nova test`.
* **LSP** (`nova-lsp/`); **`novac/`** (brand Carina): self-hosted compiler in Nova, developed against the Rust
  compiler as oracle ([docs/dev/novac-architecture.md](docs/dev/novac-architecture.md)); **stdlib** `std/` in `.nv`.
* **Nova workspace:** root [nova.toml](nova.toml) (D78), `members = ["std", "examples", "nova_tests", "spec_tests",
  "novac"]`; directory name == `package.name`; the Rust crates are not members.
* **CI:** `.github/workflows/`; mirrored to GitVerse and SourceCraft, GitHub is the source of truth.

## Build and test

```sh
cd nova-cli && cargo build --release && cd ..        # nova-cli/target/release/nova (.exe on Windows)
cd compiler-codegen && cargo build && cd ..           # compiler internals only

# A PATH IS REQUIRED; pass BOTH live suites (spec_tests alone silently skips std):
nova-cli/target/release/nova test spec_tests std
nova-cli/target/release/nova test spec_tests --filter syntax/closure      # targeted
./compiler-codegen/target/debug/nova-codegen test-build <fixture>.nv --toolchain clang --keep-artifacts   # single file
```

Rebuild after any change to Rust sources in `compiler-codegen/` or `nova-cli/`. `nova test` flags: `--filter <substr>`,
`--mode release`, `--toolchain clang|msvc|gcc`, `--timeout <secs>` (60), `--rerun-failed`, `--format json|junit`.

## Repository structure

`nova-cli/` · `compiler-codegen/` (+ `nova_rt/` C runtime) · `nova-lsp/` · `novac/` · `spec_tests/` THE authoritative
corpus (`conformance/` with `neg/`, `standalone/`; `soundness/`, `strict_effects/`, `p270/`) · `nova_tests/` NOT tests,
CI inputs only · `nova_tests.old/` FROZEN, never add · `std/` · `spec/` (`decisions/`) · `examples/` · `bench/` ·
`scripts/` (`guards/`, `githooks/`, `claude-hooks/`, `tools/`) · `docker/` · `docs/` · `editors/`.

## Design decisions — read before changing syntax or semantics

Search `spec/decisions/` and `spec/decisions/history/rejected.md` first; a change contradicting a D-block -> open an
issue first. **Never invent Nova syntax by analogy with other languages.**

## Writing tests

Tests live ONLY in `spec_tests/conformance/` (language; diagnostics in `neg/`, runtime in `standalone/`) or as
`std/src/<module>/*_test.nv` peers. Marker, matched as a substring against the first ~30 lines, **no colon**:

```nova
// EXPECT_STDOUT hello
fn main() Io -> () => print("hello")
```

Others: `EXPECT_COMPILE_ERROR`, `EXPECT_COMPILE_WARNING`, `EXPECT_RUNTIME_PANIC`, `EXPECT_EXIT` / `EXPECT_EXIT_CODE`,
`EXPECT_STDERR`, `EXPECT_TIMEOUT` (classified by marker, not folder). All:
[docs/dev/test-conventions.md](docs/dev/test-conventions.md).

## Security

[SECURITY.md](SECURITY.md): no public issues for vulnerabilities. Secrets: Git rules above. Known defects (security
ones too) are tracked openly in `docs/plans/221.1-bug-sweep.md`.

## Followup markers (`[M-…]`)

* **Plan-bound** markers live in that plan's Followups section; **floating** ones are rows in
  [docs/plans/backlog-followups.md](docs/plans/backlog-followups.md) (OPEN-view, only live items).
* `docs/dev/simplifications.md` lists only deliberate SIMPLIFICATIONS in force (rationale + removal condition) —
  never diagnoses, fix chronicles, closed items (-> `docs/history/simplifications-closed.md`) or reports.
* Lifecycle: create -> backlog row (plus `simplifications.md` ONLY for a deliberate simplification); resolve -> remove
  the row; grows into a plan -> move to its Followups; scan the backlog before work in a subsystem.

## Contribution rules

DCO sign-off (`git commit -s`) is enforced by CI **only on pull requests**, not on direct pushes to `main`. One
commit per logical task. `git add` by filename, the `Co-Authored-By` ban and commit language: see Rules above
(`check-commit-language.sh`, cutover 2026-08-09; `docs/dev/` and owner reports stay Russian).
License: code `MIT OR Apache-2.0`, docs `CC-BY-4.0` ([LICENSE-MIT](LICENSE-MIT), [LICENSE-APACHE](LICENSE-APACHE)).

## Key reference files

[dev-workflow](docs/dev/dev-workflow.md) · [D-block index](spec/decisions/README.md) · [plans](docs/plans/README.md) ·
[backlog-followups](docs/plans/backlog-followups.md) · [test-conventions](docs/dev/test-conventions.md) ·
[gate-guard-conventions](docs/dev/gate-guard-conventions.md) (writing gates and guards) ·
[module-conventions](docs/dev/module-conventions.md) (any module + C integration; with
[ffi-cookbook](docs/guide/ffi-cookbook.md), [nv-coding-style](docs/dev/nv-coding-style.md)) ·
[simplifications](docs/dev/simplifications.md) · [compiler-codegen/README.md](compiler-codegen/README.md) ·
[novac-architecture](docs/dev/novac-architecture.md) · [nova-cli guide](docs/guide/nova-cli.md)
