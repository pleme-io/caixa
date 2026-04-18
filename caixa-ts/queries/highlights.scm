; Tree-sitter highlights for tatara-lisp / caixa.
;
; Captures map to Neovim's `:help treesitter-highlight-groups`. Our nvim
; plugin (caixa.nvim/lua/caixa/colors.lua) binds those groups to the
; Nord/blackmatter palette — so these names stay canonical and portable.

; ── Comments + literals ───────────────────────────────────────────────

(line_comment) @comment
(string)       @string
(number)       @number
(boolean)      @boolean
(nil)          @constant.builtin

; ── Keywords (`:foo-bar`) ─────────────────────────────────────────────

(keyword) @keyword

; ── def* forms: head symbol is a keyword, second position is the name ─

((list
   .
   (symbol) @keyword
   .
   (symbol) @function)
 (#match? @keyword "^def"))

; ── Standalone def* at list head without a name (e.g. (defcaixa :k v …))

((list
   .
   (symbol) @keyword)
 (#match? @keyword "^def"))

; ── Regular function calls ────────────────────────────────────────────

(list
  .
  (symbol) @function.call)

; ── Enum-variant-style bare PascalCase symbols (Biblioteca, Critical…) ─

((symbol) @constant
 (#match? @constant "^[A-Z][A-Za-z0-9]+$"))

; ── Reader macro punctuation ──────────────────────────────────────────

"'"  @operator
"`"  @operator
","  @operator
",@" @operator
"("  @punctuation.bracket
")"  @punctuation.bracket

; ── Fall-through for other identifiers ────────────────────────────────

(symbol) @variable
