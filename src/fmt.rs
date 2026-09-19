use crate::ast::{Doc, Node, Value};

pub fn format_doc(doc: &Doc) -> String {
    let mut out = String::new();
    if !doc.imports.is_empty() {
        let parts: Vec<String> = doc
            .imports
            .iter()
            .map(|s| format!("\"{}\"", escape_str(s)))
            .collect();
        out.push_str(&format!("impt {}", parts.join(", ")));
        out.push('\n');
    }
    for (i, n) in doc.nodes.iter().enumerate() {
        if i > 0 || !doc.imports.is_empty() {
            out.push('\n');
        }
        out.push_str(&fmt_node(n, 0));
    }
    // posix tail newline. empty doc stays empty.
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

fn fmt_node(n: &Node, indent: usize) -> String {
    let pad = "  ".repeat(indent);
    let inner = "  ".repeat(indent + 1);
    let mut head = String::new();
    head.push_str(&n.name);
    for a in &n.args {
        head.push(' ');
        head.push_str(&fmt_value(a));
    }
    if n.attributes.is_empty() && n.children.is_empty() {
        return format!("{pad}{head} []");
    }
    head.push_str(" [\n");
    let mut body = String::new();
    for a in &n.attributes {
        body.push_str(&format!("{inner}{}: {},\n", a.key, fmt_value(&a.value)));
    }
    for c in &n.children {
        body.push_str(&fmt_node(c, indent + 1));
        body.push('\n');
    }
    body.push_str(&pad);
    body.push_str("]");
    format!("{pad}{head}{body}")
}

pub fn fmt_value(v: &Value) -> String {
    match v {
        Value::Bare(s) => s.clone(),
        Value::Str(s) => format!("\"{}\"", escape_str(s)),
        Value::RawStr(s) => {
            if s.contains("\"#") {
                // quoted strings interpolate {var}. raw does not.
                // spell { as \u{7B} so semantics survive.
                format!("\"{}\"", escape_str(s).replace('{', "\\u{7B}"))
            } else {
                format!("#\"{s}\"#")
            }
        }
        Value::Int(i) => i.to_string(),
        Value::Float(f) => format!("{f:?}"),
        Value::Bool(b) => b.to_string(),
        Value::Null => "null".to_string(),
        Value::Array(items) => {
            let parts: Vec<String> = items.iter().map(fmt_value).collect();
            format!("[{}]", parts.join(", "))
        }
    }
}

fn escape_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\0' => out.push_str("\\0"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\x{:02x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}
