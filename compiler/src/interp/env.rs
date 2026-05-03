//! Окружение интерпретатора — scope chain.

use super::value::Value;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

#[derive(Clone)]
pub struct Env {
    inner: Rc<RefCell<EnvInner>>,
}

struct EnvInner {
    bindings: HashMap<String, Value>,
    parent: Option<Env>,
}

impl Env {
    pub fn new() -> Self {
        Self {
            inner: Rc::new(RefCell::new(EnvInner {
                bindings: HashMap::new(),
                parent: None,
            })),
        }
    }

    pub fn new_child(parent: &Env) -> Self {
        Self {
            inner: Rc::new(RefCell::new(EnvInner {
                bindings: HashMap::new(),
                parent: Some(parent.clone()),
            })),
        }
    }

    pub fn define(&self, name: impl Into<String>, value: Value) {
        self.inner.borrow_mut().bindings.insert(name.into(), value);
    }

    pub fn lookup(&self, name: &str) -> Option<Value> {
        let inner = self.inner.borrow();
        if let Some(v) = inner.bindings.get(name) {
            return Some(v.clone());
        }
        if let Some(parent) = &inner.parent {
            return parent.lookup(name);
        }
        None
    }

    /// Изменить существующее binding'е (поднимаясь по scope chain).
    /// Возвращает true если найдено и обновлено.
    pub fn assign(&self, name: &str, value: Value) -> bool {
        let mut inner = self.inner.borrow_mut();
        if inner.bindings.contains_key(name) {
            inner.bindings.insert(name.to_string(), value);
            return true;
        }
        let parent = inner.parent.clone();
        drop(inner);
        if let Some(p) = parent {
            return p.assign(name, value);
        }
        false
    }
}

impl Default for Env {
    fn default() -> Self {
        Self::new()
    }
}
