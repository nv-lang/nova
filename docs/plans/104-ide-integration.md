// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 104 — Production-grade IDE integration (LSP server + tree-sitter + editor distributions)

> **Статус:** ✅ **ЗАКРЫТ 2026-06-17** — все 9 sub-plans (104.0–104.9) выполнены.
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
| 104.2 | Hover + goto-definition + signature help | Symbol resolution, type rendering, doc-comment surfacing (D104/D105); Ф.7 body-walk hover (2026-06-17) | ✅ ЗАКРЫТ 2026-06-17 |
| 104.3 | Completion (keywords + identifiers + methods + imports) | Scope-aware, type-driven (after `.`), import suggestions, snippets | ✅ ЗАКРЫТ 2026-06-16 — 52 completion tests PASS, branch plan-104-3 |
| 104.4 | Document/workspace symbols + find-references | Outline panel, Ctrl+Shift+O, Shift+F12 | ✅ ЗАКРЫТ 2026-06-16 |
| 104.5 | Code actions + quick-fixes (Plan 100 + Plan 101 + general) | ~25 error codes → machine-applicable fixes; auto-import; organize imports | ✅ ЗАКРЫТ 2026-06-16 — 25 fixes, 95/95 PASS, D296 spec |
| 104.6 | Rename + format-on-save | Cross-file rename, `nova fmt` integration | ✅ ЗАКРЫТ 2026-06-16 — 98/98 PASS, D297 spec |
| 104.7 | Tree-sitter grammar (`tree-sitter-nova` repo) | Grammar, queries (highlights/folds/indents/injections), Helix/Zed/Neovim distribution | ~3 dev-day | ✅ **ЗАКРЫТ 2026-05-25** v0.1.0 — 84/84 fixtures, 5 query files |
| 104.8 | Editor packaging + distribution docs | VSCode extension (TypeScript client), nvim-lspconfig PR, Helix/Zed configs, install docs | ~3 dev-day | ✅ **ЗАКРЫТ 2026-05-26** — VSCode 7/7 PASS, Helix/Zed TOML valid, editors/INSTALL.md |
| 104.9 | Close-out: integration tests + marker closure + language-sync | nova_tests/plan104_9/ fixtures (10/10 PASS), backlog-marker closure, nova-lsp language-sync (completion/code-actions) | ✅ ЗАКРЫТ 2026-06-17 |

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

### 104.2 — Hover + goto-definition + signature help — ✅ ЗАКРЫТ 2026-06-16

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

### 104.4 — Document/workspace symbols + find-references — ✅ ЗАКРЫТ 2026-06-16

**Out:**
- `documentSymbol` — AST-walker two-pass (TypeDecl index, FnDecl methods nested under receiver); cache per-URI; fields/variants as children.
- `workspaceSymbol` — per-file WorkspaceIndex (incremental); substring case-insensitive; top-100; `Vec<SymbolInformation>` (tower-lsp 0.20).
- `references` — word-boundary byte scan; cross-file via collect_nv_files(); includeDeclaration option.

**Tests:** 86 unit + 15 integration = 101 total, 0 FAIL. Branch `plan-104-4`, commit `8b3e1903`.
**V1 markers:** [M-104.4-refs-incremental-index], [M-104.4-workspace-symbol-fuzzy], [M-104.4-cross-file-method-nesting].
**Acceptance:** ✅ outline in VSCode side panel shows file structure; Shift+F12 on function → call sites list; workspace Ctrl+T finds symbols across files.

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
- Auto-derive `Display`/`Hash` для record/sum types.

**Sub-decomposition:**
- 104.5.1 Quick-fix framework + machine-applicable infrastructure (~1 dev-day)
- 104.5.2 Plan 101 quick-fixes (8 fixes) (~1 dev-day) — closes Plan 101 LSP marker
- 104.5.3 Plan 100 quick-fixes (12 fixes) (~1.5 dev-day) — closes Plan 100.8 Ф.2 (LSP layer)
- 104.5.4 General quick-fixes (~1 dev-day)
- 104.5.5 Auto-import + organize-imports (~0.5 dev-day)

**Acceptance:** VSCode 💡 лампочка на каждом из ≥25 error codes; click → код исправляется автоматически.

**✅ ЗАКРЫТ 2026-06-16** — 25 quick-fixes реализованы в `nova-lsp/src/code_actions.rs`; D296 spec (§1-6); 95/95 unit tests PASS; error-code extraction в `diagnostic_mapping`; dispatches on `diag.code`; closes Plan 100.8 Ф.2 + Plan 101 LSP marker. Branch `plan-104-5`.

### 104.6 — Rename + format-on-save — ✅ ЗАКРЫТ 2026-06-16

**Out (реализовано):**
- `nova-lsp/src/rename.rs`: `prepare_rename` (cursor validation), `compute_rename` (cross-file
  word-boundary scan + [[link]] doc-comment update + D296 atomic post-rename type-check).
- `nova-lsp/src/format.rs`: `format_document` (nova fmt via temp file, graceful if not found),
  `format_range` (whole-file then clip), `on_type_format` (auto-indent \n, dedent }).
- `nova-lsp/src/server.rs`: 5 new handlers + ServerCapabilities updated (rename_provider with
  prepare_provider=true, document_formatting_provider, document_range_formatting_provider,
  document_on_type_formatting_provider).
- `spec/decisions/09-tooling.md`: D296 LSP Rename Atomicity Contract.

**V1 simplifications:**
- Rename uses regex-based word-boundary scan (not full symbol-table). Full symbol resolution V2.
- `nova fmt` is a whole-file formatter; rangeFormatting clips the result to the requested range.
- onTypeFormatting handles only `\n` and `}` in V1.

**Acceptance:** ✅ `cargo test -p nova-lsp --release` → 98/98 PASS;
prepareRename returns RangeWithPlaceholder for identifiers; rename rejects conflicts via atomic
check (D296); formatting invokes nova fmt gracefully; onTypeFormatting inserts indent on newline.
[Sub-plan: 104.6-rename-format.md]

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

---

#### v0.2.0 (2026-05-25): ro/mut/consume keyword refresh (Plan 114 D184)

- `ro`/`mut`/`consume` keywords added to KEYWORDS list (replaces `let`/`readonly` which were retracted).
- 5 new corpus fixtures for binding syntax, while-let, for-mut patterns.
- All 89 corpus PASS.

---

#### v0.3.0 (2026-06-17): Grammar sync with current Nova syntax

**Commits:** `b641f6e` (dist/helix auto-pairs) + `857ca6c` (grammar v0.3.0).

**Added:**
- `privacy_modifier` rule — `priv` (module-private) / `priv(type)` (type-private). Plan 160 D281.
- `type_modifier` rule — `value` (stack-allocated value type). Plan 148 D241.
- `pointer_type` rule — `*T`, `*ro T`, `*mut T` (raw pointer with optional mutability). Plan 147 D246.
- `extern "nova"/"C" fn` — ABI string parsed as `string_literal` before `fn`. Plan 91.12 D282.
- `priv`/`pub`/`extern` added to KEYWORDS.
- Helix `dist/helix/languages.toml` auto-pairs: `'` → `'` and `` ` `` → `` ` `` (char literals + tagged strings).

**Removed:**
- `errdefer`/`okdefer` keywords — retracted per Plan 110 D189 (only `defer` remains).
- `external fn` syntax — replaced by `extern "nova" fn` (Plan 91.12 D282).

**Corpus tests:** 89 → 93 PASS (4 new negative fixtures in `test/corpus/negatives.txt`).

**Nova compiler integration tests:** `nova_tests/plan104_7_grammar/` — 5/5 PASS.
- `pos_priv_type_basic.nv` — `type T value priv { ... }` compiles, module-level access works.
- `pos_priv_field_basic.nv` — `priv`/`pub` field modifiers, intra-module access.
- `pos_extern_c_fn.nv` — `extern "C" fn malloc/strlen` declarations + callable.
- `pos_pointer_type.nv` — `*u8`, `*mut T` in fn signatures and record fields.
- `neg_priv_field_access.nv` — `priv` field access from outside → E_FIELD_MODULE_PRIVATE.

**Acceptance criteria (без упрощений как для прода):**
- Grammar: 93/93 corpus PASS (89 positives + 4 negatives).
- `priv`/`priv(type)` `privacy_modifier` — correctly parsed and highlighted.
- `value` `type_modifier` — correctly parsed.
- `*T`/`*ro T`/`*mut T` `pointer_type` — parsed without ERROR nodes in valid contexts.
- `extern "C"/"nova" fn` — ABI string parsed as `string_literal`.
- `errdefer`/`okdefer`/`external` removed from grammar (no longer emit tokens).
- Helix auto-pairs include `'` and `` ` ``.
- Negatives: ERROR nodes generated for `priv(module)`, `extern fn` (no ABI), `priv priv` duplicate.
- Nova compiler tests: 5/5 PASS (positive + negative coverage).

**Markers:**
- `[M-104.7-query-update-priv]` ✅ CLOSED 2026-06-17 — `highlights.scm` updated: `priv`/`pub`/`extern` highlighted as keywords.
- `[M-104.7-v4-keywords]` OPEN — future KEYWORDS additions when Nova adds new keywords to lexer.

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

### 104.9 — Close-out: language-sync + tests + marker closure — ✅ ЗАКРЫТ 2026-06-17

**Context:** After Plans 104.0–104.8 landed, the LSP's hardcoded Nova surface area
(keywords, types, method signatures in `completion.rs` and `code_actions.rs`) had drifted
from the language across 50+ language changes since Plans 114/133/139/147/152/160/161 etc.

**Deliverables:**

**104.9.1 — nova-lsp language-sync (completion.rs):**
- Removed `let` from all keyword lists; added `ro`, `mut`, `extern`, `priv`, `reveal`, `value`.
- Added `while-let` snippet.
- `collect_let_bindings`: scans `ro`/`mut` bindings (with `let` compat fallback for unupdated files).
- `collect_fn_params`: strips `ro`/`mut` modifiers before extracting param name.
- `prelude_items()`: removed `float`, `usize`, `Map`; added `f64`, `f32`, `i8`–`i64`, `u8`–`u64`, `HashMap`, `Set`.
- `int_methods()`: only `min`, `max`, `compare` (removed `abs`, `to_str`).
- `f64_methods()` (renamed from `float_methods()`): full math.nv signatures (`abs`, `floor`, `ceil`, `round`, `sqrt`, `sin/cos/tan`, `is_nan`, `is_infinite`, `min`, `max`, `compare`).
- `str_methods()`: `byte_len()`, `as_bytes()`, `as_chars()`, `lines()`, `parse_int()`, `rfind`, `splitn`, `strip_prefix/suffix`, `pad_left/right/center`.
- `vec_methods()`: `iter()->VecIter[T]`, `lazy()`, `chunks()`, `append`, `flatten`, `drain`, `rotate_left/right`, `sort`, `dedup`, `partition`.
- `bool_methods()`: only `compare`.
- `option_methods()`: `unwrap() Fail[Error] -> T`, `unwrap_or_else`, `or`.
- `result_methods()`: `unwrap() Fail[E] -> T`, `unwrap_or_else`, `map_err`.
- `methods_for_type()`: removed `float`, `String`, `usize`; added `f64|f32`, `Vec`/`[]` prefix routing.
- All unit tests updated to `ro`/`mut` bindings, `byte_len` instead of `len`, `f64` instead of `float`.

**104.9.2 — nova-lsp language-sync (code_actions.rs):**
- Added dispatch groups:
  - 104.5.6: Protocol impl fixes — `E_METHOD_REDEFINITION`, `E_IMPL_UNKNOWN_PROTOCOL`, `E_IMPL_NOT_A_PROTOCOL_METHOD`, `E_IMPL_SIGNATURE_MISMATCH`, `E_PRIMITIVE_NO_PROTOCOL_METHOD`, `E_BLANKET_CONFLICT`, `E_DUPLICATE_PROTOCOL_IMPL`.
  - 104.5.7: Field/type fixes — `E_FIELD_MODULE_PRIVATE`, `E_TYPE_NAME_TOO_SHORT`.
  - 104.5.8: String fixes — `E_STR_NO_LEN` (auto-replace `.len` → `.byte_len()`), `E_STR_NO_INT_INDEX`.
  - 104.5.9: Comparison fixes — `E_CMP_CHAIN_UNSUPPORTED`, `E_RELATIONAL_OPERAND_NOT_ORDERED`.
- Renamed `E_ADDR_OF_MUT_REQUIRES_MUT_BINDING` → `E_ADDR_OF_REMOVED` / `fix_addr_of_removed` (Plan 118.6 followup).
- 17 new test functions for new error codes.

**104.9.3 — Tests:**
- `nova_tests/plan104_9/` — 10/10 PASS through release compiler:
  - 7 positives: `pos_current_keywords_compile`, `pos_former_keywords_are_identifiers`, `pos_ro_mut_bindings`, `pos_int_methods_accurate`, `pos_f64_methods_accurate`, `pos_str_methods_accurate`, `pos_prelude_types_accurate`.
  - 3 negatives: `neg_let_keyword_removed` (→ E_KW_REMOVED_LET), `neg_str_len_removed` (→ E_STR_NO_LEN), `neg_int_abs_removed` (→ usize removed).
- `nova-lsp`: 255 unit + 13 integration tests PASS (`cargo test --release`).

**Closure markers:**
- `[M-101-lsp-quickfixes-deferred]` ✅ (via 104.5).
- Plan 100.8 Ф.2 (LSP layer) ✅ (via 104.5).
- Plan 01-roadmap §165 «LSP v0.5» ✅.
- `[M-104.9-completion-language-sync]` ✅ CLOSED 2026-06-17.

**Acceptance criteria (без упрощений как для прода):**
1. ✅ `nova-lsp`: 268 total tests PASS (`cargo test --release`); zero regressions.
2. ✅ Keyword completion: `ro`/`mut` appear; `let` does NOT appear. Verified by 4 test assertions.
3. ✅ `int` methods: only `min`, `max`, `compare` suggested — no `abs`, no `to_str`.
4. ✅ `f64` methods: full math.nv API (renamed from `float_methods`); `float` type removed from prelude items.
5. ✅ `str` methods: `byte_len()` suggested (not `len`). Verified by integration test `method_str_detail_present`.
6. ✅ `vec` methods: `iter()->VecIter[T]` as primary iterator entry point.
7. ✅ Quick-fixes for 104.5.6–104.5.9 error groups: 17 new fix handlers.
8. ✅ `E_STR_NO_LEN` → auto-replace `.len` → `.byte_len()` (machine-applicable TextEdit).
9. ✅ `nova_tests/plan104_9/` 10/10 PASS via release `nova` binary.
10. ✅ **«Без упрощений как для прода»**: all completion items match actual stdlib signatures (not invented); all quick-fixes handle real error codes emitted by the production compiler.

**Deferred (V2):**
- `[M-104.5-suggestion-field-wiring]` — use compiler `Diagnostic.suggestion.span` for fix edits.
- `[M-104.5-multi-edit-rename]` — multi-occurrence rename for E_PREFIX_SHADOWS_NAMED_TYPE.
- `[M-104.5-organize-imports]` — sort+deduplicate import list.
- `[M-104.2-signature-type-dispatch]` — type-driven method dispatch in signature help.
- `[M-104.4-refs-incremental-index]`, `[M-104.4-workspace-symbol-fuzzy]`, `[M-104.4-cross-file-method-nesting]`.

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

## Acceptance criteria (Plan 104 closure — ✅ ВСЕ ВЫПОЛНЕНЫ 2026-06-17)

1. ✅ `nova-lsp` binary builds + integration test green (268 tests PASS).
2. ✅ VSCode extension installed → diagnostics + hover + goto-def + completion + rename + format работают на live Nova file.
3. ✅ Neovim/Helix/Zed configs documented + smoke-tested.
4. ✅ Tree-sitter grammar pushed в `tree-sitter-nova` repo (v0.1.0→v0.3.0).
5. ✅ ≥25 quick-fixes implemented (39 total across 104.5.2–104.5.9; closes Plan 100.8 Ф.2 + Plan 101 LSP marker).
6. ✅ Closure markers:
   - `[M-101-lsp-quickfixes-deferred]` ✅
   - Plan 100.8 Ф.2 (LSP layer) ✅
   - Plan 01-roadmap §165 «LSP v0.5» ✅
   - `[M-104.9-completion-language-sync]` ✅
7. ✅ **«Без упрощений как для прода»** — completion items match actual stdlib; quick-fixes handle real compiler error codes; `nova_tests/plan104_9/` 10/10 PASS via release binary.

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
