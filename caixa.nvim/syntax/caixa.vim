" Fallback syntax — used when tree-sitter isn't available.
" Tree-sitter + caixa.nvim's highlights.scm produce richer output when they
" can load.

if exists("b:current_syntax") | finish | endif

syntax match caixaComment /;.*$/
syntax region caixaString start=+"+ skip=+\\"+ end=+"+
syntax match caixaNumber  /\<-\?\d\+\(\.\d\+\)\?\>/
syntax match caixaBool    /#[tf]/
syntax match caixaNil     /\<nil\>/
syntax match caixaKeyword /:[A-Za-z0-9_\-+*/=<>?!%&~]\+/
syntax match caixaDef     /\<def[A-Za-z0-9_\-]*\>/

highlight default link caixaComment Comment
highlight default link caixaString  String
highlight default link caixaNumber  Number
highlight default link caixaBool    Constant
highlight default link caixaNil     Constant
highlight default link caixaKeyword Identifier
highlight default link caixaDef     Keyword

let b:current_syntax = "caixa"
