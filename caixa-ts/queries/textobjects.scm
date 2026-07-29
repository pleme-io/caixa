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

(line_comment) @comment.outer
(string) @string.outer
