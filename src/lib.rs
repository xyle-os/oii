pub mod ast;
pub mod body;
pub mod decode;
pub mod diag;
pub mod edit;
pub mod eval;
pub mod fix;
pub mod fmt;
pub mod grammar;
pub mod lex;
pub mod lint;
#[cfg(feature = "json")]
pub mod lsp;

use std::collections::HashMap;

pub use ast::{Attribute, BinOp, Doc, Expr, Func, Node, Param, Stmt, UnOp, Value};
pub use decode::{DecodeError, FromNode, FromValue};
// same name as the trait. macro and trait live in different namespaces
pub use diag::{Diag, Lang, Level};
pub use edit::DocFile;
pub use eval::{EvalError, EvalOptions, EvalOutput, eval, eval_call};
pub use fmt::format_doc;
#[cfg(feature = "derive")]
pub use oii_derive::FromNode;

pub mod prelude {
    pub use crate::ast::{Attribute, BinOp, Doc, Expr, Func, Node, Param, Stmt, UnOp, Value};
    pub use crate::decode::{DecodeError, FromNode, FromValue};
    pub use crate::diag::{Diag, Lang, Level, render_diag, render_diag_with_file};
    pub use crate::edit::DocFile;
    pub use crate::eval::{EvalError, EvalOptions, EvalOutput, eval, eval_call};
    pub use crate::{ParseOptions, ParseOutput, format_doc, parse, parse_with};
    #[cfg(feature = "json")]
    pub use crate::{doc_to_object, to_json, to_json_string};
}
#[derive(Debug, Clone)]
pub struct ParseOptions {
    pub vars: HashMap<String, String>,
    pub lang: Lang,
    pub fix: bool,
    // keep {var} as-is fmt uses this
    pub keep_interp: bool,
}

impl Default for ParseOptions {
    fn default() -> Self {
        ParseOptions {
            vars: HashMap::new(),
            lang: Lang::Zh,
            fix: false,
            keep_interp: false,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ParseOutput {
    pub doc: Option<Doc>,
    pub diagnostics: Vec<Diag>,
    pub fixed_source: Option<String>,
    pub fix_applied: bool,
}

impl ParseOutput {
    pub fn has_errors(&self) -> bool {
        self.diagnostics.iter().any(|d| d.level == Level::Error)
    }

    pub fn warnings(&self) -> Vec<&Diag> {
        self.diagnostics
            .iter()
            .filter(|d| d.level == Level::Warning)
            .collect()
    }

    pub fn fixes(&self) -> Vec<&Diag> {
        self.diagnostics
            .iter()
            .filter(|d| d.level == Level::Fix)
            .collect()
    }
}

pub fn parse_with(src: &str, opts: &ParseOptions) -> ParseOutput {
    let lang = opts.lang;
    let mut diagnostics: Vec<Diag> = Vec::new();

    let (work_src, fix_diags, fix_applied) = if opts.fix {
        let (fixed, diags) = fix::apply_auto_fix(src, lang);
        let applied = !diags.is_empty();
        (fixed, diags, applied)
    } else {
        (src.to_string(), Vec::new(), false)
    };
    diagnostics.extend(fix_diags);

    let lexed = lex::lex_with(&work_src, &opts.vars, lang, opts.keep_interp);
    let lex_err = lexed.diags.iter().any(|d| d.level == Level::Error);
    diagnostics.extend(lexed.diags);

    let mut out = ParseOutput {
        doc: None,
        diagnostics: Vec::new(),
        fixed_source: if fix_applied {
            Some(work_src.clone())
        } else {
            None
        },
        fix_applied,
    };

    if !lex_err {
        match grammar::parse_tokens(&lexed.tokens, &work_src, lang) {
            Ok(items) => {
                let parts = grammar::split(items, &lexed.tokens, &work_src, lang);
                let parts_err = parts.diags.iter().any(|d| d.level == Level::Error);
                diagnostics.extend(parts.diags);
                if !parts_err {
                    out.doc = Some(Doc {
                        imports: parts.imports,
                        nodes: parts.nodes,
                        funcs: parts.funcs,
                    });
                    // warnings only never fail a clean parse
                    diagnostics.extend(lint::check(&lexed.tokens, &work_src, lang));
                }
            }
            Err(d) => diagnostics.push(d),
        }
    }

    out.diagnostics = diagnostics;
    out
}

pub fn parse(src: &str) -> Result<Doc, Vec<Diag>> {
    let out = parse_with(src, &ParseOptions::default());
    match out.doc {
        Some(doc) => Ok(doc),
        None => Err(out.diagnostics),
    }
}

#[cfg(feature = "json")]
pub fn to_json(doc: &Doc) -> serde_json::Value {
    serde_json::json!({
        "imports": doc.imports,
        "nodes": doc.nodes.iter().map(node_json).collect::<Vec<_>>(),
        "funcs": doc.funcs.iter().map(|f| serde_json::to_value(f).unwrap_or_default()).collect::<Vec<_>>(),
    })
}

#[cfg(feature = "json")]
pub fn doc_to_object(doc: &Doc) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    for n in &doc.nodes {
        map.insert(n.name.clone(), node_as_object(n));
    }
    serde_json::Value::Object(map)
}

#[cfg(feature = "json")]
fn node_as_object(n: &Node) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    for a in &n.attributes {
        map.insert(a.key.clone(), value_json(&a.value));
    }
    for c in &n.children {
        map.insert(c.name.clone(), node_as_object(c));
    }
    if !n.args.is_empty() {
        map.insert(
            "args".to_string(),
            serde_json::Value::Array(n.args.iter().map(value_json).collect()),
        );
    }
    serde_json::Value::Object(map)
}

#[cfg(feature = "json")]
fn node_json(n: &Node) -> serde_json::Value {
    let mut attrs = serde_json::Map::new();
    for a in &n.attributes {
        attrs.insert(a.key.clone(), value_json(&a.value));
    }
    serde_json::json!({
        "name": n.name,
        "args": n.args.iter().map(value_json).collect::<Vec<_>>(),
        "attributes": attrs,
        "children": n.children.iter().map(node_json).collect::<Vec<_>>(),
    })
}

#[cfg(feature = "json")]
pub fn value_json(v: &Value) -> serde_json::Value {
    match v {
        Value::Bare(s) => serde_json::Value::String(s.clone()),
        Value::Str(s) => serde_json::Value::String(s.clone()),
        Value::RawStr(s) => serde_json::Value::String(s.clone()),
        Value::Int(i) => serde_json::json!(i),
        Value::Float(f) => serde_json::json!(f),
        Value::Bool(b) => serde_json::json!(b),
        Value::Null => serde_json::Value::Null,
        Value::Array(items) => serde_json::Value::Array(items.iter().map(value_json).collect()),
        Value::Map(items) => {
            let mut m = serde_json::Map::new();
            for (k, v) in items {
                m.insert(k.clone(), value_json(v));
            }
            serde_json::Value::Object(m)
        }
        // func has no json form
        Value::Func(_) => serde_json::Value::Null,
    }
}

#[cfg(feature = "json")]
pub fn to_json_string(doc: &Doc) -> String {
    serde_json::to_string_pretty(&to_json(doc)).unwrap_or_default()
}
