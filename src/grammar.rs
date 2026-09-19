use std::ops::Range;

use chumsky::prelude::*;

use crate::ast::{Attribute, Node, Value};
use crate::diag::{Diag, Lang, loc_of, tr};
use crate::lex::Kind;

pub type TokErr = Simple<Kind, Range<usize>>;

pub enum ItemBody {
    Impt(Vec<(String, usize)>),
    Node(Node),
}

pub struct RawItem {
    pub idx: usize,
    pub body: ItemBody,
}

struct Cells {
    attrs: Vec<Attribute>,
    children: Vec<Node>,
}

fn scalar_parser(lang: Lang) -> impl Parser<Kind, Value, Error = TokErr> + Clone {
    choice((
        select!(Kind::Str(s) => Value::Str(s)).labelled(tr(lang, "字符串", "string")),
        select!(Kind::RawStr(s) => Value::RawStr(s)).labelled(tr(lang, "原始串", "raw")),
        select!(Kind::Int(i) => Value::Int(i)).labelled(tr(lang, "整数", "int")),
        select!(Kind::Float(f) => Value::Float(f)).labelled(tr(lang, "浮点", "float")),
        select!(Kind::Bool(b) => Value::Bool(b)).labelled(tr(lang, "布尔", "bool")),
        select!(Kind::Null => Value::Null).labelled("null"),
        select!(Kind::Name(s) => Value::Bare(s)).labelled(tr(lang, "裸词", "bare")),
    ))
    .labelled(tr(lang, "值", "value"))
}

fn value_parser(lang: Lang) -> impl Parser<Kind, Value, Error = TokErr> + Clone {
    recursive(move |value| {
        let array = just(Kind::LBracket)
            .ignored()
            .then(
                value
                    .clone()
                    .then_ignore(just(Kind::Comma).or_not().ignored())
                    .repeated(),
            )
            .then_ignore(just(Kind::RBracket).ignored())
            .map(|(_, vals)| Value::Array(vals))
            .labelled(tr(lang, "数组", "array"));
        let scalar = scalar_parser(lang);
        choice((scalar, array)).labelled(tr(lang, "值", "value"))
    })
}

fn node_parser(lang: Lang) -> impl Parser<Kind, Node, Error = TokErr> + Clone {
    recursive(move |node| {
        let name = select!(Kind::Name(s) => s).labelled(tr(lang, "节点名", "name"));
        let value = value_parser(lang);
        let scalar_value = scalar_parser(lang);
        let attr =
            name.clone()
                .then(
                    choice((just(Kind::Colon).ignored(), just(Kind::Equals).ignored()))
                        .labelled(tr(lang, "`:` 或 `=`", "`:` or `=`")),
                )
                .then(value.clone())
                .map(|((key, _), value)| Attribute { key, value })
                .labelled(tr(lang, "属性", "attr"));
        let cell = choice((
            attr.clone().map(|a| Cells {
                attrs: vec![a],
                children: Vec::new(),
            }),
            node.clone().map(|n| Cells {
                attrs: Vec::new(),
                children: vec![n],
            }),
        ))
        .then_ignore(just(Kind::Comma).or_not().ignored())
        .labelled(tr(lang, "属性或子节点", "attr or child"));
        let body = just(Kind::LBracket)
            .ignored()
            .then(cell.repeated())
            .then_ignore(just(Kind::RBracket).ignored())
            .map(|(_, cells)| {
                let mut attrs = Vec::new();
                let mut children = Vec::new();
                for c in cells {
                    attrs.extend(c.attrs);
                    children.extend(c.children);
                }
                (attrs, children)
            })
            .labelled(tr(lang, "节点体", "body"));
        name.clone()
            .then(scalar_value.repeated())
            .then(body.or_not())
            .map(|((name, args), body)| {
                let (attributes, children) = body.unwrap_or((Vec::new(), Vec::new()));
                Node {
                    name,
                    args,
                    attributes,
                    children,
                }
            })
            .labelled(tr(lang, "节点", "node"))
    })
}

fn impt_parser(lang: Lang) -> impl Parser<Kind, RawItem, Error = TokErr> + Clone {
    let entry = choice((
        select!(Kind::Str(s) => s).labelled(tr(lang, "字符串", "string")),
        select!(Kind::RawStr(s) => s).labelled(tr(lang, "原始串", "raw")),
    ))
    .then_ignore(just(Kind::Comma).or_not().ignored())
    .map_with_span(|s, sp: Range<usize>| (s, sp.start));
    just(Kind::Impt)
        .ignored()
        .then(entry.repeated())
        .map_with_span(|(_, entries), sp: Range<usize>| RawItem {
            idx: sp.start,
            body: ItemBody::Impt(entries),
        })
        .labelled(tr(lang, "impt", "impt"))
}

fn document(lang: Lang) -> impl Parser<Kind, Vec<RawItem>, Error = TokErr> + Clone {
    let node_item = node_parser(lang)
        .map_with_span(|n, sp: Range<usize>| RawItem {
            idx: sp.start,
            body: ItemBody::Node(n),
        })
        .labelled(tr(lang, "节点", "node"));
    let top =
        choice((impt_parser(lang), node_item)).labelled(tr(lang, "impt 或节点", "impt or node"));
    top.repeated().then_ignore(end())
}

pub struct DocParts {
    pub imports: Vec<String>,
    pub nodes: Vec<Node>,
    pub diags: Vec<Diag>,
}

pub fn parse_tokens(raw: &[(Kind, usize)], src: &str, lang: Lang) -> Result<Vec<RawItem>, Diag> {
    let kinds: Vec<Kind> = raw.iter().map(|t| t.0.clone()).collect();
    match document(lang).parse(kinds) {
        Ok(items) => Ok(items),
        Err(errs) => {
            let err = errs.iter().min_by_key(|e| e.span().start).unwrap();
            Err(grammar_diag(err, raw, src, lang))
        }
    }
}

pub fn split(items: Vec<RawItem>, raw: &[(Kind, usize)], src: &str, lang: Lang) -> DocParts {
    let mut imports = Vec::new();
    let mut nodes = Vec::new();
    let mut diags = Vec::new();
    let mut first_node_idx: Option<usize> = None;

    for item in items {
        match item.body {
            ItemBody::Impt(entries) => {
                if first_node_idx.is_some() {
                    let byte = raw.get(item.idx).map(|t| t.1).unwrap_or(src.len());
                    diags.push(Diag::error_hint(
                        lang,
                        "E007",
                        "impt 必须在最前面. 别放节点后面",
                        "impt goes first. not after nodes",
                        Some("把 impt 搬到文件头"),
                        Some("move impt to top"),
                        loc_of(src, byte, byte.saturating_add(4).min(src.len())),
                    ));
                } else {
                    let mut prev_idx: Option<usize> = None;
                    for (name, at) in &entries {
                        if let Some(prev) = prev_idx {
                            let between = &raw[prev + 1..*at];
                            let has_comma = between.iter().any(|(k, _)| *k == Kind::Comma);
                            if !has_comma {
                                let byte = raw.get(*at).map(|t| t.1).unwrap_or(src.len());
                                diags.push(Diag::error_hint(
                                    lang,
                                    "E006",
                                    "impt 条目间要逗号",
                                    "impt entries need commas",
                                    Some("写法是 impt \"a\", \"b\""),
                                    Some("write impt \"a\", \"b\""),
                                    loc_of(src, byte, byte + 1),
                                ));
                            }
                        }
                        prev_idx = Some(*at);
                        imports.push(name.clone());
                    }
                }
            }
            ItemBody::Node(n) => {
                if first_node_idx.is_none() {
                    first_node_idx = Some(item.idx);
                }
                nodes.push(n);
            }
        }
    }

    DocParts {
        imports,
        nodes,
        diags,
    }
}

fn grammar_diag(err: &TokErr, raw: &[(Kind, usize)], src: &str, lang: Lang) -> Diag {
    let sp = err.span();
    let idx = sp.start.min(raw.len().saturating_sub(1));
    let byte = raw.get(idx).map(|t| t.1).unwrap_or(src.len());
    let end_byte = raw.get(sp.end).map(|t| t.1).unwrap_or(src.len());
    let found = err.found().cloned();
    let found_desc = found
        .as_ref()
        .map(|k| k.describe(lang))
        .unwrap_or_else(|| tr(lang, "文件结尾", "end of input").to_string());
    let mut expects: Vec<String> = err
        .expected()
        .map(|e| match e.as_ref() {
            Some(k) => k.describe(lang),
            None => tr(lang, "文件结尾", "end of input").to_string(),
        })
        .collect();
    let label = err.label().map(|s| s.to_string());
    if let Some(l) = &label {
        if !expects.iter().any(|e| e == l) {
            expects.push(l.clone());
        }
    }
    let expects = if expects.is_empty() {
        tr(lang, "合法符号", "valid item").to_string()
    } else {
        expects.join(", ")
    };

    let (zh_msg, en_msg) = (
        format!("要 {expects}, 却看到 {found_desc}. 语法错了"),
        format!("want {expects}, got {found_desc}. bad syntax"),
    );

    let (zh_hint, en_hint) = hint_for(&found, &label, lang);

    Diag::error_hint(
        lang,
        "E001",
        &zh_msg,
        &en_msg,
        zh_hint.as_deref(),
        en_hint.as_deref(),
        loc_of(src, byte, end_byte),
    )
}

fn hint_for(
    found: &Option<Kind>,
    label: &Option<String>,
    lang: Lang,
) -> (Option<String>, Option<String>) {
    let (zh, en) = match found {
        Some(Kind::LBrace) | Some(Kind::RBrace) => (
            Some("花括号没用. 作用域只认方括号"),
            Some("braces do nothing. scopes use brackets"),
        ),
        Some(Kind::RBracket) => (
            Some("多了一个 `]`. 检查括号配对"),
            Some("stray `]`. check brackets"),
        ),
        Some(Kind::Str(_)) if label.as_deref() == Some(tr(lang, "impt 或节点", "impt or node")) => {
            (
                Some("裸字符串不能当节点. impt 条目间加逗号"),
                Some("string is not a node. separate impt entries with commas"),
            )
        }
        Some(Kind::Int(_)) | Some(Kind::Float(_))
            if label.as_deref() == Some(tr(lang, "属性或子节点", "attr or child")) =>
        {
            (
                Some("数字不是节点也不是属性. 数组写 key: [1, 2]"),
                Some("number is not a node. write arrays as key: [1, 2]"),
            )
        }
        Some(Kind::Comma) => (Some("逗号后面要有值"), Some("comma needs a value after it")),
        _ => (None, None),
    };
    (zh.map(str::to_string), en.map(str::to_string))
}
