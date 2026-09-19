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
    let found = err.found().cloned();
    let at_end = sp.start >= raw.len();
    let byte = raw.get(sp.start).map(|t| t.1).unwrap_or(src.len());
    // span covers the offending token only. never backwards.
    let end_byte = raw
        .get(sp.start)
        .map(|t| (t.1 + tok_width(&t.0)).min(src.len()))
        .unwrap_or(src.len())
        .max(byte.min(src.len()));
    let end_byte = if end_byte <= byte {
        (byte + 1).min(src.len().max(byte))
    } else {
        end_byte
    };
    let found_desc = found
        .as_ref()
        .map(|k| k.describe(lang))
        .unwrap_or_else(|| tr(lang, "文件结尾", "end of input").to_string());
    let expects = clean_expects(err, lang);
    let label = err.label().map(|s| s.to_string());

    let (zh_msg, en_msg) = message_for(&found, at_end, missing_value(raw, sp.start), &expects, &found_desc);
    let (zh_hint, en_hint) = hint_for(&found, at_end, missing_value(raw, sp.start), &label, lang);

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

// source width of one token. clamped, display only.
fn tok_width(k: &Kind) -> usize {
    match k {
        Kind::Name(s) | Kind::Str(s) | Kind::RawStr(s) => s.len().clamp(1, 12) + 2,
        Kind::Int(i) => i.to_string().len().max(1),
        Kind::Float(f) => f.to_string().len().max(1),
        Kind::Bool(true) => 4,
        Kind::Bool(false) => 5,
        Kind::Null => 4,
        Kind::Impt => 4,
        _ => 1,
    }
}

// dedupe, drop noise, cap length. stable order.
fn clean_expects(err: &TokErr, lang: Lang) -> String {
    let mut seen: Vec<String> = Vec::new();
    for e in err.expected() {
        let s = match e.as_ref() {
            Some(k) => k.describe(lang),
            None => tr(lang, "文件结尾", "end of input").to_string(),
        };
        if !seen.contains(&s) {
            seen.push(s);
        }
    }
    if let Some(l) = err.label() {
        let l = l.to_string();
        if !seen.contains(&l) {
            seen.push(l);
        }
    }
    // top label repeats the item label. drop the dup.
    let dup_top = tr(lang, "impt 或节点", "impt or node").to_string();
    if seen.contains(&dup_top) && seen.iter().any(|s| s == "impt") {
        seen.retain(|s| s != &dup_top);
    }
    if seen.is_empty() {
        return tr(lang, "合法符号", "valid item").to_string();
    }
    const MAX: usize = 3;
    if seen.len() > MAX {
        let rest = seen.len() - MAX;
        let head = seen[..MAX].join(", ");
        return match lang {
            Lang::Zh => format!("{head} 等(还有{rest}种)"),
            Lang::En => format!("{head} (+{rest} more)"),
        };
    }
    seen.join(", ")
}

// `x: ,` means empty value, not stray sep. peek next token.
fn missing_value(raw: &[(Kind, usize)], at: usize) -> bool {
    match raw.get(at + 1).map(|t| &t.0) {
        None | Some(Kind::Comma) | Some(Kind::RBracket) => true,
        _ => false,
    }
}

// tailored message per case. generic shape last.
fn message_for(
    found: &Option<Kind>,
    at_end: bool,
    empty_value: bool,
    expects: &str,
    found_desc: &str,
) -> (String, String) {
    if at_end {
        let zh = format!("文件提前结束了. 想要 {expects}");
        let en = format!("unexpected end of input. want {expects}");
        return (zh, en);
    }
    match found {
        Some(Kind::Comma) => (
            "逗号后面缺东西. 补个值或删掉逗号".to_string(),
            "comma with nothing after it. add a value or drop it".to_string(),
        ),
        Some(Kind::Colon) | Some(Kind::Equals) if empty_value => (
            "冒号后面缺值. 写成 key: value".to_string(),
            "missing value after separator. write key: value".to_string(),
        ),
        Some(Kind::Colon) | Some(Kind::Equals) => (
            "这里冒号(等号)多了. 属性写法是 key: value".to_string(),
            "stray separator. attributes look like key: value".to_string(),
        ),
        _ => {
            let zh = format!("要 {expects}, 却看到 {found_desc}. 语法错了");
            let en = format!("want {expects}, got {found_desc}. bad syntax");
            (zh, en)
        }
    }
}

fn hint_for(
    found: &Option<Kind>,
    at_end: bool,
    empty_value: bool,
    label: &Option<String>,
    lang: Lang,
) -> (Option<String>, Option<String>) {
    if at_end {
        return (
            Some("检查缺失的 `]` 或 `,`. 可试 --fix 补括号".to_string()),
            Some("check for a missing `]` or `,`. try --fix".to_string()),
        );
    }
    if matches!(found, Some(Kind::Colon) | Some(Kind::Equals)) {
        return if empty_value {
            (
                Some("删掉冒号或补上值. 如 x: 1".to_string()),
                Some("drop the separator or add a value. e.g. x: 1".to_string()),
            )
        } else {
            (
                Some("属性写法是 key: value. 检查等号两边".to_string()),
                Some("attributes look like key: value".to_string()),
            )
        };
    }
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
