use crate::ast::{BinOp, Doc, Expr, Func, Node, Pattern, Stmt, UnOp, Value};

pub fn format_doc(doc: &Doc) -> String {
    let mut parts: Vec<String> = Vec::new();
    if !doc.imports.is_empty() {
        let entries: Vec<String> = doc
            .imports
            .iter()
            .map(|s| format!("\"{}\"", escape_str(s)))
            .collect();
        parts.push(format!("impt {}", entries.join(", ")));
    }
    for f in &doc.funcs {
        parts.push(fmt_func(f, 0));
    }
    for n in &doc.nodes {
        parts.push(fmt_node(n, 0));
    }
    let mut out = parts.join("\n");
    // posix tail newline empty doc stays empty
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

fn fmt_func(f: &Func, indent: usize) -> String {
    let pad = "  ".repeat(indent);
    let inner = "  ".repeat(indent + 1);
    let params: Vec<String> = f
        .params
        .iter()
        .map(|p| match &p.default {
            Some(d) => format!("{}: {}", p.name, fmt_value(d)),
            None => p.name.clone(),
        })
        .collect();
    let head = format!("fun {}({})", f.name, params.join(", "));
    if f.desc.is_none() && f.body.is_empty() {
        return format!("{pad}{head} []");
    }
    let mut body = String::new();
    if let Some(d) = &f.desc {
        body.push_str(&format!(
            "{inner}desc: {},\n",
            fmt_value(&Value::Str(d.clone()))
        ));
    }
    for s in &f.body {
        body.push_str(&fmt_stmt(s, indent + 1));
        body.push('\n');
    }
    format!("{pad}{head} [\n{body}{pad}]")
}

fn fmt_stmt(s: &Stmt, indent: usize) -> String {
    let pad = "  ".repeat(indent);
    match s {
        Stmt::Let { bindings } => {
            let parts: Vec<String> = bindings
                .iter()
                .map(|(pat, v)| format!("{}: {}", fmt_pat(pat), fmt_expr_ind(v, 0, indent)))
                .collect();
            format!("{pad}let {}", parts.join(", "))
        }
        Stmt::Set { name, value } => format!("{pad}{name}: {}", fmt_expr_ind(value, 0, indent)),
        Stmt::Return(Some(e)) => format!("{pad}return {}", fmt_expr_ind(e, 0, indent)),
        Stmt::Return(None) => format!("{pad}return"),
        Stmt::Expr(e) => format!("{pad}{}", fmt_expr_ind(e, 0, indent)),
        Stmt::If { cond, then, els } => {
            let mut out = format!(
                "{pad}if {} {}",
                fmt_expr_ind(cond, 0, indent),
                fmt_block(then, indent)
            );
            if !els.is_empty() {
                out.push_str(&format!(" else {}", fmt_block(els, indent)));
            }
            out
        }
        Stmt::While { cond, body } => {
            format!(
                "{pad}while {} {}",
                fmt_expr_ind(cond, 0, indent),
                fmt_block(body, indent)
            )
        }
        Stmt::For { var, iter, body } => format!(
            "{pad}for {var} in {} {}",
            fmt_expr_ind(iter, 0, indent),
            fmt_block(body, indent)
        ),
    }
}

fn fmt_pat(p: &Pattern) -> String {
    match p {
        Pattern::Name(n) => n.clone(),
        Pattern::Array(elems, rest) => {
            let mut parts: Vec<String> = elems.iter().map(fmt_pat).collect();
            if let Some(r) = rest {
                parts.push(format!("*{r}"));
            }
            format!("[{}]", parts.join(", "))
        }
    }
}

fn fmt_block(stmts: &[Stmt], indent: usize) -> String {
    let pad = "  ".repeat(indent);
    if stmts.is_empty() {
        return "[]".to_string();
    }
    let mut out = String::from("[\n");
    for s in stmts {
        out.push_str(&fmt_stmt(s, indent + 1));
        out.push('\n');
    }
    out.push_str(&pad);
    out.push(']');
    out
}

fn fmt_node(n: &Node, indent: usize) -> String {
    let pad = "  ".repeat(indent);
    let inner = "  ".repeat(indent + 1);
    let mut head = String::new();
    if !n.enabled {
        head.push_str("/-");
    }
    if let Some(ty) = &n.ty {
        head.push_str(&format!("({ty})"));
    }
    head.push_str(&quote_name(&n.name));
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
        let dis = if a.enabled { "" } else { "/-" };
        body.push_str(&format!(
            "{inner}{dis}{}: {},\n",
            quote_name(&a.key),
            fmt_value(&a.value)
        ));
    }
    for c in &n.children {
        body.push_str(&fmt_node(c, indent + 1));
        body.push('\n');
    }
    body.push_str(&pad);
    body.push_str("]");
    format!("{pad}{head}{body}")
}

// bare if it lexes back as one name. else quote it
pub fn quote_name(s: &str) -> String {
    if is_bare_name(s) {
        s.to_string()
    } else {
        format!("\"{}\"", escape_str(s))
    }
}

fn is_bare_name(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_alphabetic() || c == '_' || (c as u32) > 0x7f => {}
        _ => return false,
    }
    for c in chars {
        if !(c.is_alphanumeric()
            || c == '_'
            || c == '.'
            || c == '-'
            || c == '+'
            || (c as u32) > 0x7f)
        {
            return false;
        }
    }
    !matches!(
        s,
        "impt"
            | "fun"
            | "let"
            | "if"
            | "else"
            | "while"
            | "for"
            | "in"
            | "return"
            | "true"
            | "false"
            | "null"
    )
}

// expr precedence keep parens so the tree survives a reparse
fn prec(e: &Expr) -> u8 {
    match e {
        Expr::Binary { op, .. } => match op {
            BinOp::Or => 1,
            BinOp::And => 2,
            BinOp::Eq | BinOp::Ne => 3,
            BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => 4,
            BinOp::Add | BinOp::Sub => 5,
            BinOp::Mul | BinOp::Div | BinOp::Mod => 6,
        },
        Expr::Unary { .. } => 7,
        _ => 8,
    }
}

pub fn fmt_expr(e: &Expr, parent: u8) -> String {
    fmt_expr_ind(e, parent, 0)
}

fn fmt_expr_ind(e: &Expr, parent: u8, indent: usize) -> String {
    let p = prec(e);
    let s = match e {
        Expr::Int(i) => i.to_string(),
        Expr::Float(f) => fmt_float(*f),
        Expr::Bool(b) => b.to_string(),
        Expr::Null => "null".to_string(),
        Expr::Str(s) => format!("\"{}\"", escape_str(s)),
        Expr::RawStr(s) => fmt_raw(s),
        Expr::Var(v) => v.clone(),
        Expr::Array(items) => {
            let parts: Vec<String> = items.iter().map(|x| fmt_expr_ind(x, 0, indent)).collect();
            format!("[{}]", parts.join(", "))
        }
        Expr::Call { callee, args } => {
            let parts: Vec<String> = args.iter().map(|x| fmt_expr_ind(x, 0, indent)).collect();
            match callee.as_ref() {
                Expr::Var(n) => format!("{n}({})", parts.join(", ")),
                other => format!("{}({})", fmt_expr_ind(other, 8, indent), parts.join(", ")),
            }
        }
        Expr::Lambda { params, body } => {
            let ps: Vec<String> = params
                .iter()
                .map(|p| match &p.default {
                    Some(d) => format!("{}: {}", p.name, fmt_value(d)),
                    None => p.name.clone(),
                })
                .collect();
            format!("fun({}) {}", ps.join(", "), fmt_block(body, indent))
        }
        Expr::Unary { op, expr } => {
            let sign = match op {
                UnOp::Neg => "-",
                UnOp::Not => "!",
            };
            format!("{sign}{}", fmt_expr_ind(expr, 7, indent))
        }
        Expr::Binary { op, lhs, rhs } => {
            let sym = match op {
                BinOp::Add => "+",
                BinOp::Sub => "-",
                BinOp::Mul => "*",
                BinOp::Div => "/",
                BinOp::Mod => "%",
                BinOp::Eq => "==",
                BinOp::Ne => "!=",
                BinOp::Lt => "<",
                BinOp::Le => "<=",
                BinOp::Gt => ">",
                BinOp::Ge => ">=",
                BinOp::And => "&&",
                BinOp::Or => "||",
            };
            format!(
                "{} {sym} {}",
                fmt_expr_ind(lhs, p, indent),
                fmt_expr_ind(rhs, p + 1, indent)
            )
        }
    };
    if p < parent { format!("({s})") } else { s }
}

pub fn fmt_value(v: &Value) -> String {
    match v {
        Value::Bare(s) => s.clone(),
        Value::Str(s) => format!("\"{}\"", escape_str(s)),
        Value::RawStr(s) => fmt_raw(s),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => fmt_float(*f),
        Value::Bool(b) => b.to_string(),
        Value::Null => "null".to_string(),
        Value::Array(items) => {
            let parts: Vec<String> = items.iter().map(fmt_value).collect();
            format!("[{}]", parts.join(", "))
        }
        // map has no literal render as pair arrays runtime only
        Value::Map(items) => {
            let parts: Vec<String> = items
                .iter()
                .map(|(k, v)| format!("[{}, {}]", fmt_value(&Value::Str(k.clone())), fmt_value(v)))
                .collect();
            format!("[{}]", parts.join(", "))
        }
        // func is runtime only. no literal form
        Value::Func(_) => "null".to_string(),
        Value::Typed { ty, value } => format!("({ty}){}", fmt_value(value)),
        Value::Disabled(v) => format!("/-{}", fmt_value(v)),
    }
}

// non finite floats need the # form or they lex as bare words
fn fmt_float(f: f64) -> String {
    if f.is_nan() {
        "#nan".to_string()
    } else if f.is_infinite() {
        if f.is_sign_negative() {
            "#-inf".to_string()
        } else {
            "#inf".to_string()
        }
    } else {
        format!("{f:?}")
    }
}

// raw strings never interpolate so always use the # form. grow the hash count
// until the content cannot close early
fn fmt_raw(s: &str) -> String {
    let mut n = 1;
    loop {
        let close = format!("\"{}", "#".repeat(n));
        if !s.contains(&close) {
            break;
        }
        n += 1;
    }
    let h = "#".repeat(n);
    format!("{h}\"{s}\"{h}")
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
