-- Nord + blackmatter palette for caixa sources.
-- Mirrors caixa-theme/src/blackmatter.rs so CLI and editor colors agree.

local M = {}

---@type table<string, string>
local nord = {
  nord0  = "#2E3440",
  nord1  = "#3B4252",
  nord2  = "#434C5E",
  nord3  = "#4C566A",
  nord4  = "#D8DEE9",
  nord5  = "#E5E9F0",
  nord6  = "#ECEFF4",
  nord7  = "#8FBCBB",
  nord8  = "#88C0D0",
  nord9  = "#81A1C1",
  nord10 = "#5E81AC",
  nord11 = "#BF616A",
  nord12 = "#D08770",
  nord13 = "#EBCB8B",
  nord14 = "#A3BE8C",
  nord15 = "#B48EAD",
}

---Blackmatter-dark semantic → Nord color map.
local function palette_dark()
  return {
    Keyword       = nord.nord9,
    Symbol        = nord.nord4,
    KeywordArg    = nord.nord8,
    String        = nord.nord14,
    Number        = nord.nord15,
    Literal       = nord.nord13,
    Comment       = nord.nord3,
    Accent        = nord.nord8,
    Muted         = nord.nord3,
    Error         = nord.nord11,
    Warning       = nord.nord12,
    Info          = nord.nord8,
    Hint          = nord.nord13,
  }
end

---Blackmatter-light semantic → Nord color map.
local function palette_light()
  return {
    Keyword       = nord.nord10,
    Symbol        = nord.nord0,
    KeywordArg    = nord.nord10,
    String        = nord.nord14,
    Number        = nord.nord15,
    Literal       = nord.nord12,
    Comment       = nord.nord2,
    Accent        = nord.nord10,
    Muted         = nord.nord2,
    Error         = nord.nord11,
    Warning       = nord.nord12,
    Info          = nord.nord10,
    Hint          = nord.nord13,
  }
end

local function hi(group, fg, opts)
  opts = opts or {}
  opts.fg = fg
  vim.api.nvim_set_hl(0, group, opts)
end

---Apply the palette — binds treesitter highlight groups to Nord colors.
function M.apply(variant)
  variant = variant or "dark"
  local p = variant == "light" and palette_light() or palette_dark()

  -- Base groups.
  hi("@comment.caixa",        p.Comment,    { italic = true })
  hi("@string.caixa",         p.String)
  hi("@number.caixa",         p.Number)
  hi("@boolean.caixa",        p.Literal,    { bold = true })
  hi("@constant.builtin.caixa", p.Literal,  { bold = true })
  hi("@keyword.caixa",        p.Keyword,    { bold = true })
  hi("@function.caixa",       p.Symbol)
  hi("@function.call.caixa",  p.Symbol)
  hi("@variable.caixa",       p.Symbol)
  hi("@constant.caixa",       p.Literal,    { bold = true })
  hi("@operator.caixa",       p.Accent)
  hi("@punctuation.bracket.caixa", p.Muted)

  -- LSP diagnostic groups.
  hi("DiagnosticError",       p.Error)
  hi("DiagnosticWarn",        p.Warning)
  hi("DiagnosticInfo",        p.Info)
  hi("DiagnosticHint",        p.Hint)

  -- Expose the palette so other plugins can read it.
  M.active = p
  M.nord = nord
end

return M
