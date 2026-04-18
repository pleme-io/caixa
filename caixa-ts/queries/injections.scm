; Language injections — the string body of a `:query` field in a defmonitor
; form is a PromQL expression, the body of a `:nix-expr` is Nix, etc.
;
; Phase 1.B injections; more land as domains stabilize.

((list
   .
   (symbol) @_head
   (keyword) @_kw
   .
   (string) @injection.content)
 (#match? @_head "^defmonitor$")
 (#match? @_kw "^:query$")
 (#set! injection.language "promql"))

((list
   .
   (keyword) @_kw
   .
   (string) @injection.content)
 (#match? @_kw "^:nix-expr$")
 (#set! injection.language "nix"))
