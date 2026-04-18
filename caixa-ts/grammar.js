/**
 * tree-sitter-caixa — grammar for tatara-lisp / caixa sources.
 *
 * Mirrors the surface of `caixa_ast::parser` (which is the authoritative
 * Rust grammar used by fmt/lint/LSP). Keep both in lockstep; when one
 * changes, the other follows.
 *
 * The grammar is deliberately minimal — homoiconic Lisp plus the four
 * reader macros (quote, quasiquote, unquote, unquote-splicing). Semantic
 * flavors (defcaixa, defteia, etc.) are not separate rules; they surface
 * via query captures in queries/highlights.scm.
 */

module.exports = grammar({
  name: 'caixa',

  extras: $ => [
    /\s/,
    $.line_comment,
  ],

  word: $ => $.symbol,

  rules: {
    source_file: $ => repeat($._form),

    _form: $ => choice(
      $.list,
      $.quote,
      $.quasiquote,
      $.unquote,
      $.unquote_splicing,
      $.string,
      $.number,
      $.boolean,
      $.nil,
      $.keyword,
      $.symbol,
    ),

    list: $ => seq(
      '(',
      repeat($._form),
      ')',
    ),

    quote: $ => seq('\'', $._form),
    quasiquote: $ => seq('`', $._form),
    unquote: $ => seq(',', $._form),
    unquote_splicing: $ => seq(',@', $._form),

    keyword: $ => /:[A-Za-z_+\-*/=<>?!%&~][A-Za-z0-9_+\-*/=<>?!%&~]*/,

    symbol: $ => /[A-Za-z_+\-*/=<>?!%&~][A-Za-z0-9_+\-*/=<>?!%&~]*/,

    string: $ => seq(
      '"',
      repeat(choice(
        /[^"\\]/,
        /\\./,
      )),
      '"',
    ),

    number: $ => token(seq(
      optional(choice('-', '+')),
      /[0-9]+/,
      optional(seq('.', /[0-9]+/)),
      optional(seq(/[eE]/, optional(choice('-', '+')), /[0-9]+/)),
    )),

    boolean: $ => choice('#t', '#f'),

    nil: $ => 'nil',

    line_comment: $ => token(seq(';', /.*/)),
  },
});
