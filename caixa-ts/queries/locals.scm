; Locals for tatara-lisp / caixa — lets rename, goto-def, and highlight
; work sensibly under Neovim's LSP-plus-treesitter integration.

(source_file) @scope
(list) @scope

; Top-level def* forms define a named entity (the second symbol).
((list
   .
   (symbol) @keyword
   .
   (symbol) @definition.function)
 (#match? @keyword "^def"))

; Reference sites — every symbol in a non-head position is a reference.
(list
  .
  (_)
  (symbol) @reference)
