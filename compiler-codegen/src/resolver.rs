//! Plan 03.2 Ф.3 — backtracking version resolver.
//!
//! Подбирает согласованный набор версий для (транзитивного) дерева
//! зависимостей: каждому пакету — одну конкретную версию так, чтобы
//! выполнялись все semver-ограничения.
//!
//! **Почему недостаточно «max на каждую зависимость».** Версия `A@1.5`
//! может требовать `C ^1.0`, а `A@1.6` — `C ^2.0`: выбор версии `A`
//! меняет ограничения на `C`. Нужен **backtracking**.
//!
//! **Алгоритм** — корректный backtracking-резолвер: DFS по пакетам,
//! highest-version-first, распространение ограничений (выбранная
//! версия добавляет ограничения своих deps), откат при конфликте.
//! Корректен и полон: находит решение, если оно существует, иначе —
//! диагностируемый конфликт. Полный PubGrub (CDCL — обучение на
//! конфликтах) — оптимизация скорости/explanation для больших графов,
//! followup registry-эры (Plan 03.2 §3.3 / §6).
//!
//! **`DependencyProvider`** абстрагирует источник версий и deps —
//! resolver не зависит от git/registry. В Plan 03.2 источник —
//! git-теги (Ф.3 git-backed provider); Plan 03.3 подключит registry
//! без изменения resolver'а.

use crate::semver::{Version, VersionReq};
use std::collections::HashMap;

/// Идентификатор пакета — стабильный ключ (для git-deps — URL).
pub type PkgId = String;

/// Источник версий и зависимостей пакета.
pub trait DependencyProvider {
    /// Все доступные версии пакета (порядок любой — resolver сортирует).
    /// `Err` — пакет недоступен (фатально, без backtracking).
    fn available_versions(&self, pkg: &PkgId) -> Result<Vec<Version>, String>;

    /// Зависимости конкретной версии пакета: `(PkgId, VersionReq)`.
    fn dependencies(
        &self,
        pkg: &PkgId,
        ver: &Version,
    ) -> Result<Vec<(PkgId, VersionReq)>, String>;

    /// Человекочитаемое имя пакета для диагностики. По умолчанию — id.
    fn display_name(&self, pkg: &PkgId) -> String {
        pkg.clone()
    }
}

/// Результат резолва — выбранная версия каждого пакета дерева.
#[derive(Debug, Clone)]
pub struct Resolution {
    pub selected: HashMap<PkgId, Version>,
}

/// Ошибка резолва.
#[derive(Debug)]
pub enum ResolveError {
    /// Провайдер не смог отдать версии/deps — фатально.
    Provider(String),
    /// Согласованного набора версий не существует.
    Conflict(String),
}

impl std::fmt::Display for ResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResolveError::Provider(m) => write!(f, "{}", m),
            ResolveError::Conflict(m) => write!(f, "{}", m),
        }
    }
}

/// Одно ограничение на пакет — диапазон + происхождение (для диагностики).
#[derive(Debug, Clone)]
struct Constraint {
    req: VersionReq,
    /// Кто наложил ограничение («корень» либо `<пакет> <версия>`).
    source: String,
}

/// Подобрать согласованный набор версий для корневых зависимостей.
///
/// `root_deps` — прямые зависимости резолвимого пакета.
pub fn resolve<P: DependencyProvider>(
    provider: &P,
    root_deps: &[(PkgId, VersionReq)],
) -> Result<Resolution, ResolveError> {
    let mut constraints: HashMap<PkgId, Vec<Constraint>> = HashMap::new();
    for (pkg, req) in root_deps {
        constraints.entry(pkg.clone()).or_default().push(Constraint {
            req: req.clone(),
            source: "корень".to_string(),
        });
    }
    let selected = HashMap::new();
    let sel = solve(provider, constraints, selected)?;
    Ok(Resolution { selected: sel })
}

/// Рекурсивный шаг DFS-backtracking. `constraints` / `selected`
/// передаются по значению — каждая ветка получает свежую копию
/// (откат бесплатен, без ручного undo).
fn solve<P: DependencyProvider>(
    provider: &P,
    constraints: HashMap<PkgId, Vec<Constraint>>,
    selected: HashMap<PkgId, Version>,
) -> Result<HashMap<PkgId, Version>, ResolveError> {
    // Выбрать ещё не решённый пакет. Детерминированный порядок (sorted)
    // — воспроизводимость резолва.
    let mut pending: Vec<&PkgId> = constraints
        .keys()
        .filter(|k| !selected.contains_key(*k))
        .collect();
    pending.sort();
    let Some(pkg) = pending.first().cloned() else {
        // Все пакеты решены — все ограничения выполнены (проверялись
        // при каждом добавлении). Решение найдено.
        return Ok(selected);
    };
    let pkg = pkg.clone();

    let reqs = &constraints[&pkg];
    // Кандидаты: версии пакета, удовлетворяющие ВСЕМ его ограничениям,
    // по убыванию (highest-version-first).
    let mut versions = provider
        .available_versions(&pkg)
        .map_err(ResolveError::Provider)?;
    versions.sort();
    versions.reverse();
    let candidates: Vec<Version> = versions
        .into_iter()
        .filter(|v| reqs.iter().all(|c| c.req.matches(v)))
        .collect();

    if candidates.is_empty() {
        return Err(ResolveError::Conflict(explain_no_version(provider, &pkg, reqs)));
    }

    let mut last_conflict: Option<String> = None;
    for cand in candidates {
        // Пробуем pkg = cand.
        let mut next_selected = selected.clone();
        next_selected.insert(pkg.clone(), cand.clone());

        let deps = match provider.dependencies(&pkg, &cand) {
            Ok(d) => d,
            Err(e) => return Err(ResolveError::Provider(e)),
        };

        // Добавляем ограничения от deps выбранной версии.
        let mut next_constraints = constraints.clone();
        let src = format!("{} {}", provider.display_name(&pkg), cand);
        let mut violates_existing = false;
        for (dpkg, dreq) in deps {
            // Уже выбранный пакет не должен нарушить новое ограничение.
            if let Some(chosen) = next_selected.get(&dpkg) {
                if !dreq.matches(chosen) {
                    last_conflict = Some(format!(
                        "версия `{}` пакета `{}` требует `{} {}`, но `{}` \
                         уже зафиксирован на `{}`",
                        cand,
                        provider.display_name(&pkg),
                        provider.display_name(&dpkg),
                        dreq,
                        provider.display_name(&dpkg),
                        chosen,
                    ));
                    violates_existing = true;
                    break;
                }
            }
            next_constraints
                .entry(dpkg)
                .or_default()
                .push(Constraint { req: dreq, source: src.clone() });
        }
        if violates_existing {
            continue; // backtrack — следующий кандидат
        }

        match solve(provider, next_constraints, next_selected) {
            Ok(res) => return Ok(res),
            Err(ResolveError::Provider(m)) => return Err(ResolveError::Provider(m)),
            Err(ResolveError::Conflict(m)) => {
                last_conflict = Some(m);
                // backtrack — пробуем следующего кандидата
            }
        }
    }

    Err(ResolveError::Conflict(last_conflict.unwrap_or_else(|| {
        format!("не удалось разрешить зависимости пакета `{}`", pkg)
    })))
}

/// Диагностика: ни одна доступная версия пакета не подходит.
fn explain_no_version<P: DependencyProvider>(
    provider: &P,
    pkg: &PkgId,
    reqs: &[Constraint],
) -> String {
    let name = provider.display_name(pkg);
    let avail = provider
        .available_versions(pkg)
        .map(|mut vs| {
            vs.sort();
            vs.reverse();
            vs.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(", ")
        })
        .unwrap_or_else(|_| "<неизвестно>".to_string());
    let mut s = format!(
        "не удалось подобрать версию пакета `{}` — ни одна доступная \
         версия не удовлетворяет всем ограничениям\n  ограничения:",
        name,
    );
    for c in reqs {
        s.push_str(&format!("\n    {}  ← {}", c.req, c.source));
    }
    s.push_str(&format!("\n  доступные версии: {}", avail));
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    /// In-memory provider: пакет → версия → её deps.
    struct Mock {
        pkgs: HashMap<PkgId, Vec<(Version, Vec<(PkgId, VersionReq)>)>>,
    }
    impl Mock {
        fn new() -> Mock {
            Mock { pkgs: HashMap::new() }
        }
        /// Добавить версию пакета с её зависимостями.
        fn add(&mut self, pkg: &str, ver: &str, deps: &[(&str, &str)]) {
            let v = Version::parse(ver).unwrap();
            let d: Vec<(PkgId, VersionReq)> = deps
                .iter()
                .map(|(p, r)| (p.to_string(), VersionReq::parse(r).unwrap()))
                .collect();
            self.pkgs.entry(pkg.to_string()).or_default().push((v, d));
        }
    }
    impl DependencyProvider for Mock {
        fn available_versions(&self, pkg: &PkgId) -> Result<Vec<Version>, String> {
            self.pkgs
                .get(pkg)
                .map(|vs| vs.iter().map(|(v, _)| v.clone()).collect())
                .ok_or_else(|| format!("пакет `{}` недоступен", pkg))
        }
        fn dependencies(
            &self,
            pkg: &PkgId,
            ver: &Version,
        ) -> Result<Vec<(PkgId, VersionReq)>, String> {
            self.pkgs
                .get(pkg)
                .and_then(|vs| vs.iter().find(|(v, _)| v == ver))
                .map(|(_, d)| d.clone())
                .ok_or_else(|| format!("нет версии `{}` пакета `{}`", ver, pkg))
        }
    }

    fn root(deps: &[(&str, &str)]) -> Vec<(PkgId, VersionReq)> {
        deps.iter()
            .map(|(p, r)| (p.to_string(), VersionReq::parse(r).unwrap()))
            .collect()
    }
    fn sel(res: &Resolution, pkg: &str) -> String {
        res.selected.get(pkg).unwrap().to_string()
    }

    #[test]
    fn single_dep_picks_highest() {
        let mut m = Mock::new();
        m.add("a", "1.0.0", &[]);
        m.add("a", "1.2.0", &[]);
        m.add("a", "1.5.0", &[]);
        let r = resolve(&m, &root(&[("a", "^1.0")])).expect("resolve");
        assert_eq!(sel(&r, "a"), "1.5.0");
    }

    #[test]
    fn single_dep_respects_range() {
        let mut m = Mock::new();
        m.add("a", "1.0.0", &[]);
        m.add("a", "1.5.0", &[]);
        m.add("a", "2.0.0", &[]);
        let r = resolve(&m, &root(&[("a", "^1.0")])).expect("resolve");
        assert_eq!(sel(&r, "a"), "1.5.0"); // 2.0.0 вне ^1.0
    }

    #[test]
    fn transitive_resolution() {
        let mut m = Mock::new();
        m.add("a", "1.0.0", &[("b", "^1.0")]);
        m.add("b", "1.0.0", &[("c", "^1.0")]);
        m.add("c", "1.3.0", &[]);
        let r = resolve(&m, &root(&[("a", "^1.0")])).expect("resolve");
        assert_eq!(sel(&r, "a"), "1.0.0");
        assert_eq!(sel(&r, "b"), "1.0.0");
        assert_eq!(sel(&r, "c"), "1.3.0");
    }

    #[test]
    fn diamond_shared_dep() {
        // root → a, b; a → c ^1.0; b → c ^1.0 — общая версия c.
        let mut m = Mock::new();
        m.add("a", "1.0.0", &[("c", "^1.0")]);
        m.add("b", "1.0.0", &[("c", "^1.0")]);
        m.add("c", "1.1.0", &[]);
        m.add("c", "1.4.0", &[]);
        let r = resolve(&m, &root(&[("a", "^1.0"), ("b", "^1.0")])).expect("resolve");
        assert_eq!(sel(&r, "c"), "1.4.0");
    }

    #[test]
    fn backtracking_version_dependent_deps() {
        // a@2.0 требует c ^2.0 (которой нет); a@1.0 требует c ^1.0 (есть).
        // Резолвер обязан откатиться с a@2.0 на a@1.0.
        let mut m = Mock::new();
        m.add("a", "1.0.0", &[("c", "^1.0")]);
        m.add("a", "2.0.0", &[("c", "^2.0")]);
        m.add("c", "1.5.0", &[]);
        let r = resolve(&m, &root(&[("a", "*")])).expect("resolve");
        assert_eq!(sel(&r, "a"), "1.0.0", "откат с a@2.0 на a@1.0");
        assert_eq!(sel(&r, "c"), "1.5.0");
    }

    #[test]
    fn backtracking_conflicting_transitive() {
        // root → a, b. a → c ^1.0; b → c ^2.0. У b есть две версии:
        // b@2.0 → c ^2.0 (конфликт с a), b@1.0 → c ^1.0 (ок).
        // Резолвер обязан выбрать b@1.0.
        let mut m = Mock::new();
        m.add("a", "1.0.0", &[("c", "^1.0")]);
        m.add("b", "1.0.0", &[("c", "^1.0")]);
        m.add("b", "2.0.0", &[("c", "^2.0")]);
        m.add("c", "1.2.0", &[]);
        let r = resolve(&m, &root(&[("a", "^1.0"), ("b", "*")])).expect("resolve");
        assert_eq!(sel(&r, "b"), "1.0.0", "b@2.0 откатывается из-за c");
        assert_eq!(sel(&r, "c"), "1.2.0");
    }

    #[test]
    fn unsolvable_conflict_reports() {
        // a → c ^1.0; b → c ^2.0; обе версии c существуют, но единой нет.
        let mut m = Mock::new();
        m.add("a", "1.0.0", &[("c", "^1.0")]);
        m.add("b", "1.0.0", &[("c", "^2.0")]);
        m.add("c", "1.0.0", &[]);
        m.add("c", "2.0.0", &[]);
        let err = resolve(&m, &root(&[("a", "^1.0"), ("b", "^1.0")]))
            .expect_err("must conflict");
        match err {
            ResolveError::Conflict(msg) => {
                assert!(msg.contains("`c`"), "msg: {}", msg);
            }
            ResolveError::Provider(m) => panic!("ожидался Conflict, не Provider: {}", m),
        }
    }

    #[test]
    fn missing_package_is_provider_error() {
        let m = Mock::new(); // пусто
        let err = resolve(&m, &root(&[("ghost", "^1.0")])).expect_err("must fail");
        assert!(matches!(err, ResolveError::Provider(_)));
    }

    #[test]
    fn dependency_cycle_terminates() {
        // a → b, b → a — взаимная зависимость. Резолвер не должен зависнуть.
        let mut m = Mock::new();
        m.add("a", "1.0.0", &[("b", "^1.0")]);
        m.add("b", "1.0.0", &[("a", "^1.0")]);
        let r = resolve(&m, &root(&[("a", "^1.0")])).expect("resolve cycle");
        assert_eq!(sel(&r, "a"), "1.0.0");
        assert_eq!(sel(&r, "b"), "1.0.0");
    }
}
