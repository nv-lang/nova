# Draft PR: Add Nova language server to nvim-lspconfig

**Target:** neovim/nvim-lspconfig (https://github.com/neovim/nvim-lspconfig)

---

## Title

feat: add Nova language server (nova-lsp)

## Description

This PR adds server configuration for `nova-lsp`, the official language server
for the [Nova programming language](https://github.com/nv-lang/nova).

### About Nova

Nova is a statically-typed systems programming language with algebraic effects,
async/await via structured concurrency, and a region-based memory model. The
language targets production systems code.

### nova-lsp

`nova-lsp` implements [LSP 3.17](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/)
via stdio JSON-RPC. Currently provides:
- **Diagnostics**: compiler errors and warnings with precise source positions
- **textDocument/publishDiagnostics**: incremental file-change support

Future plans (Plan 104.2–104.6): hover, completion, go-to-definition, inlay hints.

### Installation

```bash
# From source (Cargo required):
cargo install --path nova-lsp   # from https://github.com/nv-lang/nova

# Or download from releases (when available):
# https://github.com/nv-lang/nova/releases
```

## Files changed

- `lua/lspconfig/configs/nova.lua` — server configuration

## Configuration

```lua
-- Minimal setup
require("lspconfig").nova.setup({})

-- With keybindings
require("lspconfig").nova.setup({
    on_attach = function(client, bufnr)
        vim.keymap.set("n", "gd", vim.lsp.buf.definition, { buffer = bufnr })
        vim.keymap.set("n", "K",  vim.lsp.buf.hover,      { buffer = bufnr })
    end,
})
```

## Root detection

Uses `root_pattern("nova.toml", ".git")`. `nova.toml` is the Nova project
manifest (equivalent to Cargo.toml for Rust or pyproject.toml for Python).

## Test

```
./test.sh nova
```

## Checklist

- [ ] `nova-lsp` binary is available via `cargo install` (from source)
- [ ] server config follows lspconfig conventions (single_file_support = true)
- [ ] root_pattern uses `nova.toml` + `.git` fallback
- [ ] filetypes = `{ "nova" }` for `.nv` files

---

*Note: This PR is a draft prepared alongside the Nova editor packaging work
(nova-lang Plan 104.8). Will be submitted after the LSP server reaches v1.0
stability.*
