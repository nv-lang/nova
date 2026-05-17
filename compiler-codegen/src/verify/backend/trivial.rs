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
        // Ф.11.4 (Plan 33.6): transitivity propagation для equalities.
        // Извлекаем все `(= a b)` из conjunction, строим class-substitution,
        // применяем re-simplify. Это закрывает `requires a==b; requires b==c; ensures a==c`.
        let propagated = propagate_equalities(&simplified);
        // Plan 33.6 Ф.15.2 (followup, 2026-05-17): bounds propagation для
        // inequality weakening. Раньше эта step была nested внутри
        // propagate_equalities — но та early-return'ила при отсутствии
        // equalities, что блокировало bounds prop в типичных cases
        // (e.g. `requires x >= 0; ensures result >= -1` где no `=` clauses).
        // Теперь bounds prop — independent phase в check_sat pipeline.
        let final_term = apply_bounds_propagation(&propagated);
        match &final_term {
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
            // Ф.9.4 (Plan 33.6): commutativity для *.
            (a, b) if !matches!(a, SmtTerm::IntLit(_)) && !matches!(b, SmtTerm::IntLit(_)) => {
                let mut sorted = vec![a.clone(), b.clone()];
                sorted.sort_by_key(|t| t.pretty());
                SmtTerm::App("*".into(), sorted)
            }
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
                // Ф.9.4 (Plan 33.6): commutativity для = (для лучшего matching).
                (a, b) if !matches!(a, SmtTerm::IntLit(_) | SmtTerm::BoolLit(_) | SmtTerm::StrLit(_))
                       && !matches!(b, SmtTerm::IntLit(_) | SmtTerm::BoolLit(_) | SmtTerm::StrLit(_)) => {
                    let mut sorted = vec![a.clone(), b.clone()];
                    sorted.sort_by_key(|t| t.pretty());
                    SmtTerm::App("=".into(), sorted)
                }
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
            // Ф.22.3 (Plan 33.6): De Morgan для and/or (2-arg).
            // `not (and X Y)` → `or (not X) (not Y)`.
            SmtTerm::App(op2, a2) if op2 == "and" && a2.len() == 2 => {
                let nx = simplify_app("not", &[a2[0].clone()]);
                let ny = simplify_app("not", &[a2[1].clone()]);
                simplify_app("or", &[nx, ny])
            }
            SmtTerm::App(op2, a2) if op2 == "or" && a2.len() == 2 => {
                let nx = simplify_app("not", &[a2[0].clone()]);
                let ny = simplify_app("not", &[a2[1].clone()]);
                simplify_app("and", &[nx, ny])
            }
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
            // Ф.21.1 (Plan 33.6): contradiction `X ∧ ¬X = false`.
            for t in &filtered {
                let negated = SmtTerm::App("not".into(), vec![t.clone()]);
                if filtered.contains(&negated) { return SmtTerm::BoolLit(false); }
                if let SmtTerm::App(op2, a2) = t {
                    if op2 == "not" && a2.len() == 1 {
                        // ¬X already в filtered, ищем X.
                        if filtered.contains(&a2[0]) { return SmtTerm::BoolLit(false); }
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
            // Ф.21.1 (Plan 33.6): excluded middle `X ∨ ¬X = true`.
            for t in &filtered {
                let negated = SmtTerm::App("not".into(), vec![t.clone()]);
                if filtered.contains(&negated) { return SmtTerm::BoolLit(true); }
                if let SmtTerm::App(op2, a2) = t {
                    if op2 == "not" && a2.len() == 1 {
                        if filtered.contains(&a2[0]) { return SmtTerm::BoolLit(true); }
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

        // Ф.20.2 (Plan 33.6): ite identity — `(ite c X X)` → `X`.
        // Plus constant condition: `(ite true X Y)` → `X`, `(ite false X Y)` → `Y`.
        "ite" if args.len() == 3 => {
            if matches!(&args[0], SmtTerm::BoolLit(true)) { return args[1].clone(); }
            if matches!(&args[0], SmtTerm::BoolLit(false)) { return args[2].clone(); }
            if args[1] == args[2] { return args[1].clone(); }
            SmtTerm::App("ite".into(), args.to_vec())
        }

        _ => SmtTerm::App(op.into(), args.to_vec()),
    }
}

/// Ф.11.4 (Plan 33.6): transitivity propagation для equalities в conjunction.
///
/// Алгоритм:
/// 1. Извлечь все `(= a b)` из top-level `and(...)`.
/// 2. Построить union-find классы.
/// 3. Заменить во всех других conjuncts: каждый Var → representative класса.
/// 4. Re-simplify.
///
/// Закрывает паттерн `a == b; b == c; ... a == c`.
fn propagate_equalities(term: &SmtTerm) -> SmtTerm {
    let conjuncts = match term {
        SmtTerm::App(op, args) if op == "and" => args.clone(),
        _ => vec![term.clone()],
    };

    // Step 1: collect equalities as (Var name → representative).
    use std::collections::HashMap;
    let mut parent: HashMap<String, String> = HashMap::new();
    fn find(parent: &mut HashMap<String, String>, x: &str) -> String {
        let p = parent.get(x).cloned().unwrap_or_else(|| x.to_string());
        if p == x { return p; }
        let root = find(parent, &p);
        parent.insert(x.to_string(), root.clone());
        root
    }
    for c in &conjuncts {
        if let SmtTerm::App(op, args) = c {
            if op == "=" && args.len() == 2 {
                if let (SmtTerm::Var(a), SmtTerm::Var(b)) = (&args[0], &args[1]) {
                    let ra = find(&mut parent, a);
                    let rb = find(&mut parent, b);
                    if ra != rb {
                        // Choose alphabetically-smaller as representative
                        // (deterministic, no ordering issue).
                        let (root, child) = if ra < rb { (ra, rb) } else { (rb, ra) };
                        parent.insert(child, root);
                    }
                }
            }
        }
    }

    if parent.is_empty() {
        return term.clone();
    }

    // Step 2: substitute each Var → its representative in all conjuncts.
    fn subst(t: &SmtTerm, parent: &mut HashMap<String, String>) -> SmtTerm {
        match t {
            SmtTerm::Var(n) => {
                let root = find(parent, n);
                if root != *n { SmtTerm::Var(root) } else { t.clone() }
            }
            SmtTerm::App(op, args) => SmtTerm::App(
                op.clone(),
                args.iter().map(|a| subst(a, parent)).collect(),
            ),
            SmtTerm::Forall(binders, patterns, body) => SmtTerm::Forall(
                binders.clone(),
                patterns.clone(),
                Box::new(subst(body, parent)),
            ),
            _ => t.clone(),
        }
    }

    let substituted: Vec<SmtTerm> = conjuncts.iter()
        .map(|c| subst(c, &mut parent))
        .collect();

    // Step 3: re-simplify each conjunct + wrap.
    let resimplified: Vec<SmtTerm> = substituted.iter().map(simplify).collect();

    // Ф.15.2 (Plan 33.6): bounds propagation для inequality weakening.
    // Если в conjunction `(>= var L)` known, то weaker `(>= var L')` где L' ≤ L
    // → trivially true. Аналогично для `<=`.
    let bound_propagated = propagate_bounds(&resimplified);

    let result = if bound_propagated.len() == 1 {
        bound_propagated.into_iter().next().unwrap()
    } else {
        SmtTerm::App("and".into(), bound_propagated)
    };
    simplify(&result)
}

/// Plan 33.6 Ф.15.2 (followup, 2026-05-17): apply bounds propagation
/// как standalone phase. Wraps `propagate_bounds` для full-term entry
/// (вместо conjunct-list). Возвращает re-simplified term.
fn apply_bounds_propagation(term: &SmtTerm) -> SmtTerm {
    let conjuncts: Vec<SmtTerm> = match term {
        SmtTerm::App(op, args) if op == "and" => args.clone(),
        _ => vec![term.clone()],
    };
    let bound_propagated = propagate_bounds(&conjuncts);
    let result = if bound_propagated.len() == 1 {
        bound_propagated.into_iter().next().unwrap()
    } else {
        SmtTerm::App("and".into(), bound_propagated)
    };
    simplify(&result)
}

/// Ф.15.2 (Plan 33.6): bounds propagation. Collect known lower/upper bounds
/// (Var >= IntLit / Var <= IntLit) from conjunction, simplify weaker bounds
/// to BoolLit(true).
fn propagate_bounds(conjuncts: &[SmtTerm]) -> Vec<SmtTerm> {
    use std::collections::HashMap;
    // var → tightest known lower bound (max of `Var >= L_i`)
    let mut lower: HashMap<String, i64> = HashMap::new();
    // var → tightest known upper bound (min of `Var <= L_i`)
    let mut upper: HashMap<String, i64> = HashMap::new();
    for c in conjuncts {
        if let SmtTerm::App(op, args) = c {
            if args.len() != 2 { continue; }
            match (op.as_str(), &args[0], &args[1]) {
                (">=", SmtTerm::Var(v), SmtTerm::IntLit(n)) => {
                    let e = lower.entry(v.clone()).or_insert(*n);
                    if *n > *e { *e = *n; }
                }
                ("<=", SmtTerm::Var(v), SmtTerm::IntLit(n)) => {
                    let e = upper.entry(v.clone()).or_insert(*n);
                    if *n < *e { *e = *n; }
                }
                _ => {}
            }
        }
    }
    // Ф.19.1 (Plan 33.6): modulus check не требует lower/upper context (работает
    // по самой constant). Поэтому даже без bounds tracking — проходим через walker.
    conjuncts.iter().map(|c| {
        // Ф.17.2 (Plan 33.6): addition bounds propagation.
        // `(>= (+ Var1 Var2) goal)` где lower(Var1) + lower(Var2) >= goal → true.
        let try_addition_check = |inner: &SmtTerm| -> Option<bool> {
            if let SmtTerm::App(iop, iargs) = inner {
                if iop != ">=" || iargs.len() != 2 { return None; }
                let goal = match &iargs[1] { SmtTerm::IntLit(n) => *n, _ => return None };
                if let SmtTerm::App(aop, aargs) = &iargs[0] {
                    if aop != "+" || aargs.len() != 2 { return None; }
                    // Both args — Var.
                    if let (SmtTerm::Var(a), SmtTerm::Var(b)) = (&aargs[0], &aargs[1]) {
                        if let (Some(la), Some(lb)) = (lower.get(a), lower.get(b)) {
                            if la.saturating_add(*lb) >= goal { return Some(true); }
                        }
                    }
                    // (+ Var IntLit) или (+ IntLit Var).
                    let (var_opt, lit_opt) = match (&aargs[0], &aargs[1]) {
                        (SmtTerm::Var(v), SmtTerm::IntLit(n)) => (Some(v), Some(*n)),
                        (SmtTerm::IntLit(n), SmtTerm::Var(v)) => (Some(v), Some(*n)),
                        _ => (None, None),
                    };
                    if let (Some(v), Some(n)) = (var_opt, lit_opt) {
                        if let Some(known_low) = lower.get(v) {
                            if known_low.saturating_add(n) >= goal { return Some(true); }
                        }
                    }
                }
                None
            } else { None }
        };
        // Ф.25.2 (Plan 33.6): string/array length non-negative.
        // `(>= (App "_field_len_int" [obj]) goal)` если goal <= 0 → true.
        // Encoded form длины — UF `_field_len_int(obj)` через type-aware naming Ф.10.1.
        let try_len_check = |inner: &SmtTerm| -> Option<bool> {
            if let SmtTerm::App(iop, iargs) = inner {
                if iop != ">=" || iargs.len() != 2 { return None; }
                let goal = match &iargs[1] { SmtTerm::IntLit(n) => *n, _ => return None };
                if let SmtTerm::App(lop, _) = &iargs[0] {
                    if lop.starts_with("_field_len") && goal <= 0 {
                        return Some(true);
                    }
                }
                None
            } else { None }
        };
        // Ф.20.1 (Plan 33.6): division bounds. `(/ Var Lit)` где Lit > 0:
        // - если goal <= 0 и known lower(Var) >= 0 → result non-negative → true.
        // - integer division: result <= Var/Lit (всегда <= Var/Lit для positive).
        let try_division_check = |inner: &SmtTerm| -> Option<bool> {
            if let SmtTerm::App(iop, iargs) = inner {
                if iop != ">=" || iargs.len() != 2 { return None; }
                let goal = match &iargs[1] { SmtTerm::IntLit(n) => *n, _ => return None };
                if let SmtTerm::App(dop, dargs) = &iargs[0] {
                    if dop != "/" || dargs.len() != 2 { return None; }
                    if let (SmtTerm::Var(v), SmtTerm::IntLit(divisor)) = (&dargs[0], &dargs[1]) {
                        if *divisor > 0 && goal <= 0 {
                            if let Some(known_low) = lower.get(v) {
                                if *known_low >= 0 {
                                    return Some(true);
                                }
                            }
                        }
                    }
                }
                None
            } else { None }
        };
        // Ф.19.1 (Plan 33.6): modulus bounds. `(% Var Lit)` где Lit > 0:
        // - result >= 0 если goal <= 0 → true.
        // - result < Lit (но это эквивалентно `<` form).
        let try_modulus_check = |inner: &SmtTerm| -> Option<bool> {
            if let SmtTerm::App(iop, iargs) = inner {
                if iargs.len() != 2 { return None; }
                let goal = match &iargs[1] { SmtTerm::IntLit(n) => *n, _ => return None };
                if let SmtTerm::App(mop, margs) = &iargs[0] {
                    if mop != "%" || margs.len() != 2 { return None; }
                    if let (SmtTerm::Var(_), SmtTerm::IntLit(divisor)) = (&margs[0], &margs[1]) {
                        if *divisor > 0 {
                            // Lower bound: `(>= x%k goal)` где goal <= 0 → true.
                            if iop == ">=" && goal <= 0 { return Some(true); }
                            // Ф.26.1 (Plan 33.6): upper bound: `(< x%k goal)` где goal >= divisor → true,
                            // `(<= x%k goal)` где goal >= divisor-1 → true.
                            if iop == "<" && goal >= *divisor { return Some(true); }
                            if iop == "<=" && goal >= divisor - 1 { return Some(true); }
                        }
                    }
                }
                None
            } else { None }
        };
        // Ф.18.1 (Plan 33.6): subtraction bounds.
        // `(>= (- VarA VarB) goal)` где lower(A) - upper(B) >= goal → true.
        // `(>= (- Var IntLit) goal)` где lower(Var) - N >= goal → true.
        // `(>= (- IntLit Var) goal)` где N - upper(Var) >= goal → true.
        let try_subtraction_check = |inner: &SmtTerm| -> Option<bool> {
            if let SmtTerm::App(iop, iargs) = inner {
                if iop != ">=" || iargs.len() != 2 { return None; }
                let goal = match &iargs[1] { SmtTerm::IntLit(n) => *n, _ => return None };
                if let SmtTerm::App(sop, sargs) = &iargs[0] {
                    if sop != "-" || sargs.len() != 2 { return None; }
                    match (&sargs[0], &sargs[1]) {
                        (SmtTerm::Var(a), SmtTerm::Var(b)) => {
                            if let (Some(la), Some(ub)) = (lower.get(a), upper.get(b)) {
                                if la.saturating_sub(*ub) >= goal { return Some(true); }
                            }
                        }
                        (SmtTerm::Var(v), SmtTerm::IntLit(n)) => {
                            if let Some(known_low) = lower.get(v) {
                                if known_low.saturating_sub(*n) >= goal { return Some(true); }
                            }
                        }
                        (SmtTerm::IntLit(n), SmtTerm::Var(v)) => {
                            if let Some(known_up) = upper.get(v) {
                                if n.saturating_sub(*known_up) >= goal { return Some(true); }
                            }
                        }
                        _ => {}
                    }
                }
                None
            } else { None }
        };
        // Ф.17.1 (Plan 33.6): negation monotone.
        // `(>= (- 0 Var) goal)` где known upper(Var) <= -goal → true.
        let try_negation_check = |inner: &SmtTerm| -> Option<bool> {
            if let SmtTerm::App(iop, iargs) = inner {
                if iop != ">=" || iargs.len() != 2 { return None; }
                let goal = match &iargs[1] { SmtTerm::IntLit(n) => *n, _ => return None };
                if let SmtTerm::App(sop, sargs) = &iargs[0] {
                    if sop != "-" || sargs.len() != 2 { return None; }
                    if let (SmtTerm::IntLit(0), SmtTerm::Var(v)) = (&sargs[0], &sargs[1]) {
                        if let Some(known_up) = upper.get(v) {
                            if known_up.saturating_neg() >= goal { return Some(true); }
                        }
                    }
                }
                None
            } else { None }
        };
        // Ф.16.3 (Plan 33.6): strict-monotone constant multiply.
        // `(>= (* L Var) goal)` где L > 0 и known lower(Var) * L >= goal → true.
        let try_const_mul_check = |inner: &SmtTerm| -> Option<bool> {
            if let SmtTerm::App(iop, iargs) = inner {
                if iop != ">=" || iargs.len() != 2 { return None; }
                let goal = match &iargs[1] { SmtTerm::IntLit(n) => *n, _ => return None };
                // arg[0] должен быть (* L Var) или (* Var L).
                if let SmtTerm::App(mop, margs) = &iargs[0] {
                    if mop != "*" || margs.len() != 2 { return None; }
                    let (l, v) = match (&margs[0], &margs[1]) {
                        (SmtTerm::IntLit(l), SmtTerm::Var(v)) => (*l, v),
                        (SmtTerm::Var(v), SmtTerm::IntLit(l)) => (*l, v),
                        _ => return None,
                    };
                    if l > 0 {
                        if let Some(known_low) = lower.get(v) {
                            if known_low.saturating_mul(l) >= goal {
                                return Some(true);
                            }
                        }
                    }
                }
                None
            } else { None }
        };
        // Helper for inner check: bool result или None.
        let try_check = |inner: &SmtTerm| -> Option<bool> {
            if let SmtTerm::App(iop, iargs) = inner {
                if iargs.len() == 2 {
                    match (iop.as_str(), &iargs[0], &iargs[1]) {
                        (">=", SmtTerm::Var(v), SmtTerm::IntLit(n)) => {
                            if let Some(known) = lower.get(v) {
                                if *known >= *n { return Some(true); }
                            }
                            if let Some(known) = upper.get(v) {
                                if *known < *n { return Some(false); }
                            }
                        }
                        ("<=", SmtTerm::Var(v), SmtTerm::IntLit(n)) => {
                            if let Some(known) = upper.get(v) {
                                if *known <= *n { return Some(true); }
                            }
                            if let Some(known) = lower.get(v) {
                                if *known > *n { return Some(false); }
                            }
                        }
                        ("<", SmtTerm::Var(v), SmtTerm::IntLit(n)) => {
                            if let Some(known) = lower.get(v) {
                                if *known >= *n { return Some(false); }
                            }
                            if let Some(known) = upper.get(v) {
                                if *known < *n { return Some(true); }
                            }
                        }
                        (">", SmtTerm::Var(v), SmtTerm::IntLit(n)) => {
                            if let Some(known) = upper.get(v) {
                                if *known <= *n { return Some(false); }
                            }
                            if let Some(known) = lower.get(v) {
                                if *known > *n { return Some(true); }
                            }
                        }
                        _ => {}
                    }
                }
            }
            None
        };
        // Direct check.
        if let Some(b) = try_check(c) {
            return SmtTerm::BoolLit(b);
        }
        // Ф.16.3: const-mul check.
        if let Some(b) = try_const_mul_check(c) {
            return SmtTerm::BoolLit(b);
        }
        // Ф.17.2: addition check.
        if let Some(b) = try_addition_check(c) {
            return SmtTerm::BoolLit(b);
        }
        // Ф.18.1: subtraction check.
        if let Some(b) = try_subtraction_check(c) {
            return SmtTerm::BoolLit(b);
        }
        // Ф.19.1: modulus check.
        if let Some(b) = try_modulus_check(c) {
            return SmtTerm::BoolLit(b);
        }
        // Ф.20.1: division check.
        if let Some(b) = try_division_check(c) {
            return SmtTerm::BoolLit(b);
        }
        // Ф.25.2: string/array length check.
        if let Some(b) = try_len_check(c) {
            return SmtTerm::BoolLit(b);
        }
        // Ф.17.1: negation check.
        if let Some(b) = try_negation_check(c) {
            return SmtTerm::BoolLit(b);
        }
        // (not <inequality>) — invert result.
        if let SmtTerm::App(op, args) = c {
            if op == "not" && args.len() == 1 {
                if let Some(b) = try_check(&args[0]) {
                    return SmtTerm::BoolLit(!b);
                }
                if let Some(b) = try_const_mul_check(&args[0]) {
                    return SmtTerm::BoolLit(!b);
                }
                if let Some(b) = try_addition_check(&args[0]) {
                    return SmtTerm::BoolLit(!b);
                }
                if let Some(b) = try_subtraction_check(&args[0]) {
                    return SmtTerm::BoolLit(!b);
                }
                if let Some(b) = try_modulus_check(&args[0]) {
                    return SmtTerm::BoolLit(!b);
                }
                if let Some(b) = try_division_check(&args[0]) {
                    return SmtTerm::BoolLit(!b);
                }
                if let Some(b) = try_len_check(&args[0]) {
                    return SmtTerm::BoolLit(!b);
                }
                if let Some(b) = try_negation_check(&args[0]) {
                    return SmtTerm::BoolLit(!b);
                }
            }
        }
        c.clone()
    }).collect()
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
