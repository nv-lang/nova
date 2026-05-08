# Nova Language — VSCode syntax highlighting

Локальное расширение VSCode для подсветки синтаксиса `.nv` файлов.

Это **TextMate grammar** — regex-based подсветка. Не понимает семантики
(эффекты, типы, контракты), но раскрашивает ключевые слова, типы,
литералы, операторы. Для полной интеграции нужен LSP-сервер — отдельная
работа.

## Что подсвечивается

### Ключевые слова

**Объявления:** `fn`, `type`, `alias`, `effect`, `handler`, `protocol`,
`external`, `let`, `const`, `module`, `import`, `export`, `as`, `use`,
`test`.

**Control flow:** `if`, `else`, `match`, `for`, `while`, `loop`, `break`,
`continue`, `return`, `throw`, `with`, `interrupt`.

**Concurrency** ([D14](../../spec/decisions/06-concurrency.md#d14),
[D50](../../spec/decisions/06-concurrency.md#d50),
[D75](../../spec/decisions/06-concurrency.md#d75)):
`spawn`, `detach`, `parallel`, `supervised`, `cancel_scope`,
`race`, `select`, `with_timeout`.

**Memory / safety** ([D6](../../spec/decisions/05-memory.md#d6),
[D63](../../spec/decisions/04-effects.md#d63),
[D64](../../spec/decisions/04-effects.md#d64)):
`region`, `forbid`, `realtime`.

**Operators / patterns:** `is` ([D54](../../spec/decisions/03-syntax.md#d54)
runtime type-check), `old` (контракты),
`in` (for-loop binding).

**Modifiers:** `mut`, `readonly`.

**Зарезервировано на будущее:** `defer`
([Q20 open question](../../spec/open-questions.md), семантика не
определена).

### Типы prelude
([D26](../../spec/decisions/08-runtime.md#d26))

**Sum-типы:** `Option`, `Some`, `None`, `Result`, `Ok`, `Err`,
`Ordering`, `Less`, `Equal`, `Greater`.

**Records / специальные:** `Error`, `RuntimeError`, `Never`, `Self`.

**Iterator / Range** ([D58](../../spec/decisions/03-syntax.md#d58)):
`Iter`, `Range`, `RangeIter`.

**Concurrency** ([D75](../../spec/decisions/06-concurrency.md#d75),
[D79](../../spec/decisions/06-concurrency.md#d79)): `Channel`,
`CancelToken`, `Handler`.

**Конверсия** ([D73](../../spec/decisions/08-runtime.md#d73),
[D77](../../spec/decisions/08-runtime.md#d77)): `From`, `Into`,
`TryFrom`, `TryInto`.

**Структурные protocols:** `Hashable`, `Eq`, `Ord`.

**Builders** (Plan 04, не реализовано): `StringBuilder`, `WriteBuffer`,
`ReadBuffer`, `ReadBufferError`.

### Стандартные эффекты
([D2](../../spec/decisions/04-effects.md#d2),
[D62](../../spec/decisions/04-effects.md#d62))

`Fail`, `Io`, `Net`, `Db`, `Fs`, `Time`, `Random`, `Log`, `Trace`,
`Ask`, `Alloc`, `Detach`, `Blocking`, `Mem`.

### Примитивные типы

`int`, `i8`–`i64`, `u8`–`u64`, `f32`, `f64`, `str`, `bool`, `char`,
`byte`, `any`.

### Контракты ([D24](../../spec/decisions/09-tooling.md#d24))

`requires`, `ensures`, `invariant`.

### Стрелки и операторы

- `->` (типы), `=>` (тела/match-arm)
- `?` (Fail propagation), `??` (Option/Result coalesce)
- `..` / `..=` (range)
- `==` `!=` `<=` `>=` `<` `>` (сравнения)
- `&&` `||` `!` (логические)
- `&` `|` `^` `<<` `>>` (битовые)
- `=` `+=` `-=` `*=` `/=` `%=` (присваивание)

### Прочее

- `@field` / `@` — поле self / текущий инстанс
  ([D35](../../spec/decisions/03-syntax.md#d35))
- Литералы: числа (hex/binary/octal/decimal/float), строки `"..."` с
  интерполяцией `${...}`, raw-strings `` json`...` ``, char `'a'`
- Комментарии: `//`, `///` (doc), `/* */`
- **PascalCase идентификаторы** → подсвечиваются как типы
- **SCREAMING_SNAKE** → константы
- **Объявления функций** `fn name`, `fn Type.method` — выделяют имя
- **Объявления типов** `type Name`, `effect Name`, `handler Name` —
  выделяют имя

## Как установить (Windows)

Реальный путь к extension'у — `editors\vscode\` в этой репе.
VSCode подхватывает extension по symlink/copy в
`%USERPROFILE%\.vscode\extensions\`.

### Вариант 1: Symbolic link (рекомендуется для разработки)

Symbolic link означает что **изменения в репе сразу видны VSCode**
после reload.

```powershell
# В PowerShell от администратора (для symlink нужны права)
New-Item -ItemType SymbolicLink `
    -Path "$env:USERPROFILE\.vscode\extensions\nova-lang-local-0.1.0" `
    -Target "d:\Sources\nova-lang\editors\vscode"
```

После этого перезапусти VSCode. Extension появится в списке расширений.

### Вариант 2: Копирование

Если symlink не работает — простое копирование:

```powershell
Copy-Item -Path "d:\Sources\nova-lang\editors\vscode" `
          -Destination "$env:USERPROFILE\.vscode\extensions\nova-lang-local-0.1.0" `
          -Recurse
```

После любых изменений в этой папке нужно **перекопировать**.

### Вариант 3: Сборка `.vsix` и установка

Если установлен `vsce` (VSCode extension packaging tool):

```bash
cd d:/Sources/nova-lang/editors/vscode
npx @vscode/vsce package
code --install-extension nova-lang-0.1.0.vsix
```

`.vsix` — стандартный формат VSCode extension'а, можно отдать
коллегам.

## Как установить (Linux / Mac)

```sh
# Symbolic link
ln -s /path/to/nova-lang/editors/vscode \
      ~/.vscode/extensions/nova-lang-local-0.1.0

# Или копирование
cp -r /path/to/nova-lang/editors/vscode \
      ~/.vscode/extensions/nova-lang-local-0.1.0
```

После reload VSCode (`Ctrl+Shift+P` → `Developer: Reload Window`)
extension подхватывается.

## Проверка

1. Открой любой `.nv` файл — например, `examples/basics/hello.nv`
   или `nova_tests/types/literals.nv`.
2. В правом нижнем углу VSCode должно показывать **«Nova»**. Если
   показывает «Plain Text» — выбери `Nova` вручную через `Ctrl+K M`.
3. Код должен раскраситься: `fn`/`type`/`let` синие, типы (PascalCase)
   зелёные/циан, числа жёлтые, строки красные, комментарии серые.

## Рекомендуемые настройки VSCode

Для лучшего experience с Nova-кодом — добавь в `settings.json`
(`Ctrl+,` → иконка `{ }` справа сверху):

```json
{
    "editor.bracketPairColorization.enabled": true,
    "editor.guides.bracketPairs": "active",
    "editor.guides.indentation": true,
    "[nova]": {
        "editor.tabSize": 4,
        "editor.insertSpaces": true,
        "files.trimTrailingWhitespace": true,
        "files.insertFinalNewline": true
    }
}
```

**Что это даёт:**

- **`bracketPairColorization`** — парные `{}`/`()`/`[]` подсвечиваются
  6 циклическими цветами по уровню вложенности. Глобальная VSCode-фича
  (с 1.60), не часть нашего extension'а — работает для любого языка.
- **`guides.bracketPairs: "active"`** — вертикальные направляющие
  показывают границы текущего bracket-блока.
- **`guides.indentation`** — направляющие отступов.
- **Per-Nova settings** — 4-space indent, без tabs, trim trailing
  whitespace при сохранении, final newline. Согласовано со стилем
  существующего nova-кода в репе.

## Что не работает (намеренно)

- **Семантический анализ** — type-check, effect inference, контракты
  не проверяются. Это работа LSP-сервера (которого ещё нет).
- **Auto-complete** — VSCode будет предлагать только слова из
  открытых файлов, не из stdlib.
- **Go to definition** — не работает.
- **Подсветка ошибок** — нет.
- **Inline hints** для эффектов и типов — нет.

Для полноценной IDE-интеграции нужен LSP-сервер на стороне компилятора
(когда он появится).

## Структура

```
editors/vscode/
├── package.json                 манифест (id языка, fileTypes)
├── language-configuration.json  скобки, комментарии, авто-закрытие, indent
├── syntaxes/
│   └── nova.tmLanguage.json     TextMate grammar (главный файл)
└── README.md                    этот файл
```

## Изменения

Если в Nova меняется синтаксис (например, добавляется новое ключевое
слово или тип) — правь `syntaxes/nova.tmLanguage.json`:

| Что изменилось | Куда |
|---|---|
| Новый keyword (control flow / declaration / modifier) | `repository.keywords.patterns` |
| Новый stdlib-effect | `repository.effects.match` |
| Новый prelude-тип | `repository.prelude-types.match` |
| Новый примитив | `repository.primitive-types.match` |
| Новый оператор | `repository.operators.patterns` |

После правки:
1. **Если symlink-вариант установки** — перезапусти VSCode
   (`Ctrl+Shift+P` → `Developer: Reload Window`).
2. **Если копирование** — пересоздай copy в `extensions/`, потом
   reload.
3. **Если `.vsix`** — пересобери и переустанови.

## Синхронизация со spec'ом

Подсветка отражает **финальные D-решения** из `spec/decisions/`. Если
изменяется D-решение, влияющее на синтаксис — обновляй подсветку
синхронно. Например:

- D49 убрал `or`/`and`/`not` keyword-алиасы → удалить из
  `repository.keywords.patterns` (если были).
- D61 заменил `resume` на `return`/`interrupt` → удалить `resume` из
  `keyword.operator.expression`.
- D79 отверг `Mutex`/`RwLock`/`Atomic` → удалить из `prelude-types`.

Регулярно проверяй `repository.keywords` против списка keyword'ов
в `compiler-codegen/src/lexer/mod.rs:376-415` (function
`lex_ident_or_keyword`) — это **source of truth** для keyword'ов
языка.

## Известные ограничения TextMate-подсветки

1. **Не различает поле и метод** — `obj.method` и `obj.field`
   подсветятся одинаково.
2. **Не подсвечивает эффекты в сигнатуре особым цветом** — `Fail`
   в позиции эффекта (между `)` и `->`) и `Fail[E]` в позиции
   значения (как тип параметра) выглядят одинаково.
3. **Generic-параметры в `fn name[T]`** не выделяются от обычных
   типов.
4. **Контракты `requires`/`ensures`** подсветятся как ключевые
   слова, но их выражения парсятся как обычный код (`old(x)` не
   будет особым).
5. **Tagged template `` json`...` ``** подсвечивается как одна
   строка, без раскрашивания внутреннего содержимого по
   sub-grammar'у.

Эти ограничения — фундаментальные для regex-based подсветки.
Решаются только через LSP.
