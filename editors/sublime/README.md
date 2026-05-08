# Nova Language — Sublime Text syntax highlighting

Sublime Text прямо понимает TextMate-грамматики. Поэтому отдельной
грамматики для Sublime **не делаем** — переиспользуем
`editors/vscode/syntaxes/nova.tmLanguage.json`.

## Установка

### Вариант 1: Symbolic link (рекомендуется)

После установки изменения в репе сразу видны Sublime после reload.

**Windows (PowerShell от админа):**

```powershell
$packages = "$env:APPDATA\Sublime Text\Packages\Nova"
New-Item -ItemType SymbolicLink `
    -Path $packages `
    -Target "d:\Sources\nova-lang\editors\vscode\syntaxes"
```

**Linux:**

```sh
mkdir -p ~/.config/sublime-text/Packages
ln -s /path/to/nova-lang/editors/vscode/syntaxes \
      ~/.config/sublime-text/Packages/Nova
```

**macOS:**

```sh
mkdir -p ~/Library/Application\ Support/Sublime\ Text/Packages
ln -s /path/to/nova-lang/editors/vscode/syntaxes \
      ~/Library/Application\ Support/Sublime\ Text/Packages/Nova
```

### Вариант 2: Копирование

Если symlink не работает:

```sh
# Linux/Mac
cp -r /path/to/nova-lang/editors/vscode/syntaxes \
      ~/.config/sublime-text/Packages/Nova
```

```powershell
# Windows
Copy-Item -Path "d:\Sources\nova-lang\editors\vscode\syntaxes" `
          -Destination "$env:APPDATA\Sublime Text\Packages\Nova" `
          -Recurse
```

После любых изменений в репе нужно перекопировать.

## Проверка

1. Открой любой `.nv` файл (`examples/basics/hello.nv`).
2. В правом нижнем углу Sublime должно появиться **«Nova»**.
3. Если показывает «Plain Text» — `View → Syntax → Nova` или
   `Ctrl+Shift+P` → `Set Syntax: Nova`.
4. Код должен раскраситься.

## Что подсвечивается

То же что и в VSCode — keywords, types, эффекты, литералы, операторы,
комментарии. Полный список и synchronization-правила смотри в
[`../vscode/README.md`](../vscode/README.md).

## Рекомендуемые plugin'ы

Для лучшего experience с Nova-кодом:

- **[BracketHighlighter](https://packagecontrol.io/packages/BracketHighlighter)**
  — подсветка парных `{}`/`()`/`[]`.
- **[Rainbow Brackets](https://packagecontrol.io/packages/Rainbow%20Brackets)**
  или **[BracketColorizer](https://packagecontrol.io/packages/Rainbow%20Brackets)**
  — парные скобки разными цветами по уровню вложенности (как в VSCode
  bracket pair colorization).
- **[Indent X](https://packagecontrol.io/packages/Indent%20X)** —
  направляющие отступов.

Установка — через **Package Control** (`Ctrl+Shift+P` → `Install Package`).

## Известные ограничения

Sublime парсит TextMate-грамматику **через Oniguruma regex** (тот же
движок что VSCode). Поведение должно быть идентичным.

Если Sublime показывает грамматические ошибки в сравнении с VSCode —
это вероятно **bug в самом `.tmLanguage.json`**, а не в адаптации.
Сообщай о таких случаях.

## Альтернатива: native `.sublime-syntax` (YAML)

Sublime имеет собственный native-формат `.sublime-syntax` (YAML),
быстрее и мощнее TextMate. Конвертация:

```sh
# В Sublime: Tools → Developer → New Syntax (использовать template)
# Или сторонний tool: tmlanguage-to-sublime-syntax
```

Native-формат не сделан, потому что:

1. **TextMate работает** — нет measurable выгоды для нашего use-case'а.
2. **Дублирование** — каждое изменение spec'а пришлось бы синхронизировать
   в двух местах.
3. **VSCode primary target** — там TextMate единственный путь.

Если кто-то хочет native `.sublime-syntax` — PR welcome.
