-- Tree-sitter parser registration for the tatara-lisp grammar.
--
-- The grammar is `tatara_lisp` (it parses the whole language; caixa
-- manifests are one dialect of it) and the filetype is `tlisp`. Both
-- names are load-bearing: the grammar name is the dlsym symbol AND the
-- `queries/<name>/` directory nvim looks in, and the filetype is what
-- nvim keys highlight/indent/fold on.

local M = {}

function M.setup()
  local ok, parsers = pcall(require, "nvim-treesitter.parsers")
  if not ok then
    return -- nvim-treesitter not installed; silent no-op
  end
  local parser_config = parsers.get_parser_configs()
  parser_config.tatara_lisp = {
    install_info = {
      url = "https://github.com/pleme-io/caixa",
      files = { "src/parser.c" },
      location = "caixa-ts",
      branch = "main",
    },
    filetype = "tlisp",
    maintainers = { "@pleme-io" },
  }
end

return M
