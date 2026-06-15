# Nova Language — Editor support

Поддержка `.nv` файлов в редакторах. Все варианты — **локальные** plugin'ы
(через symlink/copy), потому что Nova ещё не публикуется в официальных
marketplace'ах.

→ Инструкция установки для всех редакторов: **[INSTALL.md](INSTALL.md)**

## Поддерживаемые редакторы

| Редактор | Подкаталог | Syntax | LSP | Установка |
|---|---|---|---|---|
| **VSCode** | [`vscode/`](vscode/) | ✅ TextMate grammar | ✅ nova-lsp client | F5 (dev) или symlink в `~/.vscode/extensions/` |
| **Cursor** | [`vscode/`](vscode/) | ✅ то же что VSCode | ✅ то же что VSCode | symlink в `~/.cursor/extensions/` |
| **VSCodium** | [`vscode/`](vscode/) | ✅ то же что VSCode | ✅ то же что VSCode | symlink в `~/.vscode-oss/extensions/` |
| **Neovim** | [`neovim/`](neovim/) | ✅ Vim syntax | ✅ nvim-lspconfig snippet | copy lspconfig.lua + ftdetect.lua |
| **Helix** | [`helix/`](helix/) | ✅ tree-sitter | ✅ nova-lsp language-server | append languages.toml fragment |
| **Zed** | [`zed/`](zed/) | ✅ tree-sitter | ✅ nova-lsp side-load | copy extension folder |
| **Vim** | [`vim/`](vim/) | ✅ native syntax | ❌ (нет LSP) | symlink в `~/.vim/` |
| **Emacs** | [`emacs/`](emacs/) | ✅ nova-mode.el | ❌ (нет LSP) | `(load-file ...)` |
| **Sublime Text** | [`sublime/`](sublime/) | ✅ TextMate re-use | ❌ (нет LSP) | symlink в `Packages/Nova/` |

## LSP — nova-lsp

Все LSP-фичи реализованы через единый binary `nova-lsp`, который работает по
stdio JSON-RPC. Каждый редактор конфигурирует только как его запустить.

```
editors/
├── vscode/     # TypeScript LanguageClient → nova-lsp
├── neovim/     # nvim-lspconfig snippet → nova-lsp
├── helix/      # languages.toml language-server entry → nova-lsp
├── zed/        # extension.toml language_servers entry → nova-lsp
├── vim/        # syntax only (нет LSP client)
├── emacs/      # nova-mode.el syntax only
├── sublime/    # TextMate re-use syntax only
└── INSTALL.md  # unified install guide (этот файл = ссылка на него)
```

## Source-of-truth для keyword'ов

Все подсветки синхронизированы с **компилятором** —
[`compiler-codegen/src/lexer/mod.rs`](../compiler-codegen/src/lexer/mod.rs)
функция `lex_ident_or_keyword`. Это **единственный авторитативный список**
keyword'ов Nova ([D278](../spec/decisions/09-tooling.md#d278)). Различай три класса:
**ACTIVE** (лексер даёт `Kw*`, валидно → подсвечивать), **RETIRED** (`let`/`readonly`/`safe`
— `Kw*` только для ошибки удаления → НЕ подсвечивать), **не keyword** (`handler`,
`and`/`or`/`not`, `race`/`with_timeout`/`cancel_scope`/`region` → лексер даёт `Ident`).

При добавлении/удалении keyword'а в lexer — обновляй все editor-plugin'ы:

| Файл | Что обновить |
|---|---|
| `vscode/syntaxes/nova.tmLanguage.json` | `repository.keywords.patterns` |
| `vim/syntax/nova.vim` | секция `syntax keyword nova*` |
| `zed/languages/nova/highlights.scm` | список `@keyword` (gated на грамматику tree-sitter-nova) |
| `emacs/nova-mode.el` | константа `nova-keywords-*` |
| `tree-sitter-nova/grammar.js` | keyword rules (отдельный репо `github.com/nv-lang/tree-sitter-nova`) |
| `www/site/public/js/nova-highlight.js` | `CTRL_KEYWORDS` / `DECL_KEYWORDS` (отдельный репо) |

**Защита от дрейфа (автоматическая):**
- `compiler-codegen/tests/syntax_highlight_conformance.rs` — `cargo test` лексит каждое
  слово через живой `lex()` и проверяет VSCode/vim/Zed на фантомы + полноту.
- `www/site/scripts/check-highlight-keywords.mjs` — `npm run check:highlight` (отдельный репо сайта).

Открытый вопрос об авто-генерации этих списков из лексера —
[Q38](../spec/open-questions.md#q38). tree-sitter-грамматика отстаёт от лексера →
followup `[M-treesitter-grammar-keyword-bump]`.

## Не поддерживается (отдельные проекты)

| Редактор | Причина |
|---|---|
| **JetBrains IDEs** | Native plugin требует Java/Kotlin + IntelliJ Platform SDK |
| **Visual Studio (full)** | Аудитория мало пересекается с Nova; native VS extension сложен |
| **Atom** | Deprecated 2022 |

## Roadmap

- **LSP hover / completion** (Plan 104.2–104.6) — расширят возможности всех редакторов
  через тот же nova-lsp binary.
- **nvim-lspconfig upstream PR** — `editors/neovim/UPSTREAM_PR_DRAFT.md` готов.
- **Marketplace публикация** — VSCode marketplace и Zed marketplace deferred
  (упоминаются в `docs/simplifications.md`).
- **Tree-sitter для Neovim / Emacs** — через tree-sitter-nova (Plan 104.7 ✅).
