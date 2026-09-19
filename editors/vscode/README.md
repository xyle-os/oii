# OII for VS Code

`.oii` highlighting, brackets, comments, snippets.

```oii
impt "common.oii"

server [
  host: 127.0.0.1
  port: 4377
  welcome: "hi {name}"
  icon: #"raw \n kept"#
  ports: [4377, 4378]
]
```

## Covers

- `//`, `///`, `/* */` comments
- `impt` keyword, `true` `false` `null`
- `"strings"` with escapes and `{var}` interpolation
- `#"raw strings"#`, verbatim, multiline
- hex `0xFF`, oct `0o17`, bin `0b1010`, ints, floats
- `:` `=` `,` `[]` punctuation
- `{}`, lone `/` `#` flagged illegal (braces do nothing in OII)

Tab is 2 spaces, matching `oii fmt`.

## CLI pair

The [oii CLI](https://github.com/xyle-os/oii) adds check, format and LSP:

```
oii check file.oii
oii fmt -w file.oii
oii lsp
```

## License

Apache-2.0
