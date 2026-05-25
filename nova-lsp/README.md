# nova-lsp

**nova-lsp** is the official Language Server Protocol (LSP) server for the [Nova programming language](https://nv-lang.org).

It powers IDE features — diagnostics, hover, go-to-definition, completion, quick-fixes, rename — for any editor that speaks LSP (VSCode, Cursor, Neovim, Helix, Zed, and more).

> **Status:** Plan 104.0 — foundation skeleton (JSON-RPC transport, lifecycle handlers, document cache).
> Features land in Plans 104.1 – 104.6. See [docs/plans/104-ide-integration.md](../docs/plans/104-ide-integration.md).

---

## Build

```sh
# From the nova root:
cd nova-lsp
cargo build --release

# Binary:
#   Windows:  nova-lsp\target\release\nova-lsp.exe
#   Linux/Mac: nova-lsp/target/release/nova-lsp
```

Or from the repo root:

```sh
cargo build --release --manifest-path nova-lsp/Cargo.toml
```

---

## Editor configuration

### VSCode / Cursor / VSCodium

Add to your workspace `.vscode/settings.json`:

```json
{
  "nova-lsp.serverPath": "/path/to/nova-lsp/target/release/nova-lsp"
}
```

> Full VSCode extension (TypeScript client) lands in Plan 104.8.  
> In the meantime you can use the generic [LSP client extension](https://marketplace.visualstudio.com/items?itemName=mads-hartmann.bash-language-server) or point [Vim LSP](https://github.com/prabirshrestha/vim-lsp) at the binary.

### Neovim (nvim-lspconfig)

```lua
-- In your init.lua / after/plugin/lsp.lua
require("lspconfig").nova_lsp.setup({
  cmd = { "/path/to/nova-lsp/target/release/nova-lsp" },
  filetypes = { "nova" },
  root_dir = require("lspconfig.util").root_pattern("nova.toml", ".git"),
})
```

> Official nvim-lspconfig PR lands in Plan 104.8.

### Helix

Add to `~/.config/helix/languages.toml`:

```toml
[[language]]
name = "nova"
scope = "source.nova"
file-types = ["nv"]
roots = ["nova.toml"]
language-servers = ["nova-lsp"]

[language-server.nova-lsp]
command = "/path/to/nova-lsp/target/release/nova-lsp"
```

### Zed

> Zed extension configuration lands in Plan 104.8.

---

## Logging

nova-lsp logs to **stderr** (stdout is reserved for JSON-RPC).
Control verbosity via the `NOVA_LSP_LOG` environment variable:

```sh
NOVA_LSP_LOG=debug nova-lsp   # verbose
NOVA_LSP_LOG=trace nova-lsp   # very verbose (includes JSON-RPC frames)
NOVA_LSP_LOG=warn  nova-lsp   # quiet (warnings + errors only)
```

In VSCode, editor stderr output appears in *Output → Nova LSP*.

---

## Development

```sh
# Run all tests
cd nova-lsp && cargo test

# Check for warnings
cd nova-lsp && cargo clippy -- -D warnings

# Build optimised binary
cd nova-lsp && cargo build --release
```

Tests are in `nova-lsp/tests/`:
- `build_smoke.rs` — binary exists, `--version`, graceful exit on closed stdin
- `lifecycle.rs`   — initialize/initialized/shutdown (Plan 104.0.2)
- `document_cache.rs` — didOpen/didChange/didClose state (Plan 104.0.3)
- `integration.rs` — end-to-end JSON-RPC handshake (Plan 104.0.4)

---

## Architecture

```
Editor (VSCode / Helix / Neovim / Zed)
   │
   │  JSON-RPC over stdio
   ▼
nova-lsp  (this crate)
   │
   │  Rust API  [Plan 104.1+]
   ▼
nova_codegen (compiler-codegen crate)
   ├── lexer/parser
   ├── types/  ← type-check incremental
   └── ast/    ← spans for hover + goto-def
```

See [Plan 104 architecture](../docs/plans/104-ide-integration.md#архитектура) for the full design rationale.
