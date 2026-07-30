; Tree-sitter highlights for tatara-lisp / caixa.
;
; Captures map to Neovim's `:help treesitter-highlight-groups`. Our nvim
; plugin (caixa.nvim/lua/caixa/colors.lua) binds those groups to the
; Nord/blackmatter palette — so these names stay canonical and portable.
;
; ── ORDER IS PRECEDENCE, AND IT RUNS TOP-TO-BOTTOM WEAKEST-TO-STRONGEST ──
;
; Neovim's treesitter highlighter applies every capture that covers a
; range and lets the LAST one win (equal extmark priority → later write
; paints over earlier). So in THIS file a rule further DOWN beats a rule
; further UP — the opposite of upstream tree-sitter's first-match-wins.
;
; That is not a stylistic note. Until 2026-07-29 the catch-all
; `(symbol) @variable` sat at the BOTTOM, so it overrode @keyword,
; @function.call and @constant on every single symbol and the whole
; language rendered in one flat near-white (#ECEFF4 under Nord) — only
; `:keywords`, strings, numbers and comments ever picked up a colour.
;
; The invariant: BROADEST rule first, NARROWEST rule last. Anything
; appended to the end of this file wins over everything above it.

; ── Weakest: every identifier is a variable until something says otherwise

(symbol) @variable

; ── Comments + literals ───────────────────────────────────────────────

(shebang)      @keyword.directive
(line_comment) @comment
(string)       @string
(number)       @number
(boolean)      @boolean
(nil)          @constant.builtin

; ── Reader macro punctuation ──────────────────────────────────────────

"'"  @operator
"`"  @operator
","  @operator
",@" @operator
"("  @punctuation.bracket
")"  @punctuation.bracket
"{"  @punctuation.bracket
"}"  @punctuation.bracket
"["  @punctuation.bracket
"]"  @punctuation.bracket

; ── Keywords (`:foo-bar`) ─────────────────────────────────────────────

(keyword) @keyword

; ── Word-shaped booleans ──────────────────────────────────────────────
;
; The grammar's `(boolean)` node is only `#t` / `#f`, but every caixa.lisp
; in the fleet writes `:publish-to-git true` / `:no-verify false` — those
; parse as plain symbols and would otherwise read as variables.

((symbol) @boolean
 (#any-of? @boolean "true" "false"))

; ── Enum-variant-style bare PascalCase symbols (Biblioteca, Critical…) ─

((symbol) @constant
 (#match? @constant "^[A-Z][A-Za-z0-9]+$"))

; ── Regular function calls ────────────────────────────────────────────

(list
  .
  (symbol) @function.call)

; ── Strongest: def* forms. Head symbol is the keyword; when a second
;    symbol follows the head, that symbol is the name being defined.

((list
   .
   (symbol) @keyword)
 (#match? @keyword "^def"))

((list
   .
   (symbol) @keyword
   .
   (symbol) @function)
 (#match? @keyword "^def"))
