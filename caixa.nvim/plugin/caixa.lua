-- Auto-load for plugin managers that source plugin/*.lua.
-- When the user calls `require("caixa").setup(...)`, this file is a no-op;
-- otherwise it registers minimal filetype + highlights so .caixa / .lisp
-- files open with basic color even before explicit setup.

if vim.g.loaded_caixa_nvim then return end
vim.g.loaded_caixa_nvim = true

-- Register filetype extensions.
vim.filetype.add({
  extension = {
    caixa = "caixa",
  },
  filename = {
    ["caixa.lisp"] = "caixa",
    ["lacre.lisp"] = "caixa",
    ["flake.lisp"] = "caixa",
  },
})

-- Apply default Nord palette with no extra config.
pcall(function()
  require("caixa.colors").apply("dark")
end)
