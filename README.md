# OII

Config language for Xyle, a package manager in development. Files end with .oii.

Brackets define scope. That is the whole format.

Compared to the Nix language, OII is easier to start with. Names, values, brackets. 1.0 adds funcs, a small evaluator, and lossless file edits. Config stays a plain tree. Code lives in `fun` blocks.

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

## 1.1 syntax

Slashdash `/-` comments out the next node, attribute, or argument. Disabled items stay in the tree for edits but are skipped by reads and JSON.

```oii
server [
  host: 127.0.0.1
  /-port: 4377
  /-legacy []
]
```

Type annotations `(type)` on a node or a value. The parser keeps the name.

```oii
(u8)count 7
created: (date)"2021-02-03"
```

Strings. Raw strings take any number of `#` so the content can hold `"#`. Triple quotes give a dedented multiline string, raw or cooked.

```oii
regex: ##"$\d+"##
msg: """
  hello
  world
  """
```

Special floats `#inf`, `#-inf`, `#nan`. A backslash continues a line. A bare name that is a keyword can be quoted.

```oii
scale: #inf
matrix: [1 2 3 \
         4 5 6]
"fun": 1
```

## Funcs

A func is a named method. Name, params, optional `desc`, bracket body. `desc` is lifted out of the body by the parser and exposed as `Func::desc`.

```oii
fun fact(n) [
  desc: "factorial of n"
  let r: 1
  while n > 1 [
    r: r * n
    n: n - 1
  ]
  return r
]
```

Params take a default with `:` or `=`. Statements are `let`, assignment by plain `name: expr`, `if`/`else`, `while`, `for x in xs`, `return`, plus a bare expression. Expressions have arithmetic, comparison, logic, calls, arrays, and lambdas.

`let` binds many names and destructures arrays:

```oii
let a: 1, b: 2
let [x, y, *rest]: [10, 20, 30, 40]
```

`fun(x) [ ... ]` is a lambda. It captures the scope where it is written and is a first-class value. Named funcs can be passed by name too.

```oii
fun adder(n) [ return fun(x) [ return x + n ] ]
fun apply(f, x) [ return f(x) ]
```

Operators need spaces around `+` and `-` so bare words with hyphens keep working.

Run a func:

```
oii run app.oii fact --arg n=5
oii run app.oii greet --arg name=bo --json
```

The evaluator is Turing complete. Step and depth limits stop runaway code (`E102`, `E101`). Builtins: `len push get keys contains type str int float range print map put abs min max`.

## File edits

`DocFile` parses a file, edits it, and writes it back. Comments and layout survive because edits splice exact byte ranges. Every splice reparses the whole doc. A bad edit rolls back.

Read and change values:

```
oii get app.oii server.port
oii set app.oii server.port 8080        # prints new source
oii set app.oii server.port 8080 -w     # writes back
oii set app.oii fact.desc "n factorial"
oii insert app.oii packages fastfetch   # add a line to a func
oii insert app.oii packages git --before neovim  # insert before an entry
oii insert app.oii config.registry aur  # add an array element
oii set-index app.oii config.registry 0 '"main"'
oii insert-index app.oii config.registry 1 '"aur"'
oii rm-index app.oii config.registry 0
oii rm app.oii packages.git             # drop a func line
oii rm app.oii config.registry.extra    # drop an array element
oii watch app.oii                       # re-check on change
oii watch app.oii --fmt                 # reformat and write on change
```

`insert target line` picks the container by target:

- a func name -> appends a statement line to the func body
- an array attribute path -> appends an element
- a node path -> appends a child line

A declarative package manager needs nothing more. On `pkg add fastfetch` it hands the file and the target to this library:

```rust
let mut f = oii::DocFile::load("pkgs.oii")?;
f.insert("packages", "fastfetch")?;   // func body
f.insert("config.registry", "\"aur\"")?; // array attr
f.save()?;

f.remove("packages.git")?;             // func line
f.remove("config.registry.extra")?;    // array element
f.save()?;
```

The full edit surface:

- `get(path)`, `set(path, value)`, `set_desc(func, desc)`
- `insert(target, raw)`, `insert_before(target, anchor, raw)`
- `add_node(parent, src)`, `add_func(src)`
- `set_index(path, i, value)`, `insert_index(path, i, raw)`, `remove_index(path, i)`
- `set_enabled(path, on)`, `toggle(path)` slashdash on or off
- `set_ty(path, ty)`, `rename(path, name)`, `sort_attrs(path)`
- `remove(path)` for a func, node, attr, func statement, or array element
- `replace(start, end, raw)` raw byte range escape hatch
- `transaction(|f| ...)` all or nothing batch
- `save()`, `save_as(path)`

Paths are dot separated. `server.plugin.port`, `packages.git`, `config.registry.aur`. Insert infers the indent style from the file and terminates a trailing bare sibling so it does not swallow the new line.

## Typed decode

Pull values straight off the ast. No JSON library needed.

```rust
use oii::FromNode;

#[derive(FromNode)]
struct Server {
    port: i64,
    #[oii(rename = "name")]
    label: Option<String>,
    #[oii(default)]
    ttl: i64,
    #[oii(skip)]
    cached: bool,
}

let doc = oii::parse(src)?;
let server: Server = doc.node("server").unwrap().decode()?;
```

`Option` fields use `field_opt`. `default` uses `field_or`. `skip` uses `Default::default()`. Or implement `FromNode` by hand and call `n.field`, `n.field_opt`, `n.field_or`.

The derive macro is behind the default `derive` feature.

`serde_json` is optional. The default `json` feature turns on `to-json`, `doc_to_object`, and the language server. The default `derive` feature turns on the `FromNode` derive. Build with `--no-default-features` and the core parse, decode, eval, and edit code pulls in no JSON, no CLI stack, and no proc macros.

CLI:

```
oii parse <file|->     dump AST, --json dumps JSON instead
oii to-json <file|->   dump JSON
oii funcs <file>       list funcs and desc, --json for JSON
oii run <file> <func>  run a func, --arg name=value, --json
oii get <file> <path>  read one value
oii set <file> <path> <value>  edit one value, -w writes back
oii insert <file> <target> <line>  add a func line, node line, or array element, --before for position, -w writes back
oii rm <file> <path>   remove a func node attr func line or array element, -w writes back
oii toggle <file> <path>  slashdash on or off, --on --off, -w writes back
oii set-ty <file> <path> <ty>  set or clear a (type) annotation, - clears
oii rename <file> <path> <name>  rename a node or attr key
oii sort <file> <path>  sort a node's single line attrs by key
oii set-index <file> <path> <i> <value>  replace an array element
oii insert-index <file> <path> <i> <line>  insert an array element
oii rm-index <file> <path> <i>  remove an array element
oii fmt [file|-]       print formatted source, --check exits 1 on diff, -w writes back
oii check <file|->     diags only, exit 1 on error, --deny-warnings also fails on warnings
oii completion <shell> print shell completion script (bash zsh fish powershell elvish)
oii lsp                language server on stdio (diagnostics, formatting, completion, hover)
oii watch <file>       re-check on change, --fmt reformats and writes

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
- In func bodies write `n - 1` with spaces. `n-1` is one bare word, not subtraction.

## Changes in 1.1.0

All additive. Old files parse unchanged.

- slashdash `/-`, type annotations `(type)`, `#inf #-inf #nan`, line continuation `\`
- raw strings take any number of `#`, triple quoted multiline strings are dedented
- quoted identifiers let keywords be data names
- `Node` gains `enabled` and `ty`. `Attribute` gains `enabled`. `Value` gains `Typed` and `Disabled`
- edit api: `set_enabled`, `toggle`, `set_ty`, `rename`, `sort_attrs`
- formatter always writes raw strings with hashes so they stay raw

## Breaking changes in 1.0.0

- `fun`, `let`, `if`, `else`, `while`, `for`, `in`, `return` are keywords. They no longer work as bare words or node names.
- `desc` is a func keyword in func bodies.
- `Doc` gains a `funcs` field. Struct literals need updating. `#[serde(default)]` keeps old JSON loading.
- `Value` gains `Map` and `Func` variants for the evaluator. Downstream `match` needs new arms.
- `Stmt::Let` now holds `bindings: Vec<(Pattern, Expr)>`. `Expr::Call` holds a `callee` expression.
- `Pattern` is new. `Expr::Lambda` is new.
- `(` `)` and the operators `== != <= >= < > + - * / % && || !` are now tokens. A lone `+`, `-`, `*`, `/`, `%` no longer errors at lex time.
- `serde_json` is optional behind the default `json` feature. Use `FromNode`/`decode` to avoid it.

Notes:

Handwritten lexer, chumsky grammar. Core depends on serde. The `json` feature adds serde_json, clap, and the language server. `cargo test` runs the suite.
Syntax highlighting lives in `editors/`: VSCode (`vscode/`), Sublime, Vim, Emacs, Nano, Helix (with `oii lsp` wiring), plus `tree-sitter-oii/`.

License: Apache-2.0. Copyright 2026 Celvra.
