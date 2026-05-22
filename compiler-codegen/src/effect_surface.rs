//! Plan 03.4 Ф.1 — effect-surface пакета.
//!
//! **Effect-surface** = объединение effect-row всех публичных
//! (`export`) функций API пакета: «пакет использует `Net`, `Fs`».
//! Это Nova-уникальная фича менеджера пакетов (Plan 03 §4): в Cargo/npm
//! узнать, что зависимость ходит в сеть, без аудита кода **невозможно**.
//!
//! Источник — `doc`-инфраструктура (`DocModule::effect_matrix`,
//! Plan 45 Ф.24.16): per-`export fn` набор **объявленных** эффектов.
//! D28 — public fn объявляет эффекты в сигнатуре явно → surface точна
//! by construction, без межпроцедурного анализа.

use crate::doc::doctree::{DocTree, ItemKind};
use anyhow::{anyhow, bail, Result};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Агрегированная effect-surface пакета.
#[derive(Debug, Clone)]
pub struct EffectSurface {
    /// Все эффекты публичного API — отсортировано, дедуп.
    pub effects: Vec<String>,
    /// Эффект → квалифицированные имена функций, которые его вносят
    /// (отсортировано) — для аудита «откуда `Net`».
    pub by_effect: BTreeMap<String, Vec<String>>,
    /// Публичных функций всего.
    pub total_public_fns: usize,
    /// Публичных функций с ≥1 эффектом.
    pub effectful_fns: usize,
}

impl EffectSurface {
    /// `true` — публичный API без эффектов (полностью «чистый»).
    pub fn is_pure(&self) -> bool {
        self.effects.is_empty()
    }
}

/// Вычислить effect-surface из `DocTree` пакета.
///
/// `effect_matrix` уже содержит только `export`-функции с ≥1 эффектом
/// (см. `doc::collector`); `total_public_fns` корректен, если дерево
/// предварительно прогнано через `doc::strip_private`.
pub fn compute(tree: &DocTree) -> EffectSurface {
    let mut by_effect: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut total_public_fns = 0usize;
    let mut effectful_fns = 0usize;
    for m in &tree.modules {
        for it in &m.items {
            if matches!(it.kind, ItemKind::Fn(_)) {
                total_public_fns += 1;
            }
        }
        for e in &m.effect_matrix {
            effectful_fns += 1;
            let qualified = if m.path.is_empty() {
                e.fn_name.clone()
            } else {
                format!("{}.{}", m.path.join("."), e.fn_name)
            };
            for eff in &e.effects {
                by_effect
                    .entry(eff.clone())
                    .or_default()
                    .insert(qualified.clone());
            }
        }
    }
    let effects: Vec<String> = by_effect.keys().cloned().collect();
    let by_effect = by_effect
        .into_iter()
        .map(|(k, v)| (k, v.into_iter().collect::<Vec<_>>()))
        .collect();
    EffectSurface {
        effects,
        by_effect,
        total_public_fns,
        effectful_fns,
    }
}

/// Plan 03.4 Ф.2 — разница двух effect-surface (supply-chain сигнал).
#[derive(Debug, Clone)]
pub struct EffectDiff {
    /// Эффекты, появившиеся в `new` (отсутствовали в `old`).
    pub added: Vec<String>,
    /// Эффекты, исчезнувшие из `new` (были в `old`).
    pub removed: Vec<String>,
}

impl EffectDiff {
    /// `true` — surface не изменилась.
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty()
    }
}

/// Эффекты, добавленные/убранные при переходе `old` → `new`.
/// Ненулевой `added` — повод для ревью (новая сетевая/FS-активность в
/// patch/minor-релизе — классический supply-chain-вектор).
pub fn diff(old: &EffectSurface, new: &EffectSurface) -> EffectDiff {
    let o: BTreeSet<&str> = old.effects.iter().map(|s| s.as_str()).collect();
    let n: BTreeSet<&str> = new.effects.iter().map(|s| s.as_str()).collect();
    EffectDiff {
        added: n.difference(&o).map(|s| s.to_string()).collect(),
        removed: o.difference(&n).map(|s| s.to_string()).collect(),
    }
}

/// Рекурсивно собрать `.nv`-файлы пакета, пропуская служебные каталоги
/// (`target/`, `.git/`, скрытые и `_`-префиксные).
fn collect_nv(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if name == "target" || name.starts_with('.') || name.starts_with('_') {
                continue;
            }
            collect_nv(&p, out);
        } else if p.extension().and_then(|s| s.to_str()) == Some("nv") {
            out.push(p);
        }
    }
}

/// Plan 03.4 Ф.3: effect-surface пакета по его каталогу — парсит все
/// `.nv`-модули, строит `DocTree`, оставляет публичный API, агрегирует.
pub fn surface_of_package(pkg_dir: &Path) -> Result<EffectSurface> {
    let mut files: Vec<PathBuf> = Vec::new();
    collect_nv(pkg_dir, &mut files);
    if files.is_empty() {
        bail!("в `{}` не найдено .nv-файлов", pkg_dir.display());
    }
    files.sort();
    let mut modules = Vec::new();
    for f in &files {
        let src = std::fs::read_to_string(f)
            .map_err(|e| anyhow!("чтение {}: {}", f.display(), e))?;
        let m = crate::parser::parse(&src)
            .map_err(|d| anyhow!("{}", d.render(&src, &f.to_string_lossy())))?;
        modules.push(m);
    }
    let mut tree = crate::doc::build_workspace(&modules);
    crate::doc::strip_private(&mut tree);
    Ok(compute(&tree))
}

/// Эффект `surf` нарушает запрет `forbidden`: точное совпадение либо
/// параметризованный (`forbid = ["Fail"]` ловит `Fail[IoError]`).
fn violates(surf: &str, forbidden: &str) -> bool {
    surf == forbidden || surf.starts_with(&format!("{}[", forbidden))
}

/// Plan 03.4 Ф.3: проверить capability-confined зависимости пакета
/// `entry_pkg_dir`. Для каждой `[dependencies]`-записи с `forbid`
/// вычисляет её effect-surface и падает, если запрещённый эффект в ней
/// присутствует. Граница в типах, не в рантайме.
pub fn check_forbidden(entry_pkg_dir: &Path) -> Result<()> {
    let toml = entry_pkg_dir.join("nova.toml");
    let Some(manifest) = crate::manifest::parse_manifest(&toml, entry_pkg_dir) else {
        return Ok(());
    };
    for dep in &manifest.dependencies {
        if dep.forbid.is_empty() {
            continue;
        }
        let dep_dir: PathBuf = match &dep.source {
            crate::manifest::DepSource::Path(rel) => entry_pkg_dir.join(rel),
            crate::manifest::DepSource::Git { url, pin } => {
                crate::git_cache::resolve_git_dep(url, pin, None)
                    .map_err(|e| anyhow!("forbid-проверка `{}`: {}", dep.name, e))?
                    .checkout
            }
            // registry/invalid — диагностируется резолвом импортов; пропуск.
            _ => continue,
        };
        if !dep_dir.is_dir() {
            continue;
        }
        let surface = surface_of_package(&dep_dir)
            .map_err(|e| anyhow!("forbid-проверка `{}`: {}", dep.name, e))?;
        let mut report = String::new();
        for forbidden in &dep.forbid {
            let hit: Vec<&String> = surface
                .effects
                .iter()
                .filter(|s| violates(s, forbidden))
                .collect();
            if !hit.is_empty() {
                let fns: Vec<String> = hit
                    .iter()
                    .flat_map(|e| surface.by_effect.get(*e).cloned().unwrap_or_default())
                    .collect();
                report.push_str(&format!(
                    "\n  запрещённый эффект `{}` в публичном API: {}",
                    forbidden,
                    fns.join(", "),
                ));
            }
        }
        if !report.is_empty() {
            bail!(
                "зависимость `{}` нарушает capability-границу `forbid`{}\n  \
                 объявлено: forbid = [{}]",
                dep.name,
                report,
                dep.forbid.join(", "),
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Построить effect-surface из исходника одного модуля.
    fn surface_of(src: &str) -> EffectSurface {
        let module = crate::parser::parse(src).expect("parse");
        let mut tree = crate::doc::build(&module);
        crate::doc::strip_private(&mut tree);
        compute(&tree)
    }

    #[test]
    fn aggregates_public_effects() {
        let s = surface_of(
            "module pkg.api\n\n\
             export fn connect() Net -> int => 0\n\
             export fn load() Fs -> int => 0\n\
             export fn pure_fn() -> int => 0\n",
        );
        assert_eq!(s.effects, vec!["Fs".to_string(), "Net".to_string()]);
        assert_eq!(s.effectful_fns, 2);
        assert_eq!(s.total_public_fns, 3);
        assert!(!s.is_pure());
        // by_effect — откуда эффект.
        assert_eq!(s.by_effect["Net"], vec!["pkg.api.connect".to_string()]);
        assert_eq!(s.by_effect["Fs"], vec!["pkg.api.load".to_string()]);
    }

    #[test]
    fn private_fns_excluded() {
        // Приватная функция с Net не должна попадать в surface.
        let s = surface_of(
            "module pkg.api\n\n\
             export fn ok() -> int => 0\n\
             fn secret_net() Net -> int => 0\n",
        );
        assert!(s.is_pure(), "приватный Net не в публичной surface");
        assert!(s.by_effect.get("Net").is_none());
    }

    #[test]
    fn pure_package_has_empty_surface() {
        let s = surface_of(
            "module pkg.math\n\n\
             export fn add(a int, b int) -> int => a + b\n",
        );
        assert!(s.is_pure());
        assert_eq!(s.effects.len(), 0);
        assert_eq!(s.total_public_fns, 1);
        assert_eq!(s.effectful_fns, 0);
    }

    #[test]
    fn diff_detects_added_and_removed() {
        let pure = surface_of(
            "module p.a\n\nexport fn f() -> int => 0\n",
        );
        let netted = surface_of(
            "module p.a\n\nexport fn f() Net -> int => 0\n",
        );
        // pure → netted: Net добавлен.
        let d = diff(&pure, &netted);
        assert_eq!(d.added, vec!["Net".to_string()]);
        assert!(d.removed.is_empty());
        assert!(!d.is_empty());
        // netted → pure: Net убран.
        let d2 = diff(&netted, &pure);
        assert_eq!(d2.removed, vec!["Net".to_string()]);
        assert!(d2.added.is_empty());
        // identical → пусто.
        assert!(diff(&pure, &pure).is_empty());
    }

    /// Временный каталог для filesystem-тестов.
    fn temp_dir(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "nova_effsurf_{}_{}_{}",
            tag,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    /// Создать пакет-зависимость `netlib` с публичной `Net`-функцией.
    fn make_netlib(dir: &Path) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(
            dir.join("nova.toml"),
            "[package]\nname = \"netlib\"\nversion = \"0.1.0\"\n[lib]\nsrc = \".\"\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("api.nv"),
            "module netlib.api\n\nexport fn fetch(u str) Net -> str => u\n",
        )
        .unwrap();
    }

    #[test]
    fn forbid_catches_violation() {
        let base = temp_dir("forbid_viol");
        let entry = base.join("entry");
        let netlib = base.join("netlib");
        make_netlib(&netlib);
        std::fs::create_dir_all(&entry).unwrap();
        std::fs::write(
            entry.join("nova.toml"),
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n[lib]\nsrc = \".\"\n\
             [dependencies]\nnetlib = { path = \"../netlib\", forbid = [\"Net\"] }\n",
        )
        .unwrap();
        let err = check_forbidden(&entry).expect_err("Net под forbid → ошибка");
        let msg = err.to_string();
        assert!(msg.contains("netlib"), "err: {}", msg);
        assert!(msg.contains("Net"), "err: {}", msg);
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn forbid_allows_when_not_used() {
        let base = temp_dir("forbid_ok");
        let entry = base.join("entry");
        let netlib = base.join("netlib");
        make_netlib(&netlib); // использует Net
        std::fs::create_dir_all(&entry).unwrap();
        // forbid Fs — netlib его не использует → ок.
        std::fs::write(
            entry.join("nova.toml"),
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n[lib]\nsrc = \".\"\n\
             [dependencies]\nnetlib = { path = \"../netlib\", forbid = [\"Fs\"] }\n",
        )
        .unwrap();
        assert!(check_forbidden(&entry).is_ok(), "Fs не используется — нарушения нет");
        std::fs::remove_dir_all(&base).ok();
    }
}
