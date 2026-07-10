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
}

impl TypeSet {
    /// Проверить членство ЛИСТА (уже полностью решённого `ResolvedType`,
    /// без вложенных переменных) — членство переменной или композита с
    /// нерешённой переменной внутри не определено здесь (см.
    /// `Solution::member_of`, которая сперва резолвит терм).
    fn contains_resolved(&self, rt: &ResolvedType) -> bool {
        match self {
            TypeSet::Primitive => matches!(
                rt,
                ResolvedType::Scalar { .. }
                    | ResolvedType::Float { .. }
                    | ResolvedType::Bool
                    | ResolvedType::Str
                    | ResolvedType::Unit
            ) || matches!(rt, ResolvedType::Named { name, args, .. } if args.is_empty() && name == "char"),
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
        }
    }
}

/// Констрейнт, порождаемый генератором (будущие Ф.2-продюсеры), решаемый
/// `solve()`. Ровно два вида на старте (Ф.1) — покрывают равенство и
/// членство type-set; `Join`/`Project` (Binary-арифметика,
/// Index-проекция-в-элемент — см. Ф.0-инвентарь) остаются за следующей
/// волной, решатель сегодня расширяем без слома API (новый вариант enum).
#[derive(Debug, Clone, PartialEq)]
pub enum Constraint {
    /// Два терма должны унифицироваться в один тип.
    Eq(Ty, Ty),
    /// Терм должен принадлежать type-set (соблюдается ТОЛЬКО когда терм
    /// в итоге решается до листа без переменных — см. `member_of`).
    MemberOf(Ty, TypeSet),
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
}
