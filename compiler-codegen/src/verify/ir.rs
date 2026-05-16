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
    /// IEEE 754 single-precision (f32 → FP 8 24).
    F32,
    /// IEEE 754 double-precision (f64 → FP 11 53).
    F64,
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
    /// IEEE 754 float literals (f64 хранит как bits для точности).
    F32Lit(u32),  // f32::to_bits()
    F64Lit(u64),  // f64::to_bits()
    /// Symbolic variable (параметр функции, `result`, `old(...)`).
    Var(String),
    /// Application: `op(args...)`.
    App(String, Vec<SmtTerm>),
    /// Plan 33.3 Ф.9: `forall (binders) <body>` — universal
    /// quantifier. Используется для encoding'а axioms эффектов:
    /// `axiom non_negative(id) => balance(id) >= 0` →
    /// `Forall([("id", Int)], patterns, _view_Db_balance(id) >= 0)`.
    /// TrivialBackend не reasoning'ует над quantifiers (Unknown);
    /// Z3 backend через Z3_mk_forall_const + Z3_mk_pattern.
    ///
    /// `patterns`: N multi-patterns (Ф.1.2, Plan 33.5). Каждый pattern —
    /// список term'ов (multi-trigger). Z3 instantiate'ит квантор при
    /// матче всех term'ов одного pattern'а. Пустой вектор = no hint,
    /// Z3 использует heuristic instantiation.
    Forall(Vec<(String, SortRef)>, Vec<Vec<SmtTerm>>, Box<SmtTerm>),
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
            SmtTerm::Forall(binders, patterns, body) => {
                if binders.iter().any(|(b, _)| b == name) {
                    self.clone()
                } else {
                    SmtTerm::Forall(
                        binders.clone(),
                        patterns.iter().map(|p| p.iter().map(|t| t.substitute(name, replacement)).collect()).collect(),
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
            SmtTerm::F32Lit(bits) => format!("{}f32", f32::from_bits(*bits)),
            SmtTerm::F64Lit(bits) => format!("{}f64", f64::from_bits(*bits)),
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
            SmtTerm::Forall(binders, patterns, body) => {
                let bs: Vec<String> = binders.iter()
                    .map(|(n, s)| format!("{}: {:?}", n, s))
                    .collect();
                if patterns.is_empty() {
                    format!("(forall ({}) {})", bs.join(", "), body.pretty())
                } else {
                    let pats: Vec<String> = patterns.iter()
                        .map(|p| format!("[{}]", p.iter().map(|t| t.pretty()).collect::<Vec<_>>().join(", ")))
                        .collect();
                    format!("(forall ({}) {:?} {})", bs.join(", "), pats.join("; "), body.pretty())
                }
            }
        }
    }
}
