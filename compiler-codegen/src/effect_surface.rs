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
use std::collections::{BTreeMap, BTreeSet};

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
}
