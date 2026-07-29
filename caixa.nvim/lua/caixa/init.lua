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
  ---Enable LSP auto-start on the tlisp filetype.
  lsp = true,
  ---Blackmatter colorscheme variant: "dark" | "light".
  theme = "dark",
  ---Format on save. DEFAULT OFF, deliberately.
  ---
  ---This routes to vim.lsp.buf.format -> caixa-lsp -> caixa-fmt, and
  ---caixa-fmt DELETES COMMENTS INSIDE FORMS. Reproduction:
  ---
  ---  (defcaixa demo          ->   (defcaixa demo :versao "0.1.0")
  ---    ;; why this version
  ---    :versao "0.1.0")
  ---
  ---printer.rs calls emit_leading only from the top-level loop, so trivia
  ---on nested nodes is dropped. The crate's proptest cannot catch it: it
  ---compares to_tatara_sexp() trees, which are trivia-blind by
  ---construction, so the invariant is vacuous on exactly the property it
  ---appears to guard. Measured on examples/checkout-aplicacao/caixa.lisp:
  ---24 comment lines in, 20 out.
  ---
  ---Turn this back on when caixa-fmt round-trips comments, not before.
  format_on_save = false,
}

local function deep_merge(base, over)
  local out = {}
  for k, v in pairs(base) do out[k] = v end
  for k, v in pairs(over or {}) do out[k] = v end
  return out
end

function M.setup(user)
  M.config = deep_merge(defaults, user or {})

  -- Filetype registration — `tlisp` is the canonical ft. caixa manifests
  -- are a dialect of tatara-lisp, not a language of their own.
  --
  -- Bare `*.lisp` and `*.lsp` are deliberately NOT claimed. This block used
  -- to map BOTH unconditionally, with no content sniff, which silently
  -- stole every Common Lisp and every `.lsp` buffer on the machine. Only
  -- the manifests we own are taken, by exact filename.
  vim.filetype.add({
    extension = {
      caixa = "tlisp",
      tlisp = "tlisp",
    },
    filename = {
      ["caixa.lisp"] = "tlisp",
      ["lacre.lisp"] = "tlisp",
      ["flake.lisp"] = "tlisp",
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
      pattern = { "*.lisp", "*.tlisp", "caixa.lisp", "lacre.lisp", "flake.lisp" },
      callback = function()
        if vim.bo.filetype == "tlisp" then
          vim.lsp.buf.format({ async = false })
        end
      end,
    })
  end
end

return M
