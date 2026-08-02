# nova-lsp

**nova-lsp** is the official Language Server Protocol (LSP) server for the [Nova programming language](https://nv-lang.org).

It powers IDE features — diagnostics, hover, go-to-definition, completion, document/workspace symbols, find-references, quick-fixes, rename, and format — for any editor that speaks LSP (VSCode, Cursor, Neovim, Helix, Zed, and more).

> **Status:** **V1 complete** — [Plan 104](../docs/plans/104-ide-integration.md) (IDE integration) closed 2026-06-17; all sub-plans 104.0–104.9 landed
> (diagnostics, hover + goto + signature-help, completion, symbols + references, ≥25 code-actions, rename + format, tree-sitter grammar,
> editor packaging, keyword sync). **V2** — [Plan 104.10](../docs/plans/104.10-lsp-v2-production.md) (production parity with
> rust-analyzer / gopls / tsserver / IntelliJ) is proposed / in progress.
>
> ⚠ **Conventions:** the server reuses `compiler-codegen` as a library and must follow
> [docs/dev/compiler-conventions.md](../docs/dev/compiler-conventions.md) — no hardcoded type/method/keyword lists (§3: "LSP обязан резолвить
> методы из `.nv`"), single source of truth for types (§0/`ResolvedType`), no silent holes (§4). Known gaps are tracked as `[M-104.10-*]`
> markers and in Plan 104.10.

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

A full TypeScript extension (LanguageClient with auto-restart + configurable binary path) lives in
[`editors/vscode`](../editors/vscode). Point it at your built binary via workspace `.vscode/settings.json`:

```json
{
  "nova-lsp.serverPath": "/path/to/nova-lsp/target/release/nova-lsp"
}
```

### Neovim (nvim-lspconfig)

```lua
-- In your init.lua / after/plugin/lsp.lua
require("lspconfig").nova_lsp.setup({
  cmd = { "/path/to/nova-lsp/target/release/nova-lsp" },
  filetypes = { "nova" },
  root_dir = require("lspconfig.util").root_pattern("nova.toml", ".git"),
})
```

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

### Zed / Sublime / Emacs / Vim

Per-editor packaging lives under [`editors/`](../editors) (Plan 104.8). See each editor's directory for setup.

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

Integration tests live in `nova-lsp/tests/` — build/lifecycle/document-cache smoke tests plus
per-feature suites (`completion.rs`, `symbols_references.rs`, `compiler_adapter.rs`,
`field_cache_lens.rs`, `workspace.rs`, `perf.rs`, `publish_workflow.rs`, `e2e_smoke.rs`,
`lifecycle_memory.rs`, …).

---

## Architecture

```
Editor (VSCode / Helix / Neovim / Zed)
   │
   │  JSON-RPC over stdio
   ▼
nova-lsp  (this crate)  — tower-lsp + tokio; per-feature modules:
   │        compiler.rs · diagnostic_mapping.rs · hover.rs · goto_definition.rs
   │        completion.rs · symbols.rs · signature_help.rs · code_actions.rs
   │        rename.rs · format.rs · incremental.rs · semantic_tokens_delta.rs
   │
   │  Rust library API (reuse, not fork)
   ▼
nova_codegen (compiler-codegen crate)
   ├── lexer/parser
   ├── types/  ← ResolvedType single source of truth (compiler-conventions §0)
   └── ast/    ← spans for hover + goto-def
```

See [Plan 104 architecture](../docs/plans/104-ide-integration.md#архитектура) for the full V1 design and
[Plan 104.10](../docs/plans/104.10-lsp-v2-production.md) for the V2 production-parity roadmap.
