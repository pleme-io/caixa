-- Tree-sitter parser registration for the caixa grammar.
--
-- If nvim-treesitter is present, register the grammar source and map the
-- "caixa" filetype to the parser.

local M = {}

function M.setup()
  local ok, parsers = pcall(require, "nvim-treesitter.parsers")
  if not ok then
    return -- nvim-treesitter not installed; silent no-op
  end
  local parser_config = parsers.get_parser_configs()
  parser_config.caixa = {
    install_info = {
      url = "https://github.com/pleme-io/caixa",
      files = { "src/parser.c" },
      location = "caixa-ts",
      branch = "main",
    },
    filetype = "caixa",
    maintainers = { "@pleme-io" },
  }
end

return M
