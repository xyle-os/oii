use std::collections::HashMap;
use std::fmt;

use crate::ast::{Node, Value};

// decode runs straight off the ast no json round trip
#[derive(Debug, Clone)]
pub struct DecodeError {
    pub path: String,
    pub message: String,
}

impl DecodeError {
    fn at(path: &str, message: impl Into<String>) -> DecodeError {
        DecodeError {
            path: path.to_string(),
            message: message.into(),
        }
    }
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.path.is_empty() {
            write!(f, "{}", self.message)
        } else {
            write!(f, "{}: {}", self.path, self.message)
        }
    }
}

impl std::error::Error for DecodeError {}

// implement for your own types to pull values out of a node
pub trait FromValue: Sized {
    fn from_value(v: &Value) -> Result<Self, DecodeError>;
}

// implement for your structs read fields with node helpers
pub trait FromNode: Sized {
    fn from_node(n: &Node) -> Result<Self, DecodeError>;
}

impl FromValue for Value {
    fn from_value(v: &Value) -> Result<Self, DecodeError> {
        Ok(v.clone())
    }
}

impl FromValue for String {
    fn from_value(v: &Value) -> Result<Self, DecodeError> {
        match v.inner() {
            Value::Bare(s) | Value::Str(s) | Value::RawStr(s) => Ok(s.clone()),
            other => Err(DecodeError::at(
                "",
                format!("want string got {}", other.type_name()),
            )),
        }
    }
}

impl FromValue for i64 {
    fn from_value(v: &Value) -> Result<Self, DecodeError> {
        match v.inner() {
            Value::Int(i) => Ok(*i),
            other => Err(DecodeError::at(
                "",
                format!("want int got {}", other.type_name()),
            )),
        }
    }
}

impl FromValue for f64 {
    fn from_value(v: &Value) -> Result<Self, DecodeError> {
        match v.inner() {
            Value::Float(f) => Ok(*f),
            Value::Int(i) => Ok(*i as f64),
            other => Err(DecodeError::at(
                "",
                format!("want float got {}", other.type_name()),
            )),
        }
    }
}

impl FromValue for bool {
    fn from_value(v: &Value) -> Result<Self, DecodeError> {
        match v.inner() {
            Value::Bool(b) => Ok(*b),
            other => Err(DecodeError::at(
                "",
                format!("want bool got {}", other.type_name()),
            )),
        }
    }
}

impl<T: FromValue> FromValue for Option<T> {
    fn from_value(v: &Value) -> Result<Self, DecodeError> {
        if v.is_null() {
            Ok(None)
        } else {
            T::from_value(v).map(Some)
        }
    }
}

impl<T: FromValue> FromValue for Vec<T> {
    fn from_value(v: &Value) -> Result<Self, DecodeError> {
        match v.inner() {
            Value::Array(items) => {
                let mut out = Vec::with_capacity(items.len());
                for (i, it) in items.iter().enumerate() {
                    out.push(
                        T::from_value(it)
                            .map_err(|e| DecodeError::at(&format!("[{i}]"), e.message))?,
                    );
                }
                Ok(out)
            }
            other => Err(DecodeError::at(
                "",
                format!("want array got {}", other.type_name()),
            )),
        }
    }
}

impl<T: FromValue> FromValue for HashMap<String, T> {
    fn from_value(v: &Value) -> Result<Self, DecodeError> {
        match v.inner() {
            Value::Map(items) => {
                let mut out = HashMap::new();
                for (k, val) in items {
                    out.insert(
                        k.clone(),
                        T::from_value(val).map_err(|e| DecodeError::at(k, e.message))?,
                    );
                }
                Ok(out)
            }
            other => Err(DecodeError::at(
                "",
                format!("want map got {}", other.type_name()),
            )),
        }
    }
}

impl Node {
    // typed field read no json needed
    pub fn field<T: FromValue>(&self, key: &str) -> Result<T, DecodeError> {
        let v = self
            .get(key)
            .ok_or_else(|| DecodeError::at(key, "missing field"))?;
        T::from_value(v).map_err(|e| DecodeError::at(key, e.message))
    }

    pub fn field_or<T: FromValue>(&self, key: &str, default: T) -> Result<T, DecodeError> {
        match self.get(key) {
            Some(v) => T::from_value(v).map_err(|e| DecodeError::at(key, e.message)),
            None => Ok(default),
        }
    }

    pub fn field_opt<T: FromValue>(&self, key: &str) -> Result<Option<T>, DecodeError> {
        match self.get(key) {
            Some(Value::Null) | None => Ok(None),
            Some(v) => T::from_value(v)
                .map(Some)
                .map_err(|e| DecodeError::at(key, e.message)),
        }
    }

    pub fn decode<T: FromNode>(&self) -> Result<T, DecodeError> {
        T::from_node(self)
    }
}

impl FromNode for Node {
    fn from_node(n: &Node) -> Result<Self, DecodeError> {
        Ok(n.clone())
    }
}

impl FromNode for Value {
    fn from_node(n: &Node) -> Result<Self, DecodeError> {
        // a node with one attr `value` decodes to it else the node folds
        if let Some(v) = n.get("value") {
            return Ok(v.clone());
        }
        let mut map = Vec::new();
        for a in &n.attributes {
            if a.enabled {
                map.push((a.key.clone(), a.value.clone()));
            }
        }
        for c in &n.children {
            if c.enabled {
                map.push((c.name.clone(), Value::from_node(c)?));
            }
        }
        Ok(Value::Map(map))
    }
}
