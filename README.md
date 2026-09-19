# OII

Object2. Files end with .oii.

Brackets define scope. That is the whole format.

```oii
impt "common.oii", "ui.oii"

server [
  host: 127.0.0.1
  port: 4377
  price: 19.99
  welcome: "hello, {player_name}"
  icon: #"raw, \n left alone"#,
  ports: [4377, 4378, 80]
  plugin [
    name: "chat"
    enabled = true
  ]
]
```

Comments are `//`, `///`, `/* */`.

A node is a name, optional scalar args, then an optional bracket body:

```oii
server [ host: 127.0.0.1, port: 4377 ]
bare_arg_node 1 2 three []
```

Inside brackets you mix attrs and child nodes. Split them however you want. Spaces, newlines and commas are equivalent, trailing commas allowed. `:` and `=` mean the same thing. Run `oii fmt` and it all becomes `:` with 2 space indent.

Values:

- bare words like `127.0.0.1` or `foo-bar`. No spaces, no `//` inside. Things that look numeric but fail to parse as numbers stay bare, so dates and IPs just work.
- `"strings"` on one line. Standard escapes plus `\xNN` and `\u{...}`.
- `#"raw strings"#`, multiline, verbatim, no interpolation.
- ints as `42`, `-7`, `0xFF`, `0o17`, `0b1010`, `1_000_000`. i64.
- floats as `3.14`, `1e3`, `.5`, `1.`. f64.
- `true`, `false`, `null`.
- arrays like `[1, 2, 3]`. Arrays only show up where a value is expected, never as node args. `[` already denotes scope so this restriction keeps parsing sane.

Only double quoted strings interpolate: `"hello {name}"`. Pass values with `--var name=value`, `--env`, or `ParseOptions::vars` from Rust. Unknown names interpolate to empty and emit `W001`.

CLI:

```
oii parse <file|->     dump AST, --json dumps JSON instead
oii to-json <file|->   dump JSON
oii fmt [file|-]       print formatted source, --check exits 1 on diff, -w writes back

--fix --write          append missing ] at EOF, optionally write back
--var name=value       repeatable, for interpolation
--env                  pull interpolation vars from environment
--lang zh|en           diagnostic language, OII_LANG also respected, default zh
```

A diagnostic looks like this:

```
error 2:14 [E006] impt entries need commas
   2 | impt "a" "b"
   2 |          ^
   hint: write impt "a", "b"
```

`to-json` gives:

```json
{
  "imports": ["common.oii"],
  "nodes": [
    { "name": "server", "args": [], "attributes": { "port": 4377 }, "children": [] }
  ]
}
```

Attribute maps keep last key on duplicates. All three string kinds become JSON strings. There is also `doc_to_object` which folds the doc by top level node name for direct deserialization into your own structs.

Rust:

```toml
[dependencies]
oii = { path = "../oii" }
```

```rust
use oii::prelude::*;

let doc = parse("server [ port: 8080, name: \"web\" ]").unwrap();
let server = doc.node("server").unwrap();

assert_eq!(server.get("name").and_then(Value::as_str), Some("web"));
assert_eq!(server.get("port").and_then(Value::as_int), Some(8080));

#[derive(serde::Deserialize, Debug)]
#[serde(default)]
struct Server { port: u16, name: Option<String> }
impl Default for Server {
    fn default() -> Self { Server { port: 0, name: None } }
}

#[derive(serde::Deserialize, Debug)]
struct Cfg { server: Server }

let cfg: Cfg = serde_json::from_value(doc_to_object(&doc)).unwrap();
```

Things that bite:

- A bracketless node swallows following bare words as args. `foo` then `bar` on the next line is one node `foo` with arg `bar`. Terminate with `foo []` if that is not what you meant.
- `foo [1 2]` is a scope, not an array argument. Arrays live in attribute values.
- Bare words cannot hold whitespace or `//`. Quote URLs: `link: "https://a/b"`.
- `{` and `}` mean nothing outside double quoted strings. They error.
- `--fix` only appends missing `]` at EOF. A stray `]` is always an error.

Notes:

Handwritten lexer, chumsky grammar. Depends on serde, serde_json, clap. `cargo test` runs the suite.

License: Apache-2.0. Copyright 2026 Celvra.
