use std::collections::HashMap;

use oii::ast::{Doc, Node, Value};
use oii::diag::Lang;
use oii::{ParseOptions, parse, parse_with, to_json, to_json_string};

fn opts(vars: &[(&str, &str)]) -> ParseOptions {
    let mut map = HashMap::new();
    for (k, v) in vars {
        map.insert(k.to_string(), v.to_string());
    }
    ParseOptions {
        vars: map,
        lang: Lang::Zh,
        fix: false,
        keep_interp: false,
    }
}

fn ok(src: &str) -> Doc {
    match parse(src) {
        Ok(d) => d,
        Err(e) => panic!("解析失败: {:#?}\n源文件:\n{src}", e),
    }
}

fn attr<'a>(node: &'a Node, key: &str) -> &'a Value {
    &node
        .attributes
        .iter()
        .find(|a| a.key == key)
        .unwrap_or_else(|| panic!("节点 {} 没有属性 {key}", node.name))
        .value
}

fn get<'a>(node: &'a Node) -> impl Fn(&str) -> &'a Value + 'a {
    move |k: &str| &node.attributes.iter().find(|a| a.key == k).unwrap().value
}

#[test]
fn loose_attr_separators() {
    let doc = ok("a [ x: 1 y: 2\nz:3, w: 4 ,]\n");
    let n = &doc.nodes[0];
    assert_eq!(
        n.attributes
            .iter()
            .map(|a| a.key.as_str())
            .collect::<Vec<_>>(),
        vec!["x", "y", "z", "w"]
    );
    assert_eq!(*attr(n, "x"), Value::Int(1));
    assert_eq!(*attr(n, "y"), Value::Int(2));
    assert_eq!(*attr(n, "z"), Value::Int(3));
    assert_eq!(*attr(n, "w"), Value::Int(4));

    let doc = ok("a [ x: 1, y: 2, z: 3, ]\n");
    assert_eq!(doc.nodes[0].attributes.len(), 3);
}

#[test]
fn array_with_space_or_comma_sep() {
    let doc = ok("a [ v: [1 2 3], w: [4, 5, 6], x: [7 8 9,] ]");
    let n = &doc.nodes[0];
    assert_eq!(
        *attr(n, "v"),
        Value::Array(vec![Value::Int(1), Value::Int(2), Value::Int(3)])
    );
    assert_eq!(
        *attr(n, "w"),
        Value::Array(vec![Value::Int(4), Value::Int(5), Value::Int(6)])
    );
    assert_eq!(
        *attr(n, "x"),
        Value::Array(vec![Value::Int(7), Value::Int(8), Value::Int(9)])
    );

    let doc = ok("a [ matrix: [[1, 2], [3 4]] ]");
    let Value::Array(row) = attr(&doc.nodes[0], "matrix") else {
        panic!("期望数组")
    };
    assert_eq!(row.len(), 2);
}

#[test]
fn auto_close_brackets_with_fix() {
    let src = "foo [\n  a: 1\n  bar [ b: 2\n";
    let out = parse_with(
        src,
        &ParseOptions {
            fix: true,
            ..opts(&[])
        },
    );
    assert_eq!(out.doc.as_ref().expect("修复后应能解析").nodes.len(), 1);
    assert!(out.fix_applied);
    assert!(out.fixes().iter().any(|d| d.code == "F001"));
    let fixed = out.fixed_source.as_deref().unwrap();
    assert!(fixed.ends_with(']'));

    let out = parse_with("foo [ a: 1", &opts(&[]));
    assert!(out.has_errors() && out.doc.is_none());
    assert!(!out.fix_applied);
}

#[test]
fn interpolation_success_missing_warning() {
    let doc = parse_with(
        "msg [ text: \"你好 {name} 同学!\" ]",
        &opts(&[("name", "小明")]),
    );
    assert!(doc.doc.is_some());
    assert!(doc.warnings().is_empty());
    let n = &doc.doc.unwrap().nodes[0];
    assert_eq!(*attr(n, "text"), Value::Str("你好 小明 同学!".to_string()));

    let doc = parse_with("msg [ text: \"你好 {nobody}!\" ]", &opts(&[("other", "x")]));
    assert_eq!(
        doc.doc.as_ref().unwrap().nodes[0].attributes[0].value,
        Value::Str("你好 !".into())
    );
    assert_eq!(doc.warnings().len(), 1);
    assert_eq!(doc.warnings()[0].code, "W001");
}

#[test]
fn interpolation_survives_brace_and_escape() {
    let doc = parse_with("m [ a: \"{x}a{b}\" ]", &opts(&[("x", "1"), ("b", "2")]));
    assert_eq!(
        attr(&doc.doc.unwrap().nodes[0], "a"),
        &Value::Str("1a2".to_string())
    );
}

#[test]
fn impt_requires_commas_and_is_first() {
    let doc = ok("impt \"a\", \"b\"\nfoo []\n");
    assert_eq!(doc.imports, vec!["a".to_string(), "b".to_string()]);
    assert_eq!(doc.nodes.len(), 1);

    let e = parse("impt \"a\" \"b\"").unwrap_err();
    assert!(
        e.iter().any(|d| d.code == "E006"),
        "缺逗号应报 E006: {e:#?}"
    );

    let e = parse("foo []\nimpt \"a\"").unwrap_err();
    assert!(
        e.iter().any(|d| d.code == "E007"),
        "impt 必须在最前: {e:#?}"
    );

    let doc = parse("impt \"a\", \"b\", ").unwrap();
    assert_eq!(doc.imports.len(), 2);
}

#[test]
fn value_types_all() {
    let src = r##"v [
  hex: 0xFF, oct: 0o17, bin: 0b1010, under: 1_000_000,
  neg: -42, plus: +7, float1: 3.14, float2: -2.5, float3: 1e3, float4: 1.5e-2, float5: .5, float6: 1.,
  ok1: true, ok2: false, nothing: null,
  bare: 裸词-abc, esc: "a\nb\t\"q\"" , raw: #"c:\path\file"#,
]"##;
    let doc = ok(src);
    let n = &doc.nodes[0];
    let g = get(n);
    assert_eq!(*g("hex"), Value::Int(0xFF));
    assert_eq!(*g("oct"), Value::Int(0o17));
    assert_eq!(*g("bin"), Value::Int(0b1010));
    assert_eq!(*g("under"), Value::Int(1_000_000));
    assert_eq!(*g("neg"), Value::Int(-42));
    assert_eq!(*g("plus"), Value::Int(7));
    assert_eq!(*g("float1"), Value::Float(3.14));
    assert_eq!(*g("float2"), Value::Float(-2.5));
    assert_eq!(*g("float3"), Value::Float(1000.0));
    assert_eq!(*g("float4"), Value::Float(0.015));
    assert_eq!(*g("float5"), Value::Float(0.5));
    assert_eq!(*g("float6"), Value::Float(1.0));
    assert_eq!(*g("ok1"), Value::Bool(true));
    assert_eq!(*g("ok2"), Value::Bool(false));
    assert_eq!(*g("nothing"), Value::Null);
    assert_eq!(*g("bare"), Value::Bare("裸词-abc".to_string()));
    assert_eq!(*g("esc"), Value::Str("a\nb\t\"q\"".to_string()));
    assert_eq!(*g("raw"), Value::RawStr("c:\\path\\file".to_string()));
}

#[test]
fn comments_all_kinds() {
    let src = "// 行注释\n/// 文档注释\n/* 块\n注释 */\na [ // 行内注释\n b: 1 /* c: 2 */]";
    let doc = ok(src);
    assert_eq!(doc.nodes.len(), 1);
    assert_eq!(doc.nodes[0].attributes[0].value, Value::Int(1));
}

#[test]
fn braces_not_for_scopes() {
    let e = parse("foo { a: 1 }").unwrap_err();
    assert!(e.iter().any(|d| d.code == "E009"));
}

#[test]
fn colon_and_equals_both_work() {
    let doc = ok("a [ x: 1, y = 2 z: \"right\", r: raw, k: null ]");
    let n = &doc.nodes[0];
    assert_eq!(n.attributes.len(), 5);
    assert_eq!(*attr(n, "x"), Value::Int(1));
    assert_eq!(*attr(n, "y"), Value::Int(2));
    assert_eq!(*attr(n, "k"), Value::Null);
}

#[test]
fn node_name_vs_arg() {
    let doc = ok("foo bar baz []\nchild [ sub [] ]");
    assert_eq!(doc.nodes.len(), 2);
    assert_eq!(doc.nodes[0].name, "foo");
    assert_eq!(
        doc.nodes[0].args,
        vec![Value::Bare("bar".into()), Value::Bare("baz".into())]
    );
    assert_eq!(doc.nodes[1].name, "child");
    assert_eq!(doc.nodes[1].children[0].name, "sub");
}

#[test]
fn error_line_and_col_precise() {
    let src = "a [ x: 1 ]\nb [ y: ]\n";
    let e = parse(src).unwrap_err();
    let d = e.iter().find(|d| d.code == "E001").unwrap();
    assert_eq!(d.loc.line, 2, "错误应在第 2 行: {d:#?}");
}

#[test]
fn i18n_zh_en() {
    let src = "a [ x: , ]";
    let e = parse_with(
        src,
        &ParseOptions {
            lang: Lang::Zh,
            ..opts(&[])
        },
    );
    let zh = e
        .diagnostics
        .iter()
        .find(|d| d.level == oii::diag::Level::Error)
        .unwrap();
    assert!(zh.message.contains("缺值"));
    assert!(zh.hint.as_deref().unwrap_or("").contains("x: 1"));

    let e = parse_with(
        src,
        &ParseOptions {
            lang: Lang::En,
            ..opts(&[])
        },
    );
    let en = e
        .diagnostics
        .iter()
        .find(|d| d.level == oii::diag::Level::Error)
        .unwrap();
    assert_ne!(zh.message, en.message);
    assert!(en.message.contains("value"));
}

#[test]
fn eof_missing_bracket_points_at_end() {
    let e = parse("foo [ a: 1").unwrap_err();
    let d = e.iter().find(|d| d.code == "E001").unwrap();
    assert!(d.message.contains("结束") || d.message.contains("end"));
    assert!(d.hint.as_deref().unwrap_or("").contains("--fix"));
    assert_eq!(d.loc.line, 1);
}

#[test]
fn render_covers_cjk_and_tabs() {
    use oii::diag::{Diag, Lang, Level, Loc, render_diag};
    let src = "a [\n\t键: 中文\n]";
    let d = Diag {
        level: Level::Error,
        level_word: "error",
        code: "E000",
        message: "x".into(),
        hint: Some("h".into()),
        loc: Loc {
            line: 2,
            col: 4,
            end_line: 2,
            end_col: 6,
        },
    };
    let _ = Lang::En;
    let _ = Level::Warning;
    let s = render_diag(src, &d);
    assert!(s.contains("2:4"));
    assert!(s.contains('^'));
}

#[test]
fn to_json_structure() {
    let doc =
        ok("impt \"a\"\nuser [ name: \"mike\", age: 28, admin: true, flags: [1, 2], note: null ]");
    let j = to_json(&doc);
    assert_eq!(j["imports"], serde_json::json!(["a"]));
    assert_eq!(j["nodes"][0]["name"], serde_json::json!("user"));
    assert_eq!(j["nodes"][0]["attributes"]["age"], serde_json::json!(28));
    assert_eq!(
        j["nodes"][0]["attributes"]["flags"],
        serde_json::json!([1, 2])
    );
    assert!(j["nodes"][0]["attributes"]["note"].is_null());
    let s = to_json_string(&doc);
    assert!(s.contains("\"imports\""));
}

#[test]
fn fmt_unifies_colon_and_two_space_indent() {
    let doc = ok("Foo [ a=1 b :2, child [x=\"y\", child2: [] ] ]");
    let f = oii::format_doc(&doc);
    assert_eq!(
        f,
        "Foo [\n  a: 1,\n  b: 2,\n  child [\n    x: \"y\",\n    child2: [],\n  ]\n]\n"
    );
}

#[test]
fn fmt_roundtrip_idempotent() {
    let src = "Foo [ a=1 b:2,  child [x: \"y\"], arr: [1, 2, 3] ]";
    let d1 = parse(src).unwrap();
    let f1 = oii::format_doc(&d1);
    assert_eq!(parse(&f1).unwrap(), d1);
    let f2 = oii::format_doc(&parse(&f1).unwrap());
    assert_eq!(f1, f2);
}

#[test]
fn fmt_of_empty_simple() {
    let doc = ok("foo []\nbar [ ]\n");
    let f = oii::format_doc(&doc);
    assert_eq!(f, "foo []\nbar []\n");
    assert_eq!(parse(&f).unwrap(), doc);
}

#[test]
fn escape_sequences_and_raw_string_multiline() {
    let doc = ok("m [ a: \"x\\u{4E2D}y\", b: #\"multi\nline\"#, c: \"tab\\tcol\" ]");
    let n = &doc.nodes[0];
    assert_eq!(*attr(n, "a"), Value::Str("x中y".to_string()));
    assert_eq!(*attr(n, "b"), Value::RawStr("multi\nline".to_string()));
    assert_eq!(*attr(n, "c"), Value::Str("tab\tcol".to_string()));
}

#[test]
fn unterminated_string_reports_e003() {
    let e = parse("a [ b: \"oops ]").unwrap_err();
    assert!(e.iter().any(|d| d.code == "E003"));
}

#[test]
fn deep_nesting_and_sibling_lists() {
    let src = "root [\n  alpha: 1\n  group_a [ g: 2 ]\n  group_b [ g: 3 ]\n]";
    let doc = ok(src);
    let root = &doc.nodes[0];
    assert_eq!(root.children.len(), 2);
    assert_eq!(root.attributes[0].value, Value::Int(1));
    assert_eq!(root.children[1].attributes[0].value, Value::Int(3));
}

#[test]
fn keyword_values_are_not_bare() {
    let doc = ok("a [ x: null, y: true, z: false ]");
    assert_eq!(*attr(&doc.nodes[0], "x"), Value::Null);
    assert_eq!(*attr(&doc.nodes[0], "y"), Value::Bool(true));
    assert_eq!(*attr(&doc.nodes[0], "z"), Value::Bool(false));
    let e = parse("a [ w: impt ]").unwrap_err();
    assert!(!e.is_empty());
}

#[test]
fn value_accessors() {
    let doc = ok(
        "svc [ name: \"web\", port: 8080, ratio: 1.5, live: true, nil: null, xs: [1, 2], bare: 原生词 ]",
    );
    let n = &doc.nodes[0];
    assert_eq!(n.get("name").and_then(Value::as_str), Some("web"));
    assert_eq!(n.get("port").and_then(Value::as_int), Some(8080));
    assert_eq!(n.get("ratio").and_then(Value::as_float), Some(1.5));
    assert_eq!(n.get("port").and_then(Value::as_float), Some(8080.0));
    assert_eq!(n.get("live").and_then(Value::as_bool), Some(true));
    assert_eq!(n.get("bare").and_then(Value::as_str), Some("原生词"));
    assert!(n.get("nil").unwrap().is_null());
    assert_eq!(n.get("xs").and_then(Value::as_array).unwrap().len(), 2);
    assert!(n.get("missing").is_none());
    assert!(n.get("live").unwrap().as_int().is_none());
}

#[test]
fn node_getters() {
    let doc = ok("root [ child [ a: 1 ] child [ a: 2 ] leaf [] ]");
    let root = &doc.nodes[0];
    assert_eq!(
        root.get_node("child").unwrap().get("a").unwrap(),
        &Value::Int(1)
    );
    assert_eq!(root.find_all("child").count(), 2);
    assert!(root.get_node("leaf").is_some());
    assert!(root.get_node("nope").is_none());
    assert_eq!(doc.node("root").map(|n| n.name.as_str()), Some("root"));
    assert_eq!(doc.node("other"), None);
}

#[test]
fn serde_roundtrip() {
    let doc = ok(
        "impt \"a\"\nm [ s: \"x\", i: 7, f: 1.5, b: true, n: null, arr: [1, \"2\"], r: #\"raw\"#, bare: 词 ]",
    );
    let j = serde_json::to_string(&doc).unwrap();
    let back: Doc = serde_json::from_str(&j).unwrap();
    assert_eq!(back, doc);
}

#[test]
fn deserialize_into_user_struct() {
    #[derive(serde::Deserialize, PartialEq, Debug)]
    struct Cfg {
        server: Server,
    }
    #[derive(serde::Deserialize, PartialEq, Debug)]
    #[serde(default)]
    struct Server {
        port: u16,
        name: Option<String>,
        timers: Option<Vec<u32>>,
    }
    impl Default for Server {
        fn default() -> Self {
            Server {
                port: 0,
                name: None,
                timers: None,
            }
        }
    }
    let doc = ok("server [ port: 8080, name: \"web\" ]");
    let value = oii::doc_to_object(&doc);
    let cfg: Cfg = serde_json::from_value(value).unwrap();
    assert_eq!(cfg.server.port, 8080);
    assert_eq!(cfg.server.name.as_deref(), Some("web"));
    assert_eq!(cfg.server.timers, None);
}

#[test]
fn fmt_keeps_interp_template() {
    use oii::diag::Lang;
    let opts = oii::ParseOptions {
        vars: HashMap::new(),
        lang: Lang::Zh,
        fix: false,
        keep_interp: true,
    };
    let out = oii::parse_with("msg [ text: \"hi {name}!\" ]", &opts);
    assert!(out.warnings().is_empty());
    let doc = out.doc.unwrap();
    let f = oii::format_doc(&doc);
    assert!(f.contains("{name}"), "fmt ate interp: {f}");
    // reparse keeps template too
    let out2 = oii::parse_with(&f, &opts);
    let n = &out2.doc.unwrap().nodes[0];
    assert_eq!(
        *n.get("text").unwrap(),
        Value::Str("hi {name}!".to_string())
    );
}

#[test]
fn fmt_raw_with_close_delim_survives() {
    // lexer can never emit this (first "# always closes).
    // build by hand to cover the fallback path.
    let doc = Doc {
        imports: vec![],
        nodes: vec![Node {
            name: "m".into(),
            args: vec![],
            attributes: vec![oii::ast::Attribute {
                key: "r".into(),
                value: Value::RawStr("a\"#b {v}".into()),
            }],
            children: vec![],
        }],
    };
    let f = oii::format_doc(&doc);
    // quoted fallback parses as Str, not RawStr. content must match.
    let back = parse(&f).unwrap();
    assert_eq!(
        back.nodes[0].get("r").and_then(Value::as_str),
        doc.nodes[0].get("r").and_then(Value::as_str)
    );
}

#[test]
fn fmt_ends_with_newline() {
    let doc = ok("foo []");
    assert!(oii::format_doc(&doc).ends_with('\n'));
}

#[test]
fn number_fallback_stays_bare() {
    // bad numbers must not half-eat input. whole token is bare.
    let doc = ok("a [ v1: 1e, v2: 123abc, v3: 1.foo, v4: 127.0.0.1 ]");
    let n = &doc.nodes[0];
    assert_eq!(*attr(n, "v1"), Value::Bare("1e".into()));
    assert_eq!(*attr(n, "v2"), Value::Bare("123abc".into()));
    assert_eq!(*attr(n, "v3"), Value::Bare("1.foo".into()));
    assert_eq!(*attr(n, "v4"), Value::Bare("127.0.0.1".into()));
    let doc = ok("a [ v: 1.e3 ]");
    assert_eq!(*attr(&doc.nodes[0], "v"), Value::Float(1000.0));
}

#[test]
fn dup_key_warns_but_keeps_last() {
    let out = parse_with("a [ x: 1, x: 2 ]", &opts(&[]));
    let doc = out.doc.expect("dup must still parse");
    assert_eq!(doc.nodes[0].attributes.len(), 2); // both kept, fold is last wins
    assert!(out.diagnostics.iter().any(|d| d.code == "W002"));
    // json folds last wins
    assert_eq!(oii::to_json(&doc)["nodes"][0]["attributes"]["x"], 2);
}

#[test]
fn bare_node_swallow_warns() {
    let out = parse_with("foo\nbar []", &opts(&[]));
    assert!(out.doc.is_some());
    assert!(out.diagnostics.iter().any(|d| d.code == "W003"));
    // same line args stay quiet
    let out = parse_with("foo bar []", &opts(&[]));
    assert!(!out.diagnostics.iter().any(|d| d.code == "W003"));
}
