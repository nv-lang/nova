# Nova Language — Editor Installation Guide

Единый guide по установке Nova editor support во все поддерживаемые редакторы.
Все редакторы используют один и тот же `nova-lsp` binary — setup отличается только
декларативной конфигурацией.

## Prerequisites: сборка nova-lsp

Все LSP-фичи требуют `nova-lsp` binary. Синтаксическая подсветка работает без него.

```bash
# Из репы Nova (Cargo required):
git clone https://github.com/nv-lang/nova
cd nova
cargo build --release -p nova-lsp

# Binary: target/release/nova-lsp  (Linux/macOS)
#         target\release\nova-lsp.exe  (Windows)
```

Добавь binary в PATH (рекомендуется) или укажи путь явно в настройках редактора.

---

## VSCode / Cursor / VSCodium

→ Подробнее: [editors/vscode/README.md](vscode/README.md)

**Что получаешь:** TextMate syntax highlighting + LSP diagnostics + auto-restart on crash.

### Установка (dev режим, рекомендуется)

```bash
# 1. Установи зависимости
cd editors/vscode
npm install

# 2. Скомпилируй TypeScript
npm run build

# 3. Открой VSCode в корне репы (File → Open Folder → /path/to/nova)
#    Нажми F5 — запускается Extension Development Host с нашим расширением
#    Открой любой .nv файл → подсветка + (если nova-lsp в PATH) diagnostics
```

### Установка (симлинк, постоянная)

```bash
# Linux/macOS
ln -s /path/to/nova/editors/vscode ~/.vscode/extensions/nova-lang-local-0.2.0

# Windows (PowerShell от администратора)
New-Item -ItemType SymbolicLink `
    -Path "$env:USERPROFILE\.vscode\extensions\nova-lang-local-0.2.0" `
    -Target "D:\Sources\nv-lang\nova\editors\vscode"
```

Перезапусти VSCode после создания симлинка.

### Конфигурация

В `settings.json` (Ctrl+, → иконка `{ }`):
```json
{
    "nova.lsp.path": "/home/me/.cargo/bin/nova-lsp",
    "nova.lsp.enabled": true,
    "[nova]": {
        "editor.tabSize": 4,
        "editor.insertSpaces": true
    }
}
```

### Верификация

1. Открой любой `.nv` файл — должна появиться подсветка.
2. Если nova-lsp в PATH — через ~500ms появятся диагностики в gutter.
3. `Output → Nova LSP` — лог запуска LSP.

---

## Neovim

→ Подробнее: [editors/neovim/README.md](neovim/README.md)

**Что получаешь:** LSP diagnostics через nvim-lspconfig.

### Установка

```bash
# 1. Установи nvim-lspconfig (пример для lazy.nvim):
# { "neovim/nvim-lspconfig" }

# 2. Filetype detection
cp editors/neovim/ftdetect.lua ~/.config/nvim/lua/ftdetect/nova.lua
# Или используй старый vim-совместимый вариант:
cp editors/vim/ftdetect/nova.vim ~/.config/nvim/ftdetect/nova.vim

# 3. LSP snippet (Linux/macOS)
cp editors/neovim/lspconfig.lua ~/.config/nvim/after/plugin/nova-lsp.lua
# Или добавь содержимое напрямую в init.lua
```

### Конфигурация

```lua
-- Путь к binary (если не в PATH):
vim.g.nova_lsp_path = "/home/me/.cargo/bin/nova-lsp"

-- С keybindings:
require("lspconfig").nova.setup({
    on_attach = function(client, bufnr)
        local opts = { buffer = bufnr }
        vim.keymap.set("n", "gd", vim.lsp.buf.definition, opts)
        vim.keymap.set("n", "K",  vim.lsp.buf.hover,      opts)
    end,
})
```

### Верификация

```vim
:LspInfo        " → должно показать nova attached
:lua vim.diagnostic.get()   " → список диагностик в текущем буфере
```

---

## Helix

→ Подробнее: [editors/helix/README.md](helix/README.md)

**Что получаешь:** tree-sitter highlighting + LSP diagnostics + auto-pairs.

### Установка

```bash
# 1. Скопируй или добавь languages.toml

# Linux/macOS (если файла нет):
cp editors/helix/languages.toml ~/.config/helix/languages.toml

# Linux/macOS (если файл есть — добавь в конец):
cat editors/helix/languages.toml >> ~/.config/helix/languages.toml

# Windows:
Copy-Item editors\helix\languages.toml "$env:APPDATA\helix\languages.toml"

# 2. Загрузи tree-sitter grammar:
hx --grammar fetch nova
hx --grammar build nova
```

### Кастомный путь к binary

В `languages.toml`:
```toml
[language-server.nova-lsp]
command = "/home/me/.cargo/bin/nova-lsp"
```

### Верификация

```bash
hx --health nova
# Ожидаемый вывод:
# Language:   nova
# LSP:        ✓ nova-lsp
# Highlight:  ✓
```

---

## Zed

→ Подробнее: [editors/zed/README.md](zed/README.md)

**Что получаешь:** tree-sitter highlighting + LSP diagnostics + auto-pairs.

### Установка (side-load)

```bash
# Linux/macOS
mkdir -p ~/.config/zed/extensions/nova
cp -r editors/zed/* ~/.config/zed/extensions/nova/

# Windows
New-Item -ItemType Directory -Force "$env:APPDATA\Zed\extensions\nova"
Copy-Item -Recurse editors\zed\* "$env:APPDATA\Zed\extensions\nova\"
```

Перезапусти Zed → Extensions panel должен показать Nova extension.

### Конфигурация nova-lsp path

В Zed `settings.json` (`Ctrl+,`):
```json
{
    "lsp": {
        "nova-lsp": {
            "binary": {
                "path": "/home/me/.cargo/bin/nova-lsp"
            }
        }
    }
}
```

### Верификация

Открой `.nv` файл → syntax highlighting работает сразу.
Если nova-lsp в PATH → диагностики появляются через ~500ms.

---

## Troubleshooting

### nova-lsp не найден

Проверь следующее по порядку:

1. **Собран ли binary?**
   ```bash
   ls target/release/nova-lsp   # или nova-lsp.exe на Windows
   ```

2. **В PATH ли binary?**
   ```bash
   which nova-lsp      # Linux/macOS
   where.exe nova-lsp  # Windows
   ```

3. **Задан ли явный путь?**
   - VSCode: `nova.lsp.path` в settings.json
   - Neovim: `vim.g.nova_lsp_path`
   - Helix: `command = "..."` в `[language-server.nova-lsp]`
   - Zed: `"lsp"."nova-lsp"."binary"."path"` в settings.json

### Диагностики не появляются

- Убедись что файл имеет расширение `.nv`
- Проверь Output → Nova LSP (VSCode) или `:LspLog` (Neovim)
- nova-lsp требует nova.toml в корне workspace (или `.git`)

### Подсветка не работает

- **VSCode/Cursor/VSCodium**: убедись что расширение установлено. `Ctrl+K M` → Nova.
- **Helix**: запусти `hx --grammar build nova` после `hx --grammar fetch nova`.
- **Neovim**: проверь filetype: `:set filetype?` — должно быть `nova`.

### LSP версии не совпадают

При обновлении nova-lsp → перезапусти редактор. LSP protocol version совместим
backward (LSP 3.17 baseline).
