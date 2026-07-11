//! Plan 172.13 Ф.1 — constraint-based type inference core (scaffold).
//!
//! Мотивация (см. `docs/plans/172.13-constraint-inference.md`): инференс
//! Nova сегодня — зоопарк ad-hoc продюсеров канала (`f1_expr_inner` в
//! `types/mod.rs`), каждый новый КОНТЕКСТ = отдельная рука-написанная
//! ветка. Этот модуль — общее унификационное ядро, которым продюсеры
//! постепенно заменяются (Ф.2/Ф.3), а НЕ ещё один продюсер сам по себе.
//!
//! КЛЮЧЕВОЙ ПРИНЦИП: переменные — это СВЕЖИЕ числовые идентичности
//! (`TypeVar(u32)`), НЕ голые имена генериков. Класс дефектов d119
//! (загрязнение подстановки по совпадению ИМЕНИ — два разных generic `T`
//! в двух разных областях видимости случайно объединяются, потому что
//! решатель ключует по строке) становится невозможен ПО ПОСТРОЕНИЮ: две
//! переменные никогда не совпадают только оттого, что имеющийся
//! `ResolvedType::TypeParam("T")`-носитель делит написание в разных
//! функциях — генератор констрейнтов минтит новый `TypeVar` на каждый
//! РЕАЛЬНЫЙ generic-слот вызова/функции, а не переиспользует строку.
//!
//! Область этого модуля (Ф.1): типы констрейнтов + решатель (unify +
//! occurs-check + подстановка) + type-set членство (обобщение разбросанных
//! ad-hoc гейтов вроде `primitive_gate` / "sized non-wide scalar" / "все
//! generic-аргументы конкретны" в ОДИН язык предикатов). Продюсеры НЕ
//! подключены к этому ядру глобально в этой волне — миграция идёт пакетами
//! (Ф.2), гейтящимися byte-parity по пакету.

use super::ResolvedType;
use std::collections::HashMap;

/// Свежая числовая идентичность решателя — НЕ имя. Минтится генератором
/// констрейнтов (`next_var`), никогда не выводится из строки generic-имени
/// напрямую (см. doc модуля — принцип anti-d119).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TypeVar(pub u32);

/// Свежий генератор переменных — один на сессию генерации констрейнтов
/// (обычно на тело функции/выражение, которое сейчас решается).
#[derive(Debug, Default)]
pub struct VarGen(u32);

impl VarGen {
    pub fn new() -> Self {
        VarGen(0)
    }

    pub fn fresh(&mut self) -> TypeVar {
        let v = TypeVar(self.0);
        self.0 += 1;
        v
    }
}

/// Открытый тип-терм: как `ResolvedType`, но с решателевыми переменными в
/// ЛЮБОЙ позиции (в т.ч. вложенной — `Tuple`/`Named`-аргумент/`Func`
/// параметр). `ResolvedType` сам остаётся lossless-носителем финального
/// разрешённого типа (172.1 U.5.1) — `Ty` это его "открытая" надстройка
/// ТОЛЬКО для времени решения; после `solve()` каждый терм без свободных
/// переменных конвертируется обратно в `ResolvedType` через `Solution::
/// resolve_to_resolved_type`.
#[derive(Debug, Clone, PartialEq)]
pub enum Ty {
    /// Нерешённая переменная.
    Var(TypeVar),
    /// Полностью известный ЛИСТ — атом без внутренней структуры, которую
    /// решателю нужно раскрывать (примитивы, `Str`/`Bool`/`Unit`/… и т.п.).
    /// Композитные `ResolvedType` (Tuple/Named/Func/Array) представляются
    /// СТРУКТУРНО через варианты ниже, а не спрятаны здесь — иначе
    /// unify не смог бы заглянуть внутрь и связать вложенную переменную.
    Concrete(ResolvedType),
    Tuple(Vec<Ty>),
    Named { name: String, args: Vec<Ty> },
    Array(Box<Ty>),
    Func { params: Vec<Ty>, ret: Box<Ty> },
}

impl Ty {
    /// Поднять полностью конкретный (без переменных) `ResolvedType` в `Ty`,
    /// раскрывая композиты структурно, чтобы будущий `unify` мог смотреть
    /// внутрь них наравне с термами, содержащими переменные.
    pub fn from_resolved(rt: &ResolvedType) -> Ty {
        match rt {
            ResolvedType::Tuple(items) => {
                Ty::Tuple(items.iter().map(Ty::from_resolved).collect())
            }
            ResolvedType::Named { name, args, module } if module.is_empty() => Ty::Named {
                name: name.clone(),
                args: args.iter().map(Ty::from_resolved).collect(),
            },
            ResolvedType::Array(inner) => Ty::Array(Box::new(Ty::from_resolved(inner))),
            ResolvedType::Func { params, ret, effects } if effects.is_empty() => Ty::Func {
                params: params.iter().map(Ty::from_resolved).collect(),
                ret: Box::new(Ty::from_resolved(ret)),
            },
            other => Ty::Concrete(other.clone()),
        }
    }
}

/// Type-set членство — единый язык для гейтов, которые сегодня разбросаны
/// инлайн-булевыми выражениями по продюсерам (`primitive_gate`, "sized
/// scalar не wide-default", "все generic-аргументы конкретны"…). Ф.2
/// переводит один пакет продюсеров на эти предикаты вместо копий инлайн-
/// проверки в каждом сайте.
#[derive(Debug, Clone, PartialEq)]
pub enum TypeSet {
    /// Разделяемый гейт `primitive_gate`: Scalar/Float/Bool/Str/Unit/`char`.
    Primitive,
    /// Sized (НЕ wide-default `int`/`uint`) integer scalar — гейт
    /// literal-coercion семьи (172.1 [literal-coercion channel]): только
    /// сайзд-имя коэрсит литерал, `int`/`uint` — не-op относительно сида.
    SizedScalarNonWide,
    /// Scalar или Float (числовой операнд арифметики/сравнения).
    Numeric,
    /// `Named` с пустыми `args` (конкретное non-generic имя) — гейт
    /// "все generic-аргументы конкретны" (Some/Ok/Err ctor annotation,
    /// `annotate_expected_concrete`).
    ConcreteNamedNoArgs,
    /// Ф.2 (literal-coercion миграция, 2026-07-10): IntLit-коэрсия гейт —
    /// ЛЮБОЙ `Scalar`, КРОМЕ ровно `int` (width=64,signed=true,
    /// wide_default=true — сид литерала, D227 Rule 1). НЕ то же самое, что
    /// `SizedScalarNonWide`: `uint` (width=64,signed=false,wide_default=
    /// true) ПРОХОДИТ этот гейт (коэрсия int-сид→uint НЕ no-op — разный
    /// C-тип), но НЕ проходит `SizedScalarNonWide` (wide_default=true).
    /// Byte-parity ловушка — зафиксирована explicit-тестом, не обобщать.
    ScalarNotWideDefaultInt,
    /// Логическое ИЛИ вложенных type-set — язык для гейтов вида
    /// "primitive_gate(x) || concrete-named(x)", разбросанных инлайн по
    /// продюсерам до Ф.2.
    Union(Vec<TypeSet>),
}

impl TypeSet {
    /// Проверить членство ЛИСТА (уже полностью решённого `ResolvedType`,
    /// без вложенных переменных) — членство переменной или композита с
    /// нерешённой переменной внутри не определено здесь (см.
    /// `Solution::member_of`, которая сперва резолвит терм).
    fn contains_resolved(&self, rt: &ResolvedType) -> bool {
        match self {
            // `peel_view()` — тот же unwrap, что делает `primitive_gate`
            // (через `is_primitive_lowerable`) на живом пути; без него
            // `readonly int` не прошёл бы гейт, которым проходит на
            // существующих сайтах вызова.
            TypeSet::Primitive => {
                let peeled = rt.peel_view();
                matches!(
                    peeled,
                    ResolvedType::Scalar { .. }
                        | ResolvedType::Float { .. }
                        | ResolvedType::Bool
                        | ResolvedType::Str
                        | ResolvedType::Unit
                ) || matches!(peeled, ResolvedType::Named { name, args, .. }
                    if args.is_empty() && name == "char")
            }
            TypeSet::SizedScalarNonWide => matches!(
                rt,
                ResolvedType::Scalar { wide_default: false, .. }
            ),
            TypeSet::Numeric => {
                matches!(rt, ResolvedType::Scalar { .. } | ResolvedType::Float { .. })
            }
            TypeSet::ConcreteNamedNoArgs => {
                matches!(rt, ResolvedType::Named { args, .. } if args.is_empty())
            }
            TypeSet::ScalarNotWideDefaultInt => {
                matches!(rt, ResolvedType::Scalar { .. })
                    && !matches!(
                        rt,
                        ResolvedType::Scalar { width: 64, signed: true, wide_default: true }
                    )
            }
            TypeSet::Union(sets) => sets.iter().any(|s| s.contains_resolved(rt)),
        }
    }
}

/// Констрейнт, порождаемый генератором (будущие Ф.2-продюсеры), решаемый
/// `solve()`. `Eq`/`MemberOf` — равенство и членство type-set (Ф.1). `Join`
/// (Plan 196 Ф.4a, Binary-арифметика) — слияние двух известных типов в один
/// результирующий по КАНОНИЧЕСКОЙ семантике `number_exprs::promote_arith_rt`
/// (§0 — правило живёт в ОДНОМ месте, `Join` его лишь ВЫЗЫВАЕТ, а не
/// переизобретает). `Project` (Index-проекция-в-элемент — см. Ф.0-инвентарь)
/// остаётся за следующей волной; решатель расширяем без слома API.
#[derive(Debug, Clone, PartialEq)]
pub enum Constraint {
    /// Два терма должны унифицироваться в один тип.
    Eq(Ty, Ty),
    /// Терм должен принадлежать type-set (соблюдается ТОЛЬКО когда терм
    /// в итоге решается до листа без переменных — см. `member_of`).
    MemberOf(Ty, TypeSet),
    /// `out` = арифметическое слияние (`⋈`) операндов `left`/`right` по
    /// §0-каноническому `number_exprs::promote_arith_rt`. Решается ТОЛЬКО
    /// когда ОБА операнда сводятся к листу без переменных И слияние
    /// определено (`join_arith`): либо (a) оба numeric-листа → promote, либо
    /// (b) один и тот же `TypeParam` с обеих сторон → тот же `TypeParam`
    /// (арифметика сохраняет numeric-bounded generic; D405 запрещает
    /// mixed-width). Иначе `out` остаётся несвязанным (недоопределено →
    /// продюсер уходит на legacy — честный «без аннотации», не ложь). Гейт
    /// «операнды numeric / bounded» остаётся у ПРОДЮСЕРА (контекстное знание
    /// чекера), как AST-обход остался у продюсера в Ф.2 literal-coercion.
    Join { out: Ty, left: Ty, right: Ty },
}

/// Причина отказа решателя.
#[derive(Debug, Clone, PartialEq)]
pub enum Conflict {
    /// Два несовместимых конкретных типа (разное имя/форма/примитив).
    Mismatch { left: Ty, right: Ty },
    /// Переменная встречает саму себя внутри терма, с которым её пытаются
    /// связать (бесконечный тип) — `unify(Var(a), Tuple[Var(a)])`.
    Occurs { var: TypeVar, in_ty: Ty },
    /// `MemberOf`, проверенный на решённый лист, провалился.
    NotAMember { ty: Ty, set: TypeSet },
}

/// Решатель: копит подстановку `TypeVar -> Ty` по мере обработки `Eq`.
#[derive(Debug, Default)]
pub struct Solver {
    subst: HashMap<TypeVar, Ty>,
}

impl Solver {
    pub fn new() -> Self {
        Solver { subst: HashMap::new() }
    }

    /// Решить пакет констрейнтов целиком (порядок: сначала все `Eq` —
    /// строят подстановку, — затем все `MemberOf` против финально решённых
    /// термов). Первый конфликт останавливает решение (fail-fast — Ф.1
    /// scaffold; пакетная диагностика с несколькими конфликтами — при
    /// необходимости в Ф.2/Ф.3).
    pub fn solve(mut self, constraints: &[Constraint]) -> Result<Solution, Conflict> {
        for c in constraints {
            if let Constraint::Eq(a, b) = c {
                self.unify(a, b)?;
            }
        }
        // Join AFTER Eq (operands must be resolved through the substitution
        // built by Eq) and BEFORE MemberOf (so `out` is bound for any later
        // membership check). Only binds `out` when BOTH operands collapse to
        // a variable-free leaf AND `join_arith` yields a result — otherwise
        // `out` stays free (undetermined, not a conflict).
        for c in constraints {
            if let Constraint::Join { out, left, right } = c {
                let l = self.resolve(left);
                let r = self.resolve(right);
                if let (Some(lrt), Some(rrt)) =
                    (self.as_concrete_leaf(&l), self.as_concrete_leaf(&r))
                {
                    if let Some(joined) = join_arith(&lrt, &rrt) {
                        self.unify(out, &Ty::Concrete(joined))?;
                    }
                }
            }
        }
        for c in constraints {
            if let Constraint::MemberOf(t, set) = c {
                let resolved = self.resolve(t);
                if !matches!(resolved, Ty::Var(_)) {
                    match self.as_concrete_leaf(&resolved) {
                        Some(rt) if set.contains_resolved(&rt) => {}
                        Some(_) => {
                            return Err(Conflict::NotAMember { ty: resolved, set: set.clone() });
                        }
                        // Композит без листа (напр. Tuple с ещё-переменной
                        // внутри) — недостаточно информации, не фатально
                        // (зеркалит поведение сегодняшних ad-hoc гейтов:
                        // недоказанное членство = "без аннотации", не ложь).
                        None => {}
                    }
                }
            }
        }
        Ok(Solution { subst: self.subst })
    }

    /// Унифицировать два терма, попутно наполняя подстановку.
    fn unify(&mut self, a: &Ty, b: &Ty) -> Result<(), Conflict> {
        let a = self.resolve(a);
        let b = self.resolve(b);
        match (&a, &b) {
            (Ty::Var(va), Ty::Var(vb)) if va == vb => Ok(()),
            (Ty::Var(v), other) | (other, Ty::Var(v)) => {
                if Self::occurs(*v, other) {
                    return Err(Conflict::Occurs { var: *v, in_ty: other.clone() });
                }
                self.subst.insert(*v, other.clone());
                Ok(())
            }
            (Ty::Concrete(r1), Ty::Concrete(r2)) => {
                if r1 == r2 {
                    Ok(())
                } else {
                    Err(Conflict::Mismatch { left: a.clone(), right: b.clone() })
                }
            }
            (Ty::Tuple(xs), Ty::Tuple(ys)) if xs.len() == ys.len() => {
                for (x, y) in xs.iter().zip(ys.iter()) {
                    self.unify(x, y)?;
                }
                Ok(())
            }
            (Ty::Named { name: n1, args: a1 }, Ty::Named { name: n2, args: a2 })
                if n1 == n2 && a1.len() == a2.len() =>
            {
                for (x, y) in a1.iter().zip(a2.iter()) {
                    self.unify(x, y)?;
                }
                Ok(())
            }
            (Ty::Array(i1), Ty::Array(i2)) => self.unify(i1, i2),
            (
                Ty::Func { params: p1, ret: r1 },
                Ty::Func { params: p2, ret: r2 },
            ) if p1.len() == p2.len() => {
                for (x, y) in p1.iter().zip(p2.iter()) {
                    self.unify(x, y)?;
                }
                self.unify(r1, r2)
            }
            _ => Err(Conflict::Mismatch { left: a.clone(), right: b.clone() }),
        }
    }

    /// occurs-check: истина, если `v` встречается где-либо внутри `ty`
    /// (после применения ТЕКУЩЕЙ подстановки — так `unify(Var(a),
    /// Tuple[Var(b)])` с уже известным `b == Var(a)` тоже ловится).
    fn occurs(v: TypeVar, ty: &Ty) -> bool {
        match ty {
            Ty::Var(v2) => *v2 == v,
            Ty::Concrete(_) => false,
            Ty::Tuple(items) => items.iter().any(|t| Self::occurs(v, t)),
            Ty::Named { args, .. } => args.iter().any(|t| Self::occurs(v, t)),
            Ty::Array(inner) => Self::occurs(v, inner),
            Ty::Func { params, ret } => {
                params.iter().any(|t| Self::occurs(v, t)) || Self::occurs(v, ret)
            }
        }
    }

    /// Полностью применить текущую подстановку к терму (рекурсивно, с
    /// защитой от циклов — occurs-check в `unify` не даёт им возникнуть,
    /// но при обрыве цепочки на неполном решении лучше остановиться, чем
    /// зациклиться).
    fn resolve(&self, ty: &Ty) -> Ty {
        self.resolve_depth(ty, 0)
    }

    fn resolve_depth(&self, ty: &Ty, depth: u32) -> Ty {
        if depth > 256 {
            return ty.clone();
        }
        match ty {
            Ty::Var(v) => match self.subst.get(v) {
                Some(bound) => self.resolve_depth(bound, depth + 1),
                None => ty.clone(),
            },
            Ty::Concrete(_) => ty.clone(),
            Ty::Tuple(items) => {
                Ty::Tuple(items.iter().map(|t| self.resolve_depth(t, depth)).collect())
            }
            Ty::Named { name, args } => Ty::Named {
                name: name.clone(),
                args: args.iter().map(|t| self.resolve_depth(t, depth)).collect(),
            },
            Ty::Array(inner) => Ty::Array(Box::new(self.resolve_depth(inner, depth))),
            Ty::Func { params, ret } => Ty::Func {
                params: params.iter().map(|t| self.resolve_depth(t, depth)).collect(),
                ret: Box::new(self.resolve_depth(ret, depth)),
            },
        }
    }

    /// Если полностью решённый терм не содержит переменных, свернуть его
    /// обратно в один `ResolvedType` лист (для `TypeSet` членства).
    fn as_concrete_leaf(&self, ty: &Ty) -> Option<ResolvedType> {
        match ty {
            Ty::Concrete(rt) => Some(rt.clone()),
            Ty::Var(_) => None,
            Ty::Tuple(items) => {
                let items: Option<Vec<ResolvedType>> =
                    items.iter().map(|t| self.as_concrete_leaf(t)).collect();
                items.map(ResolvedType::Tuple)
            }
            Ty::Named { name, args } => {
                let args: Option<Vec<ResolvedType>> =
                    args.iter().map(|t| self.as_concrete_leaf(t)).collect();
                args.map(|args| ResolvedType::Named { name: name.clone(), module: vec![], args })
            }
            Ty::Array(inner) => self
                .as_concrete_leaf(inner)
                .map(|i| ResolvedType::Array(Box::new(i))),
            Ty::Func { params, ret } => {
                let params: Option<Vec<ResolvedType>> =
                    params.iter().map(|t| self.as_concrete_leaf(t)).collect();
                let ret = self.as_concrete_leaf(ret)?;
                params.map(|params| ResolvedType::Func {
                    params,
                    ret: Box::new(ret),
                    effects: vec![],
                })
            }
        }
    }
}

/// Результат успешного решения — подстановка, доступная генератору
/// констрейнтов для материализации итоговых типов в канал
/// (`resolved_types_buf`, Ф.2).
#[derive(Debug)]
pub struct Solution {
    subst: HashMap<TypeVar, Ty>,
}

impl Solution {
    /// Итоговый тип переменной, если он полностью решён (без остаточных
    /// переменных) — `None` означает "недоопределено", честный аналог
    /// сегодняшнего "без аннотации → legacy-навигация".
    pub fn type_of(&self, v: TypeVar) -> Option<ResolvedType> {
        let solver = Solver { subst: self.subst.clone() };
        let resolved = solver.resolve(&Ty::Var(v));
        solver.as_concrete_leaf(&resolved)
    }
}

/// Numeric-операнд арифметики — ТОЧНАЯ копия предиката `is_num` продюсера
/// Binary-арма (`types/mod.rs`): Scalar / Float / голый `char`. Шире, чем
/// `TypeSet::Numeric` (тот без `char`) — умышленно: `char` арифметически
/// promotable (`is_typed_int` в `number_exprs` его включает), и byte-parity
/// Binary-арма требует именно этот набор.
fn is_numeric_leaf(rt: &ResolvedType) -> bool {
    matches!(rt, ResolvedType::Scalar { .. } | ResolvedType::Float { .. })
        || matches!(rt, ResolvedType::Named { name, args, .. }
            if args.is_empty() && name.as_str() == "char")
}

/// Ядро `Constraint::Join`: слить два УЖЕ РЕШЁННЫХ листа в результирующий
/// тип по семантике арифметики Nova. Возвращает `None`, когда слияние не
/// определено (продюсер тогда не аннотирует → legacy). Правило:
///   1. один и тот же `TypeParam` с обеих сторон → тот же `TypeParam`
///      (арифметика сохраняет numeric-bounded generic; проверка bounded —
///      у продюсера, сюда доходят только уже-гейченные пары);
///   2. оба numeric-листа → §0-канонический `number_exprs::promote_arith_rt`
///      (правило НЕ дублируется — единственный источник).
/// Не-numeric / разные TypeParam / numeric+TypeParam → `None` (никогда не
/// штампуем ложный тип для operator-overload операндов — анти-POISON-6875).
fn join_arith(l: &ResolvedType, r: &ResolvedType) -> Option<ResolvedType> {
    if let (ResolvedType::TypeParam(a), ResolvedType::TypeParam(b)) = (l, r) {
        if a == b {
            return Some(ResolvedType::TypeParam(a.clone()));
        }
        return None;
    }
    if is_numeric_leaf(l) && is_numeric_leaf(r) {
        return Some(crate::number_exprs::promote_arith_rt(l, r));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scalar(width: u8, signed: bool, wide_default: bool) -> ResolvedType {
        ResolvedType::Scalar { width, signed, wide_default }
    }

    #[test]
    fn unify_equal_concrete_ok() {
        let a = Ty::Concrete(ResolvedType::Bool);
        let b = Ty::Concrete(ResolvedType::Bool);
        let sol = Solver::new().solve(&[Constraint::Eq(a, b)]);
        assert!(sol.is_ok());
    }

    #[test]
    fn unify_conflicting_concrete_is_mismatch() {
        let a = Ty::Concrete(ResolvedType::Bool);
        let b = Ty::Concrete(ResolvedType::Str);
        let err = Solver::new().solve(&[Constraint::Eq(a, b)]).unwrap_err();
        assert!(matches!(err, Conflict::Mismatch { .. }));
    }

    #[test]
    fn var_binds_to_concrete_and_is_queryable() {
        let mut g = VarGen::new();
        let v = g.fresh();
        let u8_ty = scalar(8, false, false);
        let sol = Solver::new()
            .solve(&[Constraint::Eq(Ty::Var(v), Ty::Concrete(u8_ty.clone()))])
            .unwrap();
        assert_eq!(sol.type_of(v), Some(u8_ty));
    }

    #[test]
    fn transitive_chain_var_a_eq_var_b_eq_concrete() {
        let mut g = VarGen::new();
        let a = g.fresh();
        let b = g.fresh();
        let u32_ty = scalar(32, false, false);
        let sol = Solver::new()
            .solve(&[
                Constraint::Eq(Ty::Var(a), Ty::Var(b)),
                Constraint::Eq(Ty::Var(b), Ty::Concrete(u32_ty.clone())),
            ])
            .unwrap();
        // `a` резолвится ЧЕРЕЗ `b` — доказывает, что решатель следует
        // цепочке переменная→переменная→конкретный тип, а не только
        // прямому единичному связыванию.
        assert_eq!(sol.type_of(a), Some(u32_ty.clone()));
        assert_eq!(sol.type_of(b), Some(u32_ty));
    }

    #[test]
    fn transitive_chain_reverse_order_still_resolves() {
        // Порядок обратный предыдущему тесту — решатель не должен зависеть
        // от порядка поступления констрейнтов в пакете.
        let mut g = VarGen::new();
        let a = g.fresh();
        let b = g.fresh();
        let bool_ty = ResolvedType::Bool;
        let sol = Solver::new()
            .solve(&[
                Constraint::Eq(Ty::Var(b), Ty::Concrete(bool_ty.clone())),
                Constraint::Eq(Ty::Var(a), Ty::Var(b)),
            ])
            .unwrap();
        assert_eq!(sol.type_of(a), Some(bool_ty));
    }

    #[test]
    fn occurs_check_rejects_self_referential_tuple() {
        let mut g = VarGen::new();
        let a = g.fresh();
        let self_ref = Ty::Tuple(vec![Ty::Var(a)]);
        let err = Solver::new()
            .solve(&[Constraint::Eq(Ty::Var(a), self_ref)])
            .unwrap_err();
        assert!(matches!(err, Conflict::Occurs { .. }));
    }

    #[test]
    fn occurs_check_rejects_indirect_cycle_through_named() {
        // a = Vec[b], b = Vec[a] — no direct self-reference in a single
        // constraint, but the SECOND unify must catch the cycle once `a`
        // is already bound (occurs-check reads through the substitution).
        let mut g = VarGen::new();
        let a = g.fresh();
        let b = g.fresh();
        let vec_of = |inner: Ty| Ty::Named { name: "Vec".to_string(), args: vec![inner] };
        let mut solver = Solver::new();
        solver.unify(&Ty::Var(a), &vec_of(Ty::Var(b))).unwrap();
        let err = solver.unify(&Ty::Var(b), &vec_of(Ty::Var(a))).unwrap_err();
        assert!(matches!(err, Conflict::Occurs { .. }));
    }

    #[test]
    fn structural_unify_named_binds_nested_var() {
        let mut g = VarGen::new();
        let elem = g.fresh();
        let u8_ty = scalar(8, false, false);
        let vec_var = Ty::Named { name: "Vec".to_string(), args: vec![Ty::Var(elem)] };
        let vec_concrete = Ty::Named {
            name: "Vec".to_string(),
            args: vec![Ty::Concrete(u8_ty.clone())],
        };
        let sol = Solver::new()
            .solve(&[Constraint::Eq(vec_var, vec_concrete)])
            .unwrap();
        assert_eq!(sol.type_of(elem), Some(u8_ty));
    }

    #[test]
    fn structural_mismatch_different_named_is_conflict() {
        let vec_ty = Ty::Named { name: "Vec".to_string(), args: vec![] };
        let opt_ty = Ty::Named { name: "Option".to_string(), args: vec![] };
        let err = Solver::new()
            .solve(&[Constraint::Eq(vec_ty, opt_ty)])
            .unwrap_err();
        assert!(matches!(err, Conflict::Mismatch { .. }));
    }

    #[test]
    fn member_of_sized_scalar_non_wide_accepts_u8_rejects_wide_int() {
        let mut g = VarGen::new();
        let a = g.fresh();
        let u8_ty = scalar(8, false, false);
        let ok = Solver::new().solve(&[
            Constraint::Eq(Ty::Var(a), Ty::Concrete(u8_ty)),
            Constraint::MemberOf(Ty::Var(a), TypeSet::SizedScalarNonWide),
        ]);
        assert!(ok.is_ok());

        let mut g2 = VarGen::new();
        let b = g2.fresh();
        let wide_int = scalar(64, true, true);
        let bad = Solver::new().solve(&[
            Constraint::Eq(Ty::Var(b), Ty::Concrete(wide_int)),
            Constraint::MemberOf(Ty::Var(b), TypeSet::SizedScalarNonWide),
        ]);
        assert!(matches!(bad.unwrap_err(), Conflict::NotAMember { .. }));
    }

    #[test]
    fn member_of_unresolved_var_is_not_fatal() {
        // Недоопределённая переменная (без Eq-связывания) — MemberOf не
        // должен паниковать/конфликтовать: честное "недоказано", зеркалит
        // сегодняшнее поведение ad-hoc гейтов ("без аннотации → legacy").
        let mut g = VarGen::new();
        let a = g.fresh();
        let sol = Solver::new()
            .solve(&[Constraint::MemberOf(Ty::Var(a), TypeSet::Primitive)])
            .unwrap();
        assert_eq!(sol.type_of(a), None);
    }

    #[test]
    fn concrete_named_no_args_set() {
        assert!(TypeSet::ConcreteNamedNoArgs.contains_resolved(&ResolvedType::Named {
            name: "Foo".into(),
            module: vec![],
            args: vec![],
        }));
        assert!(!TypeSet::ConcreteNamedNoArgs.contains_resolved(&ResolvedType::Named {
            name: "Vec".into(),
            module: vec![],
            args: vec![ResolvedType::Bool],
        }));
    }

    #[test]
    fn from_resolved_round_trips_through_as_concrete_leaf() {
        let rt = ResolvedType::Tuple(vec![ResolvedType::Bool, scalar(32, true, false)]);
        let ty = Ty::from_resolved(&rt);
        let solver = Solver::new();
        assert_eq!(solver.as_concrete_leaf(&ty), Some(rt));
    }

    fn ts_ok(rt: &ResolvedType, set: TypeSet) -> bool {
        Solver::new()
            .solve(&[Constraint::MemberOf(Ty::Concrete(rt.clone()), set)])
            .is_ok()
    }

    /// Ф.2 byte-parity ловушка (см. doc-комментарий на
    /// `ScalarNotWideDefaultInt`): исходный ad-hoc гейт в
    /// `materialize_literal_coercion` исключал ТОЛЬКО ровно `int`
    /// (signed wide-default), а НЕ `uint` (unsigned wide-default) — эта
    /// асимметрия должна пережить миграцию на TypeSet.
    #[test]
    fn scalar_not_wide_default_int_excludes_only_signed_wide_default() {
        let int_ty = scalar(64, true, true); // `int` — сид литерала, excluded
        let uint_ty = scalar(64, false, true); // `uint` — NOT excluded
        let u8_ty = scalar(8, false, false);
        assert!(!ts_ok(&int_ty, TypeSet::ScalarNotWideDefaultInt));
        assert!(ts_ok(&uint_ty, TypeSet::ScalarNotWideDefaultInt));
        assert!(ts_ok(&u8_ty, TypeSet::ScalarNotWideDefaultInt));
        assert!(!ts_ok(&ResolvedType::Bool, TypeSet::ScalarNotWideDefaultInt));
    }

    #[test]
    fn union_is_true_if_any_member_set_matches() {
        let rt = ResolvedType::Named { name: "Foo".into(), module: vec![], args: vec![] };
        assert!(ts_ok(
            &rt,
            TypeSet::Union(vec![TypeSet::Primitive, TypeSet::ConcreteNamedNoArgs])
        ));
        assert!(!ts_ok(
            &ResolvedType::Named { name: "Vec".into(), module: vec![], args: vec![ResolvedType::Bool] },
            TypeSet::Union(vec![TypeSet::Primitive, TypeSet::ConcreteNamedNoArgs])
        ));
    }

    #[test]
    fn primitive_set_peels_readonly_view() {
        let ro_int = ResolvedType::Readonly(Box::new(scalar(64, true, true)));
        assert!(ts_ok(&ro_int, TypeSet::Primitive));
    }

    // ── Plan 196 Ф.4a — `Constraint::Join` (Binary-арифметика) ────────────

    /// Прогнать один Join и вернуть решённый тип `out` (или `None`, если
    /// решатель оставил его несвязанным — недоопределённое слияние).
    fn join(l: &ResolvedType, r: &ResolvedType) -> Option<ResolvedType> {
        let mut g = VarGen::new();
        let out = g.fresh();
        Solver::new()
            .solve(&[Constraint::Join {
                out: Ty::Var(out),
                left: Ty::from_resolved(l),
                right: Ty::from_resolved(r),
            }])
            .ok()
            .and_then(|sol| sol.type_of(out))
    }

    fn char_ty() -> ResolvedType {
        ResolvedType::Named { name: "char".into(), module: vec![], args: vec![] }
    }

    #[test]
    fn join_f64_wins_over_int() {
        // f64 ⋈ i32 → f64 (float-арифметика доминирует, позиция не важна).
        let f64_ty = ResolvedType::Float { width: 64 };
        let i32_ty = scalar(32, true, false);
        assert_eq!(join(&f64_ty, &i32_ty), Some(f64_ty.clone()));
        assert_eq!(join(&i32_ty, &f64_ty), Some(f64_ty));
    }

    #[test]
    fn join_typed_int_beats_wide_int_position_independent() {
        // u8 ⋈ int → u8 и int ⋈ u8 → u8 (typed/narrow бьёт wide-default int
        // НЕЗАВИСИМО от позиции — ключевой RANK-1 инвариант promote_arith_rt).
        let u8_ty = scalar(8, false, false);
        let int_ty = scalar(64, true, true);
        assert_eq!(join(&u8_ty, &int_ty), Some(u8_ty.clone()));
        assert_eq!(join(&int_ty, &u8_ty), Some(u8_ty));
    }

    #[test]
    fn join_uint_beats_wide_int() {
        // uint (wide-default unsigned) бьёт int в смешанной арифметике
        // (1 + uint_n → uint) — `is_typed_int` включает uint (Plan 172.1-K2).
        let uint_ty = scalar(64, false, true);
        let int_ty = scalar(64, true, true);
        assert_eq!(join(&uint_ty, &int_ty), Some(uint_ty.clone()));
        assert_eq!(join(&int_ty, &uint_ty), Some(uint_ty));
    }

    #[test]
    fn join_both_wide_int_takes_left() {
        // int ⋈ int → int (оба wide-default → левый, как promote_arith_rt).
        let int_ty = scalar(64, true, true);
        assert_eq!(join(&int_ty, &int_ty), Some(int_ty));
    }

    #[test]
    fn join_char_is_typed_int() {
        // char ⋈ int → char (char — typed int в promote_arith_rt).
        let int_ty = scalar(64, true, true);
        assert_eq!(join(&char_ty(), &int_ty), Some(char_ty()));
    }

    #[test]
    fn join_same_type_param_preserved() {
        // T ⋈ T → T (арифметика сохраняет numeric-bounded generic).
        let t = ResolvedType::TypeParam("T".into());
        assert_eq!(join(&t, &t), Some(t.clone()));
    }

    #[test]
    fn join_different_type_params_undetermined() {
        // T ⋈ U → None (разные параметры — не сливаем; продюсер уходит на legacy).
        let t = ResolvedType::TypeParam("T".into());
        let u = ResolvedType::TypeParam("U".into());
        assert_eq!(join(&t, &u), None);
    }

    #[test]
    fn join_type_param_with_numeric_undetermined() {
        // T ⋈ i32 → None: TypeParam+конкретный numeric НЕ сливается ядром
        // (случай «T + литерал» продюсер моделирует как T ⋈ T, коэрсируя
        // литерал к типу параметра, — а НЕ передаёт сюда смешанную пару).
        let t = ResolvedType::TypeParam("T".into());
        let i32_ty = scalar(32, true, false);
        assert_eq!(join(&t, &i32_ty), None);
        assert_eq!(join(&i32_ty, &t), None);
    }

    #[test]
    fn join_non_numeric_operands_never_promote() {
        // Record ⋈ Record → None: анти-POISON-6875 — оператор-overload
        // операнды (`Vec + Vec` → `@plus`) НИКОГДА не получают ложный
        // promote-штамп (иначе канал лжёт, требуя недоверия потребителя).
        let rec = ResolvedType::Named { name: "Vector".into(), module: vec![], args: vec![] };
        assert_eq!(join(&rec, &rec), None);
        let i32_ty = scalar(32, true, false);
        assert_eq!(join(&rec, &i32_ty), None);
    }

    #[test]
    fn join_out_var_is_queryable_through_eq_chain() {
        // `out` связывается через unify — доказываем, что решение доступно
        // и транзитивно (Eq(a, out) до Join → `a` тоже резолвится в результат).
        let mut g = VarGen::new();
        let out = g.fresh();
        let a = g.fresh();
        let u16_ty = scalar(16, false, false);
        let int_ty = scalar(64, true, true);
        let sol = Solver::new()
            .solve(&[
                Constraint::Eq(Ty::Var(a), Ty::Var(out)),
                Constraint::Join {
                    out: Ty::Var(out),
                    left: Ty::from_resolved(&u16_ty),
                    right: Ty::from_resolved(&int_ty),
                },
            ])
            .unwrap();
        assert_eq!(sol.type_of(out), Some(u16_ty.clone()));
        assert_eq!(sol.type_of(a), Some(u16_ty));
    }
}
