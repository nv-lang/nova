# Nova Language — Helix Configuration

Helix editor support для Nova: **LSP diagnostics + tree-sitter syntax highlighting**.

→ Полная инструкция установки: **[editors/INSTALL.md](../INSTALL.md)**

## Что даёт конфигурация

| Feature | Как работает |
|---|---|
| Syntax highlighting | tree-sitter-nova v0.1.0 (grammar fetch) |
| LSP diagnostics в gutter | nova-lsp via stdio JSON-RPC |
| Auto-pairs `'` и `` ` `` | Включены (Nova char literals + tagged templates) |
| Workspace root detection | `nova.toml` + `.git` |

## Установка

### 1. Сборка nova-lsp

```bash
cd /path/to/nova
cargo build --release -p nova-lsp
# Добавь target/release/ в PATH, или укажи полный путь в languages.toml
```

### 2. Добавить конфигурацию

**Linux/macOS** — создать или обновить `~/.config/helix/languages.toml`:

```bash
# Если файла нет — просто скопируй:
cp editors/helix/languages.toml ~/.config/helix/languages.toml

# Если уже есть languages.toml — добавь блоки в конец:
cat editors/helix/languages.toml >> ~/.config/helix/languages.toml
```

**Windows** — `%AppData%\helix\languages.toml`:

```powershell
Copy-Item editors\helix\languages.toml "$env:APPDATA\helix\languages.toml"
```

### 3. Загрузить tree-sitter grammar

```bash
hx --grammar fetch nova
hx --grammar build nova
```

### 4. Верификация

```bash
hx --health nova
```

Должно показать:
```
Language:        nova
LSP:             ✓ nova-lsp
Highlight:       ✓
Textobjects:     ✗
Indent:          ✗
```

### 5. Кастомный путь к nova-lsp

Если nova-lsp не в PATH, укажи явно в `languages.toml`:

```toml
[language-server.nova-lsp]
command = "/home/me/.cargo/bin/nova-lsp"
```

## Структура

```
editors/helix/
├── languages.toml      # Helix config (добавить в ~/.config/helix/)
├── README.md           # этот файл
└── tests/
    └── smoke.sh        # smoke tests (skip если hx не в PATH)
```

## Примечания

- **Auto-pairs**: `'` и `` ` `` включены специально для Nova (char literals `'a'`
  и tagged template literals `` json`...` ``).
- **Tree-sitter**: grammar берётся из `github.com/nv-lang/tree-sitter-nova` v0.1.0 —
  не дублируется локально (D104.8-6).
- **TextMate vs tree-sitter**: Helix использует только tree-sitter, не TextMate grammar
  из `editors/vscode/`. Нет конфликта.
