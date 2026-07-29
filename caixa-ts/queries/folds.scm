; Folds for tatara-lisp / caixa.
;
; Every compound form folds. With `foldmethod=expr` this gives
; fold-by-form over a whole manifest, which is the navigation primitive
; a 300-line caixa.lisp or a 240-line .tlisp script actually needs.

(list)   @fold
(map)    @fold
(vector) @fold
