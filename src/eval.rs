use std::collections::HashMap;
use std::fmt;

use crate::ast::{BinOp, Closure, Doc, Expr, Func, Pattern, Stmt, UnOp, Value};

// limits keep runaway code from eating the machine
#[derive(Debug, Clone)]
pub struct EvalOptions {
    pub max_steps: u64,
    pub max_depth: u32,
}

impl Default for EvalOptions {
    fn default() -> Self {
        EvalOptions {
            max_steps: 1_000_000,
            max_depth: 512,
        }
    }
}

#[derive(Debug, Clone)]
pub struct EvalOutput {
    pub value: Value,
    // print builtin lands here
    pub output: Vec<String>,
    pub steps: u64,
}

#[derive(Debug, Clone)]
pub struct EvalError {
    pub code: &'static str,
    pub message: String,
}

impl fmt::Display for EvalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}

impl std::error::Error for EvalError {}

fn err(code: &'static str, message: impl Into<String>) -> EvalError {
    EvalError {
        code,
        message: message.into(),
    }
}

// call a func by name with the default limits
pub fn eval(doc: &Doc, name: &str, args: &[Value]) -> Result<EvalOutput, EvalError> {
    eval_call(doc, name, args, &EvalOptions::default())
}

pub fn eval_call(
    doc: &Doc,
    name: &str,
    args: &[Value],
    opts: &EvalOptions,
) -> Result<EvalOutput, EvalError> {
    let func = doc
        .func(name)
        .ok_or_else(|| err("E100", format!("no func `{name}`")))?;
    let mut ev = Ev {
        doc,
        scopes: vec![HashMap::new()],
        out: Vec::new(),
        steps: 0,
        max_steps: opts.max_steps,
        depth: 0,
        max_depth: opts.max_depth,
    };
    let value = ev.call_user(func, args)?;
    Ok(EvalOutput {
        value,
        output: ev.out,
        steps: ev.steps,
    })
}

// value or early return return unwinds to the func call
enum Flow {
    Val(Value),
    Ret(Value),
}

struct Ev<'a> {
    doc: &'a Doc,
    scopes: Vec<HashMap<String, Value>>,
    out: Vec<String>,
    steps: u64,
    max_steps: u64,
    depth: u32,
    max_depth: u32,
}

impl<'a> Ev<'a> {
    fn tick(&mut self) -> Result<(), EvalError> {
        self.steps += 1;
        if self.steps > self.max_steps {
            return Err(err(
                "E102",
                format!("step limit hit. over {} steps", self.max_steps),
            ));
        }
        Ok(())
    }

    fn lookup(&self, name: &str) -> Option<Value> {
        for scope in self.scopes.iter().rev() {
            if let Some(v) = scope.get(name) {
                return Some(v.clone());
            }
        }
        None
    }

    fn assign(&mut self, name: &str, value: Value) -> Result<(), EvalError> {
        for scope in self.scopes.iter_mut().rev() {
            if scope.contains_key(name) {
                scope.insert(name.to_string(), value);
                return Ok(());
            }
        }
        Err(err("E104", format!("set on unbound `{name}`")))
    }

    fn call_user(&mut self, func: &Func, args: &[Value]) -> Result<Value, EvalError> {
        self.depth += 1;
        if self.depth > self.max_depth {
            return Err(err(
                "E101",
                format!("recursion too deep. over {}", self.max_depth),
            ));
        }
        if args.len() > func.params.len() {
            self.depth -= 1;
            return Err(err(
                "E103",
                format!(
                    "func `{}` wants {} args got {}",
                    func.name,
                    func.params.len(),
                    args.len()
                ),
            ));
        }
        let mut scope = HashMap::new();
        for (i, p) in func.params.iter().enumerate() {
            let v = match args.get(i) {
                Some(v) => v.clone(),
                None => match &p.default {
                    Some(d) => d.clone(),
                    None => {
                        self.depth -= 1;
                        return Err(err("E103", format!("missing arg `{}`", p.name)));
                    }
                },
            };
            scope.insert(p.name.clone(), v);
        }
        self.scopes.push(scope);
        let flow = self.exec_block(&func.body);
        self.scopes.pop();
        self.depth -= 1;
        match flow? {
            Flow::Ret(v) => Ok(v),
            Flow::Val(v) => Ok(v),
        }
    }

    fn exec_block(&mut self, stmts: &[Stmt]) -> Result<Flow, EvalError> {
        for s in stmts {
            match self.exec(s)? {
                Flow::Ret(v) => return Ok(Flow::Ret(v)),
                Flow::Val(_) => {}
            }
        }
        Ok(Flow::Val(Value::Null))
    }

    fn exec(&mut self, stmt: &Stmt) -> Result<Flow, EvalError> {
        self.tick()?;
        match stmt {
            Stmt::Let { bindings } => {
                for (pat, value) in bindings {
                    let v = self.eval(value)?;
                    self.bind_pattern(pat, v)?;
                }
                Ok(Flow::Val(Value::Null))
            }
            Stmt::Set { name, value } => {
                let v = self.eval(value)?;
                self.assign(name, v)?;
                Ok(Flow::Val(Value::Null))
            }
            Stmt::If { cond, then, els } => {
                if self.cond(cond)? {
                    self.exec_block(then)
                } else {
                    self.exec_block(els)
                }
            }
            Stmt::While { cond, body } => {
                while self.cond(cond)? {
                    self.tick()?;
                    if let Flow::Ret(v) = self.exec_block(body)? {
                        return Ok(Flow::Ret(v));
                    }
                }
                Ok(Flow::Val(Value::Null))
            }
            Stmt::For { var, iter, body } => {
                let items = self.eval(iter)?;
                let list: Vec<Value> = match items {
                    Value::Array(xs) => xs,
                    Value::Map(m) => m.into_iter().map(|(k, _)| Value::Str(k)).collect(),
                    other => {
                        return Err(err(
                            "E106",
                            format!("for wants array or map got {}", other.type_name()),
                        ));
                    }
                };
                for item in list {
                    self.tick()?;
                    let mut scope = HashMap::new();
                    scope.insert(var.clone(), item);
                    self.scopes.push(scope);
                    let flow = self.exec_block(body);
                    self.scopes.pop();
                    if let Flow::Ret(v) = flow? {
                        return Ok(Flow::Ret(v));
                    }
                }
                Ok(Flow::Val(Value::Null))
            }
            Stmt::Return(e) => {
                let v = match e {
                    Some(e) => self.eval(e)?,
                    None => Value::Null,
                };
                Ok(Flow::Ret(v))
            }
            Stmt::Expr(e) => Ok(Flow::Val(self.eval(e)?)),
        }
    }

    fn cond(&mut self, e: &Expr) -> Result<bool, EvalError> {
        let v = self.eval(e)?;
        match v {
            Value::Bool(b) => Ok(b),
            other => Err(err(
                "E106",
                format!("condition wants bool got {}", other.type_name()),
            )),
        }
    }

    fn eval(&mut self, e: &Expr) -> Result<Value, EvalError> {
        self.tick()?;
        match e {
            Expr::Int(i) => Ok(Value::Int(*i)),
            Expr::Float(f) => Ok(Value::Float(*f)),
            Expr::Bool(b) => Ok(Value::Bool(*b)),
            Expr::Null => Ok(Value::Null),
            Expr::Str(s) => Ok(Value::Str(s.clone())),
            Expr::RawStr(s) => Ok(Value::RawStr(s.clone())),
            // a name is a binding first then a func as a value
            Expr::Var(name) => {
                if let Some(v) = self.lookup(name) {
                    return Ok(v);
                }
                if let Some(f) = self.doc.func(name) {
                    return Ok(Value::Func(Box::new(Closure {
                        params: f.params.clone(),
                        body: f.body.clone(),
                        env: Vec::new(),
                    })));
                }
                Err(err("E104", format!("unbound `{name}`")))
            }
            Expr::Array(items) => {
                let mut out = Vec::with_capacity(items.len());
                for it in items {
                    out.push(self.eval(it)?);
                }
                Ok(Value::Array(out))
            }
            Expr::Unary { op, expr } => {
                let v = self.eval(expr)?;
                match op {
                    UnOp::Neg => match v {
                        Value::Int(i) => i
                            .checked_neg()
                            .map(Value::Int)
                            .ok_or_else(|| err("E105", "int overflow")),
                        Value::Float(f) => Ok(Value::Float(-f)),
                        other => Err(err(
                            "E106",
                            format!("neg wants number got {}", other.type_name()),
                        )),
                    },
                    UnOp::Not => match v {
                        Value::Bool(b) => Ok(Value::Bool(!b)),
                        other => Err(err(
                            "E106",
                            format!("! wants bool got {}", other.type_name()),
                        )),
                    },
                }
            }
            Expr::Binary { op, lhs, rhs } => self.binary(*op, lhs, rhs),
            Expr::Call { callee, args } => self.eval_call(callee, args),
            Expr::Lambda { params, body } => Ok(Value::Func(Box::new(Closure {
                params: params.clone(),
                body: body.clone(),
                env: self.scopes.clone(),
            }))),
        }
    }

    fn eval_call(&mut self, callee: &Expr, args: &[Expr]) -> Result<Value, EvalError> {
        // named calls. bindings shadow funcs which shadow builtins
        if let Expr::Var(name) = callee {
            if let Some(v) = self.lookup(name) {
                let vals = self.eval_args(args)?;
                return self.call_value(v, &vals);
            }
            if let Some(f) = self.doc.func(name).cloned() {
                let vals = self.eval_args(args)?;
                return self.call_user(&f, &vals);
            }
            let vals = self.eval_args(args)?;
            return self.builtin(name, vals);
        }
        let c = self.eval(callee)?;
        let vals = self.eval_args(args)?;
        self.call_value(c, &vals)
    }

    fn eval_args(&mut self, args: &[Expr]) -> Result<Vec<Value>, EvalError> {
        let mut vals = Vec::with_capacity(args.len());
        for a in args {
            vals.push(self.eval(a)?);
        }
        Ok(vals)
    }

    fn call_value(&mut self, v: Value, args: &[Value]) -> Result<Value, EvalError> {
        match v {
            Value::Func(c) => self.call_closure(&c, args),
            other => Err(err("E109", format!("not callable: {}", other.type_name()))),
        }
    }

    fn call_closure(&mut self, c: &Closure, args: &[Value]) -> Result<Value, EvalError> {
        self.depth += 1;
        if self.depth > self.max_depth {
            self.depth -= 1;
            return Err(err(
                "E101",
                format!("recursion too deep. over {}", self.max_depth),
            ));
        }
        if args.len() > c.params.len() {
            self.depth -= 1;
            return Err(err(
                "E103",
                format!("func wants {} args got {}", c.params.len(), args.len()),
            ));
        }
        let mut scope = HashMap::new();
        for (i, p) in c.params.iter().enumerate() {
            let v = match args.get(i) {
                Some(v) => v.clone(),
                None => match &p.default {
                    Some(d) => d.clone(),
                    None => {
                        self.depth -= 1;
                        return Err(err("E103", format!("missing arg `{}`", p.name)));
                    }
                },
            };
            scope.insert(p.name.clone(), v);
        }
        // captured scope goes under the param scope
        let saved = std::mem::replace(&mut self.scopes, c.env.clone());
        self.scopes.push(scope);
        let flow = self.exec_block(&c.body);
        self.scopes = saved;
        self.depth -= 1;
        match flow? {
            Flow::Ret(v) => Ok(v),
            Flow::Val(v) => Ok(v),
        }
    }

    fn bind_pattern(&mut self, pat: &Pattern, v: Value) -> Result<(), EvalError> {
        match pat {
            Pattern::Name(n) => {
                self.scopes.last_mut().unwrap().insert(n.clone(), v);
                Ok(())
            }
            Pattern::Array(elems, rest) => {
                let items = match v {
                    Value::Array(xs) => xs,
                    other => {
                        return Err(err(
                            "E106",
                            format!("destructure wants array got {}", other.type_name()),
                        ));
                    }
                };
                if rest.is_none() && items.len() != elems.len() {
                    return Err(err(
                        "E106",
                        format!(
                            "destructure wants {} elems got {}",
                            elems.len(),
                            items.len()
                        ),
                    ));
                }
                for (i, p) in elems.iter().enumerate() {
                    let item = items.get(i).cloned().unwrap_or(Value::Null);
                    self.bind_pattern(p, item)?;
                }
                if let Some(rn) = rest {
                    let tail: Vec<Value> = items.into_iter().skip(elems.len()).collect();
                    self.scopes
                        .last_mut()
                        .unwrap()
                        .insert(rn.clone(), Value::Array(tail));
                }
                Ok(())
            }
        }
    }

    fn binary(&mut self, op: BinOp, lhs: &Expr, rhs: &Expr) -> Result<Value, EvalError> {
        // short circuit before touching rhs
        if op == BinOp::And {
            return Ok(Value::Bool(self.cond(lhs)? && self.cond(rhs)?));
        }
        if op == BinOp::Or {
            return Ok(Value::Bool(self.cond(lhs)? || self.cond(rhs)?));
        }
        let l = self.eval(lhs)?;
        let r = self.eval(rhs)?;
        match op {
            BinOp::Add => add(l, r),
            BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod => arith(op, l, r),
            BinOp::Eq => Ok(Value::Bool(l == r)),
            BinOp::Ne => Ok(Value::Bool(l != r)),
            BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => cmp(op, l, r),
            BinOp::And | BinOp::Or => unreachable!(),
        }
    }

    fn builtin(&mut self, name: &str, args: Vec<Value>) -> Result<Value, EvalError> {
        let arg = |i: usize| -> Result<&Value, EvalError> {
            args.get(i)
                .ok_or_else(|| err("E103", format!("`{name}` missing arg {}", i + 1)))
        };
        match name {
            "len" => {
                let v = arg(0)?;
                let n = match v {
                    Value::Array(xs) => xs.len(),
                    Value::Map(m) => m.len(),
                    Value::Str(s) | Value::RawStr(s) | Value::Bare(s) => s.chars().count(),
                    other => {
                        return Err(err(
                            "E106",
                            format!("len wants collection got {}", other.type_name()),
                        ));
                    }
                };
                Ok(Value::Int(n as i64))
            }
            "push" => {
                let mut xs = match arg(0)?.clone() {
                    Value::Array(xs) => xs,
                    other => {
                        return Err(err(
                            "E106",
                            format!("push wants array got {}", other.type_name()),
                        ));
                    }
                };
                xs.push(arg(1)?.clone());
                Ok(Value::Array(xs))
            }
            "get" => {
                let c = arg(0)?.clone();
                let k = arg(1)?.clone();
                match (&c, &k) {
                    (Value::Array(xs), Value::Int(i)) => {
                        let idx = *i;
                        let len = xs.len() as i64;
                        let idx = if idx < 0 { idx + len } else { idx };
                        xs.get(idx as usize)
                            .cloned()
                            .ok_or_else(|| err("E107", format!("index {i} out of range")))
                    }
                    (Value::Map(m), Value::Str(s)) | (Value::Map(m), Value::Bare(s)) => m
                        .iter()
                        .find(|(k, _)| k == s)
                        .map(|(_, v)| v.clone())
                        .ok_or_else(|| err("E107", format!("no key `{s}`"))),
                    (c, k) => Err(err(
                        "E106",
                        format!(
                            "get wants array+int or map+str got {}+{}",
                            c.type_name(),
                            k.type_name()
                        ),
                    )),
                }
            }
            "keys" => match arg(0)? {
                Value::Map(m) => Ok(Value::Array(
                    m.iter().map(|(k, _)| Value::Str(k.clone())).collect(),
                )),
                other => Err(err(
                    "E106",
                    format!("keys wants map got {}", other.type_name()),
                )),
            },
            "contains" => {
                let hay = arg(0)?.clone();
                let needle = arg(1)?;
                let found = match &hay {
                    Value::Array(xs) => xs.iter().any(|x| x == needle),
                    Value::Map(m) => match needle {
                        Value::Str(s) | Value::Bare(s) => m.iter().any(|(k, _)| k == s),
                        _ => false,
                    },
                    Value::Str(s) => match needle {
                        Value::Str(n) | Value::Bare(n) => s.contains(n.as_str()),
                        _ => false,
                    },
                    other => {
                        return Err(err(
                            "E106",
                            format!("contains wants collection got {}", other.type_name()),
                        ));
                    }
                };
                Ok(Value::Bool(found))
            }
            "type" => Ok(Value::Str(arg(0)?.type_name().to_string())),
            "str" => Ok(Value::Str(to_display(arg(0)?))),
            "int" => match arg(0)? {
                Value::Int(i) => Ok(Value::Int(*i)),
                Value::Float(f) => Ok(Value::Int(*f as i64)),
                Value::Bool(b) => Ok(Value::Int(*b as i64)),
                Value::Str(s) | Value::RawStr(s) | Value::Bare(s) => s
                    .trim()
                    .parse::<i64>()
                    .map(Value::Int)
                    .map_err(|_| err("E108", format!("int from bad `{s}`"))),
                other => Err(err("E108", format!("int from {}", other.type_name()))),
            },
            "float" => match arg(0)? {
                Value::Float(f) => Ok(Value::Float(*f)),
                Value::Int(i) => Ok(Value::Float(*i as f64)),
                Value::Bool(b) => Ok(Value::Float(*b as u8 as f64)),
                Value::Str(s) | Value::RawStr(s) | Value::Bare(s) => s
                    .trim()
                    .parse::<f64>()
                    .map(Value::Float)
                    .map_err(|_| err("E108", format!("float from bad `{s}`"))),
                other => Err(err("E108", format!("float from {}", other.type_name()))),
            },
            "map" => {
                if args.len() % 2 != 0 {
                    return Err(err("E103", "map wants even pairs"));
                }
                let mut m: Vec<(String, Value)> = Vec::new();
                let mut i = 0;
                while i < args.len() {
                    let k = match &args[i] {
                        Value::Str(s) | Value::RawStr(s) | Value::Bare(s) => s.clone(),
                        other => {
                            return Err(err(
                                "E106",
                                format!("map key wants str got {}", other.type_name()),
                            ));
                        }
                    };
                    m.retain(|(ek, _)| ek != &k);
                    m.push((k, args[i + 1].clone()));
                    i += 2;
                }
                Ok(Value::Map(m))
            }
            "put" => {
                let m = match arg(0)?.clone() {
                    Value::Map(m) => m,
                    other => {
                        return Err(err(
                            "E106",
                            format!("put wants map got {}", other.type_name()),
                        ));
                    }
                };
                let k = match arg(1)? {
                    Value::Str(s) | Value::RawStr(s) | Value::Bare(s) => s.clone(),
                    other => {
                        return Err(err(
                            "E106",
                            format!("put key wants str got {}", other.type_name()),
                        ));
                    }
                };
                let v = arg(2)?.clone();
                let mut out: Vec<(String, Value)> = m;
                out.retain(|(ek, _)| ek != &k);
                out.push((k, v));
                Ok(Value::Map(out))
            }
            "abs" => match arg(0)? {
                Value::Int(i) => i
                    .checked_abs()
                    .map(Value::Int)
                    .ok_or_else(|| err("E105", "int overflow")),
                Value::Float(f) => Ok(Value::Float(f.abs())),
                other => Err(err(
                    "E106",
                    format!("abs wants number got {}", other.type_name()),
                )),
            },
            "min" | "max" => {
                let l = arg(0)?.clone();
                let r = arg(1)?.clone();
                let want_max = name == "max";
                let ord = if let Some((a, b)) = both_num(&l, &r) {
                    a.partial_cmp(&b)
                } else {
                    match (l.as_str(), r.as_str()) {
                        (Some(a), Some(b)) => Some(a.cmp(b)),
                        _ => None,
                    }
                };
                let ord = ord.ok_or_else(|| {
                    err(
                        "E106",
                        format!(
                            "{name} wants numbers or strings got {}+{}",
                            l.type_name(),
                            r.type_name()
                        ),
                    )
                })?;
                let take_right = if want_max { ord.is_gt() } else { ord.is_lt() };
                Ok(if take_right { r } else { l })
            }
            "range" => {
                let (a, b) = if args.len() >= 2 {
                    (as_int(arg(0)?)?, as_int(arg(1)?)?)
                } else {
                    (0, as_int(arg(0)?)?)
                };
                if b < a {
                    return Ok(Value::Array(Vec::new()));
                }
                let n = (b - a) as u64;
                if n > self.max_steps {
                    return Err(err("E102", "range too big"));
                }
                Ok(Value::Array((a..b).map(Value::Int).collect()))
            }
            "print" => {
                let s = to_display(arg(0)?);
                self.out.push(s);
                Ok(Value::Null)
            }
            _ => Err(err("E100", format!("no builtin `{name}`"))),
        }
    }
}

fn as_int(v: &Value) -> Result<i64, EvalError> {
    match v {
        Value::Int(i) => Ok(*i),
        other => Err(err("E106", format!("wants int got {}", other.type_name()))),
    }
}

fn both_int(l: &Value, r: &Value) -> Option<(i64, i64)> {
    match (l, r) {
        (Value::Int(a), Value::Int(b)) => Some((*a, *b)),
        _ => None,
    }
}

fn both_num(l: &Value, r: &Value) -> Option<(f64, f64)> {
    match (l.as_float(), r.as_float()) {
        (Some(a), Some(b)) => Some((a, b)),
        _ => None,
    }
}

fn add(l: Value, r: Value) -> Result<Value, EvalError> {
    match (&l, &r) {
        (Value::Str(_) | Value::RawStr(_) | Value::Bare(_), _)
        | (_, Value::Str(_) | Value::RawStr(_) | Value::Bare(_)) => {
            Ok(Value::Str(format!("{}{}", to_display(&l), to_display(&r))))
        }
        (Value::Array(a), Value::Array(b)) => {
            let mut out = a.clone();
            out.extend(b.clone());
            Ok(Value::Array(out))
        }
        _ => arith(BinOp::Add, l, r),
    }
}

fn arith(op: BinOp, l: Value, r: Value) -> Result<Value, EvalError> {
    if let Some((a, b)) = both_int(&l, &r) {
        let v = match op {
            BinOp::Add => a.checked_add(b),
            BinOp::Sub => a.checked_sub(b),
            BinOp::Mul => a.checked_mul(b),
            BinOp::Div => {
                if b == 0 {
                    return Err(err("E105", "divide by zero"));
                }
                a.checked_div(b)
            }
            BinOp::Mod => {
                if b == 0 {
                    return Err(err("E105", "mod by zero"));
                }
                a.checked_rem(b)
            }
            _ => unreachable!(),
        };
        return v.map(Value::Int).ok_or_else(|| err("E105", "int overflow"));
    }
    let (a, b) = both_num(&l, &r).ok_or_else(|| {
        err(
            "E106",
            format!(
                "arithmetic wants numbers got {}+{}",
                l.type_name(),
                r.type_name()
            ),
        )
    })?;
    let v = match op {
        BinOp::Add => a + b,
        BinOp::Sub => a - b,
        BinOp::Mul => a * b,
        BinOp::Div => {
            if b == 0.0 {
                return Err(err("E105", "divide by zero"));
            }
            a / b
        }
        BinOp::Mod => {
            if b == 0.0 {
                return Err(err("E105", "mod by zero"));
            }
            a % b
        }
        _ => unreachable!(),
    };
    Ok(Value::Float(v))
}

fn cmp(op: BinOp, l: Value, r: Value) -> Result<Value, EvalError> {
    let ord = if let Some((a, b)) = both_num(&l, &r) {
        a.partial_cmp(&b)
    } else {
        match (l.as_str(), r.as_str()) {
            (Some(a), Some(b)) => Some(a.cmp(b)),
            _ => None,
        }
    };
    let ord = ord.ok_or_else(|| {
        err(
            "E106",
            format!(
                "compare wants numbers or strings got {}+{}",
                l.type_name(),
                r.type_name()
            ),
        )
    })?;
    let b = match op {
        BinOp::Lt => ord.is_lt(),
        BinOp::Le => ord.is_le(),
        BinOp::Gt => ord.is_gt(),
        BinOp::Ge => ord.is_ge(),
        _ => unreachable!(),
    };
    Ok(Value::Bool(b))
}

// human string for print str strings stay raw
pub fn to_display(v: &Value) -> String {
    match v {
        Value::Bare(s) | Value::Str(s) | Value::RawStr(s) => s.clone(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => format!("{f:?}"),
        Value::Bool(b) => b.to_string(),
        Value::Null => "null".to_string(),
        Value::Array(_) | Value::Map(_) => crate::fmt::fmt_value(v),
        Value::Func(_) => "<func>".to_string(),
        Value::Typed { value, .. } => to_display(value),
        Value::Disabled(v) => to_display(v),
    }
}
