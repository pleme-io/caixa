; nvim-treesitter-style highlights — mirrors caixa-ts/queries/highlights.scm
; but includes nvim-specific @-captures for the locals/injection pipeline.

(line_comment) @comment
(string)       @string
(number)       @number
(boolean)      @boolean
(nil)          @constant.builtin

(keyword) @keyword

((list
   .
   (symbol) @keyword
   .
   (symbol) @function)
 (#match? @keyword "^def"))

((list
   .
   (symbol) @keyword)
 (#match? @keyword "^def"))

(list
  .
  (symbol) @function.call)

((symbol) @constant
 (#match? @constant "^[A-Z][A-Za-z0-9]+$"))

"'"  @operator
"`"  @operator
","  @operator
",@" @operator
"("  @punctuation.bracket
")"  @punctuation.bracket

(symbol) @variable
