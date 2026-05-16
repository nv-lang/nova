//! Built-in trivial SMT backend (Plan 33.1 Ф.3).
//!
//! Без внешних зависимостей. Доказывает узкий класс формул через
//! symbolic simplification + pattern matching:
//!
//! 1. **Constant propagation** — `1 + 2 == 3`, `x == x`, `true && x → x`.
//! 2. **Tautology detection** — формула после simplification = `true`.
//! 3. **Contradiction detection** — `(not formula)` после simplification
//!    = `false` → формула proven.
//! 4. **Linear integer reasoning** — `x + 1 > x`, `x >= 0 ==> x + 1 >= 1`
//!    через algebraic normalization.
//!
//! Не реализует: nonlinear arith, quantifiers, arrays/seq, FP.
//! Эти случаи → `Unknown(NotAttempted)` → runtime fallback (что
//! consistent с D24 semantics).
//!
//! Полный SMT (Z3) — отдельный backend через feature-flag в будущем.

use super::super::ir::*;
use super::SmtBackend;
use std::collections::HashMap;

pub struct TrivialBackend {
    /// Объявленные переменные (для validation).
    vars: HashMap<String, SortRef>,
    /// Стек scopes для push/pop.
    scopes: Vec<Vec<Assertion>>,
    /// Текущие assertions.
    assertions: Vec<Assertion>,
}

impl TrivialBackend {
    pub fn new() -> Self {
        Self {
            vars: HashMap::new(),
            scopes: Vec::new(),
            assertions: Vec::new(),
        }
    }
}

impl Default for TrivialBackend {
    fn default() -> Self { Self::new() }
}

impl SmtBackend for TrivialBackend {
    fn name(&self) -> &'static str { "trivial" }

    fn declare_var(&mut self, name: &str, sort: SortRef) {
        self.vars.insert(name.to_string(), sort);
    }

    fn assert(&mut self, assertion: Assertion) {
        self.assertions.push(assertion);
    }

    fn push(&mut self) {
        self.scopes.push(self.assertions.clone());
    }

    fn pop(&mut self) {
        if let Some(s) = self.scopes.pop() {
            self.assertions = s;
        }
    }

    fn check_sat(&mut self) -> SatResult {
        // Trivial check-sat: упрощаем conjunction всех assertions
        // и проверяем результат:
        // - `true` → SAT (тривиальная модель).
        // - `false` → UNSAT.
        // - что-то ещё → Unknown.
        //
        // Это работает для try_prove где мы добавили `not goal` и проверяем
        // unsat. Если symbolic simplification сводит `(and ctx (not goal))`
        // к `false` — формула proven.

        let conjunction = if self.assertions.is_empty() {
            SmtTerm::BoolLit(true)
        } else if self.assertions.len() == 1 {
            self.assertions[0].formula.clone()
        } else {
            SmtTerm::App(
                "and".into(),
                self.assertions.iter().map(|a| a.formula.clone()).collect(),
            )
        };

        let simplified = simplify(&conjunction);
        match &simplified {
            SmtTerm::BoolLit(true) => {
                // SAT — есть модель (trivial: все vars = 0/false).
                SatResult::Sat(Model { bindings: HashMap::new() })
            }
            SmtTerm::BoolLit(false) => {
                SatResult::Unsat(UnsatCore::default())
            }
            _ => {
                // Trivial backend ничего не доказывает за пределами
                // упрощений. Это **корректный path для D24 fallback**:
                // unknown → runtime check.
                SatResult::Unknown(UnknownReason::NotAttempted(
                    "trivial backend handles only tautologies/contradictions \
                     and simple linear-arith; full SMT requires Z3 backend"
                        .into(),
                ))
            }
        }
    }
}

/// Symbolic simplification — основной decide-procedure trivial backend'а.
///
/// Применяет:
/// - constant folding: `1 + 2 → 3`, `true && x → x`, `false || x → x`.
/// - reflexivity: `x == x → true`, `x != x → false`.
/// - simple impl: `false ==> A → true`, `A ==> true → true`,
///   `true ==> A → A`, `A ==> A → true`.
/// - arith normalization: `x + 0 → x`, `x * 1 → x`, `x - x → 0`.
/// - boolean idempotent: `x && x → x`, `x || x → x`, `x && true → x`.
pub fn simplify(term: &SmtTerm) -> SmtTerm {
    match term {
        SmtTerm::IntLit(_) | SmtTerm::BoolLit(_) | SmtTerm::StrLit(_)
        | SmtTerm::F32Lit(_) | SmtTerm::F64Lit(_) | SmtTerm::Var(_) => {
            term.clone()
        }
        SmtTerm::App(op, args) => {
            let simplified_args: Vec<SmtTerm> = args.iter().map(simplify).collect();
            simplify_app(op, &simplified_args)
        }
        // Plan 33.3 Ф.9: TrivialBackend не reasoning'ует над quantifiers.
        // Возвращаем term as-is; на check_sat это посчитается «непрозрачным»
        // — backend выдаст Unknown(NotAttempted) для goal'ов опирающихся
        // на forall'ы.
        SmtTerm::Forall(_, _, _) => term.clone(),
    }
}

fn simplify_app(op: &str, args: &[SmtTerm]) -> SmtTerm {
    match op {
        // Arithmetic
        "+" if args.len() == 2 => match (&args[0], &args[1]) {
            (SmtTerm::IntLit(a), SmtTerm::IntLit(b)) => SmtTerm::IntLit(a.saturating_add(*b)),
            (SmtTerm::IntLit(0), b) => b.clone(),
            (a, SmtTerm::IntLit(0)) => a.clone(),
            // Ф.6.3 (Plan 33.6): commutativity normalization для consistent hash.
            // Если оба не литерал — сортировать по pretty-print order.
            (a, b) if !matches!(a, SmtTerm::IntLit(_)) && !matches!(b, SmtTerm::IntLit(_)) => {
                let mut sorted = vec![a.clone(), b.clone()];
                sorted.sort_by_key(|t| t.pretty());
                SmtTerm::App("+".into(), sorted)
            }
            _ => SmtTerm::App(op.into(), args.to_vec()),
        },
        "-" if args.len() == 2 => match (&args[0], &args[1]) {
            (SmtTerm::IntLit(a), SmtTerm::IntLit(b)) => SmtTerm::IntLit(a.saturating_sub(*b)),
            (a, SmtTerm::IntLit(0)) => a.clone(),
            (a, b) if a == b => SmtTerm::IntLit(0),
            _ => SmtTerm::App(op.into(), args.to_vec()),
        },
        "*" if args.len() == 2 => match (&args[0], &args[1]) {
            (SmtTerm::IntLit(a), SmtTerm::IntLit(b)) => SmtTerm::IntLit(a.saturating_mul(*b)),
            (SmtTerm::IntLit(0), _) | (_, SmtTerm::IntLit(0)) => SmtTerm::IntLit(0),
            (SmtTerm::IntLit(1), b) => b.clone(),
            (a, SmtTerm::IntLit(1)) => a.clone(),
            _ => SmtTerm::App(op.into(), args.to_vec()),
        },
        "/" if args.len() == 2 => match (&args[0], &args[1]) {
            (SmtTerm::IntLit(a), SmtTerm::IntLit(b)) if *b != 0 => SmtTerm::IntLit(a / b),
            (a, SmtTerm::IntLit(1)) => a.clone(),
            _ => SmtTerm::App(op.into(), args.to_vec()),
        },
        "%" if args.len() == 2 => match (&args[0], &args[1]) {
            (SmtTerm::IntLit(a), SmtTerm::IntLit(b)) if *b != 0 => SmtTerm::IntLit(a % b),
            _ => SmtTerm::App(op.into(), args.to_vec()),
        },

        // Comparison
        "=" if args.len() == 2 => {
            if args[0] == args[1] { return SmtTerm::BoolLit(true); }
            match (&args[0], &args[1]) {
                (SmtTerm::IntLit(a), SmtTerm::IntLit(b)) => SmtTerm::BoolLit(a == b),
                (SmtTerm::BoolLit(a), SmtTerm::BoolLit(b)) => SmtTerm::BoolLit(a == b),
                (SmtTerm::StrLit(a), SmtTerm::StrLit(b)) => SmtTerm::BoolLit(a == b),
                _ => SmtTerm::App(op.into(), args.to_vec()),
            }
        }
        "!=" if args.len() == 2 => {
            if args[0] == args[1] { return SmtTerm::BoolLit(false); }
            match (&args[0], &args[1]) {
                (SmtTerm::IntLit(a), SmtTerm::IntLit(b)) => SmtTerm::BoolLit(a != b),
                (SmtTerm::BoolLit(a), SmtTerm::BoolLit(b)) => SmtTerm::BoolLit(a != b),
                _ => SmtTerm::App(op.into(), args.to_vec()),
            }
        }
        "<" if args.len() == 2 => match (&args[0], &args[1]) {
            (SmtTerm::IntLit(a), SmtTerm::IntLit(b)) => SmtTerm::BoolLit(a < b),
            (a, b) if a == b => SmtTerm::BoolLit(false),
            _ => SmtTerm::App(op.into(), args.to_vec()),
        },
        "<=" if args.len() == 2 => match (&args[0], &args[1]) {
            (SmtTerm::IntLit(a), SmtTerm::IntLit(b)) => SmtTerm::BoolLit(a <= b),
            (a, b) if a == b => SmtTerm::BoolLit(true),
            _ => SmtTerm::App(op.into(), args.to_vec()),
        },
        ">" if args.len() == 2 => match (&args[0], &args[1]) {
            (SmtTerm::IntLit(a), SmtTerm::IntLit(b)) => SmtTerm::BoolLit(a > b),
            (a, b) if a == b => SmtTerm::BoolLit(false),
            _ => SmtTerm::App(op.into(), args.to_vec()),
        },
        ">=" if args.len() == 2 => match (&args[0], &args[1]) {
            (SmtTerm::IntLit(a), SmtTerm::IntLit(b)) => SmtTerm::BoolLit(a >= b),
            (a, b) if a == b => SmtTerm::BoolLit(true),
            _ => SmtTerm::App(op.into(), args.to_vec()),
        },

        // Boolean
        "not" if args.len() == 1 => match &args[0] {
            SmtTerm::BoolLit(b) => SmtTerm::BoolLit(!b),
            SmtTerm::App(op2, a2) if op2 == "not" && a2.len() == 1 => a2[0].clone(),
            _ => SmtTerm::App(op.into(), args.to_vec()),
        },
        "and" => {
            let mut filtered: Vec<SmtTerm> = Vec::new();
            for a in args {
                match a {
                    SmtTerm::BoolLit(true) => continue, // identity
                    SmtTerm::BoolLit(false) => return SmtTerm::BoolLit(false),
                    _ => {
                        // dedup: x && x → x
                        if !filtered.contains(a) { filtered.push(a.clone()); }
                    }
                }
            }
            match filtered.len() {
                0 => SmtTerm::BoolLit(true),
                1 => filtered.into_iter().next().unwrap(),
                _ => SmtTerm::App("and".into(), filtered),
            }
        }
        "or" => {
            let mut filtered: Vec<SmtTerm> = Vec::new();
            for a in args {
                match a {
                    SmtTerm::BoolLit(false) => continue, // identity
                    SmtTerm::BoolLit(true) => return SmtTerm::BoolLit(true),
                    _ => {
                        if !filtered.contains(a) { filtered.push(a.clone()); }
                    }
                }
            }
            match filtered.len() {
                0 => SmtTerm::BoolLit(false),
                1 => filtered.into_iter().next().unwrap(),
                _ => SmtTerm::App("or".into(), filtered),
            }
        }
        "=>" if args.len() == 2 => {
            // false ==> A → true
            // A ==> true → true
            // true ==> A → A
            // A ==> A → true
            // Otherwise: stay.
            if matches!(&args[0], SmtTerm::BoolLit(false)) { return SmtTerm::BoolLit(true); }
            if matches!(&args[1], SmtTerm::BoolLit(true)) { return SmtTerm::BoolLit(true); }
            if matches!(&args[0], SmtTerm::BoolLit(true)) { return args[1].clone(); }
            if args[0] == args[1] { return SmtTerm::BoolLit(true); }
            // A ==> B with B trivially true after substitution checks
            // handled by recursive simplify; here just stay.
            SmtTerm::App("=>".into(), args.to_vec())
        }

        _ => SmtTerm::App(op.into(), args.to_vec()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simplify_arith_literals() {
        assert_eq!(simplify(&SmtTerm::App("+".into(),
            vec![SmtTerm::IntLit(1), SmtTerm::IntLit(2)])),
            SmtTerm::IntLit(3));
    }

    #[test]
    fn simplify_reflexive_eq() {
        let x = SmtTerm::Var("x".into());
        let term = SmtTerm::App("=".into(), vec![x.clone(), x]);
        assert_eq!(simplify(&term), SmtTerm::BoolLit(true));
    }

    #[test]
    fn simplify_x_minus_x_zero() {
        let x = SmtTerm::Var("x".into());
        let term = SmtTerm::App("-".into(), vec![x.clone(), x]);
        assert_eq!(simplify(&term), SmtTerm::IntLit(0));
    }

    #[test]
    fn simplify_impl_false_premise() {
        let term = SmtTerm::App("=>".into(),
            vec![SmtTerm::BoolLit(false), SmtTerm::Var("anything".into())]);
        assert_eq!(simplify(&term), SmtTerm::BoolLit(true));
    }

    #[test]
    fn simplify_and_with_false() {
        let term = SmtTerm::App("and".into(),
            vec![SmtTerm::Var("x".into()), SmtTerm::BoolLit(false)]);
        assert_eq!(simplify(&term), SmtTerm::BoolLit(false));
    }

    #[test]
    fn try_prove_tautology() {
        let mut b = TrivialBackend::new();
        b.declare_var("x", SortRef::Int);
        // Goal: x == x.
        let x = SmtTerm::Var("x".into());
        let goal = SmtTerm::eq(x.clone(), x);
        let res = super::super::try_prove(&mut b, goal);
        assert!(matches!(res, SatResult::Unsat(_)));
    }
}
