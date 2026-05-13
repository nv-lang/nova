//! SMT-нейтральный IR для контракт-формул.
//!
//! Plan 33.1 (D24, D96): engine-agnostic representation. Trivial-backend
//! работает напрямую с этим IR; Z3-backend (когда добавлен) транслирует
//! в Z3 AST через FFI.
//!
//! Поддерживаемые сейчас типы (33.1):
//! - `Int` (bounded, INT_MIN..INT_MAX — глобальное решение из roadmap-индекса).
//! - `Bool`.
//! - `Str` (только equality, без операций — расширим в 33.3).
//! - Uninterpreted sort (record types).
//!
//! Не входит в 33.1: FloatingPoint (33.3), Seq (33.3), Arrays (33.2 partial),
//! algebraic datatypes для sum-types (33.3 для ContractResult).

use std::collections::HashMap;

/// SMT sort (тип в SMT-LIB v2 sense).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SortRef {
    Int,
    Bool,
    Str,
    /// Uninterpreted sort, named (например для record-типов).
    Named(String),
}

/// Term — выражение в SMT-IR. Все Nova-выражения контрактов
/// транслируются в `SmtTerm` через `encode.rs`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SmtTerm {
    /// Литералы.
    IntLit(i64),
    BoolLit(bool),
    StrLit(String),
    /// Symbolic variable (параметр функции, `result`, `old(...)`).
    Var(String),
    /// Application: `op(args...)`.
    App(String, Vec<SmtTerm>),
    /// Plan 33.3 Ф.9: `forall (binders) <body>` — universal
    /// quantifier. Используется для encoding'а axioms эффектов:
    /// `axiom non_negative(id) => balance(id) >= 0` →
    /// `Forall([("id", Int)], _view_Db_balance(id) >= 0)`.
    /// TrivialBackend не reasoning'ует над quantifiers (Unknown);
    /// Z3 backend через Z3_mk_forall_const.
    Forall(Vec<(String, SortRef)>, Box<SmtTerm>),
}

/// Formula = SmtTerm типа Bool. Alias для семантической ясности.
pub type Formula = SmtTerm;

/// Encoded `assert`. Backend хранит их как assumptions для check_sat.
#[derive(Debug, Clone)]
pub struct Assertion {
    pub formula: Formula,
    pub label: Option<String>,
}

/// Контр-пример: значения параметров при которых формула не выполняется.
#[derive(Debug, Clone)]
pub struct Model {
    pub bindings: HashMap<String, ModelValue>,
}

#[derive(Debug, Clone)]
pub enum ModelValue {
    Int(i64),
    Bool(bool),
    Str(String),
    /// Backend не смог восстановить конкретное значение.
    Unknown,
}

/// Unsat core — список assumption labels, противоречие которых
/// доказало unsat. Используется для diagnostic.
#[derive(Debug, Clone, Default)]
pub struct UnsatCore {
    pub labels: Vec<String>,
}

/// Результат check_sat.
#[derive(Debug, Clone)]
pub enum SatResult {
    /// Формула SAT (есть модель — counterexample).
    Sat(Model),
    /// Формула UNSAT (доказано).
    Unsat(UnsatCore),
    /// Backend не справился.
    Unknown(UnknownReason),
}

/// Категории «не справился» — используются для AI-friendly diag.
#[derive(Debug, Clone)]
pub enum UnknownReason {
    /// Timeout.
    Timeout,
    /// Нелинейная арифметика (x * y, x / y без линейного hint).
    NonLinearArithmetic,
    /// Backend не реализует требуемую теорию (FP, strings beyond eq).
    UnsupportedTheory(String),
    /// OOM / crash backend'а.
    BackendError(String),
    /// Trivial backend: не пытался доказать (формула слишком сложная).
    NotAttempted(String),
}

impl SmtTerm {
    /// Helper для построения impl: `(=> A B)`.
    pub fn implies(a: Formula, b: Formula) -> Formula {
        SmtTerm::App("=>".into(), vec![a, b])
    }

    /// Helper: `(= A B)`.
    pub fn eq(a: SmtTerm, b: SmtTerm) -> Formula {
        SmtTerm::App("=".into(), vec![a, b])
    }

    /// Helper: `(and A B ...)`.
    pub fn and(args: Vec<Formula>) -> Formula {
        if args.len() == 1 {
            return args.into_iter().next().unwrap();
        }
        SmtTerm::App("and".into(), args)
    }

    /// Helper: `(or A B ...)`.
    pub fn or(args: Vec<Formula>) -> Formula {
        if args.len() == 1 {
            return args.into_iter().next().unwrap();
        }
        SmtTerm::App("or".into(), args)
    }

    /// Helper: `(not A)`.
    pub fn not(a: Formula) -> Formula {
        SmtTerm::App("not".into(), vec![a])
    }

    /// Substitute `name` → `replacement` в term. Used для:
    /// - подстановки `result` → return-value во время verify.
    /// - inline `old(x)` → fresh `_old_x`.
    pub fn substitute(&self, name: &str, replacement: &SmtTerm) -> SmtTerm {
        match self {
            SmtTerm::Var(n) if n == name => replacement.clone(),
            SmtTerm::App(op, args) => SmtTerm::App(
                op.clone(),
                args.iter().map(|a| a.substitute(name, replacement)).collect(),
            ),
            // Plan 33.3 Ф.9: capture-avoiding — если `name` shadowed
            // одним из binders, не подменяем внутри. Иначе recurse.
            SmtTerm::Forall(binders, body) => {
                if binders.iter().any(|(b, _)| b == name) {
                    self.clone()
                } else {
                    SmtTerm::Forall(
                        binders.clone(),
                        Box::new(body.substitute(name, replacement)),
                    )
                }
            }
            _ => self.clone(),
        }
    }

    /// Pretty-print для диагностики.
    pub fn pretty(&self) -> String {
        match self {
            SmtTerm::IntLit(n) => n.to_string(),
            SmtTerm::BoolLit(b) => b.to_string(),
            SmtTerm::StrLit(s) => format!("\"{}\"", s),
            SmtTerm::Var(n) => n.clone(),
            SmtTerm::App(op, args) => {
                let args_str: Vec<String> = args.iter().map(|a| a.pretty()).collect();
                match op.as_str() {
                    "+" | "-" | "*" | "/" | "%" | "<" | "<=" | ">" | ">=" | "=" | "!=" => {
                        if args.len() == 2 {
                            format!("({} {} {})", args_str[0], op, args_str[1])
                        } else {
                            format!("({} {})", op, args_str.join(" "))
                        }
                    }
                    "and" => format!("({})", args_str.join(" && ")),
                    "or" => format!("({})", args_str.join(" || ")),
                    "not" => format!("!{}", args_str[0]),
                    "=>" => format!("({} ==> {})", args_str[0], args_str[1]),
                    _ => format!("{}({})", op, args_str.join(", ")),
                }
            }
            SmtTerm::Forall(binders, body) => {
                let bs: Vec<String> = binders.iter()
                    .map(|(n, s)| format!("{}: {:?}", n, s))
                    .collect();
                format!("(forall ({}) {})", bs.join(", "), body.pretty())
            }
        }
    }
}
