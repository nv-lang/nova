# Nova Language — Emacs major mode

Emacs major mode для подсветки синтаксиса `.nv` файлов.

## Установка

### Вариант 1: `load-file` напрямую

В `~/.emacs` или `~/.emacs.d/init.el`:

```elisp
(load-file "/path/to/nova-lang/editors/emacs/nova-mode.el")
```

После этого `.nv` файлы автоматически открываются в `nova-mode`.

### Вариант 2: Через `load-path` + `require`

В `~/.emacs.d/init.el`:

```elisp
(add-to-list 'load-path "/path/to/nova-lang/editors/emacs")
(require 'nova-mode)
```

### Вариант 3: Через `use-package`

Если используешь `use-package`:

```elisp
(use-package nova-mode
  :load-path "/path/to/nova-lang/editors/emacs"
  :mode "\\.nv\\'")
```

### Вариант 4: Через `straight.el` (local repo)

```elisp
(use-package nova-mode
  :straight (:type built-in :local-repo "/path/to/nova-lang/editors/emacs"))
```

## Проверка

1. Открой любой `.nv` файл (`examples/basics/hello.nv`).
2. `M-x` → `describe-mode` (или `C-h m`) должно показать `Nova mode`.
3. Если нет — `M-x nova-mode` вручную; затем проверь, что
   `auto-mode-alist` правильно добавлен.
4. Код должен раскраситься (зависит от theme).

## Что подсвечивается

То же что в VSCode-версии — keywords (declaration, control flow,
concurrency, memory), prelude-типы, эффекты, примитивы, числовые/
строковые литералы, char-литералы, комментарии, `@field`-доступ,
PascalCase identifier'ы как типы.

Полный list синхронизирован с
[`../vscode/syntaxes/nova.tmLanguage.json`](../vscode/syntaxes/nova.tmLanguage.json).
Source-of-truth для keyword'ов —
[`compiler-codegen/src/lexer/mod.rs`](../../compiler-codegen/src/lexer/mod.rs)
функция `lex_ident_or_keyword`.

## Рекомендуемые package'ы

Для лучшего experience с Nova-кодом (как в VSCode bracket pair
colorization):

- **[rainbow-delimiters](https://github.com/Fanael/rainbow-delimiters)**
  — парные `{}`/`()`/`[]` разными цветами по уровню вложенности.
  `nova-mode` сам активирует его если package установлен.
- **[highlight-indent-guides](https://github.com/DarthFennec/highlight-indent-guides)**
  — вертикальные направляющие отступов.
- **[smartparens](https://github.com/Fuco1/smartparens)** —
  умное автозакрытие скобок.

Установка через `use-package`:

```elisp
(use-package rainbow-delimiters
  :hook (nova-mode . rainbow-delimiters-mode))

(use-package highlight-indent-guides
  :hook (nova-mode . highlight-indent-guides-mode)
  :config (setq highlight-indent-guides-method 'character))

(use-package smartparens
  :hook (nova-mode . smartparens-mode))
```

## Что не работает

- **Семантический анализ** — нет (нужен LSP-сервер).
- **Auto-complete** — только `dabbrev` / `hippie-expand` из открытых
  буферов.
- **Go to definition** — нет (можно через ctags если генерируешь
  TAGS file самостоятельно).
- **Подсветка ошибок** — нет.
- **Indentation rules** — базовые (через `prog-mode` defaults), без
  Nova-specific логики (block-bodies, match-arms). Можно расширить.

Для полноценной IDE-интеграции нужен LSP через `lsp-mode` или
`eglot` (когда LSP-сервер появится).

## Как править

| Что изменилось | Куда |
|---|---|
| Новый keyword | соответствующая `nova-keywords-*` константа |
| Новый stdlib-effect | `nova-effects` константа |
| Новый prelude-тип | `nova-prelude-types` константа |
| Новый примитив | `nova-primitive-types` константа |

После правки — `M-x eval-buffer` или перезапуск Emacs.

## Файл — структура

`nova-mode.el` — single-file mode:

| Секция | Что |
|---|---|
| `nova-mode-syntax-table` | comment syntax, string delimiters, word chars |
| `nova-keywords-*` constants | keyword groups by role |
| `nova-prelude-types`, `nova-effects`, `nova-primitive-types` | type names |
| `nova-font-lock-keywords` | regex-based highlighting rules |
| `nova-mode` (define-derived-mode) | entry point, mode setup |
| `auto-mode-alist` | автоассоциация `.nv` файлов |

## Известные ограничения

1. **Emacs regex weaker than Oniguruma** — некоторые сложные паттерны
   (вложенная string interpolation) могут работать менее точно чем в
   VSCode.
2. **String interpolation `${...}`** — подсвечивается как переменная,
   но **внутри** `${...}` Nova-код не парсится sub-grammar'ом.
3. **Tagged templates `` json`...` ``** — backtick подсвечивается как
   string-delimiter, но tag-name (`json`/`sql`/etc) не выделяется
   специально.
4. **PascalCase identifier** подсвечивается как тип, но Emacs не
   различает «декларация типа» vs «использование типа».

Эти ограничения — фундаментальные для Emacs font-lock. Для более
точной подсветки нужен tree-sitter (Emacs 29+, отдельная работа).
