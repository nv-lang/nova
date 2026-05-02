# Nova Language — VSCode syntax highlighting

Локальное расширение VSCode для подсветки синтаксиса `.nv` файлов.

Это **TextMate grammar** — regex-based подсветка. Не понимает семантики
(эффекты, типы, контракты), но раскрашивает ключевые слова, типы,
литералы, операторы. Для полной интеграции нужен LSP-сервер — отдельная
работа.

## Что подсвечивается

- **Ключевые слова:** `fn`, `type`, `let`, `mut`, `const`, `match`, `if`, `else`, `for`, `while`, `loop`, `return`, `throw`, `with`, `defer`, `spawn`, `parallel`, `supervised`, `forbid`, `test`
- **Модули и импорты:** `module`, `import`, `export`, `as`
- **Типы prelude:** `Option`, `Result`, `Some`, `None`, `Ok`, `Err`, `Error`, `Never`, `Vec`, `HashMap`, `Channel`, `Mutex`, ...
- **Стандартные эффекты:** `Throws`, `Io`, `Net`, `Db`, `Fs`, `Time`, `Random`, `Mut`, `Alloc`, `Async`, `Par`, `Log`, `Trace`, `Ask`
- **Примитивные типы:** `int`, `i8`-`i64`, `u8`-`u64`, `f32`, `f64`, `str`, `bool`, `char`, `byte`
- **Контракты:** `requires`, `ensures`, `invariant`
- **Стрелки:** `->` (типы), `=>` (тела/match), `?`, `??`
- **Модификатор `mut`** — для мутируемых параметров и полей
- **`@field` / `@`** — поле self / текущий инстанс ([D35](../../spec/decisions/03-syntax.md#d35))
- **Литералы:** числа (hex/binary/octal/decimal/float), строки `"..."` с интерполяцией `${...}`, raw-strings ``json`{}` ``
- **Комментарии:** `//`, `///` (doc), `/* */`
- **PascalCase идентификаторы** → подсвечиваются как типы
- **SCREAMING_SNAKE** → константы
- **Объявления функций** `fn name`, `fn Type.method` — выделяют имя
- **Объявления типов** `type Name` — выделяют имя

## Как установить (Windows)

### Вариант 1: Через символическую ссылку

```powershell
# В PowerShell от администратора
New-Item -ItemType SymbolicLink `
    -Path "$env:USERPROFILE\.vscode\extensions\nova-lang-local-0.1.0" `
    -Target "d:\Sources\nova-lang\.vscode\nova-extension"
```

После этого перезапусти VSCode. Extension появится в списке расширений.

### Вариант 2: Копирование

```powershell
Copy-Item -Path "d:\Sources\nova-lang\.vscode\nova-extension" `
          -Destination "$env:USERPROFILE\.vscode\extensions\nova-lang-local-0.1.0" `
          -Recurse
```

После любых изменений в этой папке нужно перекопировать.

### Вариант 3: Сборка VSIX и установка

Если установлен `vsce`:

```bash
cd d:/Sources/nova-lang/.vscode/nova-extension
npx @vscode/vsce package
code --install-extension nova-lang-0.1.0.vsix
```

## Проверка

1. Открой `d:\Sources\nova-lang\examples\audit.nv` в VSCode
2. В правом нижнем углу должно показывать **«Nova»** (если показывает
   «Plain Text» — выбери `Nova` вручную через `Ctrl+K M`)
3. Код должен раскраситься: ключевые слова, типы, строки, числа

## Что не работает (намеренно)

- **Семантический анализ** — type-check, effect inference, контракты не
  проверяются. Это работа LSP-сервера, который ещё не написан.
- **Auto-complete** — VSCode будет предлагать только слова из открытых
  файлов, не из stdlib.
- **Go to definition** — не работает.
- **Подсветка ошибок** — нет.
- **Inline hints** для эффектов и типов — нет.

Для полноценной IDE-интеграции нужен LSP-сервер на стороне компилятора
(когда он появится).

## Структура

```
nova-extension/
├── package.json                 манифест (id языка, fileTypes)
├── language-configuration.json  скобки, комментарии, авто-закрытие
├── syntaxes/
│   └── nova.tmLanguage.json     TextMate grammar (главный файл)
└── README.md                    этот файл
```

## Изменения

Если в Nova меняется синтаксис (например, добавляется новое ключевое
слово) — правь `syntaxes/nova.tmLanguage.json`:

- Ключевые слова → секция `repository.keywords.patterns`
- Типы → `prelude-types`, `effects`, `primitive-types`
- Операторы → `operators`

После правки перезапусти VSCode (или `Developer: Reload Window`,
`Ctrl+Shift+P`).

## Известные ограничения TextMate-подсветки

1. **Не различает поле и метод** — `obj.method` и `obj.field` подсветятся
   одинаково
2. **Не подсвечивает эффекты в сигнатуре особым цветом** — `Throws` в
   сигнатуре и `Throws` в позиции значения выглядят одинаково
3. **Generic-параметры в `fn name[T]`** не выделяются от обычных типов
4. **Контракты `requires`/`ensures`** подсветятся как ключевые слова, но
   их выражения парсятся как обычный код

Эти ограничения — фундаментальные для regex-based подсветки. Решаются
только через LSP.
