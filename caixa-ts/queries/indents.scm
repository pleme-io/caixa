; Indent for tatara-lisp / caixa.
;
; S-expression indentation: a compound form opens an indent level and its
; closing delimiter closes it. `@indent.branch` on the closer is what makes
; a lone `)` on its own line dedent to match its opener rather than sitting
; at the child level.

[
  (list)
  (map)
  (vector)
] @indent.begin

[
  ")"
  "}"
  "]"
] @indent.branch @indent.end
