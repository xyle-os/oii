// OII grammar. Mirrors src/grammar.rs.
// Highlighting only. Real diagnostics come from the Rust parser.
//
// Known gaps vs lex.rs: digit-led bares like 123abc, 1e or 1.foo fall
// back to one bare word in Rust but split into pieces here. Scopes
// stay value-ish either way, so highlighting looks right.
module.exports = grammar({
  name: 'oii',

  // Space and newlines separate items. Commas are explicit below.
  extras: ($) => [/[\s]/, $.comment],

  // Args are bare-only. After a name, another name keeps args open.
  conflicts: ($) => [[$.node]],

  rules: {
    // doc first. tree-sitter starts at the first rule.
    doc: ($) => seq(optional($.impt), repeat($.node)),

    // impt goes first in the file. highlighter only; real check is Rust.
    impt: ($) => seq('impt', sepBy1(',', $._import_entry)),

    _import_entry: ($) => choice($.string, $.raw_string),

    node: ($) =>
      seq(
        field('name', $.name),
        // Bare args only. Arrays live in attr values.
        field('args', repeat($._scalar)),
        field('body', optional($.body))
      ),

    body: ($) => seq('[', repeat(seq(choice($.attr, $.node), optional(','))), ']'),

    attr: ($) =>
      seq(
        field('key', $.name),
        field('sep', choice(':', '=')),
        field('value', $.value)
      ),

    value: ($) => choice($._scalar, $.array),

    array: ($) => seq('[', repeat(seq($.value, optional(','))), ']'),

    _scalar: ($) =>
      choice(
        $.string,
        $.raw_string,
        $.int,
        $.float,
        $.bool,
        $.null,
        $.bare
      ),

    name: (_) => /[A-Za-z_\u0080-\uFFFF][\w.\-+\u0080-\uFFFF]*/,
    bare: ($) =>
      choice(
        /[A-Za-z_\u0080-\uFFFF][\w.\-+\u0080-\uFFFF]*/,
        // dotted numerics like 127.0.0.1. Rust folds these to one
        // bare word. two-plus dots never ties with float, so longest
        // match just wins. no prec hack needed.
        /\d[\d_]*(\.[\d_]*[\d])(\.[\d_]*[\d])*/
      ),

    // One token. Extras must not leak inside strings.
    string: (_) =>
      token(
        seq(
          '"',
          repeat(
            choice(
              /[^"\\\n{]+/,
              /\\(n|r|t|0|\\\\|"|x[0-9A-Fa-f]{2}|u\{[0-9A-Fa-f]+\})/,
              /\{[A-Za-z_][A-Za-z0-9_]*\}/,
              '{'
            )
          ),
          '"'
        )
      ),

    // Verbatim. No escapes, no interpolation.
    raw_string: (_) => token(seq('#"', /([^"]|"[^#])*"?/, '"#')),

    int: (_) =>
      choice(
        /[+-]?0[xX][0-9A-Fa-f_]+/,
        /[+-]?0[oO][0-7_]+/,
        /[+-]?0[bB][01_]+/,
        /[+-]?[0-9][0-9_]*/
      ),

    float: (_) =>
      /[+-]?(\d[\d_]*\.\d[\d_]*([eE][+-]?\d[\d_]*)?|\d[\d_]*[eE][+-]?\d[\d_]*|\.\d[\d_]*([eE][+-]?\d[\d_]*)?|\d[\d_]*\.)/,

    bool: (_) => choice('true', 'false'),
    null: (_) => 'null',

    comment: (_) =>
      token(
        choice(
          /\/\/[^\n]*/,
          seq('/*', /([^*]|\*[^/])*/, '*/')
        )
      ),
  },
});

function sepBy1(sep, rule) {
  return seq(rule, repeat(seq(sep, rule)), optional(sep));
}
