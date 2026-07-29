-- LSP client wiring for caixa-lsp.
-- Works with or without nvim-lspconfig — we register the server directly
-- via vim.lsp.start() on the tlisp filetype.
--
-- NOTE the server is named caixa-lsp but serves the whole tatara-lisp
-- language, so it attaches to `tlisp`, not to a caixa-specific filetype.

local M = {}

function M.setup(cfg)
  local cmd = cfg.lsp_cmd or "caixa-lsp"
  if vim.fn.executable(cmd) ~= 1 then
    vim.schedule(function()
      vim.notify(
        "caixa-lsp binary not found — install with `nix run .#default` or add it to PATH",
        vim.log.levels.WARN,
        { title = "caixa.nvim" }
      )
    end)
    return
  end

  vim.api.nvim_create_autocmd("FileType", {
    pattern = "tlisp",
    callback = function(args)
      local root = vim.fs.root(args.buf, { "caixa.lisp", ".git" })
        or vim.fn.getcwd()
      vim.lsp.start({
        name = "caixa-lsp",
        cmd = { cmd },
        filetypes = { "tlisp" },
        root_dir = root,
        settings = {},
      })
    end,
  })
end

return M
