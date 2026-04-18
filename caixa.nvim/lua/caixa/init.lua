-- caixa.nvim — Neovim integration for the caixa tatara-lisp ecosystem.
--
-- Bundles:
--   * filetype + extension detection (ftdetect/caixa.vim, registered below)
--   * tree-sitter parser + queries (caixa-ts/)
--   * LSP client wiring (caixa-lsp binary)
--   * Nord / blackmatter colorscheme highlights
--   * :Caixa* user commands (format, lint, build, lock, nix)
--
-- Install with lazy.nvim:
--   { "pleme-io/caixa", dir = "caixa.nvim", config = function()
--       require("caixa").setup({})
--     end }

local M = {}

---Merge default options with user overrides.
local defaults = {
  ---Path to the `caixa-lsp` binary. Auto-discovered from PATH if nil.
  lsp_cmd = nil,
  ---Path to the `feira` CLI. Auto-discovered from PATH if nil.
  feira_cmd = nil,
  ---Enable tree-sitter parser registration (requires nvim-treesitter).
  treesitter = true,
  ---Enable LSP auto-start on caixa filetype.
  lsp = true,
  ---Blackmatter colorscheme variant: "dark" | "light".
  theme = "dark",
  ---Format on save.
  format_on_save = true,
}

local function deep_merge(base, over)
  local out = {}
  for k, v in pairs(base) do out[k] = v end
  for k, v in pairs(over or {}) do out[k] = v end
  return out
end

function M.setup(user)
  M.config = deep_merge(defaults, user or {})

  -- Filetype registration — "caixa" is the canonical ft.
  vim.filetype.add({
    extension = {
      caixa = "caixa",
      lisp = function(_, _) return "caixa" end,
      lsp = function(_, _) return "caixa" end,
    },
    filename = {
      ["caixa.lisp"] = "caixa",
      ["lacre.lisp"] = "caixa",
      ["flake.lisp"] = "caixa",
    },
  })

  require("caixa.colors").apply(M.config.theme)

  if M.config.treesitter then
    require("caixa.treesitter").setup()
  end

  if M.config.lsp then
    require("caixa.lsp").setup(M.config)
  end

  require("caixa.commands").setup(M.config)

  if M.config.format_on_save then
    vim.api.nvim_create_autocmd("BufWritePre", {
      pattern = { "*.lisp", "caixa.lisp", "lacre.lisp", "flake.lisp" },
      callback = function()
        if vim.bo.filetype == "caixa" then
          vim.lsp.buf.format({ async = false })
        end
      end,
    })
  end
end

return M
