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
    // A `.tlisp` script may open with a shebang (`#!/usr/bin/env
    // tatara-script`). Without this rule the very first byte is an ERROR
    // and the whole file loses highlighting.
    source_file: $ => seq(optional($.shebang), repeat($._form)),

    shebang: $ => token(seq('#!', /.*/)),

    _form: $ => choice(
      $.list,
      $.map,
      $.vector,
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

    // The brace/vector dialect is REAL SYNTAX, per decision D4 in
    // theory/TATARA-LISP-CONSOLIDATION.md. 62 live caixa.lisp manifests
    // author nested maps (`:package { :name "…" }`); they are consumed by
    // pleme-doc-gen. Without these rules a third of the corpus fails to
    // parse.
    map: $ => seq('{', repeat($._form), '}'),

    vector: $ => seq('[', repeat($._form), ']'),

    quote: $ => seq('\'', $._form),
    quasiquote: $ => seq('`', $._form),
    unquote: $ => seq(',', $._form),
    unquote_splicing: $ => seq(',@', $._form),

    // `.` is legal inside an identifier: module-qualified heads (`fs.read`)
    // and dotted names appear throughout the corpus.
    keyword: $ => /:[A-Za-z_+\-*/=<>?!%&~.][A-Za-z0-9_+\-*/=<>?!%&~.]*/,

    symbol: $ => /[A-Za-z_+\-*/=<>?!%&~.][A-Za-z0-9_+\-*/=<>?!%&~.]*/,

    // MUST be token(). Built from separate sub-rules, tree-sitter lets
    // `extras` interleave BETWEEN them — so a `;` inside a string literal
    // opened a line_comment and destroyed the parse for the rest of the
    // file. This one word moved corpus coverage from 72.2% to 99.4%.
    string: $ => token(seq(
      '"',
      repeat(choice(
        /[^"\\]/,
        /\\./,
      )),
      '"',
    )),

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
