// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 104 — Production-grade IDE integration (LSP server + tree-sitter + editor distributions)

> **Статус:** 🟡 roadmap 2026-05-25, **P2** (постблокирующая фаза — Plan 91 std MVP первичен; LSP начинается параллельно как только Plan 91/100/101 стабилизируются).
> **Приоритет:** master закрывает «без LSP боль высокая для внешних пользователей» (см. memory `project-plan101-status` + Plan 01-roadmap-v0.1 §165 «LSP-сервер v0.5»).
> **Оценка:** ~33 dev-day (8 sub-plans, scoped 2-5 dev-day каждый; 6-7 недель calendar при single-developer pace).
> **Зависимости:**
> - Plan 91 ✅ pending (std MVP стабилизирует core API).
> - Plan 100 ✅ pending (consume-types implementation — 12 error codes для quick-fixes).
> - Plan 101 ✅ ЗАКРЫТ 2026-05-25 (8 error codes для quick-fixes).
> - `compiler-codegen` lib (parser/types/lexer) — reuse как library crate, не fork.
> - Plan 45 (doc-comments) ✅ — для hover-документации.
> - Plan 36 (R10 color) — irrelevant LSP (stdio JSON-RPC, no ANSI).
> **Связь:**
> - Plan 100.8 — частичный LSP quick-fixes для Plan 100 consume; будет absorbed в Plan 104.5.
> - Plan 101 V2 marker — LSP quick-fixes для Plan 101 errors absorbed в Plan 104.5.
> - Plan 01-roadmap §165 — обещание «LSP к v0.5».

## Цель и контекст

Сейчас писать на Nova можно только **в блокноте с подсветкой**:
- 🟢 TextMate grammar шиплен (VSCode/Cursor/Sublime/TextMate/VSCodium).
- 🟢 native syntax-плагины для Vim/Emacs.
- 🔴 НЕТ LSP-сервера → нет hover, autocomplete, goto-def, errors-as-you-type, rename, quick-fix.
- 🔴 НЕТ tree-sitter grammar → Zed/Helix/GitHub-web/modern Neovim не поддерживаются.

Это блокирует:
1. **Внешних пользователей** — порог входа неприемлем (хочешь увидеть ошибку — запусти `nova check` в терминале).
2. **Dogfooding-команду** — даже автор переключается между файлом и терминалом по 100 раз/час.
3. **AI-first позиционирование** — LLM (Cursor/Copilot) не получает structured-context от LSP, генерирует worse code.
4. **Open-source ramp-up** — без LSP контрибьюторы не приходят.

Plan 104 — закрывает все четыре + переводит редакторы из «syntax-only» в «full-IDE» режим. Master координирует 8 sub-plan'ов:

| # | Sub-plan | Scope | Оценка |
|---|---|---|---|
| 104.0 | Foundation: `nova-lsp` crate + tower-lsp | Скелет crate, JSON-RPC stdio, integration-test scaffold, project structure | ✅ ЗАКРЫТ 2026-05-25 |
| 104.1 | Diagnostics + file watching | Compiler errors → LSP `publishDiagnostics`, incremental check, multi-file workspace | ✅ ЗАКРЫТ 2026-05-26 |
| 104.2 | Hover + goto-definition + signature help | Symbol resolution, type rendering, doc-comment surfacing (D104/D105) | ~3 dev-day |
| 104.3 | Completion (keywords + identifiers + methods + imports) | Scope-aware, type-driven (after `.`), import suggestions, snippets | ~5 dev-day |
| 104.4 | Document/workspace symbols + find-references | Outline panel, Ctrl+Shift+O, Shift+F12 | ~3 dev-day |
| 104.5 | Code actions + quick-fixes (Plan 100 + Plan 101 + general) | ~25 error codes → machine-applicable fixes; auto-import; organize imports | ~5 dev-day |
| 104.6 | Rename + format-on-save | Cross-file rename, `nova fmt` integration | ~4 dev-day |
| 104.7 | Tree-sitter grammar (`tree-sitter-nova` repo) | Grammar, queries (highlights/folds/indents/injections), Helix/Zed/Neovim distribution | ~3 dev-day | ✅ **ЗАКРЫТ 2026-05-25** v0.1.0 — 84/84 fixtures, 5 query files |
| 104.8 | Editor packaging + distribution docs | VSCode extension (TypeScript client), nvim-lspconfig PR, Helix/Zed configs, install docs | ~3 dev-day | ✅ **ЗАКРЫТ 2026-05-26** — VSCode 7/7 PASS, Helix/Zed TOML valid, editors/INSTALL.md |
| 104.9 | Close-out: integration tests + marker closure + release notes | E2E test top-3 editors, Plan 100.8 markers ✅, Plan 101 LSP marker ✅ | ~2 dev-day |

**Total scope:** ~33 dev-day (6-7 calendar weeks single-dev).

## Что значит "production-grade" (acceptance bar)

Reference: `rust-analyzer` (Rust), `gopls` (Go), `pyright` (Python), `tsserver` (TypeScript). Минимум для **V1 production**:

| Feature | rust-analyzer | gopls | Nova V1 (104) |
|---|---|---|---|
| Diagnostics streaming | ✅ instant on save | ✅ instant on save | ✅ 104.1 |
| Hover (type + doc) | ✅ | ✅ | ✅ 104.2 |
| Goto-definition | ✅ cross-file | ✅ | ✅ 104.2 |
| Find-references | ✅ workspace | ✅ | ✅ 104.4 |
| Completion (basic) | ✅ | ✅ | ✅ 104.3 |
| Completion (type-driven) | ✅ smart | ✅ smart | ✅ 104.3 |
| Signature help | ✅ | ✅ | ✅ 104.2 |
| Code actions / quick-fixes | ✅ ≥40 fixes | ✅ ≥30 fixes | ✅ ≥25 fixes (104.5) |
| Rename | ✅ workspace | ✅ workspace | ✅ 104.6 |
| Document symbols | ✅ | ✅ | ✅ 104.4 |
| Workspace symbols | ✅ | ✅ | ✅ 104.4 |
| Format-on-save | ✅ rustfmt | ✅ gofmt | ✅ 104.6 (`nova fmt`) |
| Inlay hints | ✅ types | ✅ params | 🟡 V2 (deferred) |
| Semantic tokens | ✅ | ✅ | 🟡 V2 (deferred) |
| Call hierarchy | ✅ | ✅ | 🟡 V2 (deferred) |
| Debug adapter (DAP) | ✅ | ✅ (delve) | 🔴 separate plan (post-LLVM, requires native codegen) |

**Что НЕ входит в V1 (отложено на V2 или separate plan):**
- Inlay hints, semantic tokens, call hierarchy — nice-to-have, не блокеры.
- Debug Adapter Protocol (DAP) — требует native codegen (Plan 38 LLVM) или mature interp-debugger, отдельный план.
- JetBrains native plugin — Kotlin/Java + IntelliJ SDK, отдельный план (либо 3rd-party LSP plugin).
- Refactorings (extract function/extract type) — V2 (rename — самое нужное в V1).

## Архитектура

### Топология процессов

```
Editor (VSCode/Cursor/Helix/Zed/Neovim)
   │
   │ LSP JSON-RPC over stdio
   ▼
nova-lsp (Rust binary, separate crate)
   │
   │ in-process API
   ▼
compiler-codegen (Rust lib — reused)
   ├─ lexer/
   ├─ parser/
   ├─ types/  ← реtype-check incremental
   ├─ ast/    ← reuse spans для goto-def
   └─ doc/    ← D104/D105 doc-comments для hover
```

**Ключевое решение (M-1):** `nova-lsp` — **отдельный binary crate** в workspace. Reuse `compiler-codegen` как library dependency. Никакого fork/copy. Это:
- ✅ Изолирует LSP-сложности (tower-lsp, tokio, dashmap, ropey) от nova-cli (быстрый CLI без async).
- ✅ Позволяет публиковать nova-lsp как отдельный binary (editor installers скачивают только его).
- ✅ Compiler bug fixes автоматически проявляются в LSP — single source of truth для типчекера.

**Альтернатива (отвергнута):** `nova lsp` subcommand. Минус — nova-cli раздуется до ~30MB (tower-lsp + tokio runtime), увеличит cold-start время CLI команд в 3-5x.

### Stack (M-2)

- **tower-lsp** (Rust LSP framework) — handles JSON-RPC, message routing, tower-service middleware. Mature, used by rust-analyzer-style projects.
- **tokio** — async runtime.
- **dashmap** — concurrent file cache (file-uri → parsed module).
- **ropey** — efficient text rope для inc-edits (multi-byte chars / large files).
- **anyhow + tracing** — error handling + structured logging.

Все эти deps уже широко используются в Rust LSP ecosystem.

### Incremental compilation strategy (M-3)

V1: **re-typecheck whole module on file save / didChange (debounced ~200ms).** Plan 04-package-system + ModuleEnv cache reuse возможен — V2.

Justification: full-recheck для типичного Nova-модуля (10-50 файлов peer) занимает <1s в release-build. UI-perceived latency приемлема. Industry: gopls тоже full-recheck в V1, инкремент пришёл позже.

### State management (M-4)

- **WorkspaceState** (single shared, RwLock): map<file-uri, ParsedFile>.
- **ParsedFile:** `(text rope, parsed Module, last_diagnostics)`.
- **Recompile trigger:** `didChange` event → debounce 200ms → background task → full check → publish diagnostics.
- **Cancellation:** новый `didChange` cancel'ит pending recompile.

## Decomposition deep-dive

### 104.0 — Foundation (`nova-lsp` crate setup) — ~2 dev-day ✅ ЗАКРЫТ 2026-05-25

**Out:**
- `nova-lsp/Cargo.toml`: tower-lsp ^0.20, tokio ^1 (full), dashmap ^5, ropey ^1,
  anyhow ^1, tracing ^0.1, tracing-subscriber ^0.3, clap ^4.
- `nova-lsp/src/main.rs` — `#[tokio::main]`, tracing init to stderr (NOVA_LSP_LOG env),
  `LspService::new(Backend::new)` + `Server::new(stdin, stdout, socket).serve()`.
- `nova-lsp/src/server.rs` — `Backend { client, state, shutdown_requested: Arc<AtomicBool> }`;
  `initialize()` (UTF-16, Full sync, serverInfo), `initialized()`, `shutdown()`,
  `did_open/did_change/did_close` handlers.
- `nova-lsp/src/state.rs` — `WorkspaceState { docs: DashMap<Url, ParsedFile> }`;
  `ParsedFile { text: Rope, version: i32 }`.
- `nova-lsp/README.md` — build + VSCode/Neovim/Helix editor config snippets.

**Tests: 22/22 PASS**
- `tests/build_smoke.rs` (3): binary exists + --help, --version, closed-stdin graceful exit
- `tests/lifecycle.rs` (3): initialize capabilities, full lifecycle exit-0, duplicate init → -32600
- `tests/document_cache.rs` (5 integration + 8 unit in state.rs):
  didOpen/didChange/didClose server-alive; neg1 unopened-change warn; neg2 double-open warn
- `tests/integration.rs` (3): full handshake exit-0, open/change/close alive, malformed JSON no-panic

**V1 simplifications (documented in simplifications.md):**
- textDocumentSync: Full (incremental via Plan 104.6 V2)
- nova_codegen not yet linked (gate: Plan 104.1 diagnostics)

**Acceptance:** `cargo build -p nova-lsp` → `nova-lsp.exe`; full JSON-RPC handshake
works; 22 tests pass; `echo '{"jsonrpc":"2.0","id":1,"method":"initialize",...}' | nova-lsp`
returns valid initialize response.

### 104.1 — Diagnostics + file watching — ✅ ЗАКРЫТ 2026-05-26

**Out:**
- `check_file` / `check_workspace` compiler adapter (catch_unwind + run_with_large_stack 64 MiB).
- `diagnostic_mapping`: Nova Span → LSP Range (UTF-16 positions), DiagnosticRelatedInformation.
- `incremental`: TextDocumentSyncKind::INCREMENTAL rope edits via ropey.
- `debouncer`: 200ms per-URI CancellationToken coalescing (std::sync::Mutex, race-free cancel_all).
- `didOpen` / `didChange` / `didSave` / `didClose` handlers with `publishDiagnostics`.
- PerfTimer + `measure!` macro (tracing::debug).
- 5 new test modules; 91 tests total (all PASS).

**Acceptance:** ✅ `cargo test -p nova-lsp` → 91/91 PASS; `publishDiagnostics` fires on open/change/save/close.
[Sub-plan: 104.1-lsp-diagnostics.md]

### 104.2 — Hover + goto-definition + signature help — ~3 dev-day

**Out:**
- `hover` handler — given (file, pos): resolve symbol → render type + doc-comment (D104/D105).
  - Variable hover: `let x int = 5` → `let x: int`.
  - Function hover: full signature + doc-block.
  - Type hover: declaration + doc-block.
- `definition` handler — resolve symbol → return span of declaration (cross-file via module resolution).
- `signatureHelp` handler — inside `f(│cursor│, ...)`: show parameter list + which param is active.

**Reuse:** `compiler-codegen::ast` имеет `Span` на всех node-ах. Symbol resolution делается через TypeCheckCtx scope walk — нужен exposed API.

**New work in compiler-codegen:**
- `pub fn resolve_symbol_at(module: &Module, pos: BytePos) -> Option<Symbol>` — для hover/goto.
- Symbol: enum { LocalVar | FnDecl | TypeDecl | MethodDecl | ProtocolDecl | ... } с span + type info.

**Acceptance:** VSCode Ctrl+Click на функции → переход к declaration; hover на переменной → тип + (если есть) doc-comment.

### 104.3 — Completion — ~5 dev-day

**Самая сложная фаза.** Несколько триггеров:

- **Keyword completion** в top-level / fn-body — `fn`, `type`, `let`, `mut`, `if`, `for`, `while`, `match`, `effect`, `protocol`, etc.
- **Identifier completion** — in-scope variables, types, functions (scope walk from cursor position).
- **Method completion после `.`** — resolve obj-type → enumerate methods (`compiler-codegen::method_overloads` + protocol-method registries).
- **Import completion** — `import std.collections.│` → list submodules from manifest.
- **Snippets** — `fn ${1:name}(${2:params}) -> ${3:RetTy} => ${4}`.

**Sub-decomposition:**
- 104.3.1 keyword/snippet completion (~1 dev-day)
- 104.3.2 in-scope identifier completion (~1 dev-day)
- 104.3.3 method completion после `.` (~1.5 dev-day) — самое ценное для AI/dogfooding
- 104.3.4 import path completion (~1 dev-day)
- 104.3.5 ranking + filtering polish (~0.5 dev-day)

**Acceptance:** VSCode Ctrl+Space в разных контекстах даёт relevant suggestions; type `x.` → list of methods on x's type.

### 104.4 — Document/workspace symbols + find-references — ~3 dev-day

**Out:**
- `documentSymbol` — outline для VSCode (functions, types, tests в файле).
- `workspaceSymbol` — Ctrl+T поиск по всему проекту: `?MyFn` → matches across files.
- `references` — Shift+F12: find все usages символа.

**Acceptance:** outline в VSCode левой панели показывает структуру файла; Shift+F12 на функции — список call sites.

### 104.5 — Code actions + quick-fixes — ~5 dev-day

**Cumulative absorber для Plan 100.8 + Plan 101 LSP markers.**

V1 quick-fix coverage — **минимум 25 fixes** (parity с gopls):

Plan 101 errors (8):
- `E_UNDECLARED_TYPEVAR_IN_RECEIVER` — auto-add `fn[T]` prefix.
- `E_BARE_TYPEVAR_NEEDS_PREFIX` — same.
- `E_DUPLICATE_GENERIC_DECL` — remove duplicate.
- `E_PREFIX_SHADOWS_NAMED_TYPE` — rename prefix-generic (`T` → `T1`).
- `E_UNUSED_PREFIX_TYPEVAR` — remove unused.
- `E_BOUND_UNKNOWN` — suggest import / list candidate protocols.
- `E_BOUND_NOT_PROTOCOL` — suggest converting type to protocol.
- `E_PROTOCOL_EMBED_*` (5 vars) — appropriate auto-fixes.

Plan 100 errors (12 from Plan 100.8 D166):
- `E_MUST_CONSUME_NOT_CONSUMED` — auto-add `consume` keyword.
- `E_CONSUME_RVALUE_IN_ARG_POSITION` — wrap `consume(expr)`.
- ... (full list in Plan 100.8 Ф.2.D2).

General (5+):
- `unused import` — remove line.
- `missing import` — auto-add import suggestion.
- Typo in identifier — suggest closest match.
- `prefer let over var` (style).
- Auto-derive `Display`/`Hashable` для record/sum types.

**Sub-decomposition:**
- 104.5.1 Quick-fix framework + machine-applicable infrastructure (~1 dev-day)
- 104.5.2 Plan 101 quick-fixes (8 fixes) (~1 dev-day) — closes Plan 101 LSP marker
- 104.5.3 Plan 100 quick-fixes (12 fixes) (~1.5 dev-day) — closes Plan 100.8 Ф.2 (LSP layer)
- 104.5.4 General quick-fixes (~1 dev-day)
- 104.5.5 Auto-import + organize-imports (~0.5 dev-day)

**Acceptance:** VSCode 💡 лампочка на каждом из ≥25 error codes; click → код исправляется автоматически.

### 104.6 — Rename + format-on-save — ~4 dev-day

**Out:**
- `rename` handler — cross-file. Reuse find-references + replace.
- `prepareRename` — validate cursor position (only on identifiers).
- `formatting` / `rangeFormatting` — invoke `nova fmt` (existing CLI command from Plan 01) on file → return text edits.

**Edge cases for rename:**
- Symbol used in another module (через import) — must update all usages.
- Receiver shadowing — `fn Foo @bar()` — rename `bar` should NOT touch other `bar` methods.
- Generic params — `fn f[T](...)` rename `T` → `U` only in scope.

**Acceptance:** F2 на функции → rename, все usages updates; save файла → форматирование применилось.

### 104.7 — Tree-sitter grammar — ✅ ЗАКРЫТ 2026-05-25

**Repo:** `nv-lang/tree-sitter-nova`, tag `v0.1.0`. Standard tree-sitter project layout (`grammar.js`, generated `parser.c`).

**Результат:** 84/84 corpus fixtures (100%), 5 query files, editor dist configs (Helix/Zed/Neovim).
See [RELEASE_NOTES](https://github.com/nv-lang/tree-sitter-nova/blob/main/RELEASE_NOTES.md).

**Separate repo:** `nv-lang/tree-sitter-nova`. Standard tree-sitter project layout (`grammar.js`, generated `parser.c`).

**Why separate:** tree-sitter generated parsers — language-agnostic artefact. Helix, Zed, Neovim, GitHub web syntax все pull это автоматически.

**Out:**
- `grammar.js` — переписать grammar (mirror Nova parser productions). NOT auto-generated from Nova lexer/parser (tree-sitter syntax — JS DSL, нужно вручную).
- `queries/highlights.scm` — semantic highlight queries (keyword/type/function/variable).
- `queries/folds.scm` — code folding regions (fn body, type body, comment blocks).
- `queries/indents.scm` — indentation rules (для editor auto-indent).
- `queries/injections.scm` — string-interpolation, doc-comment markdown injection.
- Test corpus: `test/corpus/*.txt` — Nova snippets + expected parse trees.

**Distribution:**
- GitHub release `tree-sitter-nova-vX.Y.Z` → auto-picks-up by `nvim-treesitter`, `helix-editor/runtime/grammars`, Zed extensions registry, GitHub linguist.

**Acceptance:** Helix opens `.nv` file → syntax highlighting + folding работает; Zed extension marketplace показывает Nova.

### 104.8 — Editor packaging + distribution — ~3 dev-day ✅ ЗАКРЫТ 2026-05-26

**Out (реализовано):**
- **VSCode/Cursor/VSCodium extension** (TypeScript LSP client) — `editors/vscode/`:
  - `client/extension.ts` — LanguageClient, binary discovery (setting→PATH→workspace),
    auto-restart on crash (max 3 errors), configurable `nova.lsp.path` setting.
  - `package.json` activation events, config schema, vscode-languageclient ^9.0.
  - `tsconfig.json` + tests (7/7 PASS via @vscode/test-electron 2.5.2 / VSCode 1.121.0).
  - Marketplace publishing deferred [M-104.8-vscode-marketplace].
- **Neovim** — `editors/neovim/`:
  - `lspconfig.lua` — nvim-lspconfig snippet (idempotent, binary discovery).
  - `ftdetect.lua` — .nv → filetype "nova".
  - `UPSTREAM_PR_DRAFT.md` — draft PR for neovim/nvim-lspconfig.
- **Helix** — `editors/helix/languages.toml`:
  - LSP entry, auto-pairs (', backtick per D104.8-5), grammar ref tree-sitter-nova v0.1.0.
  - TOML validated, hx smoke skip [M-104.8-tool-hx-unavailable].
- **Zed** — `editors/zed/`:
  - `extension.toml` (schema_version=1, grammar sha 99111569), config.toml, highlights.scm.
  - Side-load install documented. Marketplace deferred [M-104.8-zed-marketplace].
- **Common installation guide** — `editors/INSTALL.md` (all 4 editors, troubleshooting).
- **Smoke** — `editors/SMOKE_LOG.md` + `editors/test_smoke.nv`.

**Acceptance:** ✅ VSCode F5 → diagnostics. Neovim snippet works (docs). Helix TOML valid. Zed side-load documented.

### 104.9 — Close-out: integration tests + marker closure + release notes — ~2 dev-day

**Out:**
- E2E integration tests for top-3 editors (headless if possible):
  - VSCode extension test runner.
  - Helix smoke test (spawn editor, open file, verify LSP attached via log).
  - Neovim smoke test (`:LspInfo` check).
- Closure markers:
  - Plan 100.8 Ф.2 (LSP layer) → ✅.
  - Plan 101 LSP V2 marker → ✅.
  - Plan 01-roadmap §165 «LSP v0.5» → ✅.
- Release notes — feature announcement.
- Docs:
  - `docs/ide/README.md` — overview, supported editors, installation.
  - `docs/ide/troubleshooting.md` — common issues.

## Design decisions

| ID | Decision | Status |
|---|---|---|
| M-1 | Separate `nova-lsp` crate (vs nova-cli subcommand) | ✅ user-confirmed |
| M-2 | Stack: tower-lsp + tokio + dashmap + ropey | ✅ industry-standard |
| M-3 | V1 strategy: debounced full-module recheck (200ms). Incremental — V2 | ✅ matches gopls v1 |
| M-4 | WorkspaceState = single RwLock<HashMap<Uri, ParsedFile>> | ✅ |
| M-5 | Target editors V1: VSCode/Cursor/VSCodium + Neovim + Helix + Zed | ✅ user-confirmed |
| M-6 | Tree-sitter — sub-plan 104.7 (separate repo `tree-sitter-nova`) | ✅ user-confirmed |
| M-7 | JetBrains — deferred к separate Plan (Kotlin + IntelliJ SDK, не absorption LSP) | ✅ |
| M-8 | DAP (debug adapter) — отдельный план после native codegen (LLVM / Plan 38) | ✅ deferred |
| M-9 | V1 absorbs Plan 100.8 Ф.2 (consume quick-fixes) + Plan 101 LSP V2 (8 fixes) | ✅ |
| M-10 | Quick-fix coverage ≥25 fixes (parity с gopls V1) | ✅ |
| M-11 | Reuse `compiler-codegen` as library (NOT fork) | ✅ |
| M-12 | Inlay hints / semantic tokens / call hierarchy — V2 | ⚠️ deferred |
| M-13 | Refactorings (extract function/type) — V2 | ⚠️ deferred |

## Dependencies graph

```
104.0 (foundation)
  ├─> 104.1 (diagnostics)
  │    ├─> 104.2 (hover/goto/signature)
  │    │    ├─> 104.3 (completion)
  │    │    └─> 104.4 (symbols/references)
  │    │         └─> 104.5 (code actions/quick-fixes)
  │    │              └─> 104.6 (rename/format)
  └─> 104.7 (tree-sitter) ── parallel to 104.1-104.6
  └─> 104.8 (editor packaging) ── after 104.6 + 104.7
  └─> 104.9 (close-out + tests) ── after 104.8
```

**Critical path:** 104.0 → 104.1 → 104.2 → 104.5 → 104.6 → 104.8 → 104.9 = ~22 dev-day (4-5 calendar weeks для blocking sub-plans). Tree-sitter (104.7) + completion (104.3) можно делать параллельно — добавляет 5 dev-day если параллельная работа.

## Risks

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| compiler-codegen API instability — каждый refactor типчекера ломает LSP | Medium | High | Plan 104 starts ПОСЛЕ Plan 91/100 (stabilization). Заморозить публичный API типчекера (`pub fn check_module`) в начале Plan 104. |
| Tree-sitter grammar drift от Nova parser | Low | Medium | Test corpus в 104.7 — на каждом Plan-merge запускать; CI gate. |
| LSP performance на больших файлах | Medium | Medium | Profiling в 104.1; если >1s — incremental check вне V1 scope, добавить sub-plan 104.6.5. |
| VSCode marketplace publishing trust review | Low | Low | Self-hosted .vsix install в V1; marketplace submission в 104.9. |
| Helix/Zed adoption: их API меняется чаще | Low | Medium | Just docs в V1; extension activation тестируется только в 104.9. |

## Acceptance criteria (Plan 104 closure)

1. ✅ `nova-lsp` binary builds + integration test green.
2. ✅ VSCode extension installed → diagnostics + hover + goto-def + completion + rename + format работают на live Nova file.
3. ✅ Neovim/Helix/Zed configs documented + smoke-tested.
4. ✅ Tree-sitter grammar pushed в `tree-sitter-nova` repo + Helix runtime PR submitted.
5. ✅ ≥25 quick-fixes implemented (closes Plan 100.8 Ф.2 + Plan 101 LSP marker).
6. ✅ Closure markers:
   - [M-101-lsp-quickfixes-deferred] ✅
   - [M-100-impl-deferred] partial (only LSP-side, implementation отдельно)
   - Plan 01-roadmap §165 «LSP v0.5» ✅

## Open questions (to resolve during work)

- **Q-104-1** Incremental compilation strategy V2 — separate sub-plan или absorb в 104.6? (Решить при profiling в 104.1.)
- **Q-104-2** Should LSP support `nova run`/`nova test` task integration (test runner UI)? Possibly 104.10 (V2).
- **Q-104-3** AI-completion hook (Copilot/Cursor) — special LSP extension method or vanilla completion sufficient? Defer.

## Lineage / related plans

- Plan 01-roadmap §165 — обещал v0.5.
- Plan 100.8 Ф.2 — частично absorbed в 104.5.
- Plan 101 LSP marker — absorbed в 104.5.
- Plan 45 (doc-comments D104/D105) — surfaced в 104.2 hover.
- Plan 04 (package manager) — namespace-aware completion в 104.3.4.

## Next step

Старт работы — после **Plan 91 std MVP closure** + **Plan 100 implementation landing**. Эти два — обязательная стабилизация API surface. Минимум 2-3 недели ожидания. Параллельно можно writе sub-plan files (104.0-104.9) для каждой фазы.
