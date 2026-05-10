//! Значения интерпретатора.

use crate::ast::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;

/// Одно значение во время интерпретации.
///
/// Утечки циклов терпимы (это bootstrap). Всё через `Rc<RefCell<...>>`
/// для shared mutability.
#[derive(Clone)]
pub enum Value {
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    Unit,
    Array(Rc<RefCell<Vec<Value>>>),
    Tuple(Vec<Value>),
    /// Record: имя типа (опц.) + поля.
    Record {
        type_name: Option<String>,
        fields: Rc<RefCell<HashMap<String, Value>>>,
    },
    /// Sum-вариант: имя варианта + payload.
    Variant {
        type_name: Option<String>,
        name: String,
        payload: VariantPayload,
    },
    /// Closure (lambda или free fn).
    Closure(Rc<Closure>),
    /// Native — встроенная функция (println, panic, etc.).
    Native(Rc<NativeFn>),
    /// Handler — ссылка на handler-литерал, привязанный к среде.
    Handler(Rc<Handler>),
    /// Range — start..end (D58)
    Range {
        start: i64,
        end: i64,
        inclusive: bool,
    },
    /// Iterator — внутреннее значение для for-in (упрощённо).
    Iter(Rc<RefCell<IterState>>),
}

#[derive(Clone)]
pub enum VariantPayload {
    Unit,
    Tuple(Vec<Value>),
    Record(Rc<RefCell<HashMap<String, Value>>>),
}

#[derive(Clone)]
pub struct Closure {
    pub params: Vec<String>,
    pub body: ClosureBody,
    pub env: super::env::Env,
    /// Receiver — для @-методов.
    pub receiver: Option<Value>,
    /// Plan 14 Ф.6-bis (D69): true если последний params — variadic
    /// (`...items []T`). Caller передаёт N args, interp собирает
    /// args[regular_arity..] в `Value::Array(...)` перед binding'ом
    /// последнего param'а.
    pub variadic_last: bool,
}

// `ClosureBody` переехал в `ast::ClosureBody` (Plan 19, C1) — единый
// источник истины для тела closure-light. Закрытие в interp хранит
// тело как клон AST-узла; выбор между Expr/Block решается на стадии
// AST.
pub use crate::ast::ClosureBody;

pub struct NativeFn {
    pub name: String,
    pub func: Box<dyn Fn(&[Value]) -> Result<Value, NativeError> + 'static>,
}

#[derive(Debug)]
pub struct NativeError {
    pub message: String,
}

pub enum IterState {
    /// Range-итератор.
    Range { cur: i64, end: i64, inclusive: bool },
    /// Array-итератор.
    Array { items: Rc<RefCell<Vec<Value>>>, pos: usize },
}

pub struct Handler {
    pub effect: String,
    pub methods: HashMap<String, HandlerMethod>,
    pub env: super::env::Env,
}

impl Value {
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Int(_) => "int",
            Value::Float(_) => "float",
            Value::Str(_) => "str",
            Value::Bool(_) => "bool",
            Value::Unit => "()",
            Value::Array(_) => "[]",
            Value::Tuple(_) => "tuple",
            Value::Record { .. } => "record",
            Value::Variant { .. } => "variant",
            Value::Closure(_) => "closure",
            Value::Native(_) => "native",
            Value::Handler(_) => "handler",
            Value::Range { .. } => "range",
            Value::Iter(_) => "iter",
        }
    }

    pub fn truthy(&self) -> bool {
        matches!(self, Value::Bool(true))
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Float(a), Value::Float(b)) => a == b,
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Unit, Value::Unit) => true,
            (Value::Tuple(a), Value::Tuple(b)) => a == b,
            (Value::Array(a), Value::Array(b)) => *a.borrow() == *b.borrow(),
            (
                Value::Variant {
                    name: na,
                    payload: pa,
                    ..
                },
                Value::Variant {
                    name: nb,
                    payload: pb,
                    ..
                },
            ) => {
                if na != nb {
                    return false;
                }
                match (pa, pb) {
                    (VariantPayload::Unit, VariantPayload::Unit) => true,
                    (VariantPayload::Tuple(a), VariantPayload::Tuple(b)) => a == b,
                    _ => false,
                }
            }
            _ => false,
        }
    }
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Int(n) => write!(f, "{}", n),
            Value::Float(x) => write!(f, "{}", x),
            Value::Str(s) => write!(f, "{:?}", s),
            Value::Bool(b) => write!(f, "{}", b),
            Value::Unit => write!(f, "()"),
            Value::Array(arr) => {
                let v = arr.borrow();
                write!(f, "[")?;
                for (i, e) in v.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{:?}", e)?;
                }
                write!(f, "]")
            }
            Value::Tuple(items) => {
                write!(f, "(")?;
                for (i, e) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{:?}", e)?;
                }
                write!(f, ")")
            }
            Value::Record { type_name, fields } => {
                if let Some(n) = type_name {
                    write!(f, "{} ", n)?;
                }
                write!(f, "{{")?;
                let map = fields.borrow();
                for (i, (k, v)) in map.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}: {:?}", k, v)?;
                }
                write!(f, "}}")
            }
            Value::Variant { name, payload, .. } => {
                write!(f, "{}", name)?;
                match payload {
                    VariantPayload::Unit => Ok(()),
                    VariantPayload::Tuple(items) => {
                        write!(f, "(")?;
                        for (i, e) in items.iter().enumerate() {
                            if i > 0 {
                                write!(f, ", ")?;
                            }
                            write!(f, "{:?}", e)?;
                        }
                        write!(f, ")")
                    }
                    VariantPayload::Record(fields) => {
                        let map = fields.borrow();
                        write!(f, "{{")?;
                        for (i, (k, v)) in map.iter().enumerate() {
                            if i > 0 {
                                write!(f, ", ")?;
                            }
                            write!(f, "{}: {:?}", k, v)?;
                        }
                        write!(f, "}}")
                    }
                }
            }
            Value::Closure(_) => write!(f, "<closure>"),
            Value::Native(n) => write!(f, "<native {}>", n.name),
            Value::Handler(h) => write!(f, "<handler {}>", h.effect),
            Value::Range {
                start,
                end,
                inclusive,
            } => {
                if *inclusive {
                    write!(f, "{}..={}", start, end)
                } else {
                    write!(f, "{}..{}", start, end)
                }
            }
            Value::Iter(_) => write!(f, "<iter>"),
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Str(s) => write!(f, "{}", s),
            other => write!(f, "{:?}", other),
        }
    }
}
