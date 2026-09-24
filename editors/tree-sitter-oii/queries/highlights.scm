; OII highlights. Keep in sync with tmLanguage.
; NOTE: string is one opaque token (extras must not leak inside),
; so interpolation has no separate capture here.
(comment) @comment
(impt) @keyword
("fun") @keyword.function
("let") @keyword
("if") @keyword
("else") @keyword
("while") @keyword
("for") @keyword
("in") @keyword
("return") @keyword
((name) @keyword
  (#match? @keyword "^(desc)$"))
(func name: (name) @function)
(bool) @constant.builtin.boolean
(null) @constant.builtin
(string) @string
(raw_string) @string.special
(int) @number
(float) @number.float
(bare) @variable.parameter
(attr key: (name) @property)
(node name: (name) @type)
"[" @punctuation.bracket
"]" @punctuation.bracket
":" @punctuation.delimiter
"=" @punctuation.delimiter
"," @punctuation.delimiter
