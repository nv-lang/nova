-- SPDX-License-Identifier: MIT OR Apache-2.0
-- Nova language filetype detection for Neovim
-- Plan 104.8.Ф.2
--
-- Place at: ~/.config/nvim/lua/ftdetect/nova.lua
-- (or source from your init.lua / lazy.nvim plugin spec)

-- Register .nv extension → filetype "nova"
vim.filetype.add({
    extension = {
        nv = "nova",
    },
})

-- Also handle files named exactly "nova.toml" as TOML
-- (project manifest — not syntax highlighted as Nova, just ensure TOML)
vim.filetype.add({
    filename = {
        ["nova.toml"] = "toml",
    },
})
