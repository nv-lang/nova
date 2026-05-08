# Nova Language — Editor support

Поддержка `.nv` файлов в редакторах. Все варианты — **локальные**
plugin'ы (через symlink/copy), потому что Nova ещё не публикуется в
официальных marketplace'ах.

## Поддерживаемые редакторы

| Редактор | Подкаталог | Что есть | Установка |
|---|---|---|---|
| **VSCode** | [`vscode/`](vscode/) | TextMate grammar (полная подсветка), language-config (brackets, comments, indent) | symlink в `~/.vscode/extensions/` |
| **Cursor** | [`vscode/`](vscode/) | то же что VSCode (Cursor — fork VSCode) | symlink в `~/.cursor/extensions/` |
| **VSCodium** | [`vscode/`](vscode/) | то же что VSCode (VSCodium — open-source fork VSCode) | symlink в `~/.vscode-oss/extensions/` |
| **Sublime Text** | [`sublime/`](sublime/) | переиспользует TextMate grammar из vscode/ | symlink в `Packages/Nova/` |
| **TextMate** (macOS) | [`vscode/`](vscode/) | переиспользует `.tmLanguage.json` напрямую | symlink в `~/Library/Application Support/TextMate/Bundles/` |
| **Vim** | [`vim/`](vim/) | native syntax файл (handcrafted) + ftdetect + ftplugin | symlink в `~/.vim/` |
| **Neovim** | [`vim/`](vim/) | то же что Vim, путь `~/.config/nvim/` | symlink в `~/.config/nvim/` |
| **Emacs** | [`emacs/`](emacs/) | major-mode (`nova-mode.el`) с font-lock-keywords | `(load-file ...)` или `use-package` |

## Не поддерживается (отдельные проекты)

| Редактор | Почему пропущено |
|---|---|
| **JetBrains IDEs** (IntelliJ/CLion/RustRover/etc) | Native plugin требует Java/Kotlin + IntelliJ Platform SDK; через TextMate Bundles plugin теоретически возможно, но не сделано |
| **Zed** | Требует tree-sitter grammar (отдельный проект, см. roadmap) |
| **Helix** | Требует tree-sitter grammar |
| **Visual Studio (full)** | Аудитория мало пересекается с Nova; native VS extension сложен |
| **Atom** | Deprecated 2022 |

**Tree-sitter grammar** — современный стандарт (используется Zed, Helix,
Neovim 0.5+, опционально Emacs 29+, GitHub web). Один tree-sitter
grammar = подсветка во всех этих редакторах + возможность для tree-aware
operations (rename, refactor, structural search). Это **отдельный
проект** ~10-20 часов работы, не сделан в MVP.

## Source-of-truth для keyword'ов

Все подсветки синхронизированы с **компилятором** —
[`compiler-codegen/src/lexer/mod.rs`](../compiler-codegen/src/lexer/mod.rs)
функция `lex_ident_or_keyword`. Это **единственный авторитативный
список** keyword'ов Nova. При добавлении нового keyword'а в lexer —
обновляй все editor-plugin'ы синхронно:

| Файл | Что обновить |
|---|---|
| `vscode/syntaxes/nova.tmLanguage.json` | `repository.keywords.patterns` |
| `vim/syntax/nova.vim` | секция `syntax keyword nova*` |
| `emacs/nova-mode.el` | константа `nova-keywords-*` |

Sublime автоматически наследует от VSCode (использует тот же файл).

## Что подсвечивается

Все plugin'ы покрывают одинаковый набор:

- **Keywords:** declaration (`fn`, `type`, `effect`, ...), control flow
  (`if`/`else`/`match`/`for`/...), concurrency (`spawn`/`detach`/
  `parallel`/`supervised`/...), memory (`region`/`forbid`/`realtime`).
- **Prelude types:** `Option`, `Result`, `Iter`, `Channel`, `From`/`Into`,
  `StringBuilder`, `WriteBuffer`, `ReadBuffer`, и т.д.
- **Standard effects:** `Fail`, `Io`, `Net`, `Db`, `Time`, `Random`,
  `Log`, и т.д.
- **Primitives:** `int`, `i8`-`i64`, `u8`-`u64`, `f32`/`f64`, `str`,
  `bool`, `char`, `byte`, `any`.
- **Контракты:** `requires`, `ensures`, `invariant`, `old`.
- **Литералы:** числа (hex/binary/octal/decimal/float с `_`-разделителями),
  строки `"..."` с интерполяцией `${...}`, raw-strings `` json`...` ``,
  char-литералы `'a'` / `'\n'`.
- **Комментарии:** `//`, `///` (doc), `/* */`.
- **`@field`/`@`** — доступ к полю self / текущий инстанс.
- **PascalCase** идентификаторы → подсвечиваются как типы.
- **SCREAMING_SNAKE_CASE** → подсвечиваются как константы.

## Что не работает (во всех plugin'ах)

- **Семантический анализ** — type-check, effect inference, контракты не
  проверяются.
- **Auto-complete по stdlib** — только локально из буфера.
- **Go to definition** — нет.
- **Подсветка ошибок компилятора** — нет.
- **Inline hints** для эффектов / типов — нет.

Для полноценной IDE-интеграции нужен **LSP-сервер на стороне
компилятора** — отдельный проект, ещё не реализован.

## Roadmap

В порядке приоритета:

1. **LSP-сервер** — позволит add semantic features во **все** редакторы
   через `vscode-languageclient`, `nvim-lspconfig`, `lsp-mode`/`eglot`,
   и т.д. Это **главный** unlock — всё остальное второстепенно.
2. **Tree-sitter grammar** — точная подсветка для Zed/Helix/Neovim,
   плюс bonus: GitHub web подсветка для `.nv` файлов в репах.
3. **JetBrains plugin** — если/когда будет audience.

Текущий MVP — TextMate-based highlighting в 6 редакторах через
переиспользование/handcrafted plugin'ы. Достаточно для разработки,
не достаточно для production-grade IDE experience.
