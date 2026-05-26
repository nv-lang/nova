-- SPDX-License-Identifier: MIT OR Apache-2.0
-- Nova Language — Neovim LSP configuration (nvim-lspconfig snippet)
-- Plan 104.8.Ф.2
--
-- Requirements:
--   - nvim-lspconfig (https://github.com/neovim/nvim-lspconfig)
--   - nova-lsp binary in PATH, or set vim.g.nova_lsp_path below
--
-- Usage:
--   Copy this snippet into your Neovim config, e.g.:
--     ~/.config/nvim/after/plugin/nova-lsp.lua   (direct)
--     ~/.config/nvim/lua/plugins/nova.lua          (lazy.nvim plugin spec)
--
-- Then open a .nv file and run :LspInfo to verify "nova" is attached.

-- ─────────────────────────────────────────────────────────────
-- Configuration
-- ─────────────────────────────────────────────────────────────

-- Override nova-lsp binary path (optional):
--   vim.g.nova_lsp_path = "/home/me/.cargo/bin/nova-lsp"
-- If not set, nova-lsp must be in PATH.

-- ─────────────────────────────────────────────────────────────
-- Binary discovery
-- ─────────────────────────────────────────────────────────────

--- Returns the nova-lsp command to use.
--- Priority: vim.g.nova_lsp_path > PATH lookup > workspace target/release
---@return string[] cmd  e.g. { "/usr/local/bin/nova-lsp" }
local function nova_lsp_cmd()
    -- 1. User override
    if vim.g.nova_lsp_path and vim.g.nova_lsp_path ~= "" then
        return { vim.g.nova_lsp_path }
    end

    -- 2. PATH lookup
    local exe = vim.fn.exepath("nova-lsp")
    if exe and exe ~= "" then
        return { exe }
    end

    -- 3. Workspace-relative: target/release/nova-lsp[.exe]
    local workspace_root = vim.fn.getcwd()
    local candidates = {
        workspace_root .. "/target/release/nova-lsp",
        workspace_root .. "/target/debug/nova-lsp",
        workspace_root .. "/target/release/nova-lsp.exe",
        workspace_root .. "/target/debug/nova-lsp.exe",
    }
    for _, candidate in ipairs(candidates) do
        if vim.fn.filereadable(candidate) == 1 then
            return { candidate }
        end
    end

    -- 4. Graceful fallback: return the bare name so nvim-lspconfig
    --    reports "cmd not found" cleanly (no crash).
    vim.notify(
        "[nova-lsp] binary not found.\n"
        .. "  Solutions:\n"
        .. "  1. Build: cargo build --release -p nova-lsp\n"
        .. "  2. Add nova-lsp to PATH\n"
        .. "  3. Set vim.g.nova_lsp_path in your config",
        vim.log.levels.WARN,
        { title = "Nova LSP" }
    )
    return { "nova-lsp" }
end

-- ─────────────────────────────────────────────────────────────
-- Register nova with nvim-lspconfig
-- ─────────────────────────────────────────────────────────────

local ok, lspconfig = pcall(require, "lspconfig")
if not ok then
    vim.notify(
        "[nova-lsp] nvim-lspconfig not found. Install it first:\n"
        .. "  https://github.com/neovim/nvim-lspconfig",
        vim.log.levels.ERROR,
        { title = "Nova LSP" }
    )
    return
end

local configs_ok, configs = pcall(require, "lspconfig.configs")
if not configs_ok then
    vim.notify("[nova-lsp] lspconfig.configs not found.", vim.log.levels.ERROR)
    return
end

-- Register "nova" server config (idempotent — safe to source multiple times)
if not configs.nova then
    configs.nova = {
        default_config = {
            -- Command: nova-lsp communicates via stdio JSON-RPC
            cmd = nova_lsp_cmd(),
            -- File types that activate this LSP
            filetypes = { "nova" },
            -- Workspace-aware root detection:
            --   prefer nova.toml (Nova project manifest), fallback to .git
            root_dir = lspconfig.util.root_pattern("nova.toml", ".git"),
            -- Nova-specific settings (populated as LSP features grow)
            settings = {},
            -- Server info for :LspInfo display
            single_file_support = true,
        },
        docs = {
            description = [[
Nova language server (nova-lsp).
Provides compiler diagnostics for .nv files via LSP.
https://github.com/nv-lang/nova
            ]],
        },
    }
end

-- Setup with optional user overrides
lspconfig.nova.setup({
    -- on_attach: add your keybindings here, e.g.:
    -- on_attach = function(client, bufnr)
    --   vim.keymap.set("n", "gd", vim.lsp.buf.definition, { buffer = bufnr })
    --   vim.keymap.set("n", "K",  vim.lsp.buf.hover,      { buffer = bufnr })
    -- end,

    -- Capabilities: uncomment if using nvim-cmp for completions
    -- capabilities = require("cmp_nvim_lsp").default_capabilities(),
})
