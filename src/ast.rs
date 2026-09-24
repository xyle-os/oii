use std::collections::HashMap;

use serde::{Deserialize, Serialize};

// doc is the root imports nodes funcs
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Doc {
    pub imports: Vec<String>,
    pub nodes: Vec<Node>,
    // 1.0.0 adds funcs old docs load fine
    #[serde(default)]
    pub funcs: Vec<Func>,
}

impl Doc {
    pub fn node(&self, name: &str) -> Option<&Node> {
        self.nodes.iter().find(|n| n.name == name)
    }

    pub fn func(&self, name: &str) -> Option<&Func> {
        self.funcs.iter().find(|f| f.name == name)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Node {
    pub name: String,
    pub args: Vec<Value>,
    pub attributes: Vec<Attribute>,
    pub children: Vec<Node>,
}

impl Node {
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.attributes
            .iter()
            .find(|a| a.key == key)
            .map(|a| &a.value)
    }

    pub fn get_node(&self, name: &str) -> Option<&Node> {
        self.children.iter().find(|c| c.name == name)
    }

    pub fn find_all(&self, name: &str) -> impl Iterator<Item = &Node> {
        self.children.iter().filter(move |c| c.name == name)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Attribute {
    pub key: String,
    pub value: Value,
}

// func is a named method params plus a body
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Func {
    pub name: String,
    pub params: Vec<Param>,
    // desc is optional docs parser lifts it out of the body
    pub desc: Option<String>,
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Param {
    pub name: String,
    pub default: Option<Value>,
}

// pat is a binding target. name or array destructure
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Pattern {
    Name(String),
    // elements plus an optional rest name for the tail
    Array(Vec<Pattern>, Option<String>),
}

// stmt is one line in a func body
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Stmt {
    // let binds one or more patterns left to right
    Let {
        bindings: Vec<(Pattern, Expr)>,
    },
    Set {
        name: String,
        value: Expr,
    },
    If {
        cond: Expr,
        then: Vec<Stmt>,
        els: Vec<Stmt>,
    },
    While {
        cond: Expr,
        body: Vec<Stmt>,
    },
    For {
        var: String,
        iter: Expr,
        body: Vec<Stmt>,
    },
    Return(Option<Expr>),
    Expr(Expr),
}

// closure is a lambda plus the scope it captured
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Closure {
    pub params: Vec<Param>,
    pub body: Vec<Stmt>,
    pub env: Vec<HashMap<String, Value>>,
}

// expr is a value with operators and calls
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Expr {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    RawStr(String),
    Null,
    Var(String),
    Array(Vec<Expr>),
    // call holds any callee. name calls become Var callees
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
    },
    Lambda {
        params: Vec<Param>,
        body: Vec<Stmt>,
    },
    Unary {
        op: UnOp,
        expr: Box<Expr>,
    },
    Binary {
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnOp {
    Neg,
    Not,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Value {
    Bare(String),
    Str(String),
    RawStr(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    Null,
    Array(Vec<Value>),
    // map is runtime only hand built via builtins
    Map(Vec<(String, Value)>),
    // func is a runtime closure. config never holds one
    Func(Box<Closure>),
}

impl Value {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Bare(s) | Value::Str(s) | Value::RawStr(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::Int(i) => Some(*i),
            _ => None,
        }
    }

    pub fn as_float(&self) -> Option<f64> {
        match self {
            Value::Float(f) => Some(*f),
            Value::Int(i) => Some(*i as f64),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[Value]> {
        match self {
            Value::Array(items) => Some(items),
            _ => None,
        }
    }

    pub fn as_map(&self) -> Option<&[(String, Value)]> {
        match self {
            Value::Map(items) => Some(items),
            _ => None,
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }

    // short name for type errors and the type builtin
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Bare(_) => "bare",
            Value::Str(_) => "str",
            Value::RawStr(_) => "raw",
            Value::Int(_) => "int",
            Value::Float(_) => "float",
            Value::Bool(_) => "bool",
            Value::Null => "null",
            Value::Array(_) => "array",
            Value::Map(_) => "map",
            Value::Func(_) => "func",
        }
    }

    pub fn map_get(&self, key: &str) -> Option<&Value> {
        self.as_map()?
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v)
    }
}
