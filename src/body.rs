use crate::ast::{BinOp, Expr, Param, Pattern, Stmt, UnOp, Value};
use crate::lex::Kind;

// body parse failure at is an index into the body token slice
pub struct BodyError {
    pub at: usize,
    pub zh: String,
    pub en: String,
}

impl BodyError {
    fn new(at: usize, zh: impl Into<String>, en: impl Into<String>) -> BodyError {
        BodyError {
            at,
            zh: zh.into(),
            en: en.into(),
        }
    }
}

// hand parser over the flat token slice only runs inside fun bodies
struct P<'a> {
    t: &'a [Kind],
    i: usize,
}

impl<'a> P<'a> {
    fn new(t: &'a [Kind]) -> P<'a> {
        P { t, i: 0 }
    }

    fn peek(&self) -> Option<&Kind> {
        self.t.get(self.i)
    }

    fn peek2(&self) -> Option<&Kind> {
        self.t.get(self.i + 1)
    }

    fn bump(&mut self) -> Option<&Kind> {
        let k = self.t.get(self.i);
        if k.is_some() {
            self.i += 1;
        }
        k
    }

    fn eat(&mut self, k: &Kind) -> bool {
        if self.peek() == Some(k) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    fn eat_sep(&mut self) -> bool {
        // attrs accept : and = alike statements do too
        if matches!(self.peek(), Some(Kind::Colon) | Some(Kind::Equals)) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    fn eat_any_comma(&mut self) {
        while self.eat(&Kind::Comma) {}
    }

    fn name(&mut self) -> Result<String, BodyError> {
        match self.peek() {
            Some(Kind::Name(s)) => {
                let s = s.clone();
                self.i += 1;
                Ok(s)
            }
            other => Err(BodyError::new(
                self.i,
                format!("要个名字 看到 {}", desc_or(other, "end")),
                format!("want a name got {}", desc_or(other, "end")),
            )),
        }
    }
}

// parse a func body returns desc plus statements
pub fn parse_func_body(t: &[Kind]) -> Result<(Option<String>, Vec<Stmt>), BodyError> {
    let mut p = P::new(t);
    let mut stmts = Vec::new();
    let mut desc: Option<String> = None;
    loop {
        p.eat_any_comma();
        match p.peek() {
            None => break,
            Some(Kind::RBracket) => break,
            _ => {}
        }
        if is_desc(&p) {
            p.bump(); // desc
            p.eat_sep();
            match p.bump() {
                Some(Kind::Str(s)) | Some(Kind::RawStr(s)) => desc = Some(s.clone()),
                _ => {
                    return Err(BodyError::new(
                        p.i.saturating_sub(1),
                        "desc 后面要字符串",
                        "desc wants a string",
                    ));
                }
            }
            continue;
        }
        stmts.push(parse_stmt(&mut p)?);
    }
    Ok((desc, stmts))
}

fn is_desc(p: &P) -> bool {
    matches!(p.peek(), Some(Kind::Name(s)) if s == "desc")
        && matches!(p.peek2(), Some(Kind::Colon) | Some(Kind::Equals))
}

fn parse_stmt(p: &mut P) -> Result<Stmt, BodyError> {
    match p.peek() {
        Some(Kind::Let) => {
            p.bump();
            let mut bindings = Vec::new();
            loop {
                let pat = parse_pattern(p)?;
                if !p.eat_sep() {
                    return Err(BodyError::new(
                        p.i,
                        "let 要 `:` 或 `=`",
                        "let wants `:` or `=`",
                    ));
                }
                let value = parse_expr(p)?;
                bindings.push((pat, value));
                // a comma starts another binding only if a pattern follows
                if matches!(p.peek(), Some(Kind::Comma)) && binding_start(p.peek2()) {
                    p.bump();
                    continue;
                }
                break;
            }
            Ok(Stmt::Let { bindings })
        }
        Some(Kind::If) => {
            p.bump();
            let cond = parse_expr(p)?;
            let then = parse_block(p)?;
            let mut els = Vec::new();
            if p.eat(&Kind::Else) {
                els = parse_block(p)?;
            }
            Ok(Stmt::If { cond, then, els })
        }
        Some(Kind::While) => {
            p.bump();
            let cond = parse_expr(p)?;
            let body = parse_block(p)?;
            Ok(Stmt::While { cond, body })
        }
        Some(Kind::For) => {
            p.bump();
            let var = p.name()?;
            if !p.eat(&Kind::In) {
                return Err(BodyError::new(p.i, "for 要 `in`", "for wants `in`"));
            }
            let iter = parse_expr(p)?;
            let body = parse_block(p)?;
            Ok(Stmt::For { var, iter, body })
        }
        Some(Kind::Return) => {
            p.bump();
            if starts_expr(p.peek()) {
                Ok(Stmt::Return(Some(parse_expr(p)?)))
            } else {
                Ok(Stmt::Return(None))
            }
        }
        Some(Kind::Name(s)) => {
            // name : expr assignment
            if matches!(p.peek2(), Some(Kind::Colon) | Some(Kind::Equals)) {
                let name = s.clone();
                p.bump();
                p.eat_sep();
                let value = parse_expr(p)?;
                Ok(Stmt::Set { name, value })
            } else {
                Ok(Stmt::Expr(parse_expr(p)?))
            }
        }
        _ => Ok(Stmt::Expr(parse_expr(p)?)),
    }
}

fn parse_block(p: &mut P) -> Result<Vec<Stmt>, BodyError> {
    if !p.eat(&Kind::LBracket) {
        return Err(BodyError::new(
            p.i,
            "if while for 后面要 `[ ... ]`",
            "if while for want a `[ ... ]` block",
        ));
    }
    let mut stmts = Vec::new();
    loop {
        p.eat_any_comma();
        match p.peek() {
            None => {
                return Err(BodyError::new(
                    p.i,
                    "块没闭合 缺 `]`",
                    "unclosed block. missing `]`",
                ));
            }
            Some(Kind::RBracket) => {
                p.bump();
                break;
            }
            _ => {}
        }
        stmts.push(parse_stmt(p)?);
    }
    Ok(stmts)
}

fn starts_expr(k: Option<&Kind>) -> bool {
    matches!(
        k,
        Some(Kind::Int(_))
            | Some(Kind::Float(_))
            | Some(Kind::Bool(_))
            | Some(Kind::Null)
            | Some(Kind::Str(_))
            | Some(Kind::RawStr(_))
            | Some(Kind::Name(_))
            | Some(Kind::LParen)
            | Some(Kind::LBracket)
            | Some(Kind::Minus)
            | Some(Kind::Bang)
            | Some(Kind::Fun)
    )
}

fn parse_expr(p: &mut P) -> Result<Expr, BodyError> {
    parse_or(p)
}

fn parse_or(p: &mut P) -> Result<Expr, BodyError> {
    let mut lhs = parse_and(p)?;
    while p.eat(&Kind::PipePipe) {
        let rhs = parse_and(p)?;
        lhs = Expr::Binary {
            op: BinOp::Or,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
        };
    }
    Ok(lhs)
}

fn parse_and(p: &mut P) -> Result<Expr, BodyError> {
    let mut lhs = parse_eq(p)?;
    while p.eat(&Kind::AmpAmp) {
        let rhs = parse_eq(p)?;
        lhs = Expr::Binary {
            op: BinOp::And,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
        };
    }
    Ok(lhs)
}

fn parse_eq(p: &mut P) -> Result<Expr, BodyError> {
    let mut lhs = parse_cmp(p)?;
    loop {
        let op = match p.peek() {
            Some(Kind::EqEq) => BinOp::Eq,
            Some(Kind::Ne) => BinOp::Ne,
            _ => break,
        };
        p.bump();
        let rhs = parse_cmp(p)?;
        lhs = Expr::Binary {
            op,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
        };
    }
    Ok(lhs)
}

fn parse_cmp(p: &mut P) -> Result<Expr, BodyError> {
    let mut lhs = parse_add(p)?;
    loop {
        let op = match p.peek() {
            Some(Kind::Lt) => BinOp::Lt,
            Some(Kind::Le) => BinOp::Le,
            Some(Kind::Gt) => BinOp::Gt,
            Some(Kind::Ge) => BinOp::Ge,
            _ => break,
        };
        p.bump();
        let rhs = parse_add(p)?;
        lhs = Expr::Binary {
            op,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
        };
    }
    Ok(lhs)
}

fn parse_add(p: &mut P) -> Result<Expr, BodyError> {
    let mut lhs = parse_mul(p)?;
    loop {
        let op = match p.peek() {
            Some(Kind::Plus) => BinOp::Add,
            Some(Kind::Minus) => BinOp::Sub,
            _ => break,
        };
        p.bump();
        let rhs = parse_mul(p)?;
        lhs = Expr::Binary {
            op,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
        };
    }
    Ok(lhs)
}

fn parse_mul(p: &mut P) -> Result<Expr, BodyError> {
    let mut lhs = parse_unary(p)?;
    loop {
        let op = match p.peek() {
            Some(Kind::Star) => BinOp::Mul,
            Some(Kind::Slash) => BinOp::Div,
            Some(Kind::Percent) => BinOp::Mod,
            _ => break,
        };
        p.bump();
        let rhs = parse_unary(p)?;
        lhs = Expr::Binary {
            op,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
        };
    }
    Ok(lhs)
}

fn parse_unary(p: &mut P) -> Result<Expr, BodyError> {
    match p.peek() {
        Some(Kind::Minus) => {
            p.bump();
            let e = parse_unary(p)?;
            Ok(Expr::Unary {
                op: UnOp::Neg,
                expr: Box::new(e),
            })
        }
        Some(Kind::Bang) => {
            p.bump();
            let e = parse_unary(p)?;
            Ok(Expr::Unary {
                op: UnOp::Not,
                expr: Box::new(e),
            })
        }
        _ => parse_primary(p),
    }
}

fn parse_primary(p: &mut P) -> Result<Expr, BodyError> {
    match p.peek().cloned() {
        Some(Kind::Int(i)) => {
            p.bump();
            Ok(Expr::Int(i))
        }
        Some(Kind::Float(f)) => {
            p.bump();
            Ok(Expr::Float(f))
        }
        Some(Kind::Bool(b)) => {
            p.bump();
            Ok(Expr::Bool(b))
        }
        Some(Kind::Null) => {
            p.bump();
            Ok(Expr::Null)
        }
        Some(Kind::Str(s)) => {
            p.bump();
            Ok(Expr::Str(s))
        }
        Some(Kind::RawStr(s)) => {
            p.bump();
            Ok(Expr::RawStr(s))
        }
        Some(Kind::Name(s)) => {
            p.bump();
            if p.peek() == Some(&Kind::LParen) {
                let args = parse_call_args(p)?;
                Ok(Expr::Call {
                    callee: Box::new(Expr::Var(s)),
                    args,
                })
            } else {
                Ok(Expr::Var(s))
            }
        }
        Some(Kind::Fun) => {
            p.bump();
            let params = parse_params(p)?;
            let body = parse_block(p)?;
            Ok(Expr::Lambda { params, body })
        }
        Some(Kind::LParen) => {
            p.bump();
            let e = parse_expr(p)?;
            if !p.eat(&Kind::RParen) {
                return Err(BodyError::new(p.i, "缺 `)`", "missing `)`"));
            }
            Ok(e)
        }
        Some(Kind::LBracket) => {
            p.bump();
            let mut items = Vec::new();
            p.eat_any_comma();
            if !p.eat(&Kind::RBracket) {
                loop {
                    items.push(parse_expr(p)?);
                    p.eat_any_comma();
                    if p.eat(&Kind::RBracket) {
                        break;
                    }
                }
            }
            Ok(Expr::Array(items))
        }
        other => Err(BodyError::new(
            p.i,
            format!("表达式坏了 看到 {}", desc_or(other.as_ref(), "end")),
            format!("bad expr got {}", desc_or(other.as_ref(), "end")),
        )),
    }
}

fn parse_call_args(p: &mut P) -> Result<Vec<Expr>, BodyError> {
    if !p.eat(&Kind::LParen) {
        return Err(BodyError::new(
            p.i,
            "调用要 `( ... )`",
            "call wants `( ... )`",
        ));
    }
    let mut args = Vec::new();
    p.eat_any_comma();
    if !p.eat(&Kind::RParen) {
        loop {
            args.push(parse_expr(p)?);
            p.eat_any_comma();
            if p.eat(&Kind::RParen) {
                break;
            }
        }
    }
    Ok(args)
}

fn parse_params(p: &mut P) -> Result<Vec<Param>, BodyError> {
    if !p.eat(&Kind::LParen) {
        return Err(BodyError::new(
            p.i,
            "参数要 `( ... )`",
            "params want `( ... )`",
        ));
    }
    let mut params = Vec::new();
    p.eat_any_comma();
    if !p.eat(&Kind::RParen) {
        loop {
            let name = p.name()?;
            let default = if p.eat_sep() {
                Some(parse_value_literal(p)?)
            } else {
                None
            };
            params.push(Param { name, default });
            p.eat_any_comma();
            if p.eat(&Kind::RParen) {
                break;
            }
        }
    }
    Ok(params)
}

// literal only. defaults never call or compute
fn parse_value_literal(p: &mut P) -> Result<Value, BodyError> {
    match p.peek().cloned() {
        Some(Kind::LBracket) => {
            p.bump();
            let mut items = Vec::new();
            p.eat_any_comma();
            if !p.eat(&Kind::RBracket) {
                loop {
                    items.push(parse_value_literal(p)?);
                    p.eat_any_comma();
                    if p.eat(&Kind::RBracket) {
                        break;
                    }
                }
            }
            Ok(Value::Array(items))
        }
        Some(Kind::Int(i)) => {
            p.bump();
            Ok(Value::Int(i))
        }
        Some(Kind::Float(f)) => {
            p.bump();
            Ok(Value::Float(f))
        }
        Some(Kind::Bool(b)) => {
            p.bump();
            Ok(Value::Bool(b))
        }
        Some(Kind::Null) => {
            p.bump();
            Ok(Value::Null)
        }
        Some(Kind::Str(s)) => {
            p.bump();
            Ok(Value::Str(s))
        }
        Some(Kind::RawStr(s)) => {
            p.bump();
            Ok(Value::RawStr(s))
        }
        Some(Kind::Name(s)) => {
            p.bump();
            Ok(Value::Bare(s))
        }
        Some(Kind::Minus) => {
            p.bump();
            match p.peek().cloned() {
                Some(Kind::Int(i)) => {
                    p.bump();
                    Ok(Value::Int(-i))
                }
                Some(Kind::Float(f)) => {
                    p.bump();
                    Ok(Value::Float(-f))
                }
                _ => Err(BodyError::new(p.i, "负号后面要数字", "want number after -")),
            }
        }
        other => Err(BodyError::new(
            p.i,
            format!("默认值坏了 看到 {}", desc_or(other.as_ref(), "end")),
            format!("bad default got {}", desc_or(other.as_ref(), "end")),
        )),
    }
}

fn parse_pattern(p: &mut P) -> Result<Pattern, BodyError> {
    if p.eat(&Kind::LBracket) {
        let mut elems = Vec::new();
        let mut rest = None;
        p.eat_any_comma();
        if !p.eat(&Kind::RBracket) {
            loop {
                if p.eat(&Kind::Star) {
                    rest = Some(p.name()?);
                } else {
                    elems.push(parse_pattern(p)?);
                }
                p.eat_any_comma();
                if p.eat(&Kind::RBracket) {
                    break;
                }
            }
        }
        Ok(Pattern::Array(elems, rest))
    } else {
        Ok(Pattern::Name(p.name()?))
    }
}

fn binding_start(k: Option<&Kind>) -> bool {
    matches!(k, Some(Kind::Name(_)) | Some(Kind::LBracket))
}

fn desc_or(k: Option<&Kind>, empty: &str) -> String {
    match k {
        Some(k) => k.describe(crate::diag::Lang::En),
        None => empty.to_string(),
    }
}
