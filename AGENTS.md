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
> behind every rule below, and what the guards stop you doing, is named there
> file by file. **How many guards there are, and how many are still unnamed, is
> NOT written here** -- run the guard that holds the page and read its line:
> `bash scripts/guards/check-rules-page-complete.sh .` (not every guard needs its
> own page, which is why the two numbers differ). The pair used to be copied into
> this paragraph and went stale TWICE: 168/99 stood here three days and was 20
> guards out of date when someone read it, and its replacement 148/188 was wrong
> within four days -- on 2026-09-11 the guard printed 193 with 40 unnamed. A
> number nothing compares against its source decays silently, and the warning not
> to re-count by hand did not stop either drift, because the copy itself was the
> problem. So there is no copy now:
> [docs/dev/rules-for-agents.md](docs/dev/rules-for-agents.md).

## Rules — what you may not do here

This section is the single home of the prohibitions. The root
[CLAUDE.md](CLAUDE.md) only orders you to read this file: it is loaded into a
Claude Code session automatically, this one is not, so the order has to live
there and the rules have to live here. Nothing is duplicated — two copies drift,
and you would read the stale one.

Not every line below has a guard. Which are held by a hook or guard and which by
text only is mapped in [rules-for-agents.md §12](docs/dev/rules-for-agents.md).

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
* **Secrets are unreadable by the environment, not by your good behaviour.** `.claude/settings.json` carries a `permissions.deny` list: env files, private keys and certificates cannot be read at all, and `git reset --hard` / `git clean -fd` cannot be run (the git hook judges commit scope and stash, not those two — they erase someone else's uncommitted work in a shared tree without a trace). The list itself is NOT repeated here: a second copy would drift, and the file is the thing that actually enforces it. A leaked secret is leaked forever — no later commit removes it, you have to rotate the secret.
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

**Время**

* **Каждое сообщение человеку начинается с МЕСТНОГО времени**, взятого командой
  `date "+%H:%M"`, а не из головы: у окна нет часов, и «примерно сейчас» врёт на
  часы. Требование владельца, распространено на все окна и всех агентов
  2026-09-16 (план 290 пункт 6). Без времени доклад не сопоставить с чужими
  прогонами и с логами гейта, а на одной машине их идёт несколько.
* **Команда зовётся ПЕРЕД КАЖДЫМ таким сообщением, а не раз за ход** (правка
  2026-09-20, замер владельца: доклад помечен 05:56 при машинных 05:54 — время
  снято в начале хода и проставлено спустя одиннадцать минут работы). Ход длится
  десятки минут; значение, взятое в его начале, к докладу уже неверно, и ошибка
  всегда уходит ВПЕРЁД. **Прибавлять к прежнему значению «сколько прошло»
  запрещено:** это та же память, только с арифметикой. Правило распространяется и
  на сообщения СОСЕДНИМ ОКНАМ — по ним сверяют порядок событий.
* **Гейт печатает время сам:** при старте — местное время, ярус и ожидаемую
  длительность из `scripts/guards/gate-budget.baseline`; по завершении — время
  конца, полное время, ярус и где смотреть профиль шагов. Ожидаемое берётся из
  ФАЙЛА, потому что число в голове протухает молча (замер: профиль плана 275 был
  снят на 67 шагах и пережил рост до 114).

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
* **Temporary files and scratch directories go in your session's scratchpad, and
  NOWHERE else.** Never at the root of a drive, never beside the repository,
  never in the parent of the working copy. Owner's instruction, 2026-09-09, after
  finding what a month of sessions had left: 53 MB of test artefacts sitting at
  `D:\` itself (`nova_test_art4..6`, `nova_test_artifacts1..3`, `net_smoke_tmp`,
  `nova202repro`, `nova202rootpeers*`, newest file 13 July), and 1.1 GB beside the
  repository -- two orphaned copies of the whole tree (`nova-pk2fix` 347 MB,
  `nova-premerge` 170 MB, neither a git checkout), a `.tmp` log heap of 510 MB,
  plus twenty-one loose `nova-scratch-*.txt` / `msg*.txt` / `tmp_*` files. None of
  it was named in any plan; all of it was somebody's "just for a minute".
  **Why it is a rule and not tidiness:** a stray directory beside the repository
  is swept into greps and guard scans and reddens someone else's snapshot; a
  full-tree copy without `.git` looks like a worktree to a human and holds work
  nobody can merge; and a drive that fills up turns a build failure into thirty
  fake test failures. If you genuinely need a file to outlive your session, it
  belongs in the repository under a named path, in a commit -- or it does not
  need to outlive the session.
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

Nova is a systems programming language with algebraic effects, structured concurrency, and optional contracts. Side effects are visible in function signatures (`Db Net Fail`), enabling local code review and handler-based testing without mocks. Nova compiles to C, then to a native binary — no VM, no interpreter; memory is managed by a Boehm GC by default, with `consume`/ownership for deterministic cleanup. See [README.md](README.md) for a full overview.

**Status: v0.1.0, the first public release.** The repository holds the compiler,
the standard library, the specification, and the tooling. Separately versioned
packages (`nova-http`, `nova-tls`, `nova-compress`, `nova-polaris`,
`nova-bignum`, ...) live in their own satellite repositories and are pulled in
via `nova.lock.toml`.

## Technology stack

* **Compiler** (`compiler-codegen/`, crate `nova-codegen`): Rust (1.85+,
  edition 2021). Parser, type-checker, C-backend codegen, plus the native C
  runtime `nova_rt/` (effects, fibers, Boehm GC, libuv M:N scheduler). Kept
  deliberately dependency-light (`clap` + `anyhow`); the optional `z3-backend`
  cargo feature adds SMT contract verification via libz3/vcpkg.
* **CLI** (`nova-cli/`, crate `nova`): the single user-facing `nova` binary
  (`check` / `build` / `test` / `doc` / `lint` / `regen-runtime` / `test-build`),
  wrapping `nova_codegen` as a path dependency. Also carries one-shot migration
  binaries (`migrate_plan60`, `migrate_plan65`). `nova run` (interpreter) is
  currently unsupported — Nova compiles to C; use `nova build` or `nova test`.
* **LSP** (`nova-lsp/`): Rust language server with a VSCode extension.
* **`novac/`** (brand **Carina**): the self-hosted compiler being written in
  Nova itself (`novac/src/{lex,parse,resolve,sem,types,emit_c,...}`), developed
  against the Rust compiler as the oracle — see
  [docs/dev/novac-architecture.md](docs/dev/novac-architecture.md).
* **Standard library** (`std/`): written in Nova (`.nv`), grouped by domain
  under `std/src/` (collections, io, fs, net, text, unicode, time, crypto,
  concurrency, ffi, ...).
* **Nova workspace**: the root [nova.toml](nova.toml) (D78) declares
  `members = ["std", "examples", "nova_tests", "spec_tests", "novac"]`;
  directory name == `package.name`. The Rust crates are NOT part of it.
* **CI**: GitHub Actions in `.github/workflows/` (`nova-gate`, `nova-lint`,
  `nova-test-regression`, `crate-tests`, `contracts-z3`, `contracts-crosscheck`,
  `bench-regression`, `nova-doc`, `dco`). The repo is mirrored to GitVerse and
  SourceCraft; GitHub is the source of truth.

## Build

```sh
# Build the nova CLI (main entry point for everything)
cd nova-cli && cargo build --release && cd ..

# The resulting binary:
# nova-cli/target/release/nova   (Windows: nova.exe)

# Build compiler internals only (no CLI wrapper)
cd compiler-codegen && cargo build && cd ..

# Language server
cd nova-lsp && cargo build --release && cd ..

# Optional SMT-backed contract verification (needs libz3 via vcpkg,
# see docs/guide/z3-setup.md)
cd nova-cli && cargo build --release --features z3-backend && cd ..
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
├── nova-cli/            # User-facing CLI: nova build/run/test/check/doc (Rust crate "nova")
├── compiler-codegen/    # Rust compiler: parser, type-checker, C-backend codegen, runtime
│   └── nova_rt/         # C runtime: effects, fibers, GC, libuv scheduler
├── nova-lsp/            # Language server (Rust) + VSCode extension pieces
├── novac/               # Self-hosted compiler in Nova (brand Carina); lex/parse/sem/emit_c in .nv
├── spec_tests/          # THE authoritative corpus: conformance/ (+neg/, standalone/), soundness/, strict_effects/, p270/
├── nova_tests/          # NOT tests: CI inputs only (contracts/, doc/fixtures/)
├── nova_tests.old/      # FROZEN ARCHIVE — nothing runs it, never add
├── std/                 # Nova standard library source (tests: std/src/<module>/*_test.nv, peer files)
├── spec/                # Language specification
│   ├── decisions/       # Design decisions (D-blocks) — READ BEFORE CHANGING SEMANTICS
│   └── effects.md       # Effect system intro
├── examples/            # Nova code examples (incl. examples/flagship/aggregator demo)
├── bench/               # Benchmarks (corpus/, micro/, m_n/, plan*); config in bench.toml
├── scripts/             # gate.sh / gate-novac.sh, guards/, githooks/, claude-hooks/, tools/
├── docker/              # Release Dockerfile + image test runner
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

## Security considerations

* Reporting policy and honest scope statement (pre-1.0, not audited, `unsafe`/FFI
  can corrupt memory as C can): [SECURITY.md](SECURITY.md). Do not open public
  issues for vulnerabilities.
* Secrets are protected by the environment, not by discipline — see the Git rules
  above: `.claude/settings.json` denies reads of env files, private keys and
  certificates. Never use shell commands to route around that.
* Known defects, including security-relevant ones, are tracked in the open in
  `docs/plans/221.1-bug-sweep.md`.

## Followup markers (`[M-…]`)

Deferred work is tracked with `[M-<kebab-name>]` markers in docs and code comments.

- **Plan-bound** markers (followups of a specific plan) live in that plan's **Followups** section in `docs/plans/<plan>.md`.
- **Floating** markers (cross-cutting, not owned by any plan) — the *open* ones are listed in [docs/plans/backlog-followups.md](docs/plans/backlog-followups.md), the curated **OPEN-view** (what is still live and actionable).
- [docs/dev/simplifications.md](docs/dev/simplifications.md) is the live list of **deliberate SIMPLIFICATIONS in force** — each with its rationale and the condition that removes it. It is **not** a log of all markers, and it is not a status view of the backlog. **What must NOT go there** (the file says so in its own header, after a cleanup the owner ordered when it had turned into a dump): bug diagnoses and fix chronicles — not at all; closed simplifications — they move to `docs/history/simplifications-closed.md` at the moment of closing; reports of work done. Correction 2026-09-04: this line used to call it "the history log of all markers", and the Lifecycle below told every window to log EVERY floating marker there. A window following that faithfully lands in the case the target file forbids — measured, by the window that did it.

**Lifecycle:**

1. Create a floating marker → add a row to `backlog-followups.md`. **Additionally** log it in `simplifications.md` **only when the marker IS a deliberate simplification** — scope knowingly cut, with a rationale and a removal condition. A defect, a suspicion or a diagnosis does NOT go there: the marker row is its whole record.
2. Resolve it → **remove the row** from `backlog-followups.md` (the commit keeps the history; a simplification that had an entry moves to `docs/history/simplifications-closed.md`). Keep the OPEN-view short — only live items.
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
- `git add` by filename only (Rules → Git, above) — applies here too: multiple agents work in parallel worktrees.
- **One commit per logical task.** Multiple tasks → multiple commits.
- The `Co-Authored-By` ban (Rules → Git, above) is enforced automatically here: a repo hook refuses a commit that carries the trailer.
- Commit language is English (Rules → Language, above), enforced since 2026-08-09 by `scripts/guards/check-commit-language.sh` (Cyrillic after the cutover commit reddens the gate); `docs/dev/` and owner reports stay Russian.
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
| [docs/dev/novac-architecture.md](docs/dev/novac-architecture.md) | Architecture of `novac` (Carina), the self-hosted compiler in Nova |
| [docs/guide/nova-cli.md](docs/guide/nova-cli.md) | CLI command reference |
