; Text objects for tatara-lisp / caixa.
;
; In a homoiconic Lisp every form IS a call, so `af`/`if` operate on the
; enclosing form — the direct analogue of a function text object. blnvim
; already ships nvim-treesitter-textobjects; this file is what switches it
; on for this filetype.

(list) @function.outer
(list . (_) (_)* @function.inner)

; Compound literals get the class objects, so `ac`/`ic` grabs a whole
; nested `{ :name "…" :version "…" }` map in one motion.
(map) @class.outer
(map . "{" (_)* @class.inner "}")

(vector) @class.outer
(vector . "[" (_)* @class.inner "]")

; ── The `:key value` slot is THE unit of this language ────────────────
;
; A caixa manifest is not really a list of atoms, it is a list of SLOTS:
; `:kind Biblioteca`, `:package { … }`, `:workflows [ … ]`. Editing one
; means editing the keyword AND its value together — deleting just the
; value leaves a dangling key that silently changes the kwargs parity of
; every slot after it, which is the single easiest way to corrupt a
; manifest by hand.
;
; The grammar has no `pair` node to capture (a plist is flat), so the two
; siblings are joined with `#make-range!` — the mechanism
; nvim-treesitter-textobjects documents for exactly this shape. The
; result: `daa` deletes a whole slot, `caa` replaces one, `]a` / `[a`
; step slot-to-slot, and the parity invariant is preserved by the motion
; instead of by the author's attention.
;
; `@_k`/`@_v` are underscore-prefixed so they carry the pattern without
; themselves becoming selectable text objects.

((list
   (keyword) @_k
   .
   (_) @_v)
 (#make-range! "parameter.outer" @_k @_v))

((list
   (keyword) @_k
   .
   (_) @parameter.inner))

((map
   (keyword) @_k
   .
   (_) @_v)
 (#make-range! "parameter.outer" @_k @_v))

((map
   (keyword) @_k
   .
   (_) @parameter.inner))

; A vector holds bare elements, not slots — each element is its own
; parameter so `]a` still steps through `:workflows [ :a :b :c ]`.
(vector (_) @parameter.inner @parameter.outer)

; ── Navigation targets ────────────────────────────────────────────────
;
; `]f` / `[f` jump between top-level def* forms rather than every nested
; list, which is what makes a 300-line manifest navigable.

((list
   .
   (symbol) @_head)
 (#match? @_head "^def")) @block.outer

(line_comment) @comment.outer
(string) @string.outer
