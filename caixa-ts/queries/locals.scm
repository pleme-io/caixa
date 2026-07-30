; Locals for tatara-lisp / caixa — lets rename, goto-def, and highlight
; work sensibly under Neovim's LSP-plus-treesitter integration.
;
; ── EVERY CAPTURE HERE MUST LIVE IN THE `local.` NAMESPACE ────────────
;
; Neovim core and nvim-treesitter both key a locals match on the `local`
; prefix: `nvim-treesitter/query.lua` splits each capture name on `.` to
; build `{ local = { definition = … } }`, and then `locals.lua:44` does an
; UNGUARDED `loc["local"]["definition"]`.
;
; A bare `@scope` / `@reference` / `@definition.function` therefore yields
; `{ scope = … }`, `loc["local"]` is nil, and the index throws:
;
;   nvim-treesitter/locals.lua:44: attempt to index field 'local' (a nil value)
;
; vim-illuminate walks that path on CursorMoved, so until 2026-07-29 simply
; moving the cursor onto a symbol in any `.tlisp` buffer raised
; "vim-illuminate: An internal error has occured" five times in ~85ms, after
; which illuminate disabled itself for the rest of the session. The prefix is
; load-bearing, not styling. Cross-check `queries/lua/locals.scm` and
; `queries/commonlisp/locals.scm` in any nvim-treesitter tree: all `@local.*`.

[
  (source_file)
  (list)
] @local.scope

; Top-level def* forms define a named entity (the second symbol).
;
; `@_keyword` is deliberately underscore-prefixed: it carries the `#match?`
; predicate and nothing else. A bare `@keyword` is not a locals capture, so
; it would be dead weight here even where it did not break the split.
((list
   .
   (symbol) @_keyword
   .
   (symbol) @local.definition.function)
 (#match? @_keyword "^def"))

; Reference sites — every symbol in a non-head position is a reference.
(list
  .
  (_)
  (symbol) @local.reference)
