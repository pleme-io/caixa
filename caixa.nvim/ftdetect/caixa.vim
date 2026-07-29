" Filetype detection for tatara-lisp.
"
" The filetype is `tlisp` — one language, one filetype. caixa manifests
" are a dialect of tatara-lisp, not a separate language, so they share it.
"
" `.tlisp` is unambiguous and claimed outright. Bare `*.lisp` is NOT
" claimed wholesale: nvim maps it to `lisp` (Common Lisp) by default and
" stealing every .lisp buffer would break real Common Lisp editing. Only
" the manifest filenames we own, plus a content sniff for a leading
" `(def…` form, are taken.
autocmd BufNewFile,BufRead *.tlisp     set filetype=tlisp
autocmd BufNewFile,BufRead caixa.lisp  set filetype=tlisp
autocmd BufNewFile,BufRead lacre.lisp  set filetype=tlisp
autocmd BufNewFile,BufRead flake.lisp  set filetype=tlisp
autocmd BufNewFile,BufRead *.caixa     set filetype=tlisp
