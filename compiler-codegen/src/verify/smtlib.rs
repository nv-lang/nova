//! Plan 33.14 Ф.1: SMT-LIB v2 текстовый эмиттер из `SmtTerm`.
//!
//! Второй — независимый от Z3-FFI — путь кодирования verification
//! conditions. Используется:
//!  - `backend::cvc5::Cvc5Backend` — скармливает текст подпроцессу `cvc5`;
//!  - `backend::crosscheck::CrossCheckBackend` — гоняет обе линии и
//!    сравнивает ответы.
//!
//! ## Принцип точного зеркала
//!
//! Эмиттер обязан принимать **ровно тот** набор операторов, который
//! принимает `backend::z3` (`translate_app`). Если Z3-FFI умеет
//! транслировать терм, эмиттер обязан его эмитить; если Z3-FFI вернул бы
//! ошибку — эмиттер обязан вернуть [`EmitError`]. Это инвариант
//! cross-check'а: расхождение «Z3 смог / SMT-LIB не смог» не должно
//! приводить к ложному disagreement — оно даёт `cvc5 = Unknown`, что
//! классифицируется как OK.
//!
//! Поэтому эмиттер **никогда не эмитит приблизительный SMT-LIB**: при
//! малейшей неуверенности — `Err`, и cross-check эту VC пропускает, а не
//! объявляет ложное расхождение.
//!
//! ## Соответствие семантике Z3-backend'а
//!
//! - `_view_*` / `_pure_fn_*` / `_trusted_*` — настоящие uninterpreted
//!   functions (`declare-fun`), как `z3::uf_app`.
//! - `_field_*` — «свежая константа на каждый структурно-различный
//!   набор аргументов», точное зеркало `z3::legacy_uninterpreted_app`
//!   (там ключ — указатель hash-cons'нутого AST; здесь — структурный
//!   текст аргументов). Плюс axiom неотрицательности для
//!   `_field_len_int` / `_field_cap_int` / `_field_byte_len_int`.
//! - Неизвестные `Var` авто-декларируются как `Int` (как `z3::translate`).
//! - Bit-vector overflow-предикаты кодируются в чистой BV-теории
//!   (sign/zero-extend), семантически совпадая с `Z3_mk_bv*_no_*flow`.

use std::collections::{HashMap, HashSet};

use super::ir::{Assertion, Formula, SmtTerm, SortRef};

/// Невозможность построить корректный SMT-LIB. Возвращается **вместо**
/// эмиссии неверного текста — неверный текст обрушил бы смысл cross-check'а.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmitError {
    /// Оператор не поддержан (или поддержан, но не с такой арностью) —
    /// зеркалит `_ => Err(...)` в `z3::translate_app`.
    UnsupportedOp { op: String, arity: usize },
    /// Параметризованный оператор (`zero_extend N`, `extract H L`) с
    /// некорректным числовым параметром.
    BadOpParam(String),
    /// Строковый литерал содержит NUL — непредставим в SMT-LIB.
    NulInString,
    /// Не удалось определить ширину bit-vector'а для overflow-предиката.
    UnknownBitWidth(String),
}

impl std::fmt::Display for EmitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EmitError::UnsupportedOp { op, arity } => {
                write!(f, "smtlib: неподдержанный оператор `{}` с {} арг.", op, arity)
            }
            EmitError::BadOpParam(s) => write!(f, "smtlib: некорректный параметр оператора: {}", s),
            EmitError::NulInString => write!(f, "smtlib: строковый литерал содержит NUL"),
            EmitError::UnknownBitWidth(s) => {
                write!(f, "smtlib: не определена ширина bit-vector'а: {}", s)
            }
        }
    }
}

/// Рендер `SortRef` в текст SMT-LIB v2.
pub fn emit_sort(sort: &SortRef) -> String {
    match sort {
        SortRef::Int => "Int".to_string(),
        SortRef::Bool => "Bool".to_string(),
        SortRef::Str => "String".to_string(),
        // Явная форма `(_ FloatingPoint eb sb)` — не полагаемся на
        // abbreviations Float32/Float64 (хотя оба решателя их знают).
        SortRef::F32 => "(_ FloatingPoint 8 24)".to_string(),
        SortRef::F64 => "(_ FloatingPoint 11 53)".to_string(),
        SortRef::Named(name) => sym(name),
        SortRef::BitVec { width, .. } => format!("(_ BitVec {})", width),
    }
}

/// Экранировать имя в валидный symbol SMT-LIB v2.
///
/// Простые символы (буквы/цифры/`~!@$%^&*_-+=<>.?/`, не с цифры) —
/// как есть. Иначе — quoted-symbol `|...|`. Внутри `|...|` запрещены
/// только `|` и `\`; имена с ними в этом компиляторе не встречаются, но
/// на всякий случай они тоже заворачиваются (cvc5 выдаст parse-error →
/// runner вернёт Unknown — ложного disagreement не будет).
fn sym(name: &str) -> String {
    let simple = !name.is_empty()
        && !name.starts_with(|c: char| c.is_ascii_digit())
        && name.chars().all(|c| {
            c.is_ascii_alphanumeric() || "~!@$%^&*_-+=<>.?/".contains(c)
        });
    if simple {
        name.to_string()
    } else {
        format!("|{}|", name.replace('|', "_").replace('\\', "_"))
    }
}

/// Stateful построитель SMT-LIB-скрипта.
///
/// Lifecycle зеркалит `SmtBackend`: `declare_var` / `declare_function` /
/// `assert` / `push` / `pop`, затем `render`.
///
/// Декларации (`declare-sort` / `declare-const` / `declare-fun`)
/// **не скоупятся** push/pop — в Z3-backend'е соответствующие AST-узлы
/// тоже глобальны (создаются `Z3_mk_const`, а не SMT-LIB-декларацией).
/// Поэтому декларации рендерятся единым блоком в начале, а скоупится
/// только список `assert`'ов.
pub struct SmtLibEmitter {
    /// Объявленные uninterpreted sorts (record-типы) — порядок вставки.
    sorts: Vec<String>,
    sorts_seen: HashSet<String>,
    /// Объявленные константы: (имя, sort). Порядок вставки.
    consts: Vec<(String, SortRef)>,
    consts_seen: HashSet<String>,
    /// Объявленные функции: (имя, domain sorts, range sort).
    funcs: Vec<(String, Vec<SortRef>, SortRef)>,
    funcs_seen: HashSet<String>,
    /// Скоупленные assert'ы: (текст, глубина push на момент добавления).
    asserts: Vec<(String, usize)>,
    /// Текущая глубина push.
    depth: usize,
    /// `_field_*` fresh-const кеш: структурный ключ → имя константы.
    field_consts: HashMap<String, String>,
    /// Аксиомы, накопленные при создании `_field_*`-констант в текущем
    /// `assert` (напр. неотрицательность `_field_len_int`). Сливаются в
    /// `asserts` на той же глубине.
    pending_axioms: Vec<String>,
    /// Хотя бы одна формула не сэмитилась — зеркало `z3::translation_failed`.
    translation_failed: bool,
}

impl Default for SmtLibEmitter {
    fn default() -> Self {
        Self::new()
    }
}

impl SmtLibEmitter {
    pub fn new() -> Self {
        SmtLibEmitter {
            sorts: Vec::new(),
            sorts_seen: HashSet::new(),
            consts: Vec::new(),
            consts_seen: HashSet::new(),
            funcs: Vec::new(),
            funcs_seen: HashSet::new(),
            asserts: Vec::new(),
            depth: 0,
            field_consts: HashMap::new(),
            pending_axioms: Vec::new(),
            translation_failed: false,
        }
    }

    /// Была ли хоть одна непереводимая формула. Если да — `check_sat`
    /// в Cvc5Backend обязан вернуть `Unknown`, не доверяя решателю
    /// (зеркало Plan 33.8 Ф.6.2).
    pub fn translation_failed(&self) -> bool {
        self.translation_failed
    }

    fn ensure_sort(&mut self, name: &str) {
        if self.sorts_seen.insert(name.to_string()) {
            self.sorts.push(name.to_string());
        }
    }

    fn ensure_sort_of(&mut self, sort: &SortRef) {
        if let SortRef::Named(n) = sort {
            self.ensure_sort(n);
        }
    }

    /// Объявить константу, если имя ещё не занято. Идемпотентно (как
    /// `z3::declare_var`: повторный declare игнорируется).
    fn ensure_const(&mut self, name: &str, sort: SortRef) {
        if self.consts_seen.insert(name.to_string()) {
            self.ensure_sort_of(&sort);
            self.consts.push((name.to_string(), sort));
        }
    }

    fn ensure_func(&mut self, name: &str, domain: Vec<SortRef>, range: SortRef) {
        if self.funcs_seen.insert(name.to_string()) {
            for d in &domain {
                self.ensure_sort_of(d);
            }
            self.ensure_sort_of(&range);
            self.funcs.push((name.to_string(), domain, range));
        }
    }

    /// `SmtBackend::declare_var`.
    pub fn declare_var(&mut self, name: &str, sort: SortRef) {
        self.ensure_const(name, sort);
    }

    /// `SmtBackend::declare_function`.
    pub fn declare_function(&mut self, name: &str, param_sorts: &[SortRef], return_sort: SortRef) {
        self.ensure_func(name, param_sorts.to_vec(), return_sort);
    }

    /// `SmtBackend::push`.
    pub fn push(&mut self) {
        self.depth += 1;
    }

    /// `SmtBackend::pop` — отбрасывает assert'ы, добавленные глубже.
    pub fn pop(&mut self) {
        if self.depth > 0 {
            self.depth -= 1;
        }
        let d = self.depth;
        self.asserts.retain(|(_, ad)| *ad <= d);
    }

    /// `SmtBackend::assert`. На ошибке эмиссии — ставит `translation_failed`
    /// и формулу **не** добавляет (зеркало `z3::assert` Ф.6.2).
    pub fn assert(&mut self, assertion: &Assertion) {
        self.pending_axioms.clear();
        let bound: HashSet<String> = HashSet::new();
        let result = self.emit_term(&assertion.formula, &bound);
        // Аксиомы `_field_len_*` фиксируются на текущей глубине вне
        // зависимости от исхода эмиссии — `z3::legacy_uninterpreted_app`
        // ассертит их сразу при создании константы.
        let depth = self.depth;
        let pending = std::mem::take(&mut self.pending_axioms);
        for ax in pending {
            self.asserts.push((ax, depth));
        }
        match result {
            Ok(text) => self.asserts.push((text, depth)),
            Err(_) => self.translation_failed = true,
        }
    }

    /// Полный скрипт с `(check-sat)`.
    pub fn render(&self) -> String {
        self.render_inner(false)
    }

    /// Полный скрипт с `(check-sat)` и `(get-model)` — для извлечения
    /// counterexample / witness.
    pub fn render_with_get_model(&self) -> String {
        self.render_inner(true)
    }

    fn render_inner(&self, get_model: bool) -> String {
        let mut out = String::new();
        // `ALL` покрывает Int + BV + FP + Strings + Quantifiers + UF —
        // и Z3, и cvc5 принимают её.
        out.push_str("(set-logic ALL)\n");
        out.push_str("(set-option :produce-models true)\n");
        for s in &self.sorts {
            out.push_str(&format!("(declare-sort {} 0)\n", sym(s)));
        }
        for (name, sort) in &self.consts {
            out.push_str(&format!("(declare-const {} {})\n", sym(name), emit_sort(sort)));
        }
        for (name, domain, range) in &self.funcs {
            let dom: Vec<String> = domain.iter().map(emit_sort).collect();
            out.push_str(&format!(
                "(declare-fun {} ({}) {})\n",
                sym(name),
                dom.join(" "),
                emit_sort(range)
            ));
        }
        for (text, _) in &self.asserts {
            out.push_str(&format!("(assert {})\n", text));
        }
        out.push_str("(check-sat)\n");
        if get_model {
            out.push_str("(get-model)\n");
        }
        out
    }

    // ─────────────────────────────────────────────────────────────────
    // Эмиссия термов
    // ─────────────────────────────────────────────────────────────────

    /// Рекурсивная эмиссия одного `SmtTerm` в текст SMT-LIB-выражения.
    ///
    /// `bound` — имена, связанные объемлющими кванторами: они не
    /// авто-декларируются как глобальные константы.
    fn emit_term(&mut self, term: &SmtTerm, bound: &HashSet<String>) -> Result<String, EmitError> {
        match term {
            SmtTerm::IntLit(n) => {
                if *n < 0 {
                    // `(- N)` — отрицательный литерал в SMT-LIB только так.
                    // i128 защищает от переполнения abs(i64::MIN).
                    Ok(format!("(- {})", (*n as i128).unsigned_abs()))
                } else {
                    Ok(n.to_string())
                }
            }
            SmtTerm::BoolLit(b) => Ok(if *b { "true".into() } else { "false".into() }),
            SmtTerm::StrLit(s) => {
                if s.contains('\0') {
                    return Err(EmitError::NulInString);
                }
                // SMT-LIB: внутренняя `"` удваивается.
                Ok(format!("\"{}\"", s.replace('"', "\"\"")))
            }
            SmtTerm::F32Lit(bits) => {
                let sign = (*bits >> 31) & 1;
                let exp = (*bits >> 23) & 0xFF;
                let mant = *bits & 0x7F_FFFF;
                Ok(format!("(fp #b{:01b} #b{:08b} #b{:023b})", sign, exp, mant))
            }
            SmtTerm::F64Lit(bits) => {
                let sign = (*bits >> 63) & 1;
                let exp = (*bits >> 52) & 0x7FF;
                let mant = *bits & 0xF_FFFF_FFFF_FFFF;
                Ok(format!("(fp #b{:01b} #b{:011b} #b{:052b})", sign, exp, mant))
            }
            SmtTerm::BitVecLit(v, w) => {
                let masked = if *w >= 64 { *v } else { *v & ((1u64 << *w) - 1) };
                Ok(format!("(_ bv{} {})", masked, w))
            }
            SmtTerm::Var(name) => {
                if !bound.contains(name) {
                    // Авто-декларация неизвестной переменной как Int —
                    // зеркало `z3::translate` (SmtTerm::Var fallback).
                    self.ensure_const(name, SortRef::Int);
                }
                Ok(sym(name))
            }
            SmtTerm::App(op, args) => self.emit_app(op, args, bound),
            SmtTerm::Forall(binders, patterns, body) => {
                self.emit_forall(binders, patterns, body, bound)
            }
        }
    }

    fn emit_forall(
        &mut self,
        binders: &[(String, SortRef)],
        patterns: &[Vec<SmtTerm>],
        body: &SmtTerm,
        bound: &HashSet<String>,
    ) -> Result<String, EmitError> {
        // Пустой quantifier == тело без изменений (зеркало z3.rs).
        if binders.is_empty() {
            return self.emit_term(body, bound);
        }
        let mut inner = bound.clone();
        let mut decls: Vec<String> = Vec::with_capacity(binders.len());
        for (bname, bsort) in binders {
            self.ensure_sort_of(bsort);
            inner.insert(bname.clone());
            decls.push(format!("({} {})", sym(bname), emit_sort(bsort)));
        }
        let body_txt = self.emit_term(body, &inner)?;
        // Pattern'ы — лишь подсказки. Если pattern-терм не сэмитился,
        // дропаем этот pattern (как z3.rs), но quantifier сохраняем.
        let mut pat_txt = String::new();
        for pat in patterns {
            if pat.is_empty() {
                continue;
            }
            let mut terms: Vec<String> = Vec::with_capacity(pat.len());
            let mut ok = true;
            for t in pat {
                match self.emit_term(t, &inner) {
                    Ok(s) => terms.push(s),
                    Err(_) => {
                        ok = false;
                        break;
                    }
                }
            }
            if ok && !terms.is_empty() {
                pat_txt.push_str(&format!(" :pattern ({})", terms.join(" ")));
            }
        }
        if pat_txt.is_empty() {
            Ok(format!("(forall ({}) {})", decls.join(" "), body_txt))
        } else {
            Ok(format!("(forall ({}) (! {}{}))", decls.join(" "), body_txt, pat_txt))
        }
    }

    fn emit_app(
        &mut self,
        op: &str,
        args: &[SmtTerm],
        bound: &HashSet<String>,
    ) -> Result<String, EmitError> {
        // UF-семейства и `_field_*` обрабатываются до общей эмиссии
        // аргументов (у `_field_*` своя логика свежих констант).
        if op.starts_with("_view_") || op.starts_with("_pure_fn_") || op.starts_with("_trusted_") {
            return self.emit_uf(op, args, bound);
        }
        if op.starts_with("_field_") {
            return self.emit_field(op, args, bound);
        }

        // Общий путь: сначала эмитим все аргументы (это попутно
        // авто-декларирует переменные).
        let a: Vec<String> = args
            .iter()
            .map(|t| self.emit_term(t, bound))
            .collect::<Result<_, _>>()?;
        let n = a.len();
        let unsupported = || EmitError::UnsupportedOp { op: op.to_string(), arity: n };

        // Параметризованные BV-операторы (несут число в имени).
        if let Some(rest) = op.strip_prefix("zero_extend ") {
            let k: u32 = rest.trim().parse().map_err(|_| EmitError::BadOpParam(op.to_string()))?;
            return match a.as_slice() {
                [x] => Ok(format!("((_ zero_extend {}) {})", k, x)),
                _ => Err(unsupported()),
            };
        }
        if let Some(rest) = op.strip_prefix("sign_extend ") {
            let k: u32 = rest.trim().parse().map_err(|_| EmitError::BadOpParam(op.to_string()))?;
            return match a.as_slice() {
                [x] => Ok(format!("((_ sign_extend {}) {})", k, x)),
                _ => Err(unsupported()),
            };
        }
        if let Some(rest) = op.strip_prefix("extract ") {
            let mut parts = rest.split_whitespace();
            let high: u32 = parts
                .next()
                .and_then(|s| s.parse().ok())
                .ok_or_else(|| EmitError::BadOpParam(op.to_string()))?;
            let low: u32 = parts
                .next()
                .and_then(|s| s.parse().ok())
                .ok_or_else(|| EmitError::BadOpParam(op.to_string()))?;
            return match a.as_slice() {
                [x] => Ok(format!("((_ extract {} {}) {})", high, low, x)),
                _ => Err(unsupported()),
            };
        }

        // BV overflow-предикаты — кодируются в чистой BV-теории; нужна
        // ширина операндов.
        if matches!(
            op,
            "bvadd_no_overflow_s"
                | "bvadd_no_overflow_u"
                | "bvadd_no_underflow"
                | "bvsub_no_overflow"
                | "bvsub_no_underflow_s"
                | "bvsub_no_underflow_u"
                | "bvmul_no_overflow_s"
                | "bvmul_no_overflow_u"
        ) {
            return match (args, a.as_slice()) {
                ([x_t, y_t], [x, y]) => self.emit_bv_overflow(op, x_t, y_t, x, y, bound),
                _ => Err(unsupported()),
            };
        }

        // Фиксированный словарь — точное зеркало `z3::translate_app`.
        let text = match (op, a.as_slice()) {
            // ── Линейная арифметика ───────────────────────────────────
            ("+", [single]) => single.clone(),
            ("+", rest) if !rest.is_empty() => format!("(+ {})", rest.join(" ")),
            ("-", [x]) => format!("(- {})", x),
            ("-", rest) if rest.len() >= 2 => format!("(- {})", rest.join(" ")),
            ("*", [single]) => single.clone(),
            ("*", rest) if !rest.is_empty() => format!("(* {})", rest.join(" ")),
            ("/", [x, y]) => format!("(div {} {})", x, y),
            ("%", [x, y]) => format!("(mod {} {})", x, y),

            // ── Сравнения ─────────────────────────────────────────────
            ("=", [x, y]) => format!("(= {} {})", x, y),
            ("!=", [x, y]) => format!("(distinct {} {})", x, y),
            ("<", [x, y]) => format!("(< {} {})", x, y),
            ("<=", [x, y]) => format!("(<= {} {})", x, y),
            (">", [x, y]) => format!("(> {} {})", x, y),
            (">=", [x, y]) => format!("(>= {} {})", x, y),

            // ── Булева логика ─────────────────────────────────────────
            ("not", [x]) => format!("(not {})", x),
            ("and", [single]) => single.clone(),
            ("and", rest) if !rest.is_empty() => format!("(and {})", rest.join(" ")),
            ("or", [single]) => single.clone(),
            ("or", rest) if !rest.is_empty() => format!("(or {})", rest.join(" ")),
            ("=>", [x, y]) => format!("(=> {} {})", x, y),
            // Bool-iff: в SMT-LIB это `=` над Bool.
            ("<=>", [x, y]) => format!("(= {} {})", x, y),
            ("ite", [c, t, e]) => format!("(ite {} {} {})", c, t, e),

            // ── IEEE-754 floating point ───────────────────────────────
            // Арифметика — с rounding mode RNE.
            ("fp.add", [x, y]) => format!("(fp.add RNE {} {})", x, y),
            ("fp.sub", [x, y]) => format!("(fp.sub RNE {} {})", x, y),
            ("fp.mul", [x, y]) => format!("(fp.mul RNE {} {})", x, y),
            ("fp.div", [x, y]) => format!("(fp.div RNE {} {})", x, y),
            ("fp.sqrt", [x]) => format!("(fp.sqrt RNE {})", x),
            ("fp.abs", [x]) => format!("(fp.abs {})", x),
            ("fp.neg", [x]) => format!("(fp.neg {})", x),
            ("fp.eq", [x, y]) => format!("(fp.eq {} {})", x, y),
            ("fp.lt", [x, y]) => format!("(fp.lt {} {})", x, y),
            ("fp.leq", [x, y]) => format!("(fp.leq {} {})", x, y),
            ("fp.gt", [x, y]) => format!("(fp.gt {} {})", x, y),
            ("fp.geq", [x, y]) => format!("(fp.geq {} {})", x, y),
            // Предикаты — SMT-LIB-имена в camelCase (IR хранит snake_case).
            ("fp.is_nan", [x]) => format!("(fp.isNaN {})", x),
            ("fp.is_infinite", [x]) => format!("(fp.isInfinite {})", x),
            ("fp.is_positive", [x]) => format!("(fp.isPositive {})", x),
            ("fp.is_negative", [x]) => format!("(fp.isNegative {})", x),
            ("fp.is_zero", [x]) => format!("(fp.isZero {})", x),

            // ── Строки / Seq ──────────────────────────────────────────
            ("str.len", [x]) => format!("(str.len {})", x),
            ("str.contains", [x, y]) => format!("(str.contains {} {})", x, y),
            ("str.prefix", [x, y]) => format!("(str.prefixof {} {})", x, y),
            ("str.suffix", [x, y]) => format!("(str.suffixof {} {})", x, y),
            ("str.concat", [single]) => single.clone(),
            ("str.concat", rest) if !rest.is_empty() => format!("(str.++ {})", rest.join(" ")),
            ("str.substr", [s, off, len]) => format!("(str.substr {} {} {})", s, off, len),
            ("str.index", [s, sub, off]) => format!("(str.indexof {} {} {})", s, sub, off),

            // ── Bit-vectors ───────────────────────────────────────────
            ("bvadd", [x, y]) => format!("(bvadd {} {})", x, y),
            ("bvsub", [x, y]) => format!("(bvsub {} {})", x, y),
            ("bvmul", [x, y]) => format!("(bvmul {} {})", x, y),
            ("bvsdiv", [x, y]) => format!("(bvsdiv {} {})", x, y),
            ("bvsrem", [x, y]) => format!("(bvsrem {} {})", x, y),
            ("bvudiv", [x, y]) => format!("(bvudiv {} {})", x, y),
            ("bvurem", [x, y]) => format!("(bvurem {} {})", x, y),
            ("bvneg", [x]) => format!("(bvneg {})", x),
            ("bvand", [x, y]) => format!("(bvand {} {})", x, y),
            ("bvor", [x, y]) => format!("(bvor {} {})", x, y),
            ("bvxor", [x, y]) => format!("(bvxor {} {})", x, y),
            ("bvnot", [x]) => format!("(bvnot {})", x),
            ("bvshl", [x, y]) => format!("(bvshl {} {})", x, y),
            ("bvlshr", [x, y]) => format!("(bvlshr {} {})", x, y),
            ("bvashr", [x, y]) => format!("(bvashr {} {})", x, y),
            ("bvslt", [x, y]) => format!("(bvslt {} {})", x, y),
            ("bvsle", [x, y]) => format!("(bvsle {} {})", x, y),
            ("bvsgt", [x, y]) => format!("(bvsgt {} {})", x, y),
            ("bvsge", [x, y]) => format!("(bvsge {} {})", x, y),
            ("bvult", [x, y]) => format!("(bvult {} {})", x, y),
            ("bvule", [x, y]) => format!("(bvule {} {})", x, y),
            ("bvugt", [x, y]) => format!("(bvugt {} {})", x, y),
            ("bvuge", [x, y]) => format!("(bvuge {} {})", x, y),

            _ => return Err(unsupported()),
        };
        Ok(text)
    }

    /// `_view_*` / `_pure_fn_*` / `_trusted_*` — настоящая uninterpreted
    /// function. Если не предобъявлена через `declare_function` —
    /// авто-декларация с Int domain/range (зеркало `z3::uf_app`).
    fn emit_uf(
        &mut self,
        op: &str,
        args: &[SmtTerm],
        bound: &HashSet<String>,
    ) -> Result<String, EmitError> {
        let a: Vec<String> = args
            .iter()
            .map(|t| self.emit_term(t, bound))
            .collect::<Result<_, _>>()?;
        if !self.funcs_seen.contains(op) {
            let domain = vec![SortRef::Int; a.len()];
            self.ensure_func(op, domain, SortRef::Int);
        }
        if a.is_empty() {
            // 0-арная UF == константа: `(f)` невалидно, эмитим имя.
            Ok(sym(op))
        } else {
            Ok(format!("({} {})", sym(op), a.join(" ")))
        }
    }

    /// `_field_*` — record member access. Зеркалит
    /// `z3::legacy_uninterpreted_app`: свежая константа на каждый
    /// структурно-различный набор аргументов (без congruence-аксиомы UF).
    /// Для `_field_len_int` / `_field_cap_int` / `_field_byte_len_int`
    /// добавляется axiom неотрицательности.
    fn emit_field(
        &mut self,
        op: &str,
        args: &[SmtTerm],
        bound: &HashSet<String>,
    ) -> Result<String, EmitError> {
        let a: Vec<String> = args
            .iter()
            .map(|t| self.emit_term(t, bound))
            .collect::<Result<_, _>>()?;
        // Структурный ключ: имя оператора + текст каждого аргумента.
        let key = format!("{}\u{1}{}", op, a.join("\u{1}"));
        if let Some(name) = self.field_consts.get(&key) {
            return Ok(name.clone());
        }
        let const_name = format!("uf_{}_{}", op, self.field_consts.len());
        self.field_consts.insert(key, const_name.clone());
        self.ensure_const(&const_name, SortRef::Int);
        if matches!(op, "_field_len_int" | "_field_cap_int" | "_field_byte_len_int") {
            // len/cap/byte_len неотрицательны по построению.
            self.pending_axioms.push(format!("(>= {} 0)", sym(&const_name)));
        }
        Ok(sym(&const_name))
    }

    // ─────────────────────────────────────────────────────────────────
    // BV overflow-предикаты
    // ─────────────────────────────────────────────────────────────────

    /// Закодировать `bv*_no_*flow`-предикат в чистой BV-теории.
    ///
    /// Семантика точно совпадает с Z3 C API (`Z3_mk_bvadd_no_overflow`
    /// и пр.): знаковые overflow-предикаты — формула «совпадение знаков
    /// операндов влечёт совпадение знака результата»; беззнаковые —
    /// сравнение с операндом / extension high-бит.
    fn emit_bv_overflow(
        &mut self,
        op: &str,
        x_t: &SmtTerm,
        y_t: &SmtTerm,
        x: &str,
        y: &str,
        bound: &HashSet<String>,
    ) -> Result<String, EmitError> {
        let w = self
            .bv_width(x_t, bound)
            .or_else(|| self.bv_width(y_t, bound))
            .ok_or_else(|| EmitError::UnknownBitWidth(op.to_string()))?;
        let zero = format!("(_ bv0 {})", w);
        let txt = match op {
            // Знаковое сложение, положительный overflow:
            // ¬(x≥0 ∧ y≥0 ∧ x+y<0).
            "bvadd_no_overflow_s" => format!(
                "(not (and (bvsge {x} {z}) (bvsge {y} {z}) (bvslt (bvadd {x} {y}) {z})))",
                x = x, y = y, z = zero
            ),
            // Знаковое сложение, отрицательный overflow (underflow):
            // ¬(x<0 ∧ y<0 ∧ x+y≥0).
            "bvadd_no_underflow" => format!(
                "(not (and (bvslt {x} {z}) (bvslt {y} {z}) (bvsge (bvadd {x} {y}) {z})))",
                x = x, y = y, z = zero
            ),
            // Беззнаковое сложение: нет overflow ⟺ результат ≥ операнда.
            "bvadd_no_overflow_u" => {
                format!("(bvuge (bvadd {x} {y}) {x})", x = x, y = y)
            }
            // Знаковое вычитание, положительный overflow:
            // ¬(x≥0 ∧ y<0 ∧ x-y<0).
            "bvsub_no_overflow" => format!(
                "(not (and (bvsge {x} {z}) (bvslt {y} {z}) (bvslt (bvsub {x} {y}) {z})))",
                x = x, y = y, z = zero
            ),
            // Знаковое вычитание, отрицательный overflow:
            // ¬(x<0 ∧ y≥0 ∧ x-y≥0).
            "bvsub_no_underflow_s" => format!(
                "(not (and (bvslt {x} {z}) (bvsge {y} {z}) (bvsge (bvsub {x} {y}) {z})))",
                x = x, y = y, z = zero
            ),
            // Беззнаковое вычитание: нет underflow ⟺ x ≥ y.
            "bvsub_no_underflow_u" => format!("(bvuge {x} {y})", x = x, y = y),
            // Знаковое умножение: 2N-битное знаковое произведение должно
            // помещаться в N знаковых бит.
            "bvmul_no_overflow_s" => {
                let n = w;
                format!(
                    "(let ((p (bvmul ((_ sign_extend {n}) {x}) ((_ sign_extend {n}) {y})))) \
                     (= p ((_ sign_extend {n}) ((_ extract {hi} 0) p))))",
                    n = n, x = x, y = y, hi = n - 1
                )
            }
            // Беззнаковое умножение: старшие N бит 2N-произведения нулевые.
            "bvmul_no_overflow_u" => {
                let n = w;
                format!(
                    "(= ((_ extract {hi} {n}) \
                     (bvmul ((_ zero_extend {n}) {x}) ((_ zero_extend {n}) {y}))) \
                     (_ bv0 {n}))",
                    n = n, x = x, y = y, hi = 2 * n - 1
                )
            }
            _ => return Err(EmitError::UnsupportedOp { op: op.to_string(), arity: 2 }),
        };
        Ok(txt)
    }

    /// Определить ширину bit-vector-терма. `None` — если терм не
    /// BV-сортированный либо ширину вывести нельзя.
    fn bv_width(&self, term: &SmtTerm, bound: &HashSet<String>) -> Option<u32> {
        match term {
            SmtTerm::BitVecLit(_, w) => Some(*w),
            SmtTerm::Var(name) => {
                if bound.contains(name) {
                    return None;
                }
                self.consts.iter().find(|(n, _)| n == name).and_then(|(_, s)| {
                    if let SortRef::BitVec { width, .. } = s {
                        Some(*width)
                    } else {
                        None
                    }
                })
            }
            SmtTerm::App(op, args) => {
                // Параметризованные resize-операторы.
                if let Some(rest) = op.strip_prefix("zero_extend ") {
                    let k: u32 = rest.trim().parse().ok()?;
                    return self.bv_width(args.first()?, bound).map(|w| w + k);
                }
                if let Some(rest) = op.strip_prefix("sign_extend ") {
                    let k: u32 = rest.trim().parse().ok()?;
                    return self.bv_width(args.first()?, bound).map(|w| w + k);
                }
                if let Some(rest) = op.strip_prefix("extract ") {
                    let mut parts = rest.split_whitespace();
                    let high: u32 = parts.next()?.parse().ok()?;
                    let low: u32 = parts.next()?.parse().ok()?;
                    return Some(high - low + 1);
                }
                match op.as_str() {
                    // Операции, сохраняющие ширину операндов.
                    "bvadd" | "bvsub" | "bvmul" | "bvsdiv" | "bvsrem" | "bvudiv"
                    | "bvurem" | "bvand" | "bvor" | "bvxor" | "bvshl" | "bvlshr"
                    | "bvashr" => {
                        for a in args {
                            if let Some(w) = self.bv_width(a, bound) {
                                return Some(w);
                            }
                        }
                        None
                    }
                    "bvneg" | "bvnot" => self.bv_width(args.first()?, bound),
                    "ite" => {
                        // ite cond then else — ширина из ветвей.
                        for a in args.iter().skip(1) {
                            if let Some(w) = self.bv_width(a, bound) {
                                return Some(w);
                            }
                        }
                        None
                    }
                    // UF: ширина из range-сорта объявленной функции.
                    other => self
                        .funcs
                        .iter()
                        .find(|(n, _, _)| n == other)
                        .and_then(|(_, _, r)| {
                            if let SortRef::BitVec { width, .. } = r {
                                Some(*width)
                            } else {
                                None
                            }
                        }),
                }
            }
            _ => None,
        }
    }
}

/// Удобный one-shot: эмиссия одного assert'а в полный скрипт.
/// Используется в golden-тестах.
pub fn emit_single(
    declared: &[(&str, SortRef)],
    formula: &Formula,
) -> SmtLibEmitter {
    let mut e = SmtLibEmitter::new();
    for (n, s) in declared {
        e.declare_var(n, s.clone());
    }
    e.assert(&Assertion { formula: formula.clone(), label: None });
    e
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(name: &str) -> SmtTerm {
        SmtTerm::Var(name.into())
    }

    #[test]
    fn sorts_render_correctly() {
        assert_eq!(emit_sort(&SortRef::Int), "Int");
        assert_eq!(emit_sort(&SortRef::Bool), "Bool");
        assert_eq!(emit_sort(&SortRef::Str), "String");
        assert_eq!(emit_sort(&SortRef::F32), "(_ FloatingPoint 8 24)");
        assert_eq!(emit_sort(&SortRef::F64), "(_ FloatingPoint 11 53)");
        assert_eq!(emit_sort(&SortRef::BitVec { width: 32, signed: true }), "(_ BitVec 32)");
        assert_eq!(emit_sort(&SortRef::Named("User".into())), "User");
    }

    #[test]
    fn linear_arith_golden() {
        // x > 0  ==>  x + 1 > 0
        let f = SmtTerm::implies(
            SmtTerm::App(">".into(), vec![v("x"), SmtTerm::IntLit(0)]),
            SmtTerm::App(
                ">".into(),
                vec![SmtTerm::App("+".into(), vec![v("x"), SmtTerm::IntLit(1)]), SmtTerm::IntLit(0)],
            ),
        );
        let e = emit_single(&[("x", SortRef::Int)], &f);
        let s = e.render();
        assert!(s.contains("(set-logic ALL)"), "{}", s);
        assert!(s.contains("(declare-const x Int)"), "{}", s);
        assert!(s.contains("(assert (=> (> x 0) (> (+ x 1) 0)))"), "{}", s);
        assert!(s.contains("(check-sat)"), "{}", s);
    }

    #[test]
    fn negative_int_literal() {
        let mut e = SmtLibEmitter::new();
        e.declare_var("x", SortRef::Int);
        e.assert(&Assertion {
            formula: SmtTerm::eq(v("x"), SmtTerm::IntLit(-5)),
            label: None,
        });
        assert!(e.render().contains("(assert (= x (- 5)))"));
    }

    #[test]
    fn neq_becomes_distinct() {
        let f = SmtTerm::App("!=".into(), vec![v("a"), v("b")]);
        let e = emit_single(&[("a", SortRef::Int), ("b", SortRef::Int)], &f);
        assert!(e.render().contains("(assert (distinct a b))"));
    }

    #[test]
    fn unary_minus_and_iff() {
        let neg = SmtTerm::App("-".into(), vec![v("x")]);
        let e = emit_single(&[("x", SortRef::Int)], &neg);
        assert!(e.render().contains("(assert (- x))"));

        let iff = SmtTerm::App("<=>".into(), vec![v("p"), v("q")]);
        let e2 = emit_single(&[("p", SortRef::Bool), ("q", SortRef::Bool)], &iff);
        assert!(e2.render().contains("(assert (= p q))"));
    }

    #[test]
    fn auto_declare_unknown_var() {
        // `y` нигде не объявлена — должна авто-задекларироваться как Int.
        let f = SmtTerm::eq(v("y"), SmtTerm::IntLit(1));
        let mut e = SmtLibEmitter::new();
        e.assert(&Assertion { formula: f, label: None });
        assert!(e.render().contains("(declare-const y Int)"));
    }

    #[test]
    fn string_literal_escaping() {
        let f = SmtTerm::eq(v("s"), SmtTerm::StrLit("a\"b".into()));
        let e = emit_single(&[("s", SortRef::Str)], &f);
        assert!(e.render().contains("\"a\"\"b\""));
    }

    #[test]
    fn nul_in_string_is_error() {
        let mut e = SmtLibEmitter::new();
        e.declare_var("s", SortRef::Str);
        e.assert(&Assertion {
            formula: SmtTerm::eq(v("s"), SmtTerm::StrLit("a\0b".into())),
            label: None,
        });
        assert!(e.translation_failed(), "NUL должен пометить translation_failed");
    }

    #[test]
    fn unsupported_op_sets_translation_failed() {
        let mut e = SmtLibEmitter::new();
        e.assert(&Assertion {
            formula: SmtTerm::App("__no_such_op__".into(), vec![SmtTerm::IntLit(1)]),
            label: None,
        });
        assert!(e.translation_failed());
    }

    #[test]
    fn bitvec_literal_and_ops() {
        let f = SmtTerm::eq(
            SmtTerm::App("bvadd".into(), vec![v("a"), SmtTerm::BitVecLit(1, 8)]),
            SmtTerm::BitVecLit(0, 8),
        );
        let e = emit_single(&[("a", SortRef::BitVec { width: 8, signed: false })], &f);
        let s = e.render();
        assert!(s.contains("(declare-const a (_ BitVec 8))"), "{}", s);
        assert!(s.contains("(bvadd a (_ bv1 8))"), "{}", s);
        assert!(s.contains("(_ bv0 8)"), "{}", s);
    }

    #[test]
    fn bv_overflow_signed_add_uses_width() {
        let f = SmtTerm::App("bvadd_no_overflow_s".into(), vec![v("a"), v("b")]);
        let e = emit_single(
            &[
                ("a", SortRef::BitVec { width: 32, signed: true }),
                ("b", SortRef::BitVec { width: 32, signed: true }),
            ],
            &f,
        );
        let s = e.render();
        assert!(s.contains("(_ bv0 32)"), "{}", s);
        assert!(s.contains("bvsge"), "{}", s);
        assert!(s.contains("(bvadd a b)"), "{}", s);
    }

    #[test]
    fn bv_overflow_unknown_width_errors() {
        // Операнды без BV-сорта → ширину не вывести → translation_failed.
        let f = SmtTerm::App("bvadd_no_overflow_s".into(), vec![v("a"), v("b")]);
        let mut e = SmtLibEmitter::new();
        e.declare_var("a", SortRef::Int);
        e.declare_var("b", SortRef::Int);
        e.assert(&Assertion { formula: f, label: None });
        assert!(e.translation_failed());
    }

    #[test]
    fn bv_mul_overflow_unsigned_extends() {
        let f = SmtTerm::App("bvmul_no_overflow_u".into(), vec![v("a"), v("b")]);
        let e = emit_single(
            &[
                ("a", SortRef::BitVec { width: 16, signed: false }),
                ("b", SortRef::BitVec { width: 16, signed: false }),
            ],
            &f,
        );
        let s = e.render();
        assert!(s.contains("(_ zero_extend 16)"), "{}", s);
        assert!(s.contains("(_ extract 31 16)"), "{}", s);
    }

    #[test]
    fn float_literal_bit_exact() {
        // 1.0f64 == 0x3FF0000000000000
        let f = SmtTerm::eq(v("d"), SmtTerm::F64Lit(1.0f64.to_bits()));
        let e = emit_single(&[("d", SortRef::F64)], &f);
        let s = e.render();
        assert!(s.contains("(fp #b0 #b01111111111 #b0000000000000000000000000000000000000000000000000000)"), "{}", s);
    }

    #[test]
    fn fp_predicate_name_mapping() {
        let f = SmtTerm::App("fp.is_nan".into(), vec![v("d")]);
        let e = emit_single(&[("d", SortRef::F64)], &f);
        assert!(e.render().contains("(fp.isNaN d)"));
    }

    #[test]
    fn fp_arith_inserts_rounding_mode() {
        let f = SmtTerm::eq(
            SmtTerm::App("fp.add".into(), vec![v("a"), v("b")]),
            v("c"),
        );
        let e = emit_single(
            &[("a", SortRef::F64), ("b", SortRef::F64), ("c", SortRef::F64)],
            &f,
        );
        assert!(e.render().contains("(fp.add RNE a b)"));
    }

    #[test]
    fn string_ops_name_mapping() {
        let f = SmtTerm::App("str.prefix".into(), vec![v("a"), v("b")]);
        let e = emit_single(&[("a", SortRef::Str), ("b", SortRef::Str)], &f);
        assert!(e.render().contains("(str.prefixof a b)"));

        let g = SmtTerm::App("str.concat".into(), vec![v("a"), v("b")]);
        let e2 = emit_single(&[("a", SortRef::Str), ("b", SortRef::Str)], &g);
        assert!(e2.render().contains("(str.++ a b)"));
    }

    #[test]
    fn forall_with_pattern() {
        // forall (id: Int) [_view_Db_balance(id)] . _view_Db_balance(id) >= 0
        let app = SmtTerm::App("_view_Db_balance".into(), vec![v("id")]);
        let body = SmtTerm::App(">=".into(), vec![app.clone(), SmtTerm::IntLit(0)]);
        let f = SmtTerm::Forall(
            vec![("id".into(), SortRef::Int)],
            vec![vec![app]],
            Box::new(body),
        );
        let mut e = SmtLibEmitter::new();
        e.assert(&Assertion { formula: f, label: None });
        let s = e.render();
        assert!(s.contains("(forall ((id Int))"), "{}", s);
        assert!(s.contains(":pattern (("), "{}", s);
        assert!(s.contains("(declare-fun _view_Db_balance (Int) Int)"), "{}", s);
    }

    #[test]
    fn forall_binder_not_auto_declared() {
        // Связанная переменная `id` не должна стать declare-const.
        let body = SmtTerm::App(">=".into(), vec![v("id"), SmtTerm::IntLit(0)]);
        let f = SmtTerm::Forall(vec![("id".into(), SortRef::Int)], vec![], Box::new(body));
        let mut e = SmtLibEmitter::new();
        e.assert(&Assertion { formula: f, label: None });
        let s = e.render();
        assert!(!s.contains("(declare-const id"), "binder leaked into declarations:\n{}", s);
    }

    #[test]
    fn field_access_fresh_const_and_axiom() {
        // _field_len_int(obj) — свежая константа + axiom неотрицательности.
        let f = SmtTerm::App(">=".into(), vec![
            SmtTerm::App("_field_len_int".into(), vec![v("obj")]),
            SmtTerm::IntLit(0),
        ]);
        let mut e = SmtLibEmitter::new();
        e.assert(&Assertion { formula: f, label: None });
        let s = e.render();
        assert!(s.contains("uf__field_len_int_0"), "{}", s);
        assert!(s.contains(">= uf__field_len_int_0 0"), "axiom missing:\n{}", s);
    }

    #[test]
    fn field_access_same_args_same_const() {
        // Две ссылки на _field_x(obj) с тем же obj → одна константа.
        let fx = SmtTerm::App("_field_x".into(), vec![v("obj")]);
        let f = SmtTerm::eq(fx.clone(), fx);
        let mut e = SmtLibEmitter::new();
        e.assert(&Assertion { formula: f, label: None });
        let s = e.render();
        let count = s.matches("declare-const uf__field_x").count();
        assert_eq!(count, 1, "ожидалась одна константа:\n{}", s);
    }

    #[test]
    fn push_pop_scopes_assertions() {
        let mut e = SmtLibEmitter::new();
        e.declare_var("x", SortRef::Int);
        e.assert(&Assertion {
            formula: SmtTerm::App(">".into(), vec![v("x"), SmtTerm::IntLit(0)]),
            label: None,
        });
        e.push();
        e.assert(&Assertion {
            formula: SmtTerm::App("<".into(), vec![v("x"), SmtTerm::IntLit(0)]),
            label: None,
        });
        assert!(e.render().contains("(< x 0)"));
        e.pop();
        let s = e.render();
        assert!(s.contains("(> x 0)"), "{}", s);
        assert!(!s.contains("(< x 0)"), "popped assertion leaked:\n{}", s);
    }

    #[test]
    fn declarations_survive_pop() {
        // Константа, впервые упомянутая в pushed scope, остаётся
        // объявленной после pop (как глобальный AST-узел в Z3).
        let mut e = SmtLibEmitter::new();
        e.push();
        e.assert(&Assertion {
            formula: SmtTerm::eq(v("scoped"), SmtTerm::IntLit(1)),
            label: None,
        });
        e.pop();
        assert!(e.render().contains("(declare-const scoped Int)"));
    }
}
