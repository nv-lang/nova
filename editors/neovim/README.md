# Nova Language — Neovim LSP Configuration

Neovim LSP support for Nova via `nvim-lspconfig` + `nova-lsp`.

→ Полная инструкция установки: **[editors/INSTALL.md](../INSTALL.md)**

## Требования

- Neovim ≥ 0.8 (LSP API stable)
- [nvim-lspconfig](https://github.com/neovim/nvim-lspconfig) (plugin)
- `nova-lsp` binary — в PATH или в `vim.g.nova_lsp_path`

## Быстрая установка

### 1. Сборка nova-lsp

```bash
git clone https://github.com/nv-lang/nova
cd nova
cargo build --release -p nova-lsp
# binary: target/release/nova-lsp
# Добавь в PATH или укажи vim.g.nova_lsp_path
```

### 2. Установка nvim-lspconfig

```lua
-- lazy.nvim:
{ "neovim/nvim-lspconfig" }
```

### 3. Добавить filetype detection

Скопируй `ftdetect.lua` в твой Neovim config:

```bash
# Linux/macOS
cp editors/neovim/ftdetect.lua ~/.config/nvim/lua/ftdetect/nova.lua

# Или source напрямую в init.lua:
# require("ftdetect.nova")
```

Альтернатива для Vim-совместимых файлов — `editors/vim/ftdetect/nova.vim`
(уже есть в репе):
```bash
cp editors/vim/ftdetect/nova.vim ~/.config/nvim/ftdetect/nova.vim
```

### 4. Добавить LSP snippet

```bash
# Linux/macOS
cp editors/neovim/lspconfig.lua ~/.config/nvim/after/plugin/nova-lsp.lua
```

Или добавь содержимое в свой `init.lua` / lazy.nvim plugin spec.

### 5. Верификация

Открой любой `.nv` файл:
```vim
:LspInfo
```
Должно показать `nova` attached к текущему буферу.

Проверь диагностики:
```vim
:lua vim.diagnostic.get()
```

## Конфигурация

```lua
-- Опциональный override пути к binary:
vim.g.nova_lsp_path = "/home/me/.cargo/bin/nova-lsp"

-- Кастомные keybindings:
require("lspconfig").nova.setup({
    on_attach = function(client, bufnr)
        local opts = { buffer = bufnr }
        vim.keymap.set("n", "gd",         vim.lsp.buf.definition,    opts)
        vim.keymap.set("n", "K",          vim.lsp.buf.hover,          opts)
        vim.keymap.set("n", "<leader>rn", vim.lsp.buf.rename,         opts)
        vim.keymap.set("n", "<leader>ca", vim.lsp.buf.code_action,    opts)
        vim.keymap.set("n", "[d",         vim.diagnostic.goto_prev,   opts)
        vim.keymap.set("n", "]d",         vim.diagnostic.goto_next,   opts)
    end,
})
```

## Upstream PR

Планируемый PR в `neovim/nvim-lspconfig`:
→ [UPSTREAM_PR_DRAFT.md](UPSTREAM_PR_DRAFT.md)

## Структура

```
editors/neovim/
├── lspconfig.lua             # nvim-lspconfig snippet (главный файл)
├── ftdetect.lua              # .nv → filetype "nova" (Neovim-native)
├── README.md                 # этот файл
├── UPSTREAM_PR_DRAFT.md      # черновик PR в nvim-lspconfig
└── tests/
    ├── smoke.lua             # headless smoke tests
    └── run_smoke.sh          # runner (skip if nvim not found)
```
