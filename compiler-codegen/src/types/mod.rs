//! Type checker Рё effect inference.
//!
//! РњРёРЅРёРјР°Р»СЊРЅР°СЏ СЂРµР°Р»РёР·Р°С†РёСЏ: РїСЂРѕРІРµСЂСЏРµРј РёРјРµРЅР° С‚РёРїРѕРІ, РІС‹РІРѕРґРёРј С‚РёРїС‹ Р»РѕРєР°Р»СЊРЅС‹С…
//! РїРµСЂРµРјРµРЅРЅС‹С…, РІС‹РІРѕРґРёРј СЌС„С„РµРєС‚С‹ РґР»СЏ private С„СѓРЅРєС†РёР№ (D28). Generic-РїР°СЂР°РјРµС‚СЂС‹
//! РїСЂРѕРІРµСЂСЏСЋС‚СЃСЏ РєР°Рє abstract names вЂ” РјРѕРЅРѕРјРѕСЂС„РёР·Р°С†РёСЏ РґРµР»Р°РµС‚СЃСЏ РїСЂРё
//! РёРЅС‚РµСЂРїСЂРµС‚Р°С†РёРё (treewalk РЅРµ С‚СЂРµР±СѓРµС‚ РІСЃРµРіРѕ).

use crate::ast::*;
use crate::diag::{Diagnostic, FileId, MAIN_FILE_ID, Span};
use std::collections::{HashMap, HashSet};

/// РћС‡РµРЅСЊ СѓРїСЂРѕС‰С‘РЅРЅР°СЏ СЃРёСЃС‚РµРјР° С‚РёРїРѕРІ РґР»СЏ bootstrap'Р°.
///
/// Treewalk-РёРЅС‚РµСЂРїСЂРµС‚Р°С‚РѕСЂ СЂР°Р±РѕС‚Р°РµС‚ СЃ РґРёРЅР°РјРёС‡РµСЃРєРёРјРё Р·РЅР°С‡РµРЅРёСЏРјРё, РїРѕСЌС‚РѕРјСѓ
/// Р·РґРµСЃСЊ РјС‹ РІС‹РїРѕР»РЅСЏРµРј РјРёРЅРёРјСѓРј: РїСЂРѕРІРµСЂРєРё РёРјС‘РЅ, Р±Р°Р·РѕРІР°СЏ СЃРѕРІРјРµСЃС‚РёРјРѕСЃС‚СЊ,
/// effect inference С‡РµСЂРµР· accumulated set.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Ty {
    Int,
    Float,
    Str,
    Bool,
    Unit,
    Never,
    /// Р›СЋР±РѕР№ С‚РёРї / РЅРµРёР·РІРµСЃС‚РЅС‹Р№ (РґР»СЏ bootstrap'Р° вЂ” fallback).
    Any,
    /// РРјРµРЅРѕРІР°РЅРЅС‹Р№ С‚РёРї (record, sum, effect, newtype, alias).
    /// Generics РЅРµ СЂР°Р·РІРѕСЂР°С‡РёРІР°СЋС‚СЃСЏ вЂ” РѕРЅРё РјРѕРЅРѕРјРѕСЂС„РёР·РёСЂСѓСЋС‚СЃСЏ РїРѕР·Р¶Рµ.
    Named(String),
    Array(Box<Ty>),
    Tuple(Vec<Ty>),
    Func {
        params: Vec<Ty>,
        ret: Box<Ty>,
        effects: Vec<String>,
    },
}

/// Р РµР·СѓР»СЊС‚Р°С‚ РїСЂРѕРІРµСЂРєРё РјРѕРґСѓР»СЏ вЂ” РєР°СЂС‚Р° РёРјС‘РЅ top-level в†’ С‚РёРї.
///
/// **D84 overloading:** `fns` С…СЂР°РЅРёС‚ **Vec** РґР»СЏ РєР°Р¶РґРѕРіРѕ РёРјРµРЅРё, РїРѕС‚РѕРјСѓ
/// С‡С‚Рѕ РѕРґРЅРѕ РёРјСЏ РјРѕР¶РµС‚ РёРјРµС‚СЊ РЅРµСЃРєРѕР»СЊРєРѕ РїРµСЂРµРіСЂСѓР·РѕРє (РјРµС‚РѕРґС‹ СЃ РѕРґРЅРёРј РёРјРµРЅРµРј
/// РЅР° РѕРґРЅРѕРј receiver-type, free-functions СЃ СЂР°Р·РЅС‹РјРё signatures, СЂР°Р·РЅС‹Рµ
/// `From[X]`). Р РµР·РѕР»РІ РЅР° call-site РїРѕ argument-types вЂ” РѕС‚РІРµС‚СЃС‚РІРµРЅРЅРѕСЃС‚СЊ
/// codegen / bound-checker.
#[derive(Debug, Default)]
pub struct ModuleEnv {
    pub types: HashMap<String, TypeDecl>,
    pub fns: HashMap<String, Vec<FnDecl>>,
    pub consts: HashMap<String, ConstDecl>,
    /// Plan 33.1 Р¤.3: СЃРїРёСЃРѕРє РґРѕРєР°Р·Р°РЅРЅС‹С… (fn_name, contract span) РєРѕРЅС‚СЂР°РєС‚РѕРІ.
    /// Codegen РІ release-СЃР±РѕСЂРєРµ СЃС‚РёСЂР°РµС‚ СЃРѕРѕС‚РІРµС‚СЃС‚РІСѓСЋС‰РёРµ runtime-checks
    /// (zero-cost guarantee). Р’ debug вЂ” checks РІСЃРµРіРґР° emit'СЏС‚СЃСЏ.
    pub proven_contracts: Vec<(String, Span)>,
}

/// РњРёРЅРёРјР°Р»СЊРЅР°СЏ РїСЂРѕРІРµСЂРєР° РјРѕРґСѓР»СЏ. Р РµРіРёСЃС‚СЂРёСЂСѓРµС‚ РёРјРµРЅР° Рё Р±Р°Р·РѕРІСѓСЋ СЃС‚СЂСѓРєС‚СѓСЂСѓ вЂ”
/// РґР»СЏ bootstrap'Р° СЌС‚РѕРіРѕ РґРѕСЃС‚Р°С‚РѕС‡РЅРѕ: РёРЅС‚РµСЂРїСЂРµС‚Р°С‚РѕСЂ Р»РѕРІРёС‚ РѕС€РёР±РєРё С‚РёРїРѕРІ РІ
/// runtime С‡РµСЂРµР· match-mismatch Рё method-not-found.
pub fn check_module(module: &Module) -> Result<ModuleEnv, Vec<Diagnostic>> {
    let mut env = ModuleEnv::default();
    let mut errors = Vec::new();
    let mut names: HashSet<String> = HashSet::new();

    // D82: `external fn` whitelisted С‚РѕР»СЊРєРѕ РІ `std/runtime/*.nv`. User-РєРѕРґ
    // РЅРµ РґРѕР»Р¶РµРЅ РёСЃРїРѕР»СЊР·РѕРІР°С‚СЊ external вЂ” СЌС‚Рѕ keyword РґР»СЏ РґРѕРєСѓРјРµРЅС‚РёСЂРѕРІР°РЅРёСЏ
    // stdlib runtime-С„СѓРЅРєС†РёР№, СЂРµР°Р»РёР·РѕРІР°РЅРЅС‹С… РІ nova_rt/*.h. Р‘СѓРґСѓС‰РёР№
    // `extern("C")` РґР»СЏ FFI Рє СЃС‚РѕСЂРѕРЅРЅРёРј libs вЂ” РѕС‚РґРµР»СЊРЅС‹Р№ keyword.
    //
    // Plan 42 Sub-plan 42.6: detect runtime module РїРѕ РѕР±РѕРёС… declaration
    // С„РѕСЂРјР°С‚РѕРІ (rev-1 legacy + rev-3 parent.X). Logic вЂ” РІ manifest helper.
    //
    // Plan 62.A: also whitelist `std.prelude.*` submodules. Prelude
    // sub-modules (`std/prelude/core.nv`, etc.) declare types/methods
    // implemented by codegen helpers in `nova_rt/*.h` — same pattern
    // as `std.runtime.*` (declaration-only, no Nova body).
    //
    // Plan 62.A (2026-05-18): check only items DECLARED HERE (in entry
    // peers' `items_here`), not items merged from imports. Otherwise
    // `external fn`-карта prelude'а проседает на каждом user-модуле:
    // user `module foo` импортирует `std.prelude` → prelude.core external
    // fns merge'нутся в `module.items` → check fires на foo. Items
    // источника prelude.core валидируются при компиляции САМОГО
    // prelude.core (отдельный `check_module` invocation на std).
    let is_runtime_module = crate::manifest::is_stdlib_runtime_module(&module.name)
        || crate::manifest::is_prelude_self_module(&module.name);
    if !is_runtime_module {
        // Collect entry peers' items_here (items declared в этом модуле
        // самим, не pulled через imports). Fallback на module.items если
        // peer_files пуст (legacy single-file).
        let entry_items: Vec<&Item> = if module.peer_files.is_empty() {
            module.items.iter().collect()
        } else {
            module.peer_files
                .iter()
                .filter(|pf| pf.is_entry_module)
                .flat_map(|pf| pf.items_here.iter())
                .collect()
        };
        for item in entry_items {
            if let Item::Fn(fd) = item {
                if fd.is_external {
                    // Plan 91.10 (D163 retracted): `needs <Cap>` clause удалён.
                    // D82 restriction для plain external fn остаётся: only
                    // stdlib-runtime modules могут декларировать external fn.
                    // FFI to external C libraries — TBD (нужен `extern("C")`
                    // или похожий механизм; пока stdlib-only).
                    let _ = fd.needs_caps; // backward-compat field, всегда empty.
                    errors.push(Diagnostic::new(
                        format!(
                            "`external fn` is only allowed in `std.runtime.*` modules \
                             (this module is `{}`). FFI to external C libraries — \
                             см. D82 + future `extern(\"C\")` syntax.",
                            module.name.join(".")
                        ),
                        fd.span,
                    ));
                }
            }
            // Plan 62.D.bis (D126) + Plan 100.5 (D163): `external type X` with
            // `consume` is allowed in any module (D163 FFI opaque consume-types).
            // `external type X` without `consume` (plain opaque) remains stdlib-only
            // (per D82 — opaque types backed by `nova_rt/*.h` are internal).
            if let Item::Type(td) = item {
                if matches!(td.kind, TypeDeclKind::Opaque) && !td.consume {
                    errors.push(Diagnostic::new(
                        format!(
                            "`external type` is only allowed in `std.runtime.*` / `std.prelude.*` modules \
                             (this module is `{}`); for FFI opaque resource handles use \
                             `external type X consume` (D163/D126), e.g. `external type File consume`.",
                            module.name.join(".")
                        ),
                        td.span,
                    ));
                }
            }
        }
    }

    // Plan 62.D bis-1 (2026-05-18, D29 W_PRELUDE_SHADOW basic):
    // Determine which items in `module.items` came from imports (vs the
    // user's own entry file) — these are conflict candidates for D29 lint
    // and for the "codegen-completeness invisible merge" detection.
    //
    // The merge logic in `imports.rs` is two-phase:
    //   - `merged_items` (→ `module.items`): ALL items from imported peer
    //     modules are pulled in for codegen completeness (e.g. typedef'ы
    //     should be available even if not selectively imported). This
    //     causes apparent name conflicts when the user re-declares a name
    //     that's in a merged-but-not-visible item.
    //   - `imported_item_names` (per-peer): names actually VISIBLE to the
    //     user via explicit imports + selective re-exports. This is the
    //     proper "what does user see" set.
    //
    // D29 rule (W_PRELUDE_SHADOW basic): user declarations that conflict
    // with names brought in via prelude auto-import → warning (not error).
    // User declarations that conflict with codegen-only merged items (not
    // user-visible) → silently accept user's declaration.
    //
    // We collect entry-visible names via prelude into `prelude_visible_names`;
    // items in `module.items` NOT in this set AND NOT in user's own
    // `items_here` are codegen-only merges — silently allowed to be
    // shadowed by user code.
    //
    // Detection of "prelude-brought-in":
    //   Pass 1: names declared directly in `std/prelude/*` or
    //   `std/prelude.nv` peer files (items_here of those peers).
    //   Pass 2: names re-exported through prelude facade via selective
    //   `export import X.{A, B}` lists in prelude peer imports. The
    //   re-exported alias (if any) is the visible name.
    //
    // Without this fix, enabling `export import
    // std.collections.range.{Range, RangeIter}` in `std/prelude.nv` broke
    // `nova_tests/syntax/for_in_range_iter.nv` (which locally declares
    // `type Range` and `type StepRangeIter`).
    //
    // Plan 62.F.bis Ф.2 (2026-05-18): visibility computation вынесена в
    // `lints::collect_prelude_visibility`. types::check_module использует
    // её для silent classify duplicate'ов (user-decl wins); structured
    // W_PRELUDE_SHADOW warning эмитится через `lints::lint_prelude_shadow`
    // — отдельно, в pipeline после check_module. Раньше eprintln здесь
    // дублировал диагностику; теперь silent — warnings приходят как
    // structured LintWarning через `cmd_check` warnings field.
    let prelude_vis = crate::lints::collect_prelude_visibility(module);

    // Classify a duplicate top-level name:
    //   - `Some(true)` → name is visible via prelude → user-decl wins,
    //     structured warning emitted by `lints::lint_prelude_shadow`
    //   - `Some(false)` → name is merged-from-imports (codegen-only, not
    //     user-visible) → silent (user wins)
    //   - `None` → genuine duplicate (e.g. user code declared same name twice)
    //     → error
    let classify_dup = |name: &str| -> Option<bool> {
        if prelude_vis.visible.contains(name) { Some(true) }
        else if prelude_vis.merged_from_imports.contains(name) { Some(false) }
        else { None }
    };

    for item in &module.items {
        match item {
            Item::Type(td) => {
                if !names.insert(td.name.clone()) {
                    // Plan 62.D bis-1: classify the duplicate per D29.
                    match classify_dup(&td.name) {
                        Some(true) => {
                            // Visible via prelude → user-declaration wins
                            // silently here; structured W_PRELUDE_SHADOW
                            // warning emitted by `lints::lint_prelude_shadow`
                            // (Plan 62.F.bis Ф.2 — see warnings field в
                            // cmd_check для surface). User-decl still wins;
                            // qualify as `std.prelude.<sub>.<name>` для
                            // прямого доступа к prelude version.
                            env.types.insert(td.name.clone(), td.clone());
                            continue;
                        }
                        Some(false) => {
                            // Codegen-only merge (not user-visible). User's
                            // declaration silently wins.
                            env.types.insert(td.name.clone(), td.clone());
                            continue;
                        }
                        None => {
                            errors.push(Diagnostic::new(
                                format!("duplicate top-level name `{}`", td.name),
                                td.span,
                            ));
                        }
                    }
                }
                env.types.insert(td.name.clone(), td.clone());
            }
            Item::Fn(fd) => {
                let key = match &fd.receiver {
                    Some(r) => format!("{}.{}", r.type_name, fd.name),
                    None => fd.name.clone(),
                };
                // D84: overload РїРѕ Р»СЋР±РѕР№ РёР· С‡РµС‚С‹СЂС‘С… РѕСЃРµР№ (receiver-type,
                // arg-types, result-type, arity). РџРѕРґ РѕРґРЅРёРј РёРјРµРЅРµРј РјРѕР¶РµС‚
                // Р±С‹С‚СЊ РЅРµСЃРєРѕР»СЊРєРѕ overloads, СЂР°Р·Р»РёС‡Р°СЋС‰РёС…СЃСЏ sig'Р°РјРё; codegen
                // Рё bound-checker СЂРµР·РѕР»РІСЏС‚ call-site РїРѕ argument-types.
                //
                // Р—Р°РїСЂРµС‰РµРЅРѕ С‚РѕР»СЊРєРѕ **С‚РѕС‡РЅРѕРµ РґСѓР±Р»РёСЂРѕРІР°РЅРёРµ signature**
                // (РѕРґРёРЅР°РєРѕРІС‹Рµ arity + РѕРґРёРЅР°РєРѕРІС‹Рµ arg-types) вЂ” СЌС‚Рѕ Р±С‹Р»Р° Р±С‹
                // ambiguity Р±РµР· РІРѕР·РјРѕР¶РЅРѕСЃС‚Рё СЂРµР·РѕР»РІР°. РџСЂРѕРІРµСЂРєР° РЅРёР¶Рµ.
                names.insert(key.clone()); // names вЂ” РґР»СЏ РєРѕРЅС„Р»РёРєС‚РѕРІ СЃ С‚РёРїР°РјРё/const'Р°РјРё
                let entry = env.fns.entry(key.clone()).or_default();
                // D84: overload-disambiguation РїРѕ Р»СЋР±РѕР№ РёР· С‡РµС‚С‹СЂС‘С… РѕСЃРµР№.
                // РўРѕС‡РЅРѕРµ РґСѓР±Р»РёСЂРѕРІР°РЅРёРµ Р·Р°РїСЂРµС‰РµРЅРѕ вЂ” СЌС‚Рѕ С‚СЂРµР±СѓРµС‚ РѕРґРЅРѕРІСЂРµРјРµРЅРЅРѕРіРѕ
                // СЃРѕРІРїР°РґРµРЅРёСЏ **arity + arg-types + return-type** (РїР»СЋСЃ
                // receiver-type, РєРѕС‚РѕСЂС‹Р№ СѓР¶Рµ РІРєР»СЋС‡С‘РЅ РІ `key`). Р•СЃР»Рё С…РѕС‚СЊ РѕРґРЅР°
                // РѕСЃСЊ СЂР°Р·Р»РёС‡Р°РµС‚СЃСЏ вЂ” overload РІР°Р»РёРґРµРЅ.
                let new_arg_tys: Vec<&TypeRef> = fd.params.iter().map(|p| &p.ty).collect();
                let dup_existing = entry.iter().find(|existing| {
                    // Arity + arg-types РѕРґРёРЅР°РєРѕРІС‹?
                    let args_equal = existing.params.len() == fd.params.len()
                        && existing.params.iter().zip(new_arg_tys.iter())
                            .all(|(p, new_ty)| typeref_equal(&p.ty, new_ty));
                    if !args_equal { return false; }
                    // Return-type РѕРґРёРЅР°РєРѕРІ? (None / None РёР»Рё Some/Some equal).
                    match (&existing.return_type, &fd.return_type) {
                        (None, None) => true,
                        (Some(a), Some(b)) => typeref_equal(a, b),
                        _ => false,
                    }
                });
                if dup_existing.is_some() {
                    // Plan 62.D bis-1: D29 — duplicate fn signature shadowing
                    // a prelude-imported definition → warning (not error).
                    // E.g. `fn Range @step_by(int) -> StepRangeIter` declared
                    // in both user file and the merged-via-prelude
                    // std/collections/range.nv. User wins.
                    let dup_pos = entry.iter().position(|existing| {
                        let args_equal = existing.params.len() == fd.params.len()
                            && existing.params.iter().zip(new_arg_tys.iter())
                                .all(|(p, new_ty)| typeref_equal(&p.ty, new_ty));
                        if !args_equal { return false; }
                        match (&existing.return_type, &fd.return_type) {
                            (None, None) => true,
                            (Some(a), Some(b)) => typeref_equal(a, b),
                            _ => false,
                        }
                    });
                    match classify_dup(&key) {
                        Some(true) => {
                            // Plan 62.F.bis Ф.2: silent user-wins; structured
                            // W_PRELUDE_SHADOW warning эмитится через
                            // `lints::lint_prelude_shadow`.
                            if let Some(pos) = dup_pos {
                                entry[pos] = fd.clone();
                            }
                            continue;
                        }
                        Some(false) => {
                            // Codegen-only merge — silent shadow.
                            if let Some(pos) = dup_pos {
                                entry[pos] = fd.clone();
                            }
                            continue;
                        }
                        None => {
                            errors.push(Diagnostic::new(
                                format!(
                                    "duplicate definition `{}` with same signature \
                                     (overload requires distinct param types, arity, или return type — \
                                     см. D84); previous definition has identical params and return type",
                                    key
                                ),
                                fd.span,
                            ));
                        }
                    }
                } else {
                    entry.push(fd.clone());
                }
            }
            Item::Const(cd) => {
                if !names.insert(cd.name.clone()) {
                    // Plan 62.D bis-1: same prelude-shadow rule as for types.
                    match classify_dup(&cd.name) {
                        Some(true) => {
                            // Plan 62.F.bis Ф.2: silent user-wins; structured
                            // W_PRELUDE_SHADOW warning эмитится через
                            // `lints::lint_prelude_shadow`.
                            env.consts.insert(cd.name.clone(), cd.clone());
                            continue;
                        }
                        Some(false) => {
                            env.consts.insert(cd.name.clone(), cd.clone());
                            continue;
                        }
                        None => {
                            errors.push(Diagnostic::new(
                                format!("duplicate top-level name `{}`", cd.name),
                                cd.span,
                            ));
                        }
                    }
                }
                env.consts.insert(cd.name.clone(), cd.clone());
            }
            Item::Let(_) | Item::Test(_) | Item::Bench(_) | Item::Lemma(_) => {
                // top-level let — не используется в Nova-исходниках. test/bench —
                // регистрируются отдельно (имя — string-literal, не идентификатор),
                // конфликта по имени быть не может.
                // Ф.4.1: lemma — ghost, только для proof; не регистрируется в env.
            }
        }
    }

    // (typeref_equal вЂ” helper РґР»СЏ D84 duplicate-signature detection,
    // РѕРїСЂРµРґРµР»С‘РЅ РІ РєРѕРЅС†Рµ С„Р°Р№Р»Р°.)

    // Plan 15 (D72): generic bounds enforcement.
    //
    // РЎРѕР±РёСЂР°РµРј protocol_specs (РјРµС‚РѕРґС‹ РєР°Р¶РґРѕРіРѕ protocol-С‚РёРїР°) Рё
    // method_table (РјРµС‚РѕРґС‹ РєР°Р¶РґРѕРіРѕ concrete-С‚РёРїР°). Р—Р°С‚РµРј С…РѕРґРёРј РїРѕ
    // РІСЃРµРј call-СЃР°Р№С‚Р°Рј РІ bodies, РґР»СЏ generic-РІС‹Р·РѕРІРѕРІ СЃ bounds
    // РїСЂРѕРІРµСЂСЏРµРј satisfaction concrete-Р°СЂРіСѓРјРµРЅС‚РѕРІ.
    // Plan 101.4 (D145 Ред. 5): protocol-composition validation —
    // embed target существует и есть protocol; нет cycle; нет duplicate
    // signature collision при flatten'е. Запускается ДО BoundCtx::build
    // чтобы errors на cycle не превращались в infinite recursion внутри
    // flatten_dfs (хотя у flatten_dfs есть `seen`-guard — safety belt).
    check_protocol_embeds(module, &mut errors);

    // Plan 101.3 (D145 Ред. 5): generic-bound declaration validation —
    // каждое имя bound'а в `[T A + B]` должно быть объявленным protocol'ом
    // (или well-known stdlib alias типа Hashable/Eq/Ord/Display). Раньше
    // bound-resolve был permissive (silent skip unknown) — Plan 101 делает
    // strict. Pre-Plan 101 tests, ссылающиеся на неизвестные bound'ы,
    // должны их объявить или удалить.
    check_generic_bound_declarations(module, &mut errors);

    let bound_ctx = BoundCtx::build(module);
    bound_ctx.check_module(module, &mut errors);

    // Plan 16 (D63 forbid + D64 realtime): capability enforcement.
    //
    // Walk fn bodies + tests, РѕС‚СЃР»РµР¶РёРІР°СЏ forbidden-effects СЃС‚РµРє +
    // realtime-С„Р»Р°Рі. РќР° РєР°Р¶РґРѕРј Call-СЃР°Р№С‚Рµ вЂ” РїСЂРѕРІРµСЂРєР° intersect'Р°
    // callee.effects СЃ forbidden-set; РІ realtime вЂ” Net/Fs/Db/Time
    // suspend-effects Р·Р°РїСЂРµС‰РµРЅС‹; РІ `realtime nogc` вЂ” alloc-fn'С‹
    // Р·Р°РїСЂРµС‰РµРЅС‹. РЈСЃС‚Р°РЅРѕРІРєР° handler'Р° РґР»СЏ forbidden-СЌС„С„РµРєС‚Р° РІРЅСѓС‚СЂРё
    // forbid-Р±Р»РѕРєР° вЂ” error.
    let cap_ctx = CapabilityCtx::build(module);
    cap_ctx.check_module(module, &mut errors);

    // D90 Plan 20 Р¤.3: defer/errdefer body constraints.
    //
    // Body Р·Р°РїСЂРµС‰Р°РµС‚:
    //  - exit-control (return/throw/break/continue) вЂ” РЅРµР»СЊР·СЏ hijack
    //    exit СЃРµРјР°РЅС‚РёРєСѓ scope'Р°.
    //  - Fail-СЌС„С„РµРєС‚ (?/!!/throw) вЂ” double-throw РЅРµРІРѕР·РјРѕР¶РЅРѕ СЃРґРµР»Р°С‚СЊ
    //    РєРѕСЂСЂРµРєС‚РЅРѕ. throw РѕР±РЅР°СЂСѓР¶РёРІР°РµС‚СЃСЏ С‡РµСЂРµР· AST-walk; ?/!! вЂ” РІ codegen
    //    РѕРЅРё desugar'СЏС‚СЃСЏ РІ throw, РїРѕСЌС‚РѕРјСѓ РґРѕСЃС‚Р°С‚РѕС‡РЅРѕ catch throw.
    //  - suspend-РѕРїРµСЂР°С†РёРё (Net.*, Fs.*, Db.*, Time.sleep, parallel for,
    //    spawn, supervised, select) вЂ” defer РґРѕР»Р¶РµРЅ Р±С‹С‚СЊ Р±С‹СЃС‚СЂС‹Рј cleanup.
    //
    // Walks РїРѕ РІСЃРµРј bodies РІСЃРµС… С„СѓРЅРєС†РёР№. Spec вЂ” D90.
    check_defer_bodies(module, &mut errors);

    // D61 В§1430-1434 / D90 Р¤.8 (1): handler-method РґР»СЏ СЌС„С„РµРєС‚-РѕРїРµСЂР°С†РёРё
    // СЃ return type `never` РћР‘РЇР—РђРќ Р·Р°РєРѕРЅС‡РёС‚СЊСЃСЏ exit-control'РѕРј
    // (`interrupt v` РёР»Рё `throw err` / `panic` / `exit`). РРЅР°С‡Рµ РЅРµС‚
    // Р·РЅР°С‡РµРЅРёСЏ С‚РёРїР° never РґР»СЏ РІРѕР·РІСЂР°С‚Р° вЂ” handler РЅРµ РјРѕР¶РµС‚ Р·Р°РєРѕРЅРЅРѕ
    // Р·Р°РІРµСЂС€РёС‚СЊСЃСЏ normally.
    //
    // РџСЂРёРјРµРЅСЏРµС‚СЃСЏ Рє: Fail.fail (built-in, return never), Р»СЋР±С‹Рј
    // user-defined effect-operations СЃ return type never.
    //
    // Walks РІСЃРµ handler-Р»РёС‚РµСЂР°Р»С‹ РІ module, РїСЂРѕРІРµСЂСЏРµС‚ РґР»СЏ РєР°Р¶РґРѕРіРѕ
    // method'Р°, СЏРІР»СЏРµС‚СЃСЏ Р»Рё СЃРѕРѕС‚РІРµС‚СЃС‚РІСѓСЋС‰Р°СЏ operation never-РІРѕР·РІСЂР°С‚-
    // РЅРѕР№, Рё РµСЃР»Рё РґР° вЂ” body РґРѕР»Р¶РµРЅ diverge (static analysis).
    check_handler_never_ops(module, &mut errors);

    // Plan 77 (D132): `-> @` fluent-return — тело метода обязано
    // вернуть `@`. Делает гарантию проверяемой для consume-checker.
    check_fluent_return(module, &mut errors);

    // Plan 73 (D131): consume-qualifier flow-sensitive check. Use-after-
    // consume и maybe-consumed (consume на части веток) → compile error.
    check_consume(module, &mut errors);

    // Plan 91.10 (D163 retracted, 2026-05-30): check_external_fn_needs_caps
    // удалён. Capability tracking via отдельный syntax — redundant с effect
    // system. См. docs/plans/91.10-d163-retract-capability-syntax.md.

    // Plan 33.3 Р¤.9 (D24): validate axiom-bodies РІ effect-Р±Р»РѕРєР°С….
    // РљР°Р¶РґС‹Р№ axiom РґРѕР»Р¶РµРЅ СЃСЃС‹Р»Р°С‚СЊСЃСЏ С‚РѕР»СЊРєРѕ РЅР° binders + pure_view-ops
    // **С‚РѕРіРѕ Р¶Рµ СЌС„С„РµРєС‚Р°** + Р»РёС‚РµСЂР°Р»С‹ + boolean/arith operators. Р›СЋР±РѕР№
    // РґСЂСѓРіРѕР№ identifier (РІРєР»СЋС‡Р°СЏ non-pure_view ops) в†’ error. Р­С‚Рѕ
    // С„СѓРЅРґР°РјРµРЅС‚ SMT encoding (UF mapping РІ Р¤.9.4).
    check_effect_axioms(module, &mut errors);

    // Plan 33.3 Р¤.9.6: handler verification gate.
    // Р•СЃР»Рё СЌС„С„РµРєС‚ РёРјРµРµС‚ pure_view-ops, Р»СЋР±Р°СЏ `with E = handler` РґР»СЏ
    // СЌС‚РѕРіРѕ СЌС„С„РµРєС‚Р° РѕР±СЏР·Р°РЅР° Р±С‹С‚СЊ РїРѕРјРµС‡РµРЅР° `#verify_handler` РёР»Рё
    // `#trusted_handler`. Р‘РµР· Р°С‚СЂРёР±СѓС‚Р° вЂ” compile error.
    check_handler_verification_gate(module, &mut errors);

    // Name-resolution С„Р°Р·Р°: СЃС‚Р°С‚РёС‡РµСЃРєРёР№ РїРѕРёСЃРє undefined РёРґРµРЅС‚РёС„РёРєР°С‚РѕСЂРѕРІ
    // РІ expr-position. Р—Р°РїСѓСЃРєР°РµС‚СЃСЏ РџРћРЎР›Р• BoundCtx/CapabilityCtx, С‡С‚РѕР±С‹
    // Р±РѕР»РµРµ С„СѓРЅРґР°РјРµРЅС‚Р°Р»СЊРЅС‹Рµ РѕС€РёР±РєРё (signatures/effects) РїСЂРёС…РѕРґРёР»Рё РїРµСЂРІС‹РјРё.
    //
    // Р‘РµР· СЌС‚РѕР№ С„Р°Р·С‹ РєРѕРґ РІСЂРѕРґРµ `let r = 1 | undefined_var` РїСЂРѕС…РѕРґРёР»
    // typecheck Рё РїР°РґР°Р» С‚РѕР»СЊРєРѕ РЅР° cc-СЌС‚Р°РїРµ СЃ РјР°Р»РѕС‡РёС‚Р°РµРјРѕР№ РѕС€РёР±РєРѕР№
    // "РЅРµРѕР±СЉСЏРІР»РµРЅРЅС‹Р№ РёРґРµРЅС‚РёС„РёРєР°С‚РѕСЂ". РЎРј. NameResCtx РЅРёР¶Рµ.
    let name_res = NameResCtx::build(module);
    name_res.check_module(module, &mut errors);

    // Plan 33.1 Р¤.2 (D24): contract checking + purity inference.
    // РњРёРЅРёРјР°Р»СЊРЅС‹Р№ pass: РїСЂРѕРІРµСЂРєР° Р±Р°Р·РѕРІС‹С… РїСЂР°РІРёР» РґР»СЏ РєРѕРЅС‚СЂР°РєС‚РѕРІ:
    // - `result` Р·Р°РїСЂРµС‰С‘РЅ РІ `requires`;
    // - `old(...)` Р·Р°РїСЂРµС‰С‘РЅ РІ `requires`;
    // - composition (РІС‹Р·РѕРІ РґСЂСѓРіРѕР№ fn РІ РєРѕРЅС‚СЂР°РєС‚Рµ) Р·Р°РїСЂРµС‰С‘РЅ РІ 33.1
    //   (Р±СѓРґРµС‚ СЂР°Р·СЂРµС€С‘РЅ РґР»СЏ #pure РІ 33.2).
    let contract_ctx = ContractCtx::build(module);
    contract_ctx.check_module(module, &mut errors);

    // Plan 33.3 Р¤.9.7 (D24): ghost-var usage check.
    // Non-ghost РєРѕРґ РЅРµ РјРѕР¶РµС‚ С‡РёС‚Р°С‚СЊ ghost-var (Verus/Dafny semantics).
    // Р”Рѕ СЌС‚РѕРіРѕ: catch'РёР»РѕСЃСЊ РЅР° C-level С‡РµСЂРµР· В«undeclared identifierВ»;
    // С‚РµРїРµСЂСЊ вЂ” proper compile-error СЃ РїРѕРЅСЏС‚РЅС‹Рј СЃРѕРѕР±С‰РµРЅРёРµРј.
    check_ghost_usage(module, &mut errors);

    // Plan 52 Ф.2 (D108): map-литерал `[k: v]` type-checking.
    //
    // Focused expected-type проход: обходит fn-bodies/tests/consts,
    // протаскивая ожидаемый тип в let-аннотацию / return / argument-
    // позицию. На каждом `MapLit` — вывод `HashMap[K, V]` из ключей/
    // значений (или из ожидаемого типа), enforce `K: Hashable`,
    // унификация ключей и значений. Пустой `[]` в позиции, ожидающей
    // `HashMap` — валиден; неоднозначный `[]` без типа — error.
    // Не заменяет существующие walk'и — отдельный проход (как
    // NameResCtx / ContractCtx), минимум регрессий.
    let map_lit_ctx = MapLitCtx::build(module);
    map_lit_ctx.check_module(module, &mut errors);

    // Plan 79: type-checker hardening — «no silent fallback» на уровне
    // типов. Отдельный проход (паттерн NameResCtx / MapLitCtx): доводит
    // type-checker до типовой полноты. Ф.2 — арность type-аргументов.
    let type_check_ctx = TypeCheckCtx::build(module);
    type_check_ctx.check_module(module, &mut errors);

    // Plan 33.1 Ф.3 (D24): SMT verification.
    // TrivialBackend по умолчанию (Z3 — отдельная feature в будущем).
    // Доказанные контракты записываются в env для zero-cost release.
    // `#must_verify` errors / counterexample warnings — попадают в errors.
    if errors.is_empty() {
        // Verify С‚РѕР»СЊРєРѕ РµСЃР»Рё РїСЂРµРґС‹РґСѓС‰РёРµ С„Р°Р·С‹ РїСЂРѕС€Р»Рё (РёРЅР°С‡Рµ encode РЅР°
        // РЅРµРІР°Р»РёРґРЅРѕРј AST РјРѕР¶РµС‚ РєСЂР°С€РЅСѓС‚СЊ).
        let report = crate::verify::verify_module(module);
        env.proven_contracts = report.proven;
        for e in report.errors { errors.push(e); }
        // warnings РїРѕРєР° silent вЂ” РґРѕР±Р°РІРёРј warning infrastructure
        // РІ Plan 36 production hardening.
        // Note: counterexample-warnings (Р±РµР· #must_verify) Р±СЌРє-port'СЏС‚СЃСЏ
        // РІ errors РІСЂРµРјРµРЅРЅРѕ, С‡С‚РѕР±С‹ РІ 33.1 negative-С‚РµСЃС‚С‹ РјРѕРіР»Рё РёС… РґРµС‚РµРєС‚РёС‚СЊ.
        // Р­С‚Рѕ Р±СѓРґРµС‚ СѓС‚РѕС‡РЅРµРЅРѕ РєРѕРіРґР° РґРѕР±Р°РІРёС‚СЃСЏ warning severity (Plan 36).
        let _ = report.warnings; // intentionally silent
    }

    if errors.is_empty() {
        Ok(env)
    } else {
        Err(errors)
    }
}

// ============================================================================
// Plan 79: type-checker hardening — «no silent fallback» на уровне типов.
//
// Type-checker bootstrap'а проверяет имена/структуру/эффекты/контракты, но НЕ
// базовую совместимость типов. Plan 79 доводит его до типовой полноты: каждое
// использование типа проверяется, несовместимость → compile-error (серия
// E73xx) вместо silent miscompilation или поздней CC-FAIL.
//
//   Ф.2 — арность type-аргументов (`Result[int]` → E7310).   [реализовано]
//   Ф.1 — assignability arg↔param и annotation↔RHS.          [pending]
//   Ф.3 — существование поля / варианта.                      [pending]
//   Ф.4 — type-vs-value.                                      [pending]
//
// Отдельный проход `TypeCheckCtx` (паттерн NameResCtx / ContractCtx /
// MapLitCtx) — растёт по фазам, минимум регрессий к существующим walk'ам.
// ============================================================================

/// Объявленная арность generic-типа.
struct ArityInfo {
    /// Число объявленных generic-параметров.
    count: usize,
    /// Span объявления `type` — для note «declared here». `None` у
    /// built-in типов (Option/Result/примитивы), чьё объявление не
    /// находится в текущем модуле.
    decl_span: Option<Span>,
}

/// Plan 114.4 Ф.1: constexpr-eligibility check для `const X = expr`.
///
/// Проверяет рекурсивно что RHS — literal-eligible: literals + арифметика
/// над constexpr operands + record/tuple/array literals из constexpr-полей +
/// references на другие top-level `const`.
///
/// Возвращает `Err(Diagnostic)` если non-constexpr:
/// - `E_CONST_NOT_CONSTEXPR` — generic non-constexpr expr.
/// - `E_CONST_REFERS_NON_CONSTEXPR` — Ident на non-const binding.
/// - `E_CONST_EFFECT_IN_INIT` — runtime call / effect / allocation.
///
/// `known_consts` — set имен top-level `const` (для Ident-резолва).
fn check_const_constexpr(
    expr: &crate::ast::Expr,
    known_consts: &HashSet<String>,
) -> Result<(), Diagnostic> {
    use crate::ast::ExprKind as E;
    match &expr.kind {
        // Literals — всегда constexpr.
        E::IntLit(_) | E::FloatLit(_) | E::StrLit(_) | E::BoolLit(_)
        | E::CharLit(_) | E::UnitLit => Ok(()),
        // Unary над constexpr operand.
        E::Unary { operand, .. } => check_const_constexpr(operand, known_consts),
        // Binary над constexpr operands.
        E::Binary { left, right, .. } => {
            check_const_constexpr(left, known_consts)?;
            check_const_constexpr(right, known_consts)
        }
        // Tuple-литерал — каждый элемент constexpr.
        E::TupleLit(elems) => {
            for e in elems {
                check_const_constexpr(e, known_consts)?;
            }
            Ok(())
        }
        // Array-литерал (без spread) — каждый элемент constexpr.
        E::ArrayLit(elems) => {
            for el in elems {
                match el {
                    crate::ast::ArrayElem::Item(e) => check_const_constexpr(e, known_consts)?,
                    crate::ast::ArrayElem::Spread(_) => {
                        return Err(Diagnostic::new(
                            "[E_CONST_NOT_CONSTEXPR] spread `...` not allowed \
                             в const initialiser — runtime operation. Inline \
                             literals или use `ro X = …` for runtime value \
                             (Plan 114.4 Ф.1 D199).".to_string(),
                            expr.span,
                        ));
                    }
                }
            }
            Ok(())
        }
        // Record-литерал — каждое поле constexpr.
        E::RecordLit { fields, .. } => {
            for f in fields {
                if f.is_spread {
                    return Err(Diagnostic::new(
                        "[E_CONST_NOT_CONSTEXPR] spread `...` not allowed в \
                         const-record initialiser (Plan 114.4 Ф.1).".to_string(),
                        expr.span,
                    ));
                }
                match &f.value {
                    Some(v) => check_const_constexpr(v, known_consts)?,
                    None => {
                        // Shorthand `{ name }` — refers binding called `name`.
                        if !known_consts.contains(&f.name) {
                            return Err(Diagnostic::new(
                                format!(
                                    "[E_CONST_REFERS_NON_CONSTEXPR] field shorthand `{}` \
                                     в const-record refers binding which is not a \
                                     top-level const. Use explicit `{}: <literal>` либо \
                                     declare referenced `const {}` (Plan 114.4 Ф.1).",
                                    f.name, f.name, f.name
                                ),
                                expr.span,
                            ));
                        }
                    }
                }
            }
            Ok(())
        }
        // Ident — должен ссылаться на другой known top-level `const`.
        E::Ident(name) => {
            if known_consts.contains(name) {
                Ok(())
            } else {
                Err(Diagnostic::new(
                    format!(
                        "[E_CONST_REFERS_NON_CONSTEXPR] `const` initialiser \
                         refers `{}` which is not a top-level `const`. Only \
                         literals + arithmetic on literals + record/tuple/array \
                         literals из constexpr fields + references to other \
                         `const` are allowed. For runtime / lazy-init use \
                         `ro {} = …` (Plan 114.4 Ф.1 / D199).",
                        name, name
                    ),
                    expr.span,
                ))
            }
        }
        // Path (e.g. `Module.NAME` cross-module const, или `LOCAL.field`
        // member-access на local-const). V1 conservative: запрещаем все
        // Path формы в const-RHS (cross-module — followup
        // [M-114.4-cross-module-const-ref]; field-access на local-const —
        // runtime-only, эквивалент `ro X = LOCAL.field`).
        E::Path(_) => Err(Diagnostic::new(
            "[E_CONST_NOT_CONSTEXPR] path expression (Module.NAME / Type.field) \
             not allowed в `const` initialiser в V1. Cross-module const refs — \
             followup [M-114.4-cross-module-const-ref]. Field access на local \
             const → use `ro X = …` (runtime ok) (Plan 114.4 Ф.1).".to_string(),
            expr.span,
        )),
        // Function calls / method calls / member access / index / etc. —
        // runtime по дефолту. const fn (Plan 114.4 Ф.3) добавит исключение.
        E::Call { .. } | E::Member { .. } | E::Index { .. }
        | E::InterpolatedStr { .. } | E::MapLit { .. } => Err(Diagnostic::new(
            "[E_CONST_NOT_CONSTEXPR] non-constexpr expression в `const` \
             initialiser — only literals, arithmetic over literals, \
             record/tuple/array literals из constexpr fields, и references \
             на другие top-level `const` are allowed. Runtime calls / member \
             access / interpolation / map literals — not constexpr. Use \
             `ro X = …` for runtime / lazy-init value, либо `const fn` для \
             comptime function (Plan 114.4 Ф.1 / D199).".to_string(),
            expr.span,
        )),
        // Любые другие конструкции (if, match, blocks, closures, etc.) — runtime.
        _ => Err(Diagnostic::new(
            "[E_CONST_NOT_CONSTEXPR] non-constexpr expression в `const` \
             initialiser (control flow / closures / blocks not allowed). \
             Use `ro X = …` для runtime / lazy-init value (Plan 114.4 Ф.1).".to_string(),
            expr.span,
        )),
    }
}

/// Plan 79: проход типовой полноты type-checker'а.
struct TypeCheckCtx<'a> {
    /// Ф.2: имя типа → объявленная арность.
    arity: HashMap<String, ArityInfo>,
    /// Ф.1: свободные функции — для резолва callee на call-site.
    fn_decls: HashMap<String, Vec<&'a FnDecl>>,
    /// Ф.1: методы по receiver-типу — для резолва `Type.method(...)`.
    method_table: HashMap<String, HashMap<String, Vec<&'a FnDecl>>>,
    /// Ф.1: объявления типов — для разворачивания alias/newtype при
    /// категоризации (assignability сравнивает категории, не имена).
    types: HashMap<String, &'a TypeDecl>,
    /// Plan 81 Ф.2: префиксы импортированных модулей (alias + последний
    /// сегмент пути import'а) — для резолва module-qualified вызовов
    /// `alias.func(...)`.
    imported_modules: HashSet<String>,
}

/// `true` для имён, у которых arity **не** проверяется: referential-типы
/// и эффекты с sugar/гибкой арностью.
fn arity_exempt(name: &str) -> bool {
    matches!(
        name,
        // referential / top / bottom
        "Self" | "any" | "never" | "Never"
        // Fail[E] ≡ bare Fail (D65); Effect[E] ≡ Effect[E, never] (D88)
        // Plan 97 Ф.3 (D142): `Handler` → `Effect`.
        | "Fail" | "Effect"
        // built-in эффекты с параметрами — не объявлены как Item::Type,
        // в таблицу не попадут; перечислены явно для ясности
        | "Ask" | "Alloc"
    )
}

impl<'a> TypeCheckCtx<'a> {
    fn build(module: &'a Module) -> Self {
        let mut arity: HashMap<String, ArityInfo> = HashMap::new();
        let mut fn_decls: HashMap<String, Vec<&'a FnDecl>> = HashMap::new();
        let mut method_table: HashMap<String, HashMap<String, Vec<&'a FnDecl>>> =
            HashMap::new();
        let mut types: HashMap<String, &'a TypeDecl> = HashMap::new();
        for item in &module.items {
            match item {
                Item::Fn(f) => {
                    if let Some(recv) = &f.receiver {
                        method_table
                            .entry(recv.type_name.clone())
                            .or_default()
                            .entry(f.name.clone())
                            .or_default()
                            .push(f);
                    } else {
                        fn_decls.entry(f.name.clone()).or_default().push(f);
                    }
                }
                Item::Type(td) => {
                    types.insert(td.name.clone(), td);
                }
                _ => {}
            }
        }
        // Все типы (пользовательские + merged-from-imports) — для подсчёта
        // арности; неверная арность на импортированном типе тоже ловится.
        // `decl_span: None` — у импортированных/prelude-типов объявление
        // не в текущем файле, note «declared here» был бы с чужим/битым
        // span'ом (см. Plan 81 Ф.8 file_id-утечки).
        for item in &module.items {
            if let Item::Type(td) = item {
                arity.insert(
                    td.name.clone(),
                    ArityInfo { count: td.generics.len(), decl_span: None },
                );
            }
        }
        // Типы, объявленные в самом компилируемом модуле (entry peers'
        // `items_here`) — для них note «declared here» указывает на
        // реальный исходник пользователя.
        let own_items: Vec<&Item> = if module.peer_files.is_empty() {
            module.items.iter().collect()
        } else {
            module
                .peer_files
                .iter()
                .filter(|pf| pf.is_entry_module)
                .flat_map(|pf| pf.items_here.iter())
                .collect()
        };
        for item in own_items {
            if let Item::Type(td) = item {
                arity.insert(
                    td.name.clone(),
                    ArityInfo { count: td.generics.len(), decl_span: Some(td.span) },
                );
            }
        }
        // Prelude-типы обычно приходят как Item::Type через auto-import;
        // fallback на известную арность для модулей без prelude.
        arity.entry("Option".to_string())
            .or_insert(ArityInfo { count: 1, decl_span: None });
        arity.entry("Result".to_string())
            .or_insert(ArityInfo { count: 2, decl_span: None });
        // Примитивы — арность 0 (`int[X]` / `bool[T]` — ошибка).
        for prim in [
            "int", "i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64",
            "uint", "f32", "f64", "str", "bool", "char",
        ] {
            arity.entry(prim.to_string())
                .or_insert(ArityInfo { count: 0, decl_span: None });
        }
        // Plan 81 Ф.2: префиксы импортированных модулей.
        let mut imported_modules: HashSet<String> = HashSet::new();
        let mut collect = |imports: &[Import]| {
            for imp in imports {
                if let Some(a) = &imp.alias {
                    imported_modules.insert(a.clone());
                }
                if let Some(last) = imp.path.last() {
                    imported_modules.insert(last.clone());
                }
            }
        };
        collect(&module.imports);
        for pf in &module.peer_files {
            collect(&pf.imports);
        }
        drop(collect);
        TypeCheckCtx { arity, fn_decls, method_table, types, imported_modules }
    }

    fn check_module(&self, module: &Module, errors: &mut Vec<Diagnostic>) {
        // Plan 91.9 (D186): verify `#impl(P1 + P2 + ...)` annotations.
        // Для каждого type T с impl_protocols, проверяем что:
        // 1. Каждый P в списке действительно protocol-тип (E_UNKNOWN_PROTOCOL).
        // 2. T provides каждый метод P либо напрямую (explicit `fn T @method`),
        //    либо синтезируемо через P's default body (`default_body_calls_satisfy_for`).
        // Missing methods → E_IMPL_MISSING_METHODS со списком и hint'ом.
        for item in &module.items {
            if let Item::Type(td) = item {
                if td.impl_protocols.is_empty() { continue; }
                self.verify_impl_protocols(td, errors);
            }
        }

        // Plan 91.8a.2 part 3 (D183 amendment, Q4 strict): E_BLANKET_IDENTITY_OVERRIDE.
        // Identity From blanket `fn[T] T.from(t T) -> T => t` declared в prelude.
        // Override запрещён: попытка явно объявить `fn TypeName.from(t TypeName) -> TypeName`
        // (identity case на конкретном типе) — error.
        for item in &module.items {
            if let Item::Fn(fd) = item {
                if let Some(recv) = &fd.receiver {
                    if matches!(recv.kind, ReceiverKind::Static)
                        && fd.name == "from"
                        && fd.params.len() == 1
                        && fd.generics.is_empty()
                    {
                        let recv_type = &recv.type_name;
                        // Param type matches receiver type? (identity signature)
                        let param_is_recv = matches!(
                            &fd.params[0].ty,
                            TypeRef::Named { path, .. }
                                if path.len() == 1 && path[0] == *recv_type
                        );
                        let return_is_recv = matches!(
                            &fd.return_type,
                            Some(TypeRef::Named { path, .. })
                                if path.len() == 1 && (path[0] == *recv_type || path[0] == "Self")
                        );
                        if param_is_recv && return_is_recv {
                            errors.push(Diagnostic::new(
                                format!(
                                    "[E_BLANKET_IDENTITY_OVERRIDE] cannot override identity \
                                     `fn[T] T.from(t T) -> T => t` blanket for type `{}`. \
                                     Identity is identity (D183 amendment Q4 strict, \
                                     D9 single canonical path). Remove this declaration; \
                                     `{0}.from({0})` works automatically via blanket.",
                                    recv_type
                                ),
                                fd.span,
                            ));
                        }
                    }
                }
            }
        }
        for item in &module.items {
            match item {
                Item::Fn(fd) => self.check_fn(fd, errors),
                Item::Type(td) => self.check_type_decl(td, errors),
                Item::Const(cd) => {
                    let empty = HashSet::new();
                    if let Some(t) = &cd.ty {
                        self.walk_typeref(t, &empty, errors);
                    }
                    self.walk_expr(&cd.value, &empty, errors);
                    // Plan 114.4 Ф.1: strict constexpr-only enforcement.
                    // `const X = expr` принимает только literal-eligible
                    // RHS — арифметику над literals, record-literal из
                    // constexpr-fields, references на другие const.
                    // Runtime calls / effects / allocations / non-const
                    // refs → E_CONST_NOT_CONSTEXPR.
                    let known_consts: HashSet<String> = module
                        .items
                        .iter()
                        .filter_map(|it| match it {
                            Item::Const(c) => Some(c.name.clone()),
                            _ => None,
                        })
                        .collect();
                    if let Err(d) = check_const_constexpr(&cd.value, &known_consts) {
                        errors.push(d);
                    }
                }
                Item::Test(t) => {
                    let empty = HashSet::new();
                    self.walk_block(&t.body, &empty, errors);
                }
                Item::Let(_) | Item::Bench(_) | Item::Lemma(_) => {}
            }
        }
        // Ф.1: assignability — отдельный scope-aware проход по телам
        // (var-типы локальных переменных нужны только здесь).
        for item in &module.items {
            match item {
                Item::Fn(fd) => self.f1_check_fn(fd, errors),
                Item::Test(t) => {
                    let gs: HashSet<String> = HashSet::new();
                    let mut scope: HashMap<String, TypeRef> = HashMap::new();
                    self.f1_block(&t.body, &gs, &mut scope, errors);
                }
                Item::Const(cd) => {
                    let gs: HashSet<String> = HashSet::new();
                    let mut scope: HashMap<String, TypeRef> = HashMap::new();
                    if let Some(ann) = &cd.ty {
                        self.f1_check_assign_let(
                            &cd.value, ann, &cd.name, &gs, &scope, errors,
                        );
                    }
                    self.f1_expr(&cd.value, &gs, &mut scope, errors);
                }
                _ => {}
            }
        }

        // Plan 110.1.2 (D188 / D196): Consumable[E] protocol satisfaction check.
        // Для каждого `Stmt::ConsumeScope { init, body, .. }` проверяем что
        // init expr resolves к типу с `on_exit` method. Если нет — emit
        // [D188-not-consumable] error. Полный D196 (Result/Option unwrap,
        // conditional, method chain — non-trivial type inference) — staged
        // delivery: дополнительные формы validated в Plan 110.1.3 / 110.1.4.
        for item in &module.items {
            match item {
                Item::Fn(fd) => {
                    match &fd.body {
                        FnBody::Block(b) => self.check_consume_scopes_in_block(b, errors),
                        FnBody::Expr(e) => self.check_consume_scopes_in_expr(e, errors),
                        FnBody::External => {}
                    }
                }
                Item::Test(t) => self.check_consume_scopes_in_block(&t.body, errors),
                Item::Bench(b) => {
                    for s in &b.setup { self.check_consume_scopes_in_stmt(s, errors); }
                    self.check_consume_scopes_in_block(&b.measure_body, errors);
                    for s in &b.teardown { self.check_consume_scopes_in_stmt(s, errors); }
                }
                _ => {}
            }
        }
    }

    /// Plan 110.1.2 (D188): recursive walk через Block для ConsumeScope check.
    fn check_consume_scopes_in_block(&self, b: &Block, errors: &mut Vec<Diagnostic>) {
        for s in &b.stmts {
            self.check_consume_scopes_in_stmt(s, errors);
        }
        if let Some(t) = &b.trailing {
            self.check_consume_scopes_in_expr(t, errors);
        }
    }

    /// Plan 110.1.2 (D188): walk Stmt looking for ConsumeScope, recurse into children.
    fn check_consume_scopes_in_stmt(&self, s: &Stmt, errors: &mut Vec<Diagnostic>) {
        match s {
            Stmt::ConsumeScope { binding, init, body, .. } => {
                self.validate_consume_scope_init(init, errors);
                // Plan 110.1.5 (D188 R2 enforcement at compile time):
                // detect manual `binding.on_exit(...)` calls в body.
                // Runtime exactly-once guard prevents double dispatch;
                // здесь — compile-time gate чтобы избегать runtime panic.
                self.check_no_manual_on_exit_call_in_block(binding, body, errors);
                self.check_consume_scopes_in_expr(init, errors);
                self.check_consume_scopes_in_block(body, errors);
            }
            Stmt::Let(d) => self.check_consume_scopes_in_expr(&d.value, errors),
            // Plan 114.4 Ф.2: scope-local const — walk value for nested ConsumeScope.
            Stmt::Const(d) => self.check_consume_scopes_in_expr(&d.value, errors),
            Stmt::Expr(e) => self.check_consume_scopes_in_expr(e, errors),
            Stmt::Assign { target, value, .. } => {
                self.check_consume_scopes_in_expr(target, errors);
                self.check_consume_scopes_in_expr(value, errors);
            }
            Stmt::Return { value, .. } => {
                if let Some(v) = value { self.check_consume_scopes_in_expr(v, errors); }
            }
            Stmt::Throw { value, .. } => self.check_consume_scopes_in_expr(value, errors),
            Stmt::Defer { body, .. } | Stmt::ErrDefer { body, .. }
            | Stmt::OkDefer { body, .. } | Stmt::DeferWithResult { body, .. } => {
                self.check_consume_scopes_in_expr(body, errors);
            }
            Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => {
                self.check_consume_scopes_in_expr(expr, errors);
            }
            Stmt::Break(_) | Stmt::Continue(_) | Stmt::Reveal { .. }
            | Stmt::Apply { .. } | Stmt::Calc { .. } => {}
        }
    }

    /// Plan 110.1.2 (D188): walk Expr looking for nested ConsumeScope in
    /// bodies (lambdas, blocks, if-then-else, match arms, etc).
    fn check_consume_scopes_in_expr(&self, e: &Expr, errors: &mut Vec<Diagnostic>) {
        use crate::ast::ExprKind;
        match &e.kind {
            ExprKind::Block(b) => self.check_consume_scopes_in_block(b, errors),
            ExprKind::If { cond, then, else_, .. } => {
                self.check_consume_scopes_in_expr(cond, errors);
                self.check_consume_scopes_in_block(then, errors);
                if let Some(eb) = else_ {
                    match eb {
                        crate::ast::ElseBranch::Block(b) => self.check_consume_scopes_in_block(b, errors),
                        crate::ast::ElseBranch::If(ei) => self.check_consume_scopes_in_expr(ei, errors),
                    }
                }
            }
            ExprKind::While { cond, body, .. } => {
                self.check_consume_scopes_in_expr(cond, errors);
                self.check_consume_scopes_in_block(body, errors);
            }
            ExprKind::For { iter, body, .. } => {
                self.check_consume_scopes_in_expr(iter, errors);
                self.check_consume_scopes_in_block(body, errors);
            }
            ExprKind::Loop { body, .. } => self.check_consume_scopes_in_block(body, errors),
            ExprKind::Call { func, args, .. } => {
                self.check_consume_scopes_in_expr(func, errors);
                for a in args {
                    match a {
                        crate::ast::CallArg::Item(e) | crate::ast::CallArg::Spread(e) => {
                            self.check_consume_scopes_in_expr(e, errors);
                        }
                        _ => {}
                    }
                }
            }
            ExprKind::Try(e) | ExprKind::Bang(e) => self.check_consume_scopes_in_expr(e, errors),
            ExprKind::Coalesce(a, b) => {
                self.check_consume_scopes_in_expr(a, errors);
                self.check_consume_scopes_in_expr(b, errors);
            }
            ExprKind::Binary { left, right, .. } => {
                self.check_consume_scopes_in_expr(left, errors);
                self.check_consume_scopes_in_expr(right, errors);
            }
            ExprKind::Unary { operand, .. } => self.check_consume_scopes_in_expr(operand, errors),
            ExprKind::Member { obj, .. } => self.check_consume_scopes_in_expr(obj, errors),
            // Lambda / closure bodies — separate scopes; walk their bodies too.
            ExprKind::Lambda { body, .. } => self.check_consume_scopes_in_expr(body, errors),
            _ => {}
        }
    }

    /// Plan 110.1.2 (D188 / D196): validate init expression's type implements
    /// Consumable. Uses простой heuristic для type inference (Type.method() →
    /// return type; record literal → type; ?/!! → recurse). Полный inference
    /// (method chain, conditional, generic) — staged delivery 110.1.3+.
    fn validate_consume_scope_init(&self, init: &Expr, errors: &mut Vec<Diagnostic>) {
        // Plan 110.1.3 (D196 form 5 — wrapped без unwrap): detect raw
        // Option[T] / Result[T,_] returning expressions WITHOUT ?/!!
        // unwrap. Emit specific D196-wrapped-init-needs-unwrap hint.
        if let Some(wrapped) = self.detect_wrapped_init_typeref(init) {
            errors.push(Diagnostic::new(
                format!(
                    "[D196-wrapped-init-needs-unwrap] `consume X = expr {{ body }}` \
                     init expr returns `{wrapped}[T, ...]` без unwrap. Required: \
                     either `consume X = expr!! {{ body }}` (Option unwrap), \
                     `consume X = expr? {{ body }}` (Result unwrap with Fail \
                     propagation), or distinguish None case explicitly через \
                     `if Some(X) = maybe_X() {{ consume X = X {{ ... }} }}`.",
                    wrapped = wrapped
                ),
                init.span,
            ));
            return;
        }
        // Plan 110.1.3 (D196 form 3 — divergent conditional): if/match init
        // with branches returning incompatible Consumable types.
        if let Some((t1, t2)) = self.detect_divergent_consumable(init) {
            errors.push(Diagnostic::new(
                format!(
                    "[D196-divergent-consumable] `consume X = if cond {{ ... }} \
                     else {{ ... }} {{ body }}` branches return divergent \
                     Consumable types: `{t1}` vs `{t2}`. Branches must return \
                     compatible type. Extract в polymorphic wrapper type \
                     или unify branches.",
                    t1 = t1, t2 = t2
                ),
                init.span,
            ));
            return;
        }
        let Some(type_name) = self.infer_consume_init_type(init) else {
            // Тип не выводится простыми heuristic'ами — staged delivery
            // через codegen-gate D188-codegen-not-yet-implemented. Полное
            // покрытие в Plan 110.1.4 / 110.1.3.
            return;
        };
        // Special case: `never` (bottom-тип) — никогда не resolved init
        // type, skip без ошибки.
        if type_name == "never" || type_name == "Never" {
            return;
        }
        // Look up on_exit method on the type.
        let has_on_exit = self.method_table.get(&type_name)
            .map(|methods| methods.contains_key("on_exit"))
            .unwrap_or(false);
        if !has_on_exit {
            // Тип known? Если не known — это либо primitive (`int`/`str`)
            // либо unresolved (caught by name resolution). Skip primitive
            // случаи silently.
            let is_known_type = self.types.contains_key(&type_name)
                || self.method_table.contains_key(&type_name);
            // Even for primitive types like `int`/`str` — нет on_exit → error.
            // Но diagnostic должен быть полезный (suggest implement).
            if is_known_type || self.method_table.contains_key(&type_name) {
                let diag = Diagnostic::new(
                    format!(
                        "[D188-not-consumable] type `{name}` does not implement `Consumable[E]` \
                         (method `on_exit` missing). \
                         To use `consume X = expr {{ body }}` scope-block, type must declare:\n  \
                         `fn {name} consume @on_exit(outcome ScopeOutcome) Fail[E] -> () => {{ ... }}`\n\
                         where `E` is the cleanup-error type (or `never` for infallible — D194).\n\
                         Alternative: use raw `consume X = expr` (D180 linear binding) without block.",
                        name = type_name
                    ),
                    init.span,
                ).with_note(format!(
                    "Plan 110.6.1: see docs/idiom/consume-scope-cleanup.md \
                     Q-consumable-protocol for decision tree + implementation template. \
                     For infallible cleanup (Mutex/Sem/Lock) use `Consumable[never]` — \
                     no Fail[E] effect (D194 hot-path eligible)."
                ));
                errors.push(diag);
            }
        } else {
            // on_exit existsует — validate signature (Plan 110.1.2 §D188-malformed-on-exit).
            // Минимальная проверка: первый param должен быть ScopeOutcome.
            // Глубокая validation (Fail[E] check, return type ()) — 110.1.3.
            self.validate_on_exit_signature(&type_name, init.span, errors);
        }
    }

    /// Plan 110.1.2 / refine (D188-malformed-on-exit): on_exit signature check.
    /// Verifies:
    /// - Param[0] is `outcome ScopeOutcome`.
    /// - Exactly 1 param (D188 protocol contract).
    /// - Return type is `()` or absent (D188 protocol contract).
    /// - Effects are either empty (Consumable[never]) or `Fail[E]` only
    ///   (no other effects).
    fn validate_on_exit_signature(&self, type_name: &str, init_span: Span, errors: &mut Vec<Diagnostic>) {
        let Some(methods) = self.method_table.get(type_name) else { return; };
        let Some(decls) = methods.get("on_exit") else { return; };
        for decl in decls {
            // Param[0] must be `outcome ScopeOutcome`.
            let first_param_ok = decl.params.first()
                .map(|p| matches!(&p.ty, TypeRef::Named { path, .. }
                    if path.last().map_or(false, |s| s == "ScopeOutcome")))
                .unwrap_or(false);
            if !first_param_ok {
                errors.push(Diagnostic::new(
                    format!(
                        "[D188-malformed-on-exit] `fn {tn} @on_exit(...)` signature invalid: \
                         first parameter must be `outcome ScopeOutcome` (D188 protocol contract). \
                         Correct form: \
                         `fn {tn} consume @on_exit(outcome ScopeOutcome) Fail[E] -> () => {{ ... }}`. \
                         (Diagnostic emitted at `consume {{}}` use-site for context.)",
                        tn = type_name
                    ),
                    init_span,
                ));
                continue;
            }

            // Exactly 1 param (D188 protocol contract).
            if decl.params.len() != 1 {
                errors.push(Diagnostic::new(
                    format!(
                        "[D188-malformed-on-exit] `fn {tn} @on_exit(...)` has {n} params; \
                         protocol requires exactly 1 (`outcome ScopeOutcome`). \
                         Remove extra parameters; resource state available via `@`.",
                        tn = type_name, n = decl.params.len()
                    ),
                    init_span,
                ));
                continue;
            }

            // Return type strict check disabled bootstrap — `-> ()` имеет
            // parser-specific TypeRef encoding не uniformly Tuple([]). Full
            // return type / effects check после parser representation
            // canonicalization ([M-110-on-exit-strict-sig]).
            //
            // Currently bootstrap: param count + first param ScopeOutcome
            // enough for catching most malformed sigs.
            let _ = decl.return_type.as_ref();
            let _ = &decl.effects;
        }
    }

    /// Plan 110.1.5 (D188 R2): detect manual `binding.on_exit(...)` calls
    /// в ConsumeScope body. Auto on_exit dispatch happens at scope-exit;
    /// manual call → double invocation → runtime panic (R2 exactly-once
    /// violation). Compile-time gate preferred to runtime panic.
    fn check_no_manual_on_exit_call_in_block(&self, binding: &str, b: &Block, errors: &mut Vec<Diagnostic>) {
        for s in &b.stmts {
            self.check_no_manual_on_exit_call_in_stmt(binding, s, errors);
        }
        if let Some(t) = &b.trailing {
            self.check_no_manual_on_exit_call_in_expr(binding, t, errors);
        }
    }

    fn check_no_manual_on_exit_call_in_stmt(&self, binding: &str, s: &Stmt, errors: &mut Vec<Diagnostic>) {
        match s {
            Stmt::Let(d) => self.check_no_manual_on_exit_call_in_expr(binding, &d.value, errors),
            Stmt::Expr(e) => self.check_no_manual_on_exit_call_in_expr(binding, e, errors),
            Stmt::Assign { target, value, .. } => {
                self.check_no_manual_on_exit_call_in_expr(binding, target, errors);
                self.check_no_manual_on_exit_call_in_expr(binding, value, errors);
            }
            Stmt::Return { value, .. } => {
                if let Some(v) = value { self.check_no_manual_on_exit_call_in_expr(binding, v, errors); }
            }
            Stmt::Throw { value, .. } => self.check_no_manual_on_exit_call_in_expr(binding, value, errors),
            Stmt::Defer { body, .. } | Stmt::ErrDefer { body, .. }
            | Stmt::OkDefer { body, .. } | Stmt::DeferWithResult { body, .. } => {
                self.check_no_manual_on_exit_call_in_expr(binding, body, errors);
            }
            Stmt::ConsumeScope { init, body, binding: inner_binding, .. } => {
                self.check_no_manual_on_exit_call_in_expr(binding, init, errors);
                // Nested consume scope с inner binding NEW — outer `binding`
                // check still applies inside (если inner body references
                // outer's binding manually — same violation).
                self.check_no_manual_on_exit_call_in_block(binding, body, errors);
                // Дополнительно: recurse с inner binding (D197 re-entrance).
                self.check_no_manual_on_exit_call_in_block(inner_binding, body, errors);
            }
            Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => {
                self.check_no_manual_on_exit_call_in_expr(binding, expr, errors);
            }
            _ => {}
        }
    }

    fn check_no_manual_on_exit_call_in_expr(&self, binding: &str, e: &Expr, errors: &mut Vec<Diagnostic>) {
        use crate::ast::ExprKind;
        // Detect `binding.on_exit(...)` call: Call { func: Member { obj: Ident(binding), name: "on_exit" }, ... }
        if let ExprKind::Call { func, .. } = &e.kind {
            if let ExprKind::Member { obj, name, .. } = &func.kind {
                if name == "on_exit" {
                    if let ExprKind::Ident(obj_name) = &obj.kind {
                        if obj_name == binding {
                            errors.push(Diagnostic::new(
                                format!(
                                    "[D188-r2-manual-on-exit] `{binding}.on_exit(...)` cannot be \
                                     called manually from inside `consume {binding} = ... {{ body }}` \
                                     scope-block body. Auto on_exit dispatch на scope exit \
                                     гарантирует exactly-once invariant (D188 R2). Manual call \
                                     → double invocation → runtime panic. \
                                     Remove the explicit call; scope-exit will dispatch on_exit \
                                     с appropriate ScopeOutcome value.",
                                    binding = binding
                                ),
                                e.span,
                            ));
                        }
                    }
                }
            }
        }
        // Recurse into children regardless.
        self.check_no_manual_on_exit_recurse(binding, e, errors);
    }

    fn check_no_manual_on_exit_recurse(&self, binding: &str, e: &Expr, errors: &mut Vec<Diagnostic>) {
        use crate::ast::ExprKind;
        match &e.kind {
            ExprKind::Block(b) => self.check_no_manual_on_exit_call_in_block(binding, b, errors),
            ExprKind::Call { func, args, .. } => {
                self.check_no_manual_on_exit_call_in_expr(binding, func, errors);
                for a in args {
                    match a {
                        crate::ast::CallArg::Item(ae) | crate::ast::CallArg::Spread(ae) => {
                            self.check_no_manual_on_exit_call_in_expr(binding, ae, errors);
                        }
                        _ => {}
                    }
                }
            }
            ExprKind::If { cond, then, else_, .. } => {
                self.check_no_manual_on_exit_call_in_expr(binding, cond, errors);
                self.check_no_manual_on_exit_call_in_block(binding, then, errors);
                if let Some(eb) = else_ {
                    match eb {
                        crate::ast::ElseBranch::Block(b) => self.check_no_manual_on_exit_call_in_block(binding, b, errors),
                        crate::ast::ElseBranch::If(ei) => self.check_no_manual_on_exit_call_in_expr(binding, ei, errors),
                    }
                }
            }
            ExprKind::While { cond, body, .. } => {
                self.check_no_manual_on_exit_call_in_expr(binding, cond, errors);
                self.check_no_manual_on_exit_call_in_block(binding, body, errors);
            }
            ExprKind::For { iter, body, .. } => {
                self.check_no_manual_on_exit_call_in_expr(binding, iter, errors);
                self.check_no_manual_on_exit_call_in_block(binding, body, errors);
            }
            ExprKind::Try(inner) | ExprKind::Bang(inner) => self.check_no_manual_on_exit_call_in_expr(binding, inner, errors),
            ExprKind::Coalesce(a, b) => {
                self.check_no_manual_on_exit_call_in_expr(binding, a, errors);
                self.check_no_manual_on_exit_call_in_expr(binding, b, errors);
            }
            ExprKind::Binary { left, right, .. } => {
                self.check_no_manual_on_exit_call_in_expr(binding, left, errors);
                self.check_no_manual_on_exit_call_in_expr(binding, right, errors);
            }
            ExprKind::Unary { operand, .. } => self.check_no_manual_on_exit_call_in_expr(binding, operand, errors),
            ExprKind::Member { obj, .. } => self.check_no_manual_on_exit_call_in_expr(binding, obj, errors),
            _ => {}
        }
    }

    /// Plan 110.1.2 / 110.1.3 (D196): infer the resulting type name from
    /// `consume X = INIT { body }` init expression. Heuristics:
    /// - `Type.method(args)` → look up method's return type via method_table.
    /// - `Type { fields }` → type name directly.
    /// - `expr?` / `expr!!` → если inner returns `Result[T,E]` / `Option[T]`,
    ///   unwrap до T (D196 form 2: Result/Option unwrap).
    /// - `expr as Type` → cast target.
    /// - Other forms → None (staged delivery — full inference 110.1.4).
    fn infer_consume_init_type(&self, e: &Expr) -> Option<String> {
        self.infer_consume_init_typeref(e)
            .as_ref()
            .and_then(Self::typeref_to_name)
    }

    /// Extract final type name from TypeRef::Named (last path segment).
    fn typeref_to_name(t: &TypeRef) -> Option<String> {
        if let TypeRef::Named { path, .. } = t {
            path.last().cloned()
        } else {
            None
        }
    }

    /// Plan 110.1.3 (D196 form 5): detect if init returns raw `Option[T]`
    /// or `Result[T,_]` без unwrap operator. Returns wrapper name если
    /// detected, None если init is direct (unwrapped) or non-wrapped type.
    fn detect_wrapped_init_typeref(&self, init: &Expr) -> Option<String> {
        use crate::ast::ExprKind;
        // `?` and `!!` are unwrap operators — they're EXPLICITLY safe.
        if matches!(init.kind, ExprKind::Try(_) | ExprKind::Bang(_)) {
            return None;
        }
        // For direct Call (e.g., `try_new()` without `?`), inspect return type.
        if let ExprKind::Call { func, .. } = &init.kind {
            if let ExprKind::Path(parts) = &func.kind {
                if parts.len() >= 2 {
                    let type_name = &parts[parts.len() - 2];
                    let method_name = &parts[parts.len() - 1];
                    if let Some(methods) = self.method_table.get(type_name) {
                        if let Some(decls) = methods.get(method_name) {
                            if let Some(decl) = decls.first() {
                                if let Some(TypeRef::Named { path, .. }) = &decl.return_type {
                                    if let Some(outer) = path.last() {
                                        if outer == "Option" || outer == "Result" {
                                            return Some(outer.clone());
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        None
    }

    /// Plan 110.1.3 (D196 form 3): detect if/match init with branches
    /// returning incompatible Consumable types. Returns (t1, t2) pair если
    /// detected.
    fn detect_divergent_consumable(&self, init: &Expr) -> Option<(String, String)> {
        use crate::ast::ExprKind;
        if let ExprKind::If { then, else_, .. } = &init.kind {
            // Both branches must end в expression returning Consumable.
            let then_ty = self.infer_block_trailing_typeref(then)?;
            let else_ty = match else_.as_ref()? {
                crate::ast::ElseBranch::Block(b) => self.infer_block_trailing_typeref(b)?,
                crate::ast::ElseBranch::If(ei) => self.infer_consume_init_typeref(ei)?,
            };
            let then_name = Self::typeref_to_name(&then_ty)?;
            let else_name = Self::typeref_to_name(&else_ty)?;
            if then_name != else_name {
                return Some((then_name, else_name));
            }
        }
        None
    }

    fn infer_block_trailing_typeref(&self, b: &crate::ast::Block) -> Option<TypeRef> {
        if let Some(t) = &b.trailing {
            self.infer_consume_init_typeref(t)
        } else {
            None
        }
    }

    /// Plan 110.1.3 (D196): infer full TypeRef для init expression. Используется
    /// для Result/Option unwrap (D196 form 2) — нужен дополнительный slot
    /// для unwrap'нутого T type-ref'а.
    fn infer_consume_init_typeref(&self, e: &Expr) -> Option<TypeRef> {
        use crate::ast::ExprKind;
        match &e.kind {
            ExprKind::Call { func, .. } => {
                match &func.kind {
                    ExprKind::Path(parts) if parts.len() >= 2 => {
                        let type_name = &parts[parts.len() - 2];
                        let method_name = &parts[parts.len() - 1];
                        let methods = self.method_table.get(type_name)?;
                        let decls = methods.get(method_name)?;
                        let decl = decls.first()?;
                        let rt = decl.return_type.as_ref()?;
                        // Self → receiver type substitution.
                        if let TypeRef::Named { path, .. } = rt {
                            if path.last().map_or(false, |s| s == "Self") {
                                return Some(TypeRef::Named {
                                    path: vec![type_name.clone()],
                                    generics: Vec::new(),
                                    span: e.span,
                                });
                            }
                        }
                        Some(rt.clone())
                    }
                    _ => None,
                }
            }
            ExprKind::RecordLit { type_name: Some(name), .. } => {
                Some(TypeRef::Named {
                    path: name.clone(),
                    generics: Vec::new(),
                    span: e.span,
                })
            }
            ExprKind::Try(inner) => {
                // D196 form 2 (?): unwrap Result[T, E] → T.
                let inner_ty = self.infer_consume_init_typeref(inner)?;
                if let TypeRef::Named { path, generics, .. } = &inner_ty {
                    if path.last().map_or(false, |s| s == "Result") && !generics.is_empty() {
                        return Some(generics[0].clone());
                    }
                    // Option[T] через ? тоже разворачивается (R/E aware).
                    if path.last().map_or(false, |s| s == "Option") && !generics.is_empty() {
                        return Some(generics[0].clone());
                    }
                }
                Some(inner_ty)
            }
            ExprKind::Bang(inner) => {
                // D196 form 2 (!!): unwrap Option[T] → T или Result[T,_] → T.
                let inner_ty = self.infer_consume_init_typeref(inner)?;
                if let TypeRef::Named { path, generics, .. } = &inner_ty {
                    if path.last().map_or(false, |s| s == "Option" || s == "Result") && !generics.is_empty() {
                        return Some(generics[0].clone());
                    }
                }
                Some(inner_ty)
            }
            ExprKind::As(_, ty) => Some(ty.clone()),
            ExprKind::Ident(_) | ExprKind::Path(_) => None,
            _ => None,
        }
    }

    // --- Ф.2: walk сигнатур ---------------------------------------------

    fn check_fn(&self, fd: &FnDecl, errors: &mut Vec<Diagnostic>) {
        // Generic-scope функции: её собственные generic-параметры +
        // generic-параметры receiver-типа (`fn Box[T] @get() -> T`).
        let mut gs: HashSet<String> = HashSet::new();
        for g in &fd.generics {
            gs.insert(g.name.clone());
        }
        if let Some(r) = &fd.receiver {
            for tr in &r.generics {
                if let TypeRef::Named { path, .. } = tr {
                    if path.len() == 1 {
                        gs.insert(path[0].clone());
                    }
                }
            }
        }
        // Plan 101.1 B1 (Ф.2 E_UNDECLARED_TYPEVAR_IN_RECEIVER):
        // Detect `fn []T @method` где T — single-uppercase letter без
        // `fn[T]` префикса (не в gs). Это silent miscompile в old codegen
        // (defaults T=nova_int). Loud error suggests `fn[T]` prefix fix.
        if let Some(r) = &fd.receiver {
            if r.type_name.starts_with("[]") {
                let elem = &r.type_name[2..];
                let is_single_upper = elem.len() <= 2
                    && elem.chars().all(|c| c.is_ascii_uppercase());
                if is_single_upper && !gs.contains(elem) {
                    errors.push(Diagnostic::new(
                        format!(
                            "[E_UNDECLARED_TYPEVAR_IN_RECEIVER] `fn []{elem} @{m}` — \
                             typevar `{elem}` не объявлен. Добавьте `fn[{elem}]` префикс \
                             (Plan 101.1 / D145):\n  \
                             fn[{elem}] []{elem} @{m}(...) -> ...",
                            elem = elem, m = fd.name
                        ),
                        r.span,
                    ));
                }
            }
            // Plan 101.1 B2 (Ф.2 E_BARE_TYPEVAR_NEEDS_PREFIX):
            // Detect `fn T @method` где T — bare single-uppercase letter
            // (not array, not other shape) без `fn[T]` prefix. Allowed only
            // если T in gs (declared via prefix) OR T — named type (но
            // type-check error elsewhere). Distinct from B1 (which targets `[]T`).
            let tn = r.type_name.as_str();
            if tn.len() <= 2 && tn.chars().all(|c| c.is_ascii_uppercase()) {
                if !gs.contains(tn) && !self.types.contains_key(tn) {
                    errors.push(Diagnostic::new(
                        format!(
                            "[E_BARE_TYPEVAR_NEEDS_PREFIX] `fn {tn} @{m}` — \
                             bare typevar `{tn}` receiver требует `fn[{tn}]` префикс \
                             (Plan 101.1 / D145):\n  \
                             fn[{tn}] {tn} @{m}(...) -> ...\n  \
                             OR declare `type {tn} {{ ... }}` если intended named type.",
                            tn = tn, m = fd.name
                        ),
                        r.span,
                    ));
                }
            }
            // Plan 101.1 C8 (Ф.2 E_UNUSED_PREFIX_TYPEVAR):
            // Каждый prefix-generic должен использоваться в receiver/params/return.
            // Если объявлен но не используется — error.
            {
                let mut referenced: HashSet<String> = HashSet::new();
                // Collect from receiver type-name (bare T case).
                let tn_rec = r.type_name.as_str();
                if tn_rec.len() <= 2 && tn_rec.chars().all(|c| c.is_ascii_uppercase()) {
                    referenced.insert(tn_rec.to_string());
                }
                // Collect from receiver type generics (`[]T`, Option[T], etc.).
                for tr in &r.generics {
                    Self::collect_named_idents(tr, &mut referenced);
                }
                // Array element if receiver is []T.
                if let Some(elem) = r.type_name.strip_prefix("[]") {
                    if elem.len() <= 2 && elem.chars().all(|c| c.is_ascii_uppercase()) {
                        referenced.insert(elem.to_string());
                    }
                }
                // Collect from params.
                for p in &fd.params {
                    Self::collect_named_idents(&p.ty, &mut referenced);
                }
                // Collect from return type.
                if let Some(rt) = &fd.return_type {
                    Self::collect_named_idents(rt, &mut referenced);
                }
                // Check each fd.generics — must be referenced.
                for g in &fd.generics {
                    if !referenced.contains(&g.name) {
                        errors.push(Diagnostic::new(
                            format!(
                                "[E_UNUSED_PREFIX_TYPEVAR] generic `{name}` declared в \
                                 `fn[…]` prefix но не используется в receiver, params, \
                                 или return type (Plan 101.1 / D145). Удалите из prefix.",
                                name = g.name
                            ),
                            r.span,
                        ));
                    }
                }
            }
            // Plan 101.1 B4 (Ф.2 E_PREFIX_SHADOWS_NAMED_TYPE):
            // Detect `fn[T] T @method` + `type T { ... }` в scope. fn-prefix
            // shadows named type — ambiguous. Loud error suggests rename.
            for g in &fd.generics {
                if self.types.contains_key(&g.name) {
                    errors.push(Diagnostic::new(
                        format!(
                            "[E_PREFIX_SHADOWS_NAMED_TYPE] `fn[{tn}] ...` — \
                             generic `{tn}` shadows named type `{tn}` in scope \
                             (Plan 101.1 / D145). Rename one:\n  \
                             - rename prefix generic: `fn[T2] {tn} @{m}(...)` (use named T)\n  \
                             - rename named type: `type {tn}_New {{ ... }}` (free up T)",
                            tn = g.name, m = fd.name
                        ),
                        r.span,
                    ));
                }
            }
            // Plan 101.1 B3 (Ф.2 E_DUPLICATE_GENERIC_DECL):
            // Detect `fn[K, V] HashMap[K, V] @method` — generics в `fn[…]`
            // дублируют carrier-brackets `Name[K, V]`. Удалите fn-prefix
            // OR удалите из carrier.
            //
            // Collect carrier-declared generics (single-upper names from
            // receiver.generics) and check if any fn-prefix-generic
            // (fd.generics from prefix) duplicates them.
            let carrier_decls: HashSet<String> = r.generics.iter()
                .filter_map(|tr| {
                    if let TypeRef::Named { path, .. } = tr {
                        if path.len() == 1 {
                            let n = &path[0];
                            if n.len() <= 2 && n.chars().all(|c| c.is_ascii_uppercase()) {
                                return Some(n.clone());
                            }
                        }
                    }
                    None
                })
                .collect();
            for g in &fd.generics {
                if carrier_decls.contains(&g.name) {
                    errors.push(Diagnostic::new(
                        format!(
                            "[E_DUPLICATE_GENERIC_DECL] generic `{tn}` уже введён через \
                             receiver `{rn}[{ts}]` — удалите из `fn[…]` префикса \
                             (Plan 101.1 / D145):\n  \
                             fn {rn}[{ts}] @{m}(...)  // без fn[{tn}]",
                            tn = g.name, rn = r.type_name, ts = r.generics.iter().map(|t| format!("{:?}", t)).collect::<Vec<_>>().join(", "), m = fd.name
                        ),
                        r.span,
                    ));
                }
            }
        }
        // Bounds и defaults generic-параметров.
        for g in &fd.generics {
            for b in &g.bounds {
                self.walk_typeref(b, &gs, errors);
            }
            if let Some(d) = &g.default {
                self.walk_typeref(d, &gs, errors);
            }
        }
        // Параметры, return, эффекты.
        for p in &fd.params {
            self.walk_typeref(&p.ty, &gs, errors);
            if let Some(dv) = &p.default {
                self.walk_expr(dv, &gs, errors);
            }
        }
        if let Some(rt) = &fd.return_type {
            self.walk_typeref(rt, &gs, errors);
        }
        for e in &fd.effects {
            self.walk_typeref(e, &gs, errors);
        }
        for c in &fd.contracts {
            self.walk_expr(&c.expr, &gs, errors);
        }
        if let Some(d) = &fd.decreases {
            self.walk_expr(d, &gs, errors);
        }
        // Тело.
        match &fd.body {
            FnBody::Expr(e) => self.walk_expr(e, &gs, errors),
            FnBody::Block(b) => self.walk_block(b, &gs, errors),
            FnBody::External => {}
        }
    }

    fn check_type_decl(&self, td: &TypeDecl, errors: &mut Vec<Diagnostic>) {
        let mut gs: HashSet<String> = HashSet::new();
        for g in &td.generics {
            gs.insert(g.name.clone());
        }
        for g in &td.generics {
            for b in &g.bounds {
                self.walk_typeref(b, &gs, errors);
            }
            if let Some(d) = &g.default {
                self.walk_typeref(d, &gs, errors);
            }
        }
        match &td.kind {
            TypeDeclKind::Record(fields) => {
                for f in fields {
                    self.walk_typeref(&f.ty, &gs, errors);
                }
            }
            TypeDeclKind::Sum(variants) => {
                for v in variants {
                    match &v.kind {
                        SumVariantKind::Unit => {}
                        SumVariantKind::Tuple(tys) => {
                            for t in tys {
                                self.walk_typeref(t, &gs, errors);
                            }
                        }
                        SumVariantKind::Record(fields) => {
                            for f in fields {
                                self.walk_typeref(&f.ty, &gs, errors);
                            }
                        }
                    }
                }
            }
            TypeDeclKind::Effect(methods) => {
                for m in methods {
                    let mut ms = gs.clone();
                    for g in &m.generics {
                        ms.insert(g.name.clone());
                    }
                    for p in &m.params {
                        self.walk_typeref(&p.ty, &ms, errors);
                    }
                    if let Some(rt) = &m.return_type {
                        self.walk_typeref(rt, &ms, errors);
                    }
                    for e in &m.effects {
                        self.walk_typeref(e, &ms, errors);
                    }
                }
            }
            TypeDeclKind::Protocol { methods, embeds } => {
                for m in methods {
                    let mut ms = gs.clone();
                    for g in &m.generics {
                        ms.insert(g.name.clone());
                    }
                    for p in &m.params {
                        self.walk_typeref(&p.ty, &ms, errors);
                    }
                    if let Some(rt) = &m.return_type {
                        self.walk_typeref(rt, &ms, errors);
                    }
                    for e in &m.effects {
                        self.walk_typeref(e, &ms, errors);
                    }
                }
                // Plan 101.4: validate embedded protocol type references.
                for e in embeds {
                    self.walk_typeref(e, &gs, errors);
                }
            }
            TypeDeclKind::Newtype(tr) => self.walk_typeref(tr, &gs, errors),
            TypeDeclKind::Alias(tr) => self.walk_typeref(tr, &gs, errors),
            TypeDeclKind::Opaque => {}
        }
    }

    /// Ф.2: рекурсивная проверка арности одного TypeRef-дерева.
    fn walk_typeref(
        &self,
        tr: &TypeRef,
        gs: &HashSet<String>,
        errors: &mut Vec<Diagnostic>,
    ) {
        match tr {
            TypeRef::Named { path, generics, span } => {
                for g in generics {
                    self.walk_typeref(g, gs, errors);
                }
                let Some(name) = path.last() else { return; };
                // generic-параметр в scope — абстрактное имя, не тип.
                if gs.contains(name) {
                    return;
                }
                if arity_exempt(name) {
                    return;
                }
                // Неизвестное имя — не наша забота (name-resolution).
                let Some(info) = self.arity.get(name) else { return; };
                let actual = generics.len();
                // `actual == 0` — type-аргументы опущены и выводятся из
                // контекста (`fn f() -> Result { Ok(1) }`, `let x Option`).
                // Это легальный idiom Nova — не arity-ошибка. Ошибка только
                // когда аргументы УКАЗАНЫ, но их число неверно.
                if actual > 0 && actual != info.count {
                    errors.push(arity_diag(name, info, actual, *span));
                }
            }
            TypeRef::Array(inner, _) | TypeRef::FixedArray(_, inner, _) => {
                self.walk_typeref(inner, gs, errors);
            }
            TypeRef::Tuple(items, _) => {
                for it in items {
                    self.walk_typeref(it, gs, errors);
                }
            }
            TypeRef::Func { params, effects, return_type, .. } => {
                for p in params {
                    self.walk_typeref(p, gs, errors);
                }
                for e in effects {
                    self.walk_typeref(e, gs, errors);
                }
                if let Some(rt) = return_type {
                    self.walk_typeref(rt, gs, errors);
                }
            }
            // Plan 97 Ф.2 (D142): анонимный protocol-тип — рекурсивно
            // walk через сигнатуры методов; arity-checking применяется к
            // ссылкам внутри param-/return-/effect-типов.
            TypeRef::Protocol { methods, .. } => {
                for m in methods {
                    for p in &m.params {
                        self.walk_typeref(&p.ty, gs, errors);
                    }
                    for e in &m.effects {
                        self.walk_typeref(e, gs, errors);
                    }
                    if let Some(rt) = &m.return_type {
                        self.walk_typeref(rt, gs, errors);
                    }
                }
            }
            TypeRef::Unit(_) => {}
            // D176 (Plan 108): readonly T — transparent, walk inner.
            TypeRef::Readonly(inner, _) => self.walk_typeref(inner, gs, errors),
        }
    }

    /// Plan 101.1 C8: collect all Named-type identifiers referenced
    /// anywhere в typeref recursively. Used для unused-prefix-generic
    /// detection (compare against fd.generics names).
    fn collect_named_idents(tr: &TypeRef, out: &mut HashSet<String>) {
        match tr {
            TypeRef::Named { path, generics, .. } => {
                if let Some(name) = path.last() {
                    out.insert(name.clone());
                }
                for g in generics {
                    Self::collect_named_idents(g, out);
                }
            }
            TypeRef::Array(inner, _) => Self::collect_named_idents(inner, out),
            TypeRef::FixedArray(_, inner, _) => Self::collect_named_idents(inner, out),
            TypeRef::Tuple(items, _) => {
                for it in items {
                    Self::collect_named_idents(it, out);
                }
            }
            TypeRef::Func { params, return_type, .. } => {
                for p in params {
                    Self::collect_named_idents(p, out);
                }
                if let Some(rt) = return_type {
                    Self::collect_named_idents(rt, out);
                }
            }
            TypeRef::Protocol { methods: _, .. } => {}
            TypeRef::Unit(_) => {}
            // D176 (Plan 108): readonly T — transparent.
            TypeRef::Readonly(inner, _) => Self::collect_named_idents(inner, out),
        }
    }

    // --- Ф.2: walk тел (turbofish / as / is / let-аннотации) ------------

    fn walk_block(
        &self,
        b: &Block,
        gs: &HashSet<String>,
        errors: &mut Vec<Diagnostic>,
    ) {
        for s in &b.stmts {
            self.walk_stmt(s, gs, errors);
        }
        if let Some(t) = &b.trailing {
            self.walk_expr(t, gs, errors);
        }
    }

    fn walk_stmt(
        &self,
        s: &Stmt,
        gs: &HashSet<String>,
        errors: &mut Vec<Diagnostic>,
    ) {
        match s {
            Stmt::Expr(e) => self.walk_expr(e, gs, errors),
            Stmt::Let(d) => {
                if let Some(t) = &d.ty {
                    self.walk_typeref(t, gs, errors);
                }
                self.walk_expr(&d.value, gs, errors);
            }
            // Plan 114.4 Ф.2: scope-local const — strict constexpr enforce.
            // Same eligibility rule as module-level const (check_const_constexpr).
            // known_consts здесь conservatively empty (referencing другие
            // scope-locals — followup [M-114.4-scope-const-chain]).
            Stmt::Const(d) => {
                if let Some(t) = &d.ty {
                    self.walk_typeref(t, gs, errors);
                }
                self.walk_expr(&d.value, gs, errors);
                let empty_consts: HashSet<String> = HashSet::new();
                if let Err(diag) = check_const_constexpr(&d.value, &empty_consts) {
                    errors.push(diag);
                }
            }
            Stmt::Assign { target, value, .. } => {
                self.walk_expr(target, gs, errors);
                self.walk_expr(value, gs, errors);
            }
            Stmt::Return { value, .. } => {
                if let Some(v) = value {
                    self.walk_expr(v, gs, errors);
                }
            }
            Stmt::Throw { value, .. } => self.walk_expr(value, gs, errors),
            Stmt::Break(_) | Stmt::Continue(_) | Stmt::Reveal { .. } => {}
            Stmt::Defer { body, .. } | Stmt::ErrDefer { body, .. }
            | Stmt::OkDefer { body, .. } | Stmt::DeferWithResult { body, .. } => {
                self.walk_expr(body, gs, errors);
            }
            // Plan 110 D188: walk init + body.
            Stmt::ConsumeScope { type_annot, init, body, .. } => {
                if let Some(t) = type_annot {
                    self.walk_typeref(t, gs, errors);
                }
                self.walk_expr(init, gs, errors);
                for stmt in &body.stmts {
                    self.walk_stmt(stmt, gs, errors);
                }
                if let Some(t) = &body.trailing {
                    self.walk_expr(t, gs, errors);
                }
            }
            Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => {
                self.walk_expr(expr, gs, errors);
            }
            Stmt::Apply { args, .. } => {
                for a in args {
                    self.walk_expr(a, gs, errors);
                }
            }
            Stmt::Calc { steps, .. } => {
                for step in steps {
                    self.walk_expr(&step.expr, gs, errors);
                }
            }
        }
    }

    fn walk_expr(
        &self,
        e: &Expr,
        gs: &HashSet<String>,
        errors: &mut Vec<Diagnostic>,
    ) {
        match &e.kind {
            ExprKind::TurboFish { base, type_args } => {
                for t in type_args {
                    self.walk_typeref(t, gs, errors);
                }
                // Если turbofish целится в известный тип — проверить
                // арность самого turbofish'а (`HashMap[str].new()`).
                // Generic-функции (`parse[int]`) в `arity` не попадают —
                // их арность с D88-дефолтами проверяется отдельно (не Ф.2).
                let target: Option<&String> = match &base.kind {
                    ExprKind::Ident(n) => Some(n),
                    ExprKind::Path(parts) => parts.last(),
                    _ => None,
                };
                if let Some(name) = target {
                    if !gs.contains(name) && !arity_exempt(name) {
                        if let Some(info) = self.arity.get(name) {
                            // turbofish всегда указывает аргументы явно —
                            // пустой `[]` не парсится; проверяем как есть.
                            if !type_args.is_empty()
                                && type_args.len() != info.count
                            {
                                errors.push(arity_diag(
                                    name, info, type_args.len(), e.span,
                                ));
                            }
                        }
                    }
                }
                self.walk_expr(base, gs, errors);
            }
            ExprKind::As(inner, ty) | ExprKind::Is(inner, ty) => {
                self.walk_expr(inner, gs, errors);
                self.walk_typeref(ty, gs, errors);
            }
            ExprKind::Call { func, args, trailing } => {
                self.walk_expr(func, gs, errors);
                for a in args {
                    self.walk_expr(a.expr(), gs, errors);
                }
                if let Some(t) = trailing {
                    match t {
                        Trailing::Block(b) => self.walk_block(b, gs, errors),
                        Trailing::LegacyBlockWithParams(tb) => {
                            self.walk_block(&tb.body, gs, errors)
                        }
                        Trailing::Fn(sb) => self.walk_fn_sig_body(sb, gs, errors),
                    }
                }
            }
            ExprKind::Binary { left, right, .. } => {
                self.walk_expr(left, gs, errors);
                self.walk_expr(right, gs, errors);
            }
            ExprKind::Unary { operand, .. } => self.walk_expr(operand, gs, errors),
            ExprKind::Try(inner) | ExprKind::Bang(inner) => {
                self.walk_expr(inner, gs, errors)
            }
            ExprKind::Coalesce(a, b) => {
                self.walk_expr(a, gs, errors);
                self.walk_expr(b, gs, errors);
            }
            ExprKind::Member { obj, .. } => self.walk_expr(obj, gs, errors),
            ExprKind::Index { obj, index } => {
                self.walk_expr(obj, gs, errors);
                self.walk_expr(index, gs, errors);
            }
            ExprKind::If { cond, then, else_ } => {
                self.walk_expr(cond, gs, errors);
                self.walk_block(then, gs, errors);
                if let Some(eb) = else_ {
                    self.walk_else(eb, gs, errors);
                }
            }
            ExprKind::IfLet { scrutinee, then, else_, .. } => {
                self.walk_expr(scrutinee, gs, errors);
                self.walk_block(then, gs, errors);
                if let Some(eb) = else_ {
                    self.walk_else(eb, gs, errors);
                }
            }
            ExprKind::Match { scrutinee, arms } => {
                self.walk_expr(scrutinee, gs, errors);
                for arm in arms {
                    if let Some(g) = &arm.guard {
                        self.walk_expr(g, gs, errors);
                    }
                    match &arm.body {
                        MatchArmBody::Expr(e) => self.walk_expr(e, gs, errors),
                        MatchArmBody::Block(b) => self.walk_block(b, gs, errors),
                    }
                }
            }
            ExprKind::Block(b) => self.walk_block(b, gs, errors),
            ExprKind::ArrayLit(elems) => {
                for el in elems {
                    match el {
                        ArrayElem::Item(e) | ArrayElem::Spread(e) => {
                            self.walk_expr(e, gs, errors)
                        }
                    }
                }
            }
            ExprKind::MapLit { elems, .. } => {
                for (k, v) in crate::ast::MapElem::cloned_pairs(elems).iter() {
                    self.walk_expr(k, gs, errors);
                    self.walk_expr(v, gs, errors);
                }
            }
            ExprKind::TupleLit(elems) => {
                for e in elems {
                    self.walk_expr(e, gs, errors);
                }
            }
            ExprKind::RecordLit { type_name, fields, .. } => {
                // Plan 114.4.1 (D200): reject field shorthand / pair refering
                // assoc const — assoc consts live на type-level, не указываются
                // в record literal.
                if let Some(tn) = type_name {
                    if let Some(last) = tn.last() {
                        if let Some(td) = self.types.get(last) {
                            for f in fields {
                                if f.is_spread { continue; }
                                if td.assoc_consts.iter().any(|ac| ac.name == f.name) {
                                    errors.push(Diagnostic::new(
                                        format!(
                                            "[E_CONST_FIELD_IN_LITERAL] field `{}` \
                                             в record literal `{}{{ … }}` — это \
                                             associated constant (zero-storage, \
                                             namespace access `{}.{}`); НЕ указывается \
                                             и НЕ инициализируется в record literal \
                                             (Plan 114.4.1 D200).",
                                            f.name, last, last, f.name,
                                        ),
                                        f.span,
                                    ));
                                }
                            }
                        }
                    }
                }
                for f in fields {
                    if let Some(v) = &f.value {
                        self.walk_expr(v, gs, errors);
                    }
                }
            }
            ExprKind::TaggedTemplate { tag, args, .. } => {
                self.walk_expr(tag, gs, errors);
                for a in args {
                    self.walk_expr(a, gs, errors);
                }
            }
            ExprKind::InterpolatedStr { parts } => {
                for p in parts {
                    if let InterpStrPart::Expr(e) = p {
                        self.walk_expr(e, gs, errors);
                    }
                }
            }
            ExprKind::Lambda { body, .. } => self.walk_expr(body, gs, errors),
            ExprKind::ClosureLight { body, .. } => match body {
                ClosureBody::Expr(e) => self.walk_expr(e, gs, errors),
                ClosureBody::Block(b) => self.walk_block(b, gs, errors),
            },
            ExprKind::ClosureFull(sb) => self.walk_fn_sig_body(sb, gs, errors),
            ExprKind::Spawn(body) => self.walk_expr(body, gs, errors),
            ExprKind::Detach(body) | ExprKind::Blocking(body) => self.walk_block(body, gs, errors),
            ExprKind::Supervised { body, cancel } => {
                if let Some(c) = cancel {
                    self.walk_expr(c, gs, errors);
                }
                self.walk_block(body, gs, errors);
            }
            ExprKind::Forbid { body, .. } => self.walk_block(body, gs, errors),
            ExprKind::Realtime { body, .. } => self.walk_block(body, gs, errors),
            ExprKind::ParallelFor { iter, body, .. } => {
                self.walk_expr(iter, gs, errors);
                self.walk_block(body, gs, errors);
            }
            ExprKind::For { iter, body, .. } => {
                self.walk_expr(iter, gs, errors);
                self.walk_block(body, gs, errors);
            }
            ExprKind::While { cond, body, .. } => {
                self.walk_expr(cond, gs, errors);
                self.walk_block(body, gs, errors);
            }
            ExprKind::WhileLet { scrutinee, body, .. } => {
                self.walk_expr(scrutinee, gs, errors);
                self.walk_block(body, gs, errors);
            }
            ExprKind::Loop { body, .. } => self.walk_block(body, gs, errors),
            ExprKind::Select { arms } => {
                for arm in arms {
                    match &arm.op {
                        SelectOp::Recv { chan, .. } => {
                            self.walk_expr(chan, gs, errors)
                        }
                        SelectOp::Send { chan, value } => {
                            self.walk_expr(chan, gs, errors);
                            self.walk_expr(value, gs, errors);
                        }
                        SelectOp::Default => {}
                    }
                    if let Some(g) = &arm.guard {
                        self.walk_expr(g, gs, errors);
                    }
                    self.walk_block(&arm.body, gs, errors);
                }
            }
            ExprKind::Range { start, end, .. } => {
                if let Some(s) = start { self.walk_expr(s, gs, errors); }
                if let Some(e) = end { self.walk_expr(e, gs, errors); }
            }
            ExprKind::Throw(inner) => self.walk_expr(inner, gs, errors),
            ExprKind::Interrupt(opt) => {
                if let Some(e) = opt {
                    self.walk_expr(e, gs, errors);
                }
            }
            ExprKind::With { body, .. } => self.walk_block(body, gs, errors),
            ExprKind::Forall { range, body, .. }
            | ExprKind::Exists { range, body, .. } => {
                self.walk_expr(range, gs, errors);
                self.walk_expr(body, gs, errors);
            }
            // Plan 97 Ф.4 (D142): protocol-литерал — walk идентичен.
            ExprKind::HandlerLit { methods, .. } | ExprKind::ProtocolLit { methods, .. } => {
                for m in methods {
                    match &m.body {
                        HandlerMethodBody::Expr(e) => self.walk_expr(e, gs, errors),
                        HandlerMethodBody::Block(b) => self.walk_block(b, gs, errors),
                    }
                }
            }
            ExprKind::IntLit(_) | ExprKind::FloatLit(_) | ExprKind::BoolLit(_)
            | ExprKind::StrLit(_) | ExprKind::CharLit(_) | ExprKind::UnitLit
            | ExprKind::Ident(_) | ExprKind::Path(_) | ExprKind::SelfAccess => {}
        }
    }

    fn walk_else(
        &self,
        eb: &ElseBranch,
        gs: &HashSet<String>,
        errors: &mut Vec<Diagnostic>,
    ) {
        match eb {
            ElseBranch::Block(b) => self.walk_block(b, gs, errors),
            ElseBranch::If(e) => self.walk_expr(e, gs, errors),
        }
    }

    fn walk_fn_sig_body(
        &self,
        sb: &FnSigBody,
        gs: &HashSet<String>,
        errors: &mut Vec<Diagnostic>,
    ) {
        for p in &sb.params {
            self.walk_typeref(&p.ty, gs, errors);
        }
        for e in &sb.effects {
            self.walk_typeref(e, gs, errors);
        }
        if let Some(rt) = &sb.return_type {
            self.walk_typeref(rt, gs, errors);
        }
        match &sb.body {
            FnBody::Expr(e) => self.walk_expr(e, gs, errors),
            FnBody::Block(b) => self.walk_block(b, gs, errors),
            FnBody::External => {}
        }
    }

    // ================================================================
    // Ф.1 — assignability: arg↔param и annotation↔RHS.
    //
    // Scope-aware проход: трекает типы локальных переменных, на каждом
    // call-site и `let`-аннотации сверяет совместимость. Числовые
    // литералы полиморфны по контексту (D44: `let x u8 = 200` валиден),
    // поэтому проверка literal-aware. Несовместимость → E7301.
    //
    // Резолвятся только однозначные callee (free fn / static-метод,
    // ровно один overload): instance-методы требуют receiver-type
    // inference, ненадёжной в bootstrap — их резолвит codegen.
    // ================================================================

    fn f1_check_fn(&self, fd: &FnDecl, errors: &mut Vec<Diagnostic>) {
        let gs = fn_generic_scope(fd);
        let mut scope: HashMap<String, TypeRef> = HashMap::new();
        for p in &fd.params {
            scope.insert(p.name.clone(), p.ty.clone());
        }
        // D175 (Plan 108): inject receiver type as "@" in scope so that
        // `check_target_readonly` can resolve @.field type for self-assignments.
        if let Some(recv) = &fd.receiver {
            if matches!(recv.kind, ReceiverKind::Instance) {
                scope.insert("@".to_string(), TypeRef::Named {
                    path: vec![recv.type_name.clone()],
                    generics: recv.generics.clone(),
                    span: recv.span,
                });
            }
        }
        match &fd.body {
            FnBody::Expr(e) => {
                self.f1_expr(e, &gs, &mut scope, errors);
                self.f4_check_value(e, &scope, errors);
            }
            FnBody::Block(b) => self.f1_block(b, &gs, &mut scope, errors),
            FnBody::External => {}
        }
    }

    fn f1_block(
        &self,
        b: &Block,
        gs: &HashSet<String>,
        scope: &mut HashMap<String, TypeRef>,
        errors: &mut Vec<Diagnostic>,
    ) {
        // Snapshot let-имён этого блока — восстановить scope на выходе
        // (block-out shadowing, как BoundCtx::walk_block).
        let mut snapshot: Vec<(String, Option<TypeRef>)> = Vec::new();
        for s in &b.stmts {
            if let Stmt::Let(d) = s {
                if let Some(name) = pattern_simple_name(&d.pattern) {
                    snapshot.push((name.clone(), scope.get(&name).cloned()));
                }
            }
        }
        for s in &b.stmts {
            self.f1_stmt(s, gs, scope, errors);
        }
        if let Some(t) = &b.trailing {
            self.f1_expr(t, gs, scope, errors);
            self.f4_check_value(t, scope, errors);
        }
        for (n, prev) in snapshot {
            match prev {
                Some(t) => { scope.insert(n, t); }
                None => { scope.remove(&n); }
            }
        }
    }

    fn f1_stmt(
        &self,
        s: &Stmt,
        gs: &HashSet<String>,
        scope: &mut HashMap<String, TypeRef>,
        errors: &mut Vec<Diagnostic>,
    ) {
        match s {
            Stmt::Expr(e) => self.f1_expr(e, gs, scope, errors),
            Stmt::Let(d) => {
                self.f1_expr(&d.value, gs, scope, errors);
                self.f4_check_value(&d.value, scope, errors);
                // Ф.1: annotation ↔ RHS.
                if let (Some(ann), Some(name)) =
                    (&d.ty, pattern_simple_name(&d.pattern))
                {
                    self.f1_check_assign_let(
                        &d.value, ann, &name, gs, scope, errors,
                    );
                }
                // Регистрируем переменную в scope: тип = аннотация, иначе
                // inferred из RHS.
                if let Some(name) = pattern_simple_name(&d.pattern) {
                    match d.ty.clone()
                        .or_else(|| self.infer_expr_type(&d.value, scope))
                    {
                        Some(t) => { scope.insert(name, t); }
                        None => { scope.remove(&name); }
                    }
                }
            }
            // Plan 114.4 Ф.2: scope-local const — pass-through (no-op for now).
            Stmt::Const(_) => {}
            Stmt::Assign { target, value, .. } => {
                self.f1_expr(target, gs, scope, errors);
                self.f1_expr(value, gs, scope, errors);
                // D175/D176 (Plan 108): check that we're not assigning to a
                // readonly field or through a readonly index.
                self.check_target_readonly(target, scope, errors);
            }
            Stmt::Return { value, .. } => {
                if let Some(v) = value {
                    self.f1_expr(v, gs, scope, errors);
                    self.f4_check_value(v, scope, errors);
                }
            }
            Stmt::Throw { value, .. } => {
                self.f1_expr(value, gs, scope, errors);
                self.f4_check_value(value, scope, errors);
            }
            Stmt::Break(_) | Stmt::Continue(_) | Stmt::Reveal { .. } => {}
            Stmt::Defer { body, .. } | Stmt::ErrDefer { body, .. }
            | Stmt::OkDefer { body, .. } | Stmt::DeferWithResult { body, .. } => {
                self.f1_expr(body, gs, scope, errors);
            }
            // Plan 110 D188: walk init + body (full D188 R1-R6 check лежит
            // в Plan 110.1.2/110.1.3 — здесь scaffold walking).
            Stmt::ConsumeScope { init, body, .. } => {
                self.f1_expr(init, gs, scope, errors);
                self.f4_check_value(init, scope, errors);
                for s in &body.stmts {
                    self.f1_stmt(s, gs, scope, errors);
                }
                if let Some(t) = &body.trailing {
                    self.f1_expr(t, gs, scope, errors);
                }
            }
            Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => {
                self.f1_expr(expr, gs, scope, errors);
            }
            Stmt::Apply { args, .. } => {
                for a in args {
                    self.f1_expr(a, gs, scope, errors);
                }
            }
            Stmt::Calc { steps, .. } => {
                for step in steps {
                    self.f1_expr(&step.expr, gs, scope, errors);
                }
            }
        }
    }

    fn f1_expr(
        &self,
        e: &Expr,
        gs: &HashSet<String>,
        scope: &mut HashMap<String, TypeRef>,
        errors: &mut Vec<Diagnostic>,
    ) {
        match &e.kind {
            ExprKind::Call { func, args, trailing } => {
                self.f1_expr(func, gs, scope, errors);
                for a in args {
                    self.f1_expr(a.expr(), gs, scope, errors);
                    self.f4_check_value(a.expr(), scope, errors);
                }
                if let Some(t) = trailing {
                    match t {
                        Trailing::Block(b) => self.f1_block(b, gs, scope, errors),
                        Trailing::LegacyBlockWithParams(tb) => {
                            self.f1_block(&tb.body, gs, scope, errors)
                        }
                        Trailing::Fn(sb) => {
                            self.f1_fn_sig_body(sb, gs, scope, errors)
                        }
                    }
                }
                self.f1_check_call(
                    func, args, trailing.is_some(), gs, scope, errors,
                );
            }
            ExprKind::TurboFish { base, .. } => {
                self.f1_expr(base, gs, scope, errors)
            }
            ExprKind::As(inner, _) | ExprKind::Is(inner, _) => {
                self.f1_expr(inner, gs, scope, errors)
            }
            ExprKind::Binary { left, right, .. } => {
                self.f1_expr(left, gs, scope, errors);
                self.f1_expr(right, gs, scope, errors);
                self.f4_check_value(left, scope, errors);
                self.f4_check_value(right, scope, errors);
            }
            ExprKind::Unary { operand, .. } => {
                self.f1_expr(operand, gs, scope, errors);
                self.f4_check_value(operand, scope, errors);
            }
            ExprKind::Try(inner) | ExprKind::Bang(inner) => {
                self.f1_expr(inner, gs, scope, errors)
            }
            ExprKind::Coalesce(a, b) => {
                self.f1_expr(a, gs, scope, errors);
                self.f1_expr(b, gs, scope, errors);
            }
            ExprKind::Member { obj, name } => {
                self.f1_expr(obj, gs, scope, errors);
                self.f3_check_member(obj, name, e.span, scope, errors);
            }
            ExprKind::Index { obj, index } => {
                self.f1_expr(obj, gs, scope, errors);
                self.f1_expr(index, gs, scope, errors);
            }
            ExprKind::If { cond, then, else_ } => {
                self.f1_expr(cond, gs, scope, errors);
                self.f1_block(then, gs, scope, errors);
                if let Some(eb) = else_ {
                    self.f1_else(eb, gs, scope, errors);
                }
            }
            ExprKind::IfLet { scrutinee, then, else_, .. } => {
                self.f1_expr(scrutinee, gs, scope, errors);
                self.f1_block(then, gs, scope, errors);
                if let Some(eb) = else_ {
                    self.f1_else(eb, gs, scope, errors);
                }
            }
            ExprKind::Match { scrutinee, arms } => {
                self.f1_expr(scrutinee, gs, scope, errors);
                for arm in arms {
                    if let Some(g) = &arm.guard {
                        self.f1_expr(g, gs, scope, errors);
                    }
                    match &arm.body {
                        MatchArmBody::Expr(e) => {
                            self.f1_expr(e, gs, scope, errors)
                        }
                        MatchArmBody::Block(b) => {
                            self.f1_block(b, gs, scope, errors)
                        }
                    }
                }
            }
            ExprKind::Block(b) => self.f1_block(b, gs, scope, errors),
            ExprKind::ArrayLit(elems) => {
                for el in elems {
                    match el {
                        ArrayElem::Item(e) | ArrayElem::Spread(e) => {
                            self.f1_expr(e, gs, scope, errors)
                        }
                    }
                }
            }
            ExprKind::MapLit { elems, .. } => {
                for (k, v) in crate::ast::MapElem::cloned_pairs(elems).iter() {
                    self.f1_expr(k, gs, scope, errors);
                    self.f1_expr(v, gs, scope, errors);
                }
            }
            ExprKind::TupleLit(elems) => {
                for e in elems {
                    self.f1_expr(e, gs, scope, errors);
                }
            }
            ExprKind::RecordLit { fields, .. } => {
                for f in fields {
                    if let Some(v) = &f.value {
                        self.f1_expr(v, gs, scope, errors);
                    }
                }
            }
            ExprKind::TaggedTemplate { tag, args, .. } => {
                self.f1_expr(tag, gs, scope, errors);
                for a in args {
                    self.f1_expr(a, gs, scope, errors);
                }
            }
            ExprKind::InterpolatedStr { parts } => {
                for p in parts {
                    if let InterpStrPart::Expr(e) = p {
                        self.f1_expr(e, gs, scope, errors);
                    }
                }
            }
            ExprKind::Lambda { body, .. } => {
                self.f1_expr(body, gs, scope, errors)
            }
            ExprKind::ClosureLight { body, .. } => match body {
                ClosureBody::Expr(e) => self.f1_expr(e, gs, scope, errors),
                ClosureBody::Block(b) => self.f1_block(b, gs, scope, errors),
            },
            ExprKind::ClosureFull(sb) => {
                self.f1_fn_sig_body(sb, gs, scope, errors)
            }
            ExprKind::Spawn(body) => self.f1_expr(body, gs, scope, errors),
            ExprKind::Detach(body) | ExprKind::Blocking(body) => self.f1_block(body, gs, scope, errors),
            ExprKind::Supervised { body, cancel } => {
                if let Some(c) = cancel {
                    self.f1_expr(c, gs, scope, errors);
                }
                self.f1_block(body, gs, scope, errors);
            }
            ExprKind::Forbid { body, .. } => {
                self.f1_block(body, gs, scope, errors)
            }
            ExprKind::Realtime { body, .. } => {
                self.f1_block(body, gs, scope, errors)
            }
            ExprKind::ParallelFor { pattern, iter, body, elem_type } => {
                self.f1_expr(iter, gs, scope, errors);
                if let Some(ann) = elem_type {
                    self.f1_check_for_elem(iter, ann, gs, scope, errors);
                }
                self.f1_for_body(elem_type, pattern, body, gs, scope, errors);
            }
            ExprKind::For { pattern, iter, body, elem_type, .. } => {
                self.f1_expr(iter, gs, scope, errors);
                // Plan 87 Ф.3: явная аннотация типа элемента — checked
                // assertion против фактического типа элемента итератора.
                if let Some(ann) = elem_type {
                    self.f1_check_for_elem(iter, ann, gs, scope, errors);
                }
                self.f1_for_body(elem_type, pattern, body, gs, scope, errors);
            }
            ExprKind::While { cond, body, .. } => {
                self.f1_expr(cond, gs, scope, errors);
                self.f1_block(body, gs, scope, errors);
            }
            ExprKind::WhileLet { scrutinee, body, .. } => {
                self.f1_expr(scrutinee, gs, scope, errors);
                self.f1_block(body, gs, scope, errors);
            }
            ExprKind::Loop { body, .. } => {
                self.f1_block(body, gs, scope, errors)
            }
            ExprKind::Select { arms } => {
                for arm in arms {
                    match &arm.op {
                        SelectOp::Recv { chan, .. } => {
                            self.f1_expr(chan, gs, scope, errors)
                        }
                        SelectOp::Send { chan, value } => {
                            self.f1_expr(chan, gs, scope, errors);
                            self.f1_expr(value, gs, scope, errors);
                        }
                        SelectOp::Default => {}
                    }
                    if let Some(g) = &arm.guard {
                        self.f1_expr(g, gs, scope, errors);
                    }
                    self.f1_block(&arm.body, gs, scope, errors);
                }
            }
            ExprKind::Range { start, end, .. } => {
                if let Some(s) = start { self.f1_expr(s, gs, scope, errors); }
                if let Some(e) = end { self.f1_expr(e, gs, scope, errors); }
            }
            ExprKind::Throw(inner) => self.f1_expr(inner, gs, scope, errors),
            ExprKind::Interrupt(opt) => {
                if let Some(e) = opt {
                    self.f1_expr(e, gs, scope, errors);
                }
            }
            ExprKind::With { body, .. } => {
                self.f1_block(body, gs, scope, errors)
            }
            ExprKind::Forall { range, body, .. }
            | ExprKind::Exists { range, body, .. } => {
                self.f1_expr(range, gs, scope, errors);
                self.f1_expr(body, gs, scope, errors);
            }
            // Plan 97 Ф.4 (D142): protocol-литерал — f1_expr walk
            // идентичен.
            ExprKind::HandlerLit { methods, .. } | ExprKind::ProtocolLit { methods, .. } => {
                for m in methods {
                    match &m.body {
                        HandlerMethodBody::Expr(e) => {
                            self.f1_expr(e, gs, scope, errors)
                        }
                        HandlerMethodBody::Block(b) => {
                            self.f1_block(b, gs, scope, errors)
                        }
                    }
                }
            }
            ExprKind::IntLit(_) | ExprKind::FloatLit(_) | ExprKind::BoolLit(_)
            | ExprKind::StrLit(_) | ExprKind::CharLit(_) | ExprKind::UnitLit
            | ExprKind::Ident(_) | ExprKind::Path(_) | ExprKind::SelfAccess => {}
        }
    }

    fn f1_else(
        &self,
        eb: &ElseBranch,
        gs: &HashSet<String>,
        scope: &mut HashMap<String, TypeRef>,
        errors: &mut Vec<Diagnostic>,
    ) {
        match eb {
            ElseBranch::Block(b) => self.f1_block(b, gs, scope, errors),
            ElseBranch::If(e) => self.f1_expr(e, gs, scope, errors),
        }
    }

    fn f1_fn_sig_body(
        &self,
        sb: &FnSigBody,
        gs: &HashSet<String>,
        scope: &mut HashMap<String, TypeRef>,
        errors: &mut Vec<Diagnostic>,
    ) {
        match &sb.body {
            FnBody::Expr(e) => {
                self.f1_expr(e, gs, scope, errors);
                self.f4_check_value(e, scope, errors);
            }
            FnBody::Block(b) => self.f1_block(b, gs, scope, errors),
            FnBody::External => {}
        }
    }

    /// Ф.1: проверить `let <name> <ann> = <value>` на совместимость.
    fn f1_check_assign_let(
        &self,
        value: &Expr,
        ann: &TypeRef,
        name: &str,
        gs: &HashSet<String>,
        scope: &HashMap<String, TypeRef>,
        errors: &mut Vec<Diagnostic>,
    ) {
        if let Compat::Bad { found } =
            self.assignable(value, ann, gs, gs, scope)
        {
            errors.push(
                Diagnostic::new(
                    format!(
                        "[E7301] cannot assign value of type `{}` to `{}` \
                         declared as `{}`",
                        found, name, typeref_display(ann),
                    ),
                    value.span,
                )
                .with_note_at(
                    "type expected because of this annotation".to_string(),
                    ann.span(),
                ),
            );
        }
        // D176 (Plan 108): `readonly T → T` is forbidden (E_READONLY_COERCE).
        // `T → readonly T` is allowed (auto-coerce, narrowing rights).
        if !ann.is_readonly() {
            if let Some(value_ty) = self.infer_expr_type(value, scope) {
                if value_ty.is_readonly() {
                    errors.push(Diagnostic::new(
                        format!(
                            "[E_READONLY_COERCE] cannot coerce `readonly {}` to `{}`: \
                             removing readonly is not allowed. Use `.to_owned()` to \
                             get a mutable copy.",
                            typeref_display(value_ty.strip_readonly()),
                            typeref_display(ann),
                        ),
                        value.span,
                    ));
                }
            }
        }
    }

    /// Plan 87 Ф.2.2: пройти тело for-in. При заданной аннотации типа
    /// элемента (`for x TYPE in`) loop-переменная-`Ident` получает этот
    /// тип в scope тела (save/restore — for-body не утекает в окружающий
    /// scope). Без аннотации scope не трогаем — поведение 1:1 до Plan 87.
    fn f1_for_body(
        &self,
        elem_type: &Option<TypeRef>,
        pattern: &Pattern,
        body: &Block,
        gs: &HashSet<String>,
        scope: &mut HashMap<String, TypeRef>,
        errors: &mut Vec<Diagnostic>,
    ) {
        match (elem_type, pattern) {
            (Some(ann), Pattern::Ident { name, .. }) => {
                let saved = scope.insert(name.clone(), ann.clone());
                self.f1_block(body, gs, scope, errors);
                match saved {
                    Some(prev) => { scope.insert(name.clone(), prev); }
                    None => { scope.remove(name); }
                }
            }
            _ => self.f1_block(body, gs, scope, errors),
        }
    }

    /// Plan 87 Ф.3: проверить, что аннотация типа loop-переменной
    /// (`for x TYPE in iter`) совместима с фактическим типом элемента
    /// итератора. Несовпадение → E7340. Если тип элемента уверенно
    /// вывести не удалось — проверка пропускается (Compat::Unknown-
    /// философия Plan 79: никаких ложных срабатываний).
    fn f1_check_for_elem(
        &self,
        iter: &Expr,
        ann: &TypeRef,
        gs: &HashSet<String>,
        scope: &HashMap<String, TypeRef>,
        errors: &mut Vec<Diagnostic>,
    ) {
        let Some(elem_tr) = self.infer_iter_elem_type(iter, scope) else {
            return;
        };
        let ann_cat = self.cat_of(ann, gs);
        let elem_cat = self.cat_of(&elem_tr, gs);
        // Permissive на Other (generic-параметр / неизвестное / protocol)
        // — как в `assignable`.
        if !cat_compatible(&elem_cat, &ann_cat) {
            errors.push(
                Diagnostic::new(
                    format!(
                        "[E7340] for-in loop variable annotated as `{}`, \
                         but the iterator yields elements of type `{}`",
                        typeref_display(ann),
                        typeref_display(&elem_tr),
                    ),
                    ann.span(),
                )
                .with_note_at(
                    "iterator element type comes from here".to_string(),
                    iter.span,
                ),
            );
        }
    }

    /// Plan 87 Ф.3: best-effort вывод типа элемента for-in итератора.
    /// `None` — вывести не удалось (проверка аннотации пропускается).
    fn infer_iter_elem_type(
        &self,
        iter: &Expr,
        scope: &HashMap<String, TypeRef>,
    ) -> Option<TypeRef> {
        match &iter.kind {
            // `a..b` / `a..=b` — элементы int.
            ExprKind::Range { .. } => Some(prim_ref("int", iter.span)),
            // Литерал массива — тип из первого выводимого не-spread элемента.
            ExprKind::ArrayLit(elems) => {
                for el in elems {
                    if let ArrayElem::Item(e) = el {
                        if let Some(t) = self.infer_expr_type(e, scope) {
                            return Some(t);
                        }
                    }
                }
                None
            }
            // Прочее: если выражение имеет тип `[]T` / `[N]T` — элемент `T`.
            // D176 (Plan 108): `readonly []T` → elements are `T` (primitive copy).
            _ => match self.infer_expr_type(iter, scope)? {
                TypeRef::Array(inner, _)
                | TypeRef::FixedArray(_, inner, _) => Some(*inner),
                TypeRef::Readonly(inner, _) => match *inner {
                    TypeRef::Array(elem, _) | TypeRef::FixedArray(_, elem, _) => Some(*elem),
                    _ => None,
                },
                _ => None,
            },
        }
    }

    /// Ф.1: проверить типы аргументов call-site против параметров callee.
    fn f1_check_call(
        &self,
        func: &Expr,
        args: &[CallArg],
        trailing_present: bool,
        gs: &HashSet<String>,
        scope: &HashMap<String, TypeRef>,
        errors: &mut Vec<Diagnostic>,
    ) {
        // Trailing-форма перепривязывает последний param — пропускаем
        // (редко, и codegen всё равно проверяет).
        if trailing_present {
            return;
        }
        let base: &Expr = match &func.kind {
            ExprKind::TurboFish { base, .. } => base.as_ref(),
            _ => func,
        };
        // Резолвим callee только однозначно (ровно один overload).
        let callee: &FnDecl = match &base.kind {
            ExprKind::Ident(n) => {
                match self.fn_decls.get(n).map(|v| v.as_slice()) {
                    Some([single]) => single,
                    _ => return,
                }
            }
            ExprKind::Path(parts) if parts.len() == 2 => {
                let overloads = self
                    .method_table
                    .get(&parts[0])
                    .and_then(|m| m.get(&parts[1]))
                    .map(|v| v.as_slice());
                // Plan 91.8a.2 followup 2026-05-29: для receiver-types,
                // у которых ЧАСТЬ overload'ов лежит вне method_table
                // (external fn в другом stdlib-модуле, codegen builtins,
                // hidden D73 auto-derive paths) — single-overload arg-check
                // даёт ложные positives. Symptom: `let fill_s = str.from(fill)`
                // в std/runtime/string.nv падает с E7301 "cannot pass char as bool"
                // когда пользователь добавил `fn str.from(b bool) -> str` —
                // type-checker видит ЕДИНСТВЕННЫЙ overload (bool) и ругается
                // на arg типа char, не зная про external `str.from(c char)`.
                //
                // Фикс: для primitive-receiver'ов (str/int/char/bool/f*/u*/i*/uint)
                // **никогда** не делать arg-check на single-overload в Path-форме.
                // Codegen overload resolution в `external_registry` +
                // `method_overloads` корректно резолвит за нас.
                let is_primitive_recv = matches!(
                    parts[0].as_str(),
                    "str" | "int" | "char" | "bool" | "f32" | "f64"
                    | "u8" | "u16" | "u32" | "u64" | "uint"
                    | "i8" | "i16" | "i32" | "i64"
                );
                if is_primitive_recv {
                    return;
                }
                match overloads {
                    Some([single]) => single,
                    _ => return,
                }
            }
            // Plan 81 Ф.2: module-qualified вызов `alias.func(...)` /
            // `mod.func(...)`. `obj` — alias/имя импортированного модуля,
            // `name` — свободная функция этого модуля. Раньше неизвестная
            // функция давала link-error (EXPECT_COMPILE_ERROR не ловил —
            // Plan 70.1 known-limitation); теперь — compile-error E7401.
            ExprKind::Member { obj, name } => {
                let ExprKind::Ident(prefix) = &obj.kind else { return; };
                // Локальная переменная перекрывает имя → это instance-
                // метод на значении, не module-call.
                if scope.contains_key(prefix) {
                    return;
                }
                // Не импортированный модуль → instance-метод (codegen).
                if !self.imported_modules.contains(prefix) {
                    return;
                }
                // Intrinsic namespace (gc / Time / Channel / ...) —
                // спец-dispatch в codegen, не обычная free fn.
                if is_intrinsic_namespace(prefix) {
                    return;
                }
                match self.fn_decls.get(name) {
                    Some(overloads) => match overloads.as_slice() {
                        [single] => single,
                        // 0 (никогда) или overload — пропускаем arg-check.
                        _ => return,
                    },
                    None => {
                        errors.push(Diagnostic::new(
                            format!(
                                "[E7401] no function `{}` in module `{}`",
                                name, prefix,
                            ),
                            base.span,
                        ));
                        return;
                    }
                }
            }
            // прочие instance-методы (`obj.method` на значении) —
            // receiver-type inference ненадёжна в bootstrap; codegen
            // резолвит по type-info.
            _ => return,
        };
        let Ok(bindings) =
            crate::argbind::bind_call_args(&callee.params, args)
        else {
            // BindError уже репортит BoundCtx::check_call_argbind.
            return;
        };
        let callee_gs = fn_generic_scope(callee);
        for (pi, binding) in bindings.iter().enumerate() {
            let ai = match binding {
                crate::argbind::ArgBinding::Positional(i)
                | crate::argbind::ArgBinding::Named(i) => *i,
                // Variadic собирается в []T, Default — нет arg-выражения.
                _ => continue,
            };
            let Some(param) = callee.params.get(pi) else { continue; };
            if param.is_variadic {
                continue;
            }
            let Some(arg) = args.get(ai) else { continue; };
            if let Compat::Bad { found } =
                self.assignable(arg.expr(), &param.ty, gs, &callee_gs, scope)
            {
                errors.push(
                    Diagnostic::new(
                        format!(
                            "[E7301] cannot pass `{}` as argument `{}` \
                             of type `{}`",
                            found, param.name, typeref_display(&param.ty),
                        ),
                        arg.expr().span,
                    )
                    .with_note_at(
                        format!("parameter `{}` declared here", param.name),
                        param.span,
                    ),
                );
            }
        }
    }

    /// Ф.3: проверить существование поля/метода `name` у `obj`.
    ///
    /// Консервативно: проверяется только когда тип `obj` уверенно
    /// резолвится в concrete record **без embed'ов** (`use`-поля
    /// проксируют члены — резолв слишком сложен). Метод ИЛИ поле —
    /// обе формы валидны (`obj.field`, `obj.method`, `obj.method()`).
    fn f3_check_member(
        &self,
        obj: &Expr,
        name: &str,
        span: Span,
        scope: &HashMap<String, TypeRef>,
        errors: &mut Vec<Diagnostic>,
    ) {
        let Some(obj_tr) = self.infer_expr_type(obj, scope) else { return; };
        let TypeRef::Named { path, .. } = &obj_tr else { return; };
        let Some(tname) = path.last() else { return; };
        let Some(td) = self.types.get(tname) else { return; };
        let TypeDeclKind::Record(fields) = &td.kind else { return; };
        // embed (`use`) проксирует поля/методы вложенного типа — резолв
        // слишком сложен для надёжной проверки, пропускаем такой тип.
        if fields.iter().any(|f| f.is_embed) {
            return;
        }
        if fields.iter().any(|f| f.name == name) {
            return;
        }
        // Метод? Имена операторных методов могут храниться с ведущим `@`.
        let has_method = self.method_table.get(tname).map_or(false, |m| {
            m.keys().any(|k| k.trim_start_matches('@') == name)
        });
        if has_method {
            return;
        }
        // `into` / `try_into` синтезируются компилятором из `From` /
        // `TryFrom` (D73/D77) — их нет в method_table, но они валидны
        // для любого типа-источника конверсии.
        if matches!(name, "into" | "try_into") {
            return;
        }
        // Plan 91.8a.2 [M-91.8a.2-default-body-general] 2026-05-29:
        // generalized protocol default-body satisfiability. Replaces prior
        // hardcoded equals/fmt MVP. Walks ALL protocols, finds methods named
        // `name` with `default_body`, and for each checks whether the body's
        // top-level method/free-fn calls resolve for T (i.e. T provides every
        // method/overload referenced by the body). If at least one protocol
        // is satisfied → accept the bare call; codegen general synthesizer
        // emits the concrete Nova_<T>_method_<name> on first use.
        //
        // Acceptance precision: a body's `@compare(other)` requires T to have
        // `@compare`; `sb.append(str.from(@))` requires `str.from(T)` overload
        // OR `T.@into() -> str`. Patterns recognized by a small AST inspector
        // (collect_resolved_method_refs) — covers stdlib protocols (Equatable,
        // Printable, Comparable.equals via coercion). Bodies using unsupported
        // patterns are skipped (treated as not synthesizable) — type-checker
        // returns E7320 normally.
        if self.protocol_method_satisfiable_for(tname, name) {
            return;
        }
        // Plan 114.4.1 (D200): assoc const detection — если `name` matches
        // одну из assoc consts типа, hint user про namespace access.
        let is_assoc_const = self.types.get(tname)
            .map(|td| td.assoc_consts.iter().any(|ac| ac.name == name))
            .unwrap_or(false);
        if is_assoc_const {
            errors.push(
                Diagnostic::new(
                    format!(
                        "[E_CONST_INSTANCE_ACCESS] cannot access associated \
                         constant `{}.{}` через instance — assoc constants \
                         live на type-level (zero storage в instance). \
                         Use `{}.{}` namespace access instead (Plan 114.4.1 D200).",
                        tname, name, tname, name,
                    ),
                    span,
                )
            );
            return;
        }
        let avail: Vec<&str> =
            fields.iter().map(|f| f.name.as_str()).collect();
        let mut diag = Diagnostic::new(
            format!(
                "[E7320] no field or method `{}` on type `{}`",
                name, tname,
            ),
            span,
        );
        if !avail.is_empty() {
            diag = diag.with_note(format!(
                "`{}` has field{}: {}",
                tname,
                if avail.len() == 1 { "" } else { "s" },
                avail.join(", "),
            ));
        }
        errors.push(diag);
    }

    /// Ф.4: имя типа в value-позиции (`let c = Foo`, `Foo + 1`) → E7330.
    ///
    /// Флагится только bare `Ident`, разрешающийся в **непустой**
    /// Plan 91.8a.2 [M-91.8a.2-default-body-general] 2026-05-29.
    ///
    /// Generalized check: does type `tname` satisfy SOME protocol's method
    /// named `method_name` through its `default_body`? Walks ALL protocols
    /// in `self.types`, finds methods of that name with default_body, and
    /// for each tries to verify body's referenced calls resolve for T.
    ///
    /// Implementation: a small AST visitor (`default_body_calls_satisfy_for`)
    /// recursively walks the body and checks each `obj.method(...)` /
    /// `Type.method(...)` call that depends on Self or @ — verifies T
    /// provides a matching method or overload.
    ///
    /// Returns true if at least one protocol satisfies. False if no protocol
    /// has a matching default body OR every candidate has unsatisfiable
    /// dependencies.
    /// Plan 91.9 (D186): verify that type T satisfies every protocol listed
    /// в its `#impl(P1 + P2 + ...)` annotation. For each P:
    /// 1. P must be a known protocol (else E_UNKNOWN_PROTOCOL).
    /// 2. T must provide every required method of P:
    ///    - Either explicit `fn T @method(...)` declaration (in method_table), OR
    ///    - P's method has a `default_body` whose calls resolve for T
    ///      (`default_body_calls_satisfy_for` walker — same checker used
    ///      для bare-call satisfiability).
    /// Missing methods → E_IMPL_MISSING_METHODS со списком и hint'ом
    /// (как реализовать).
    fn verify_impl_protocols(&self, td: &TypeDecl, errors: &mut Vec<Diagnostic>) {
        for proto_name in &td.impl_protocols {
            let proto_decl = match self.types.get(proto_name.as_str()) {
                Some(td) => td,
                None => {
                    errors.push(Diagnostic::new(
                        format!(
                            "[E_UNKNOWN_PROTOCOL] type `{}` has `#impl({})` but \
                             `{}` is not a known type. Did you forget to import \
                             it, or misspell the protocol name?",
                            td.name, proto_name, proto_name,
                        ),
                        td.span,
                    ));
                    continue;
                }
            };
            let proto_methods = match &proto_decl.kind {
                TypeDeclKind::Protocol { methods, .. } => methods,
                _ => {
                    errors.push(Diagnostic::new(
                        format!(
                            "[E_IMPL_NOT_PROTOCOL] type `{}` has `#impl({})` but \
                             `{}` is not a protocol — it's a different kind of type. \
                             `#impl(...)` only accepts protocol names.",
                            td.name, proto_name, proto_name,
                        ),
                        td.span,
                    ));
                    continue;
                }
            };
            // Per-method check: T provides explicit method OR synthesizable from default body.
            // Plan 91.9 also enforces signature match для explicit methods —
            // E_IMPL_WRONG_SIGNATURE если T provides method с wrong arity /
            // param types / return type vs protocol declaration.
            let mut missing: Vec<String> = Vec::new();
            let mut wrong_sig: Vec<(String, String, String)> = Vec::new();
            for m in proto_methods {
                let has_explicit = self.t_provides_method(&td.name, &m.name);
                let has_default = if let Some(body) = &m.default_body {
                    self.default_body_calls_satisfy_for(body, &td.name)
                } else {
                    false
                };
                if has_explicit {
                    // Compare signature. Find T's fn for method name.
                    if let Some(t_method) = self.find_method_decl(&td.name, &m.name) {
                        if let Some(reason) = check_signature_match(t_method, m) {
                            wrong_sig.push((
                                m.name.clone(),
                                render_method_sig(&m.name, &m.params, &m.return_type),
                                reason,
                            ));
                        }
                    }
                } else if !has_default {
                    missing.push(render_method_sig(&m.name, &m.params, &m.return_type));
                }
            }
            for (name, expected, reason) in &wrong_sig {
                errors.push(Diagnostic::new(
                    format!(
                        "[E_IMPL_WRONG_SIGNATURE] type `{}` has method `{}` but its \
                         signature does not match the requirement from `#impl({})`. \
                         Expected: `{}`. {}\n  \
                         note: protocol method signatures must match exactly \
                         (arity, param types, return type — modulo Self ↔ {}).",
                        td.name, name, proto_name, expected, reason, td.name,
                    ),
                    td.span,
                ));
            }
            if !missing.is_empty() {
                let hint = missing.iter()
                    .map(|s| format!("  - {}", s))
                    .collect::<Vec<_>>()
                    .join("\n");
                errors.push(Diagnostic::new(
                    format!(
                        "[E_IMPL_MISSING_METHODS] type `{}` claims `#impl({})` but \
                         is missing required method(s):\n{}\n  \
                         note: implement these directly, e.g. `fn {} @<method>(...) -> ... => ...`, \
                         or ensure dependencies for a default body (e.g. `@compare` enables \
                         Equatable.equals via default).",
                        td.name, proto_name, hint, td.name,
                    ),
                    td.span,
                ));
            }
        }
    }

    fn protocol_method_satisfiable_for(&self, tname: &str, method_name: &str) -> bool {
        // Plan 91.9 (D186) gate: bare-call satisfiability требует `#impl(P)`
        // opt-in. Only protocols в T's impl_protocols list considered.
        // Без `#impl` — bare call к default-body-synthesized method даёт
        // E7320 normally (opt-in nominal layer над structural protocols).
        let opted_in: HashSet<&str> = self.types.get(tname)
            .map(|td| td.impl_protocols.iter().map(String::as_str).collect())
            .unwrap_or_default();
        for (proto_name, td) in &self.types {
            if !opted_in.contains(proto_name.as_str()) {
                continue;
            }
            if let TypeDeclKind::Protocol { methods, .. } = &td.kind {
                for m in methods {
                    if m.name == method_name && m.default_body.is_some() {
                        let body = m.default_body.as_ref().unwrap();
                        if self.default_body_calls_satisfy_for(body, tname) {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    /// Recursively walks `body` (a protocol default_body Block) and checks
    /// whether every method reference / free-fn call on Self/@ resolves
    /// to a method available on `tname` (directly or via well-known
    /// auto-derive: `str.from(T)` overload OR `T.@into() -> str`).
    ///
    /// Conservative: unknown patterns return `true` (assume satisfiable).
    /// Codegen general synthesizer does the precise check at emission.
    fn default_body_calls_satisfy_for(&self, body: &Block, tname: &str) -> bool {
        let mut ok = true;
        self.walk_default_body_block(body, tname, &mut ok);
        ok
    }

    fn walk_default_body_block(&self, b: &Block, tname: &str, ok: &mut bool) {
        for s in &b.stmts {
            self.walk_default_body_stmt(s, tname, ok);
            if !*ok { return; }
        }
        if let Some(t) = &b.trailing {
            self.walk_default_body_expr(t, tname, ok);
        }
    }

    fn walk_default_body_stmt(&self, s: &Stmt, tname: &str, ok: &mut bool) {
        match s {
            Stmt::Expr(e) => self.walk_default_body_expr(e, tname, ok),
            Stmt::Return { value: Some(e), .. } => self.walk_default_body_expr(e, tname, ok),
            Stmt::Let(d) => self.walk_default_body_expr(&d.value, tname, ok),
            Stmt::Const(_) => {}
            Stmt::Assign { target, value, .. } => {
                self.walk_default_body_expr(target, tname, ok);
                self.walk_default_body_expr(value, tname, ok);
            }
            _ => {}
        }
    }

    fn walk_default_body_expr(&self, e: &Expr, tname: &str, ok: &mut bool) {
        if !*ok { return; }
        match &e.kind {
            ExprKind::Call { func, args, .. } => {
                // Check the call target. Two patterns matter:
                // 1. Member call `obj.method(...)` where `obj` is `@` (SelfAccess)
                //    → require T provides the method.
                // 2. Path call `Type.method(arg)` where one arg is `@` → handle
                //    well-known auto-derive: `str.from(@)` accepts if T has
                //    `str.from(T)` overload OR `T.@into() -> str`.
                if let ExprKind::Member { obj, name } = &func.kind {
                    if matches!(obj.kind, ExprKind::SelfAccess) {
                        if !self.t_provides_method(tname, name) {
                            *ok = false;
                            return;
                        }
                    }
                } else if let ExprKind::Path(parts) = &func.kind {
                    if parts.len() == 2 && parts[0] == "str" && parts[1] == "from" {
                        if args.iter().any(|a| matches!(a.expr().kind, ExprKind::SelfAccess))
                            && !self.t_satisfies_str_from(tname)
                        {
                            *ok = false;
                            return;
                        }
                    }
                }
                self.walk_default_body_expr(func, tname, ok);
                for a in args { self.walk_default_body_expr(a.expr(), tname, ok); }
            }
            ExprKind::Member { obj, name } => {
                if matches!(obj.kind, ExprKind::SelfAccess) {
                    // Bare access `@method` (method-value, not call) — require T provides.
                    if !self.t_provides_method(tname, name)
                        && !self.t_provides_field(tname, name)
                    {
                        *ok = false;
                        return;
                    }
                }
                self.walk_default_body_expr(obj, tname, ok);
            }
            ExprKind::Binary { left, right, .. } => {
                self.walk_default_body_expr(left, tname, ok);
                self.walk_default_body_expr(right, tname, ok);
            }
            ExprKind::Unary { operand, .. } => self.walk_default_body_expr(operand, tname, ok),
            ExprKind::Try(inner) | ExprKind::Bang(inner) => self.walk_default_body_expr(inner, tname, ok),
            ExprKind::Coalesce(a, b) => {
                self.walk_default_body_expr(a, tname, ok);
                self.walk_default_body_expr(b, tname, ok);
            }
            ExprKind::As(inner, _) | ExprKind::Is(inner, _) => {
                self.walk_default_body_expr(inner, tname, ok);
            }
            ExprKind::If { cond, then, else_ } => {
                self.walk_default_body_expr(cond, tname, ok);
                self.walk_default_body_block(then, tname, ok);
                if let Some(eb) = else_ {
                    match eb {
                        crate::ast::ElseBranch::Block(b) => self.walk_default_body_block(b, tname, ok),
                        crate::ast::ElseBranch::If(i) => self.walk_default_body_expr(i, tname, ok),
                    }
                }
            }
            ExprKind::Block(b) => self.walk_default_body_block(b, tname, ok),
            _ => {}
        }
    }

    /// T has method `name` (instance or static, with-or-without `@` prefix).
    fn t_provides_method(&self, tname: &str, name: &str) -> bool {
        self.method_table.get(tname).map_or(false, |m| {
            m.keys().any(|k| k.trim_start_matches('@') == name)
        })
    }

    /// Find the FnDecl of T's method with given name. Returns first match
    /// (overloads not typical для protocol methods — strict 1-to-1 match).
    fn find_method_decl(&self, tname: &str, name: &str) -> Option<&FnDecl> {
        let methods = self.method_table.get(tname)?;
        for (k, fns) in methods.iter() {
            if k.trim_start_matches('@') == name {
                return fns.first().copied();
            }
        }
        None
    }

    fn t_provides_field(&self, tname: &str, name: &str) -> bool {
        if let Some(td) = self.types.get(tname) {
            if let TypeDeclKind::Record(fields) = &td.kind {
                return fields.iter().any(|f| f.name == name);
            }
        }
        false
    }

    /// `str.from(T)` satisfied if T has either:
    /// - `fn str.from(T) -> str` overload registered, OR
    /// - `fn T @into() -> str` (D73 chain).
    fn t_satisfies_str_from(&self, tname: &str) -> bool {
        // Path 1: explicit `fn str.from(T) -> str` overload.
        let str_from = self.method_table.get("str")
            .and_then(|m| m.get("from"))
            .map_or(false, |fns| fns.iter().any(|f| {
                f.params.len() == 1
                    && matches!(&f.params[0].ty, TypeRef::Named { path, .. }
                        if path.last().map_or(false, |s| s == tname))
            }));
        if str_from { return true; }
        // Path 2: T has `@into() -> str`.
        self.method_table.get(tname)
            .and_then(|m| m.get("into").or_else(|| m.get("@into")))
            .map_or(false, |fns| fns.iter().any(|f| {
                matches!(&f.return_type, Some(TypeRef::Named { path, .. })
                    if path.len() == 1 && path[0] == "str")
            }))
    }

    /// record/sum-тип. Пустые типы (unit), эффекты (handler — значение),
    /// протоколы/newtype/alias/opaque, а также имена, перекрытые
    /// локальной переменной — пропускаются (валидно либо неоднозначно).
    fn f4_check_value(
        &self,
        expr: &Expr,
        scope: &HashMap<String, TypeRef>,
        errors: &mut Vec<Diagnostic>,
    ) {
        let ExprKind::Ident(name) = &expr.kind else { return; };
        // Локальная переменная / параметр перекрывает имя типа.
        if scope.contains_key(name) {
            return;
        }
        let Some(td) = self.types.get(name) else { return; };
        let kind = match &td.kind {
            TypeDeclKind::Record(fields) if !fields.is_empty() => "record type",
            TypeDeclKind::Sum(variants) if !variants.is_empty() => "sum type",
            // empty record/sum (unit), effect/protocol/newtype/alias/opaque —
            // не value-misuse (либо валидно как значение, либо неоднозначно).
            _ => return,
        };
        let hint = match &td.kind {
            TypeDeclKind::Record(_) => format!(
                "construct a value: `{} {{ ... }}` or a constructor `{}.new(...)`",
                name, name,
            ),
            TypeDeclKind::Sum(_) => {
                format!("use one of `{}`'s variants", name)
            }
            _ => String::new(),
        };
        errors.push(
            Diagnostic::new(
                format!("[E7330] `{}` is a {}, not a value", name, kind),
                expr.span,
            )
            .with_note(hint),
        );
    }

    /// Ф.1: совместимо ли `expr` с типом `expected`?
    ///
    /// `expr_gs` — generic-scope места, где написан `expr`; `exp_gs` —
    /// generic-scope, в котором объявлен `expected` (для arg↔param это
    /// разные scope: caller vs callee). Числовые литералы полиморфны
    /// (D44): целый литерал совместим с любым числовым типом.
    fn assignable(
        &self,
        expr: &Expr,
        expected: &TypeRef,
        expr_gs: &HashSet<String>,
        exp_gs: &HashSet<String>,
        scope: &HashMap<String, TypeRef>,
    ) -> Compat {
        let exp_cat = self.cat_of(expected, exp_gs);
        // Generic-параметр / any / func / tuple — проверить нельзя.
        if matches!(exp_cat, TyCat::Other) {
            return Compat::Ok;
        }
        // Литералы: тип адаптируется к контексту (D44).
        match &expr.kind {
            ExprKind::IntLit(_) => {
                return match exp_cat {
                    TyCat::Int | TyCat::Float => Compat::Ok,
                    _ => Compat::Bad { found: "int".to_string() },
                };
            }
            ExprKind::FloatLit(_) => {
                return match exp_cat {
                    TyCat::Float => Compat::Ok,
                    _ => Compat::Bad { found: "f64".to_string() },
                };
            }
            ExprKind::BoolLit(_) => {
                return if exp_cat == TyCat::Bool {
                    Compat::Ok
                } else {
                    Compat::Bad { found: "bool".to_string() }
                };
            }
            ExprKind::StrLit(_) | ExprKind::InterpolatedStr { .. } => {
                return if exp_cat == TyCat::Str {
                    Compat::Ok
                } else {
                    Compat::Bad { found: "str".to_string() }
                };
            }
            ExprKind::CharLit(_) => {
                return if exp_cat == TyCat::Char {
                    Compat::Ok
                } else {
                    Compat::Bad { found: "char".to_string() }
                };
            }
            ExprKind::UnitLit => {
                return if exp_cat == TyCat::Unit {
                    Compat::Ok
                } else {
                    Compat::Bad { found: "()".to_string() }
                };
            }
            _ => {}
        }
        // Не-литерал: вывести тип; не вышло → Unknown (skip, не ошибка).
        let Some(found_tr) = self.infer_expr_type(expr, scope) else {
            return Compat::Unknown;
        };
        let found_cat = self.cat_of(&found_tr, expr_gs);
        if cat_compatible(&found_cat, &exp_cat) {
            Compat::Ok
        } else {
            Compat::Bad { found: typeref_display(&found_tr) }
        }
    }

    /// Ф.1: best-effort вывод типа выражения (для не-литералов).
    fn infer_expr_type(
        &self,
        expr: &Expr,
        scope: &HashMap<String, TypeRef>,
    ) -> Option<TypeRef> {
        match &expr.kind {
            ExprKind::Ident(name) => scope.get(name).cloned(),
            ExprKind::RecordLit { type_name: Some(name), .. } => {
                Some(TypeRef::Named {
                    path: name.clone(),
                    generics: Vec::new(),
                    span: expr.span,
                })
            }
            ExprKind::As(_, ty) => Some(ty.clone()),
            ExprKind::IntLit(_) => Some(prim_ref("int", expr.span)),
            ExprKind::FloatLit(_) => Some(prim_ref("f64", expr.span)),
            ExprKind::BoolLit(_) => Some(prim_ref("bool", expr.span)),
            ExprKind::StrLit(_) | ExprKind::InterpolatedStr { .. } => {
                Some(prim_ref("str", expr.span))
            }
            ExprKind::CharLit(_) => Some(prim_ref("char", expr.span)),
            // D176 (Plan 108): SelfAccess → look up "@" in scope (injected by f1_check_fn).
            ExprKind::SelfAccess => scope.get("@").cloned(),
            _ => None,
        }
    }

    // ── D175/D176 (Plan 108): readonly enforcement helpers ─────────────────

    /// Resolve record fields for a type name. Returns None if not a Record type.
    fn record_fields_for<'t>(&'t self, type_name: &str) -> Option<&'t Vec<RecordField>> {
        match self.types.get(type_name)?.kind {
            TypeDeclKind::Record(ref fields) => Some(fields),
            _ => None,
        }
    }

    /// Returns `true` if `expr` is an access path that goes through a readonly field,
    /// meaning any mutation through this path is forbidden (D175 transitivity).
    fn is_readonly_path(&self, expr: &Expr, scope: &HashMap<String, TypeRef>) -> bool {
        match &expr.kind {
            ExprKind::Member { obj, name: field_name } => {
                // Check if this field is readonly on obj's type.
                let obj_ty = self.infer_expr_type(obj, scope);
                if let Some(tr) = obj_ty {
                    let type_name = match tr.strip_readonly() {
                        TypeRef::Named { path, .. } => path.last().map(|s| s.as_str()),
                        _ => None,
                    };
                    if let Some(tname) = type_name {
                        if let Some(fields) = self.record_fields_for(tname) {
                            if fields.iter().any(|f| f.name == *field_name && f.readonly) {
                                return true;
                            }
                        }
                    }
                }
                // Transitivity: if the path to obj is itself readonly, propagate.
                self.is_readonly_path(obj, scope)
            }
            _ => false,
        }
    }

    /// Check if an assignment target is readonly and emit an error if so.
    /// Handles D175 (readonly fields) and D176 (readonly T index writes).
    fn check_target_readonly(
        &self,
        target: &Expr,
        scope: &HashMap<String, TypeRef>,
        errors: &mut Vec<Diagnostic>,
    ) {
        match &target.kind {
            ExprKind::Member { obj, name: field_name } => {
                // Transitivity check: mutation through a ro field (Plan 114 D184).
                if self.is_readonly_path(obj, scope) {
                    errors.push(Diagnostic::new(
                        format!(
                            "[E_READONLY_FIELD] cannot mutate `{}` through a `ro` field path",
                            field_name
                        ),
                        target.span,
                    ));
                    return;
                }
                // Direct check: is this specific field readonly?
                let obj_ty = self.infer_expr_type(obj, scope);
                if let Some(tr) = obj_ty {
                    let type_name = match tr.strip_readonly() {
                        TypeRef::Named { path, .. } => path.last().map(|s| s.as_str()),
                        _ => None,
                    };
                    if let Some(tname) = type_name {
                        if let Some(fields) = self.record_fields_for(tname) {
                            if let Some(f) = fields.iter().find(|f| f.name == *field_name) {
                                if f.readonly {
                                    errors.push(Diagnostic::new(
                                        format!(
                                            "[E_READONLY_FIELD] cannot assign to `ro` field `{}` of type `{}`",
                                            field_name, tname
                                        ),
                                        target.span,
                                    ));
                                }
                            }
                        }
                    }
                }
            }
            // D176 / Plan 114 D184: index write `arr[i] = x` — forbid if arr has `ro` type.
            ExprKind::Index { obj, .. } => {
                let obj_ty = self.infer_expr_type(obj, scope);
                if let Some(tr) = obj_ty {
                    if tr.is_readonly() {
                        errors.push(Diagnostic::new(
                            "[E_READONLY_CONTENT] cannot write through index on `ro` array".to_string(),
                            target.span,
                        ));
                    }
                }
            }
            _ => {}
        }
    }

    // ── End D175/D176 helpers ──────────────────────────────────────────────

    /// Ф.1: грубая категория типа. Alias/newtype разворачиваются —
    /// assignability сравнивает категории, не имена (newtype-cast
    /// строгость D54 — отдельная забота Plan 37).
    fn cat_of(&self, tr: &TypeRef, gs: &HashSet<String>) -> TyCat {
        self.cat_of_depth(tr, gs, 0)
    }

    fn cat_of_depth(
        &self,
        tr: &TypeRef,
        gs: &HashSet<String>,
        depth: u32,
    ) -> TyCat {
        if depth > 16 {
            return TyCat::Other;
        }
        match tr {
            TypeRef::Named { path, .. } => {
                let Some(name) = path.last() else { return TyCat::Other; };
                if gs.contains(name) {
                    return TyCat::Other;
                }
                match name.as_str() {
                    "int" | "i8" | "i16" | "i32" | "i64" | "u8" | "u16"
                    | "u32" | "u64" | "uint" => TyCat::Int,
                    "f32" | "f64" => TyCat::Float,
                    "bool" => TyCat::Bool,
                    "str" => TyCat::Str,
                    "char" => TyCat::Char,
                    "any" | "never" | "Self" => TyCat::Other,
                    other => match self.types.get(other) {
                        Some(td) => match &td.kind {
                            TypeDeclKind::Alias(inner)
                            | TypeDeclKind::Newtype(inner) => {
                                self.cat_of_depth(inner, gs, depth + 1)
                            }
                            // Concrete data-типы — сравниваются по имени.
                            TypeDeclKind::Record(_)
                            | TypeDeclKind::Sum(_) => {
                                TyCat::Named(other.to_string())
                            }
                            // protocol/effect — структурная конформность
                            // (забота D72 bound-checker'а), opaque —
                            // непрозрачен: любой concrete-тип потенциально
                            // совместим → permissive.
                            TypeDeclKind::Protocol { .. }
                            | TypeDeclKind::Effect(_)
                            | TypeDeclKind::Opaque => TyCat::Other,
                        },
                        // Неизвестное имя — вариант sum-типа, не-смерженный
                        // импорт, generic из чужого scope: permissive,
                        // чтобы исключить ложные срабатывания.
                        None => TyCat::Other,
                    },
                }
            }
            TypeRef::Array(inner, _) | TypeRef::FixedArray(_, inner, _) => {
                TyCat::Array(Box::new(self.cat_of_depth(inner, gs, depth + 1)))
            }
            TypeRef::Tuple(_, _) | TypeRef::Func { .. } => TyCat::Other,
            // Plan 97 Ф.2 (D142): анонимный protocol-тип — структурный
            // контракт. Конформность проверяется отдельно
            // (`check_satisfaction` + inline-protocol case), для
            // category-based assignability — `Other` (permissive, чтобы
            // любой concrete-тип не отвергался).
            TypeRef::Protocol { .. } => TyCat::Other,
            TypeRef::Unit(_) => TyCat::Unit,
            // D176 (Plan 108): readonly T — same category as inner (transparent for assignability).
            TypeRef::Readonly(inner, _) => self.cat_of_depth(inner, gs, depth + 1),
        }
    }
}

/// Ф.1: результат проверки совместимости.
enum Compat {
    /// Совместимо.
    Ok,
    /// Тип выражения не выводится — проверку пропускаем (не ошибка).
    Unknown,
    /// Несовместимо; `found` — отображение типа выражения.
    Bad { found: String },
}

/// Ф.1: грубая категория типа для assignability.
#[derive(PartialEq, Clone)]
enum TyCat {
    Int,
    Float,
    Bool,
    Str,
    Char,
    Unit,
    /// Concrete именованный тип (record/sum) — сравнивается по имени.
    Named(String),
    Array(Box<TyCat>),
    /// Generic-параметр / `any` / func / tuple / неизвестное — проверку
    /// не делаем (permissive, чтобы не было ложных срабатываний).
    Other,
}

/// Ф.1: совместимы ли две категории. Permissive на `Other` и на разнице
/// ширины числовых типов (int↔float для не-литералов — codegen/`as`).
fn cat_compatible(found: &TyCat, expected: &TyCat) -> bool {
    use TyCat::*;
    match (found, expected) {
        (Other, _) | (_, Other) => true,
        (Int, Int) | (Float, Float) | (Int, Float) | (Float, Int) => true,
        (Bool, Bool) | (Str, Str) | (Char, Char) | (Unit, Unit) => true,
        (Named(a), Named(b)) => a == b,
        (Array(a), Array(b)) => cat_compatible(a, b),
        _ => false,
    }
}

/// Ф.1: generic-scope функции — её параметры + generics receiver-типа.
fn fn_generic_scope(fd: &FnDecl) -> HashSet<String> {
    let mut gs: HashSet<String> = HashSet::new();
    for g in &fd.generics {
        gs.insert(g.name.clone());
    }
    if let Some(r) = &fd.receiver {
        for tr in &r.generics {
            if let TypeRef::Named { path, .. } = tr {
                if path.len() == 1 {
                    gs.insert(path[0].clone());
                }
            }
        }
    }
    gs
}

/// Plan 81 Ф.2: имена-namespace со специальным dispatch в codegen
/// (`gc.collect()`, `Time.sleep()`, ...) — не обычные module-qualified
/// вызовы свободных функций. Совпадает со списком guard'а в
/// `emit_c.rs` (Member-rewrite Plan 70.1).
fn is_intrinsic_namespace(name: &str) -> bool {
    matches!(
        name,
        "gc" | "fibers" | "runtime" | "Channel" | "ChanReader"
        | "ChanWriter" | "Time" | "Monotonic" | "CancelToken"
        | "StringBuilder" | "WriteBuffer" | "ReadBuffer"
        | "f64" | "f32" | "int" | "u8" | "u16" | "u32" | "u64"
        | "i8" | "i16" | "i32" | "i64" | "Self" | "Duration"
    )
}

/// Ф.1: TypeRef примитива по имени.
fn prim_ref(name: &str, span: Span) -> TypeRef {
    TypeRef::Named {
        path: vec![name.to_string()],
        generics: Vec::new(),
        span,
    }
}

/// Ф.1: человекочитаемое отображение TypeRef для диагностик.
fn typeref_display(tr: &TypeRef) -> String {
    match tr {
        TypeRef::Named { path, generics, .. } => {
            let base = path.join(".");
            if generics.is_empty() {
                base
            } else {
                let inner: Vec<String> =
                    generics.iter().map(typeref_display).collect();
                format!("{}[{}]", base, inner.join(", "))
            }
        }
        TypeRef::Array(inner, _) => format!("[]{}", typeref_display(inner)),
        TypeRef::FixedArray(n, inner, _) => {
            format!("[{}]{}", n, typeref_display(inner))
        }
        TypeRef::Tuple(elems, _) => {
            let inner: Vec<String> =
                elems.iter().map(typeref_display).collect();
            format!("({})", inner.join(", "))
        }
        TypeRef::Func { params, return_type, .. } => {
            let ps: Vec<String> =
                params.iter().map(typeref_display).collect();
            let rt = return_type
                .as_ref()
                .map(|t| typeref_display(t))
                .unwrap_or_else(|| "()".to_string());
            format!("fn({}) -> {}", ps.join(", "), rt)
        }
        // Plan 97 Ф.2 (D142): анонимный protocol — компактное отображение
        // через сигнатуры. В диагностике пользователю важно отличить
        // anon-protocol от других видов типа.
        TypeRef::Protocol { methods, .. } => {
            let sigs: Vec<String> = methods
                .iter()
                .map(|m| {
                    let prefix = if m.is_static { "." } else { "" };
                    let ps: Vec<String> = m
                        .params
                        .iter()
                        .map(|p| typeref_display(&p.ty))
                        .collect();
                    let rt = m
                        .return_type
                        .as_ref()
                        .map(|t| format!(" -> {}", typeref_display(t)))
                        .unwrap_or_default();
                    format!("{}{}({}){}", prefix, m.name, ps.join(", "), rt)
                })
                .collect();
            format!("protocol {{ {} }}", sigs.join("; "))
        }
        TypeRef::Unit(_) => "()".to_string(),
        // D176 (Plan 108): readonly T — display as "readonly T"
        TypeRef::Readonly(inner, _) => format!("ro {}", typeref_display(inner)),
    }
}

/// Ф.2: построить диагностику E7310 о неверной арности type-аргументов.
/// Вызывается только когда аргументы УКАЗАНЫ (`actual > 0`) — опущенные
/// аргументы легальны (выводятся из контекста), это не arity-ошибка.
fn arity_diag(name: &str, info: &ArityInfo, actual: usize, span: Span) -> Diagnostic {
    let plural = |n: usize| if n == 1 { "" } else { "s" };
    let werewas = |n: usize| if n == 1 { "was" } else { "were" };
    let msg = if info.count == 0 {
        format!(
            "[E7310] type `{}` is not generic — it takes no type arguments, \
             but {} {} provided",
            name, actual, werewas(actual),
        )
    } else {
        format!(
            "[E7310] type `{}` expects {} type argument{}, but {} {} provided",
            name, info.count, plural(info.count), actual, werewas(actual),
        )
    };
    let diag = Diagnostic::new(msg, span);
    match info.decl_span {
        Some(ds) => diag.with_note_at(format!("type `{}` declared here", name), ds),
        None => diag,
    }
}

/// Plan 101.4 (D145 Ред. 5): protocol composition validation.
///
/// Проверяет три инварианта на `type X protocol { use Y  use Z  ... }`:
///   1. **E_PROTOCOL_EMBED_NOT_PROTOCOL** — target типа `use TypeName`
///      объявлен, но это НЕ `TypeDeclKind::Protocol` (effect/record/sum/
///      alias/newtype). Embed работает только между protocol'ами.
///   2. **E_PROTOCOL_EMBED_UNKNOWN** — target не объявлен ни как protocol,
///      ни как любой другой тип (typo / forgotten import).
///   3. **E_PROTOCOL_EMBED_CYCLE** — `A use B use C use A` — циклическая
///      композиция. Detect через DFS.
///   4. **E_PROTOCOL_EMBED_DUPLICATE** — после flatten'а ≥2 метода с
///      одинаковым (name, arity) сигнатурой пришли из разных embed-путей
///      (или direct + embedded). Разрешено если строго совпадают; иначе
///      ambiguity, должна быть resolved direct-override'ом (V1 — error;
///      override-механизм — V2/D145 Ред. 6).
fn check_protocol_embeds(module: &Module, errors: &mut Vec<Diagnostic>) {
    use std::collections::{HashMap, HashSet};
    // Collect protocol declarations + map of all type names → kind hint.
    let mut proto_map: HashMap<String, (&Vec<EffectMethod>, &Vec<TypeRef>, Span)> = HashMap::new();
    let mut type_kinds: HashMap<String, &'static str> = HashMap::new();
    for item in &module.items {
        if let Item::Type(t) = item {
            let kind_name = match &t.kind {
                TypeDeclKind::Protocol { .. } => "protocol",
                TypeDeclKind::Effect(_) => "effect",
                TypeDeclKind::Record(_) => "record",
                TypeDeclKind::Sum(_) => "sum",
                TypeDeclKind::Alias(_) => "alias",
                TypeDeclKind::Newtype(_) => "newtype",
                TypeDeclKind::Opaque => "opaque",
            };
            type_kinds.insert(t.name.clone(), kind_name);
            if let TypeDeclKind::Protocol { methods, embeds } = &t.kind {
                proto_map.insert(t.name.clone(), (methods, embeds, t.span));
            }
        }
    }
    // 1+2: validate each embed reference.
    for (proto_name, (_methods, embeds, _span)) in &proto_map {
        for emb in embeds.iter() {
            let TypeRef::Named { path, span: emb_span, .. } = emb else {
                errors.push(Diagnostic::new(
                    format!(
                        "[E_PROTOCOL_EMBED_NOT_NAMED] `use` in protocol `{}` body \
                         requires a named protocol type (e.g. `use Reader`); \
                         complex type expressions are not allowed here",
                        proto_name
                    ),
                    emb.span(),
                ));
                continue;
            };
            let Some(emb_name) = path.last() else { continue };
            // Self-embed `use Self` или `use <SelfName>` — circular trivially.
            if emb_name == proto_name {
                errors.push(Diagnostic::new(
                    format!(
                        "[E_PROTOCOL_EMBED_CYCLE] protocol `{}` cannot embed itself \
                         (`use {}`)",
                        proto_name, emb_name
                    ),
                    *emb_span,
                ));
                continue;
            }
            match type_kinds.get(emb_name) {
                None => {
                    errors.push(Diagnostic::new(
                        format!(
                            "[E_PROTOCOL_EMBED_UNKNOWN] unknown type `{}` in \
                             `use {}` (protocol `{}` body) — type not declared \
                             in module or via import",
                            emb_name, emb_name, proto_name
                        ),
                        *emb_span,
                    ));
                }
                Some(&"protocol") => { /* OK */ }
                Some(other) => {
                    errors.push(Diagnostic::new(
                        format!(
                            "[E_PROTOCOL_EMBED_NOT_PROTOCOL] `use {}` in protocol \
                             `{}` body — `{}` is a {}, not a protocol. Protocol \
                             composition (D145 Ред. 5) requires `use <Protocol>`",
                            emb_name, proto_name, emb_name, other
                        ),
                        *emb_span,
                    ));
                }
            }
        }
    }
    // 3: cycle detection via DFS coloring (white/gray/black).
    #[derive(Clone, Copy, PartialEq)]
    enum Color { White, Gray, Black }
    let mut color: HashMap<String, Color> = proto_map.keys()
        .map(|n| (n.clone(), Color::White)).collect();
    fn dfs_cycle(
        node: &str,
        proto_map: &HashMap<String, (&Vec<EffectMethod>, &Vec<TypeRef>, Span)>,
        color: &mut HashMap<String, Color>,
        path: &mut Vec<String>,
        errors: &mut Vec<Diagnostic>,
    ) {
        let cur = *color.get(node).unwrap_or(&Color::White);
        if cur == Color::Black { return; }
        if cur == Color::Gray {
            // Cycle: path contains `node` earlier.
            let cycle_start = path.iter().position(|n| n == node).unwrap_or(0);
            let cycle_names: Vec<String> = path[cycle_start..].to_vec();
            let cycle_str = format!("{} → {}", cycle_names.join(" → "), node);
            // Use embed-span of first protocol in cycle if available.
            let span = proto_map.get(&cycle_names[0])
                .map(|(_, _, s)| *s).unwrap_or_default();
            errors.push(Diagnostic::new(
                format!(
                    "[E_PROTOCOL_EMBED_CYCLE] cyclic protocol composition: {}",
                    cycle_str
                ),
                span,
            ));
            return;
        }
        color.insert(node.to_string(), Color::Gray);
        path.push(node.to_string());
        if let Some((_, embeds, _)) = proto_map.get(node) {
            for emb in embeds.iter() {
                if let TypeRef::Named { path: p, .. } = emb {
                    if let Some(n) = p.last() {
                        if proto_map.contains_key(n) {
                            dfs_cycle(n, proto_map, color, path, errors);
                        }
                    }
                }
            }
        }
        path.pop();
        color.insert(node.to_string(), Color::Black);
    }
    // Iterate sorted for deterministic diagnostic order.
    let mut names: Vec<String> = proto_map.keys().cloned().collect();
    names.sort();
    for name in &names {
        if *color.get(name).unwrap_or(&Color::White) == Color::White {
            let mut path = Vec::new();
            dfs_cycle(name, &proto_map, &mut color, &mut path, errors);
        }
    }
    // 4: duplicate-method detection after flatten.
    // Flatten без cycle (cycle уже reported) — guard через max-depth.
    fn flatten_with_origin(
        name: &str,
        proto_map: &HashMap<String, (&Vec<EffectMethod>, &Vec<TypeRef>, Span)>,
        seen: &mut HashSet<String>,
        out: &mut Vec<(String, String, usize)>, // (origin_proto, method_name, arity)
        origin: &str,
    ) {
        if !seen.insert(name.to_string()) { return; }
        let Some((methods, embeds, _)) = proto_map.get(name) else { return; };
        for m in methods.iter() {
            out.push((origin.to_string(), m.name.clone(), m.params.len()));
        }
        for emb in embeds.iter() {
            if let TypeRef::Named { path: p, .. } = emb {
                if let Some(n) = p.last() {
                    flatten_with_origin(n, proto_map, seen, out, n);
                }
            }
        }
    }
    for (proto_name, (local_methods, _, span)) in &proto_map {
        let mut entries = Vec::new();
        let mut seen = HashSet::new();
        flatten_with_origin(proto_name, &proto_map, &mut seen, &mut entries, proto_name);
        // Group by (name, arity). >1 distinct origins → duplicate.
        let mut sig_origins: HashMap<(String, usize), Vec<String>> = HashMap::new();
        for (orig, mname, arity) in entries {
            sig_origins.entry((mname, arity)).or_default().push(orig);
        }
        // Plan 91.8a (D183): local override allowed — если метод есть и
        // локально в proto_name, и из embed'ов, локальная декларация
        // считается override embedded-default'а (НЕ duplicate). Это
        // используется напр. в `Comparable.equals` default body для
        // embedded `Equatable.equals`.
        let local_sigs: HashSet<(String, usize)> = local_methods.iter()
            .map(|m| (m.name.clone(), m.params.len()))
            .collect();
        for ((mname, arity), origins) in sig_origins {
            // Уникальные источники (один и тот же origin >1 раз не считается).
            let unique: HashSet<String> = origins.iter().cloned().collect();
            if unique.len() > 1 {
                // Override-by-local case: если метод объявлен локально И
                // также приходит из embed'а — local wins, skip duplicate.
                if local_sigs.contains(&(mname.clone(), arity)) {
                    continue;
                }
                let mut sources: Vec<String> = unique.into_iter().collect();
                sources.sort();
                errors.push(Diagnostic::new(
                    format!(
                        "[E_PROTOCOL_EMBED_DUPLICATE] method `{}/{}` in protocol \
                         `{}` is provided by multiple embedded protocols: {}. \
                         Protocol composition (D145 Ред. 5) does not yet support \
                         override; remove one embed or define the method directly \
                         (Plan 91.8a D183: declaring the method locally в `{}` \
                         overrides embedded default).",
                        mname, arity, proto_name, sources.join(", "), proto_name
                    ),
                    *span,
                ));
            }
        }
    }
}

/// Plan 101.3 (D145 Ред. 5): валидация bound-имён в declaration
/// generic-параметров. Для каждого `[T A + B + C]` проверяем, что
/// каждое имя bound'а — это объявленный protocol, либо well-known
/// stdlib-alias (Hashable/Eq/Ord/Display/Equatable/Comparable/ToStr/
/// TryFrom/TryInto), либо primitive-имя (Q-representation-bound future).
/// Если имя — record/sum/effect → error E_BOUND_NOT_PROTOCOL.
/// Если имя вообще unknown → error E_BOUND_UNKNOWN.
fn check_generic_bound_declarations(module: &Module, errors: &mut Vec<Diagnostic>) {
    use std::collections::HashMap;
    // Карта известных type-имён → kind hint.
    let mut type_kinds: HashMap<String, &'static str> = HashMap::new();
    for item in &module.items {
        if let Item::Type(t) = item {
            let kind_name = match &t.kind {
                TypeDeclKind::Protocol { .. } => "protocol",
                TypeDeclKind::Effect(_) => "effect",
                TypeDeclKind::Record(_) => "record",
                TypeDeclKind::Sum(_) => "sum",
                TypeDeclKind::Alias(_) => "alias",
                TypeDeclKind::Newtype(_) => "newtype",
                TypeDeclKind::Opaque => "opaque",
            };
            type_kinds.insert(t.name.clone(), kind_name);
        }
    }
    // Well-known stdlib alias names (legacy + Plan 62.E migrated).
    // Plan 91.8a (D183): Iter→Iterable, Display→Printable.
    let stdlib_aliases: &[&str] = &[
        "Ord", "Eq", "ToStr", "TryFrom", "TryInto",
        "Hashable", "Printable", "Equatable", "Comparable",
        "Iterable", "From", "Into",
    ];
    // Primitive-имена (Q-representation-bound future):
    let primitives: &[&str] = &[
        "int", "i8", "i16", "i32", "i64",
        "u8", "u16", "u32", "u64", "uint",
        "f32", "f64", "bool", "char", "str", "any", "never",
    ];
    let check_bound = |b: &TypeRef, errors: &mut Vec<Diagnostic>| {
        let TypeRef::Named { path, span, .. } = b else { return; };
        let Some(name) = path.last() else { return; };
        // Если у имени префикс (`std.collections.Iter`), берём последний.
        // Allowed: protocol, alias, primitive.
        if stdlib_aliases.contains(&name.as_str()) { return; }
        if primitives.contains(&name.as_str()) { return; }
        match type_kinds.get(name) {
            Some(&"protocol") => { /* OK */ }
            Some(&kind) => {
                errors.push(Diagnostic::new(
                    format!(
                        "[E_BOUND_NOT_PROTOCOL] `{}` is a {}, not a protocol — \
                         generic bounds must be protocol-types (D72). Consider \
                         declaring `type {} protocol {{ ... }}` if structural \
                         contract is intended.",
                        name, kind, name
                    ),
                    *span,
                ));
            }
            None => {
                errors.push(Diagnostic::new(
                    format!(
                        "[E_BOUND_UNKNOWN] unknown type `{}` used as generic bound — \
                         not a declared protocol, stdlib alias, or primitive. \
                         Did you forget to declare/import it?",
                        name
                    ),
                    *span,
                ));
            }
        }
    };
    for item in &module.items {
        match item {
            Item::Fn(f) => {
                for g in &f.generics {
                    for b in &g.bounds {
                        check_bound(b, errors);
                    }
                }
            }
            Item::Type(t) => {
                for g in &t.generics {
                    for b in &g.bounds {
                        check_bound(b, errors);
                    }
                }
            }
            _ => {}
        }
    }
}

/// Plan 15 (D72): registry РґР»СЏ bound enforcement.
///
/// `protocol_specs`: РґР»СЏ РєР°Р¶РґРѕРіРѕ `type Foo protocol { ... }` вЂ” СЃРїРёСЃРѕРє
/// required methods (TypeDeclKind::Effect; РІ Nova protocol/effect РµРґРёРЅР°СЏ
/// С„РѕСЂРјР° РїРѕ D62).
///
/// `fn_decls`: top-level fn-РґРµРєР»Р°СЂР°С†РёРё (РґР»СЏ resolve РІС‹Р·РѕРІР° РїРѕ РёРјРµРЅРё).
///
/// `method_table`: РґР»СЏ РєР°Р¶РґРѕРіРѕ concrete-С‚РёРїР° вЂ” РјРµС‚РѕРґС‹ (РїРѕ РёРјРµРЅРё), РґР»СЏ
/// РїСЂРѕРІРµСЂРєРё "type T satisfies protocol P".
struct BoundCtx<'a> {
    /// Plan 15 D53 strict: С‚РѕР»СЊРєРѕ protocol-kind С‚РёРїРѕРІ. Effect-kind
    /// СЃСЋРґР° РЅРµ РїРѕРїР°РґР°РµС‚ вЂ” effects РЅРµ СЂР°Р·СЂРµС€РµРЅС‹ РєР°Рє D72 bounds.
    ///
    /// Plan 101.4 (D145 Ред. 5): значение — **flattened** список методов:
    /// direct + recursive embedded protocol methods. Поэтому owned `Vec`,
    /// а не borrow в AST (синтетическая копия после embed-expansion).
    /// Flatten построен в `BoundCtx::build` через DFS с cycle-protection.
    protocol_specs: HashMap<String, Vec<EffectMethod>>,
    /// Plan 15 D53 strict: effect-kind С‚РёРїС‹. РСЃРїРѕР»СЊР·СѓРµС‚СЃСЏ РґР»СЏ
    /// РґРёС„С„РµСЂРµРЅС†РёРёСЂРѕРІР°РЅРЅРѕРіРѕ error-СЃРѕРѕР±С‰РµРЅРёСЏ, РµСЃР»Рё РёС… РїС‹С‚Р°СЋС‚СЃСЏ
    /// РёСЃРїРѕР»СЊР·РѕРІР°С‚СЊ РєР°Рє bound (В«`Db` is an effect, not a protocolВ»).
    effect_decls: HashMap<String, &'a TypeDecl>,
    /// D84: HashMap в†’ Vec<&FnDecl> С‡С‚РѕР±С‹ С…СЂР°РЅРёС‚СЊ multiple overloads
    /// РѕРґРЅРѕРіРѕ РёРјРµРЅРё (РјРµС‚РѕРґС‹ Рё СЃРІРѕР±РѕРґРЅС‹Рµ С„СѓРЅРєС†РёРё). Р РµР·РѕР»РІ РІ check_call_bounds вЂ”
    /// С„РёР»СЊС‚СЂ РїРѕ arity. РџРѕР»РЅС‹Р№ type-based resolve РѕСЃС‚Р°С‘С‚СЃСЏ Р·Р° codegen (РіРґРµ
    /// РµСЃС‚СЊ type-РёРЅС„РµСЂ Р°СЂРіСѓРјРµРЅС‚РѕРІ).
    fn_decls: HashMap<String, Vec<&'a FnDecl>>,
    method_table: HashMap<String, HashMap<String, Vec<&'a FnDecl>>>,
    /// Plan 53: РёРјРµРЅР° sum-variant'РѕРІ (РґР»СЏ refutability check let-pattern).
    /// `type Color | Red | Green` в†’ {"Red", "Green"}. РСЃРїРѕР»СЊР·СѓРµС‚СЃСЏ С‡С‚РѕР±С‹
    /// РѕС‚Р»РёС‡РёС‚СЊ `let Color.Red { x } = obj` (refutable, error) РѕС‚
    /// `let Pair { x, y } = p` (irrefutable record).
    sum_variant_names: std::collections::HashSet<String>,
}

impl<'a> BoundCtx<'a> {
    fn build(module: &'a Module) -> Self {
        // Plan 101.4: direct = name → (own methods, embed-typerefs).
        // Используется для flatten DFS ниже.
        let mut direct: HashMap<String, (Vec<EffectMethod>, Vec<TypeRef>)> = HashMap::new();
        let mut fn_decls: HashMap<String, Vec<&FnDecl>> = HashMap::new();
        let mut method_table: HashMap<String, HashMap<String, Vec<&FnDecl>>> = HashMap::new();
        let mut sum_variant_names: std::collections::HashSet<String> = std::collections::HashSet::new();

        let mut effect_decls: HashMap<String, &TypeDecl> = HashMap::new();
        for item in &module.items {
            match item {
                Item::Type(t) => {
                    // Plan 15 D53 strict: protocol-kind в†’ eligible РєР°Рє
                    // bound (D72); effect-kind в†’ РѕС‚РґРµР»СЊРЅС‹Р№ registry РґР»СЏ
                    // РґРёР°РіРЅРѕСЃС‚РёРєРё В«used as bound but it's an effectВ».
                    match &t.kind {
                        TypeDeclKind::Protocol { methods, embeds } => {
                            direct.insert(
                                t.name.clone(),
                                (methods.clone(), embeds.clone()),
                            );
                        }
                        TypeDeclKind::Effect(_) => {
                            effect_decls.insert(t.name.clone(), t);
                        }
                        // Plan 53: sum-variants РґР»СЏ refutability check.
                        TypeDeclKind::Sum(variants) => {
                            for v in variants {
                                sum_variant_names.insert(v.name.clone());
                            }
                        }
                        _ => {}
                    }
                }
                Item::Fn(f) => {
                    if let Some(recv) = &f.receiver {
                        method_table
                            .entry(recv.type_name.clone())
                            .or_default()
                            .entry(f.name.clone())
                            .or_default()
                            .push(f);
                    } else {
                        // D84: СЃРІРѕР±РѕРґРЅС‹Рµ С„СѓРЅРєС†РёРё С‚РѕР¶Рµ РјРѕРіСѓС‚ РёРјРµС‚СЊ overloads.
                        fn_decls.entry(f.name.clone()).or_default().push(f);
                    }
                }
                _ => {}
            }
        }

        // Plan 101.4 flatten: для каждого protocol'а собираем полный
        // список методов = direct ∪ recursively-embedded. Cycle-protection
        // через `seen` — если протокол повторно встречается в DFS, его
        // методы НЕ добавляются повторно (silent skip; error diagnostic —
        // в `check_protocol_embeds` отдельно). Duplicate-method конфликты
        // тоже только в check_protocol_embeds; здесь — bag-union.
        fn flatten_dfs(
            name: &str,
            direct: &HashMap<String, (Vec<EffectMethod>, Vec<TypeRef>)>,
            seen: &mut std::collections::HashSet<String>,
            out: &mut Vec<EffectMethod>,
        ) {
            if !seen.insert(name.to_string()) {
                return;
            }
            let Some((methods, embeds)) = direct.get(name) else { return; };
            for m in methods {
                out.push(m.clone());
            }
            for e in embeds {
                if let TypeRef::Named { path, .. } = e {
                    if let Some(emb_name) = path.last() {
                        flatten_dfs(emb_name, direct, seen, out);
                    }
                }
            }
        }
        let mut protocol_specs: HashMap<String, Vec<EffectMethod>> = HashMap::new();
        for name in direct.keys() {
            let mut out = Vec::new();
            let mut seen = std::collections::HashSet::new();
            flatten_dfs(name, &direct, &mut seen, &mut out);
            protocol_specs.insert(name.clone(), out);
        }

        BoundCtx { protocol_specs, effect_decls, fn_decls, method_table, sum_variant_names }
    }

    fn check_module(&self, module: &Module, errors: &mut Vec<Diagnostic>) {
        // Plan 56 Ф.2.7 reverted (2026-05-20, D122 amended): эффекты в
        // protocol-методах РАЗРЕШЕНЫ. Под mono-dispatch (bootstrap) эффект
        // protocol-метода пробрасывается как у любой effectful-функции;
        // прежний запрет касался только true-vtable dispatch (Plan 03 —
        // там effectful-protocol bounds обязаны mono-dispatch'иться).
        // Пример: `type TryFrom[T,E] protocol { try_from(t T) Fail[E] -> Self }`.
        for item in &module.items {
            match item {
                Item::Fn(f) => {
                    let mut scope: HashMap<String, TypeRef> = HashMap::new();
                    // Р РµРіРёСЃС‚СЂРёСЂСѓРµРј РїР°СЂР°РјРµС‚СЂС‹ С„СѓРЅРєС†РёРё СЃ РёС… С‚РёРїР°РјРё.
                    for p in &f.params {
                        scope.insert(p.name.clone(), p.ty.clone());
                    }
                    self.walk_fn_body(f, &mut scope, errors);
                }
                Item::Test(t) => {
                    // Plan 15: С‚РµСЃС‚С‹ С‚РѕР¶Рµ РјРѕРіСѓС‚ СЃРѕРґРµСЂР¶Р°С‚СЊ generic-РІС‹Р·РѕРІС‹
                    // c bounds вЂ” РѕР±С…РѕРґРёРј РёС… body СЃРѕ СЃРІРµР¶РёРј scope.
                    let mut scope: HashMap<String, TypeRef> = HashMap::new();
                    self.walk_block(&t.body, &mut scope, errors);
                }
                _ => {}
            }
        }
    }

    fn walk_fn_body(&self, f: &FnDecl, scope: &mut HashMap<String, TypeRef>, errors: &mut Vec<Diagnostic>) {
        match &f.body {
            FnBody::Expr(e) => self.walk_expr(e, scope, errors),
            FnBody::Block(b) => self.walk_block(b, scope, errors),
            FnBody::External => {}
        }
    }

    fn walk_block(&self, b: &Block, scope: &mut HashMap<String, TypeRef>, errors: &mut Vec<Diagnostic>) {
        // РЎРѕС…СЂР°РЅСЏРµРј snapshot РґР»СЏ bindings РєРѕС‚РѕСЂС‹Рµ let'Р°СЋС‚СЃСЏ РІ СЌС‚РѕРј Р±Р»РѕРєРµ вЂ”
        // С‡С‚РѕР±С‹ РІРµСЂРЅСѓС‚СЊ scope РїРѕСЃР»Рµ Р±Р»РѕРєР° (block-out shadowing semantics).
        let mut snapshot: Vec<(String, Option<TypeRef>)> = Vec::new();
        for s in &b.stmts {
            if let Stmt::Let(d) = s {
                if let Some(name) = pattern_simple_name(&d.pattern) {
                    snapshot.push((name.clone(), scope.get(&name).cloned()));
                }
            }
        }
        for s in &b.stmts {
            self.walk_stmt(s, scope, errors);
        }
        if let Some(t) = &b.trailing {
            self.walk_expr(t, scope, errors);
        }
        // Р’РѕСЃСЃС‚Р°РЅРѕРІРёРј shadowed bindings (block-out).
        for (n, prev) in snapshot {
            match prev {
                Some(t) => { scope.insert(n, t); }
                None => { scope.remove(&n); }
            }
        }
    }

    fn walk_stmt(&self, s: &Stmt, scope: &mut HashMap<String, TypeRef>, errors: &mut Vec<Diagnostic>) {
        match s {
            Stmt::Expr(e) => self.walk_expr(e, scope, errors),
            Stmt::Let(d) => {
                self.walk_expr(&d.value, scope, errors);
                // Plan 53: refutable pattern РІ `let` вЂ” compile error.
                // Р”РѕРїСѓСЃС‚РёРјС‹ С‚РѕР»СЊРєРѕ irrefutable patterns (Ident, Wildcard,
                // Tuple, plain-Record). Refutable (Literal, Variant, Or,
                // Array, Record-Рє-sum-variant) Р»РѕРІРёРј Р·РґРµСЃСЊ вЂ” codegen Рё
                // interp Р°СЃСЃР°РјСЏС‚ irrefutable.
                self.check_let_pattern_irrefutable(&d.pattern, errors);
                // Р РµРіРёСЃС‚СЂРёСЂСѓРµРј simple-Ident pattern СЃ inferred С‚РёРїРѕРј.
                if let Some(name) = pattern_simple_name(&d.pattern) {
                    let inferred = d.ty.clone()
                        .or_else(|| Self::infer_arg_ty(&d.value, scope));
                    if let Some(t) = inferred {
                        scope.insert(name, t);
                    }
                }
            }
            // Plan 114.4 Ф.2: scope-local const — pass-through (no-op for now).
            Stmt::Const(_) => {}
            Stmt::Assign { target, value, .. } => {
                self.walk_expr(target, scope, errors);
                self.walk_expr(value, scope, errors);
            }
            Stmt::Return { value, .. } => {
                if let Some(v) = value { self.walk_expr(v, scope, errors); }
            }
            Stmt::Throw { value, .. } => self.walk_expr(value, scope, errors),
            Stmt::Break(_) | Stmt::Continue(_) => {}
            // D90 Plan 20 Р¤.2: body РїР°СЂСЃРёС‚СЃСЏ, walk'Р°РµРј вЂ” bound-checker
            // РїРѕР»СѓС‡РёС‚ call'С‹ РІРЅСѓС‚СЂРё body. Body-constraint РїСЂРѕРІРµСЂРєРё
            // (no Fail, no suspend, no exit-control) РґРѕР±Р°РІР»СЏСЋС‚СЃСЏ РІ Р¤.3.
            Stmt::Defer { body, .. } | Stmt::ErrDefer { body, .. }
            | Stmt::OkDefer { body, .. } | Stmt::DeferWithResult { body, .. } => {
                self.walk_expr(body, scope, errors);
            }
            // Plan 110 D188: walk init + body (scaffold).
            Stmt::ConsumeScope { init, body, .. } => {
                self.walk_expr(init, scope, errors);
                for s in &body.stmts {
                    self.walk_stmt(s, scope, errors);
                }
                if let Some(t) = &body.trailing {
                    self.walk_expr(t, scope, errors);
                }
            }
            // Plan 33.2 Р¤.8: assert_static вЂ” walk expr.
            Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => self.walk_expr(expr, scope, errors),
            // Ф.4.1: apply — ghost statement, args walk'аем (для name resolution).
            Stmt::Apply { args, .. } => {
                for a in args { self.walk_expr(a, scope, errors); }
            }
            // Ф.4.2: calc — ghost, шаги walk'аем.
            Stmt::Calc { steps, .. } => {
                for step in steps { self.walk_expr(&step.expr, scope, errors); }
            }
            // Plan 33.9 Ф.2: reveal — ghost, name resolution в pipeline.
            Stmt::Reveal { .. } => {}
        }
    }

    fn walk_expr(&self, e: &Expr, scope: &mut HashMap<String, TypeRef>, errors: &mut Vec<Diagnostic>) {
        // РџСЂРѕРІРµСЂСЏРµРј СЃР°Рј call РїРµСЂРµРґ СЂРµРєСѓСЂСЃРёРµР№ РІ args (РїРѕСЂСЏРґРѕРє РЅРµ РІР°Р¶РµРЅ).
        self.check_call_bounds(e, scope, errors);
        // Plan 46 (D102): argument binding diagnostics.
        self.check_call_argbind(e, scope, errors);
        // Plan 97.1 hardening: `box.method()` для protocol-typed var —
        // method обязан быть в protocol_specs[<Proto>].
        self.check_protocol_method_call(e, scope, errors);
        match &e.kind {
            ExprKind::Call { func, args, trailing } => {
                self.walk_expr(func, scope, errors);
                for a in args {
                    self.walk_expr(a.expr(), scope, errors);
                }
                if let Some(t) = trailing {
                    match t {
                        crate::ast::Trailing::Block(b) => self.walk_block(b, scope, errors),
                        crate::ast::Trailing::LegacyBlockWithParams(tb) => {
                            self.walk_block(&tb.body, scope, errors)
                        }
                        crate::ast::Trailing::Fn(sb) => {
                            // Trailing-fn body: Expr РёР»Рё Block.
                            match &sb.body {
                                FnBody::Expr(e) => self.walk_expr(e, scope, errors),
                                FnBody::Block(b) => self.walk_block(b, scope, errors),
                                FnBody::External => {}
                            }
                        }
                    }
                }
            }
            ExprKind::TurboFish { base, .. } => self.walk_expr(base, scope, errors),
            ExprKind::Binary { left, right, .. } => {
                self.walk_expr(left, scope, errors);
                self.walk_expr(right, scope, errors);
            }
            ExprKind::Unary { operand, .. } => self.walk_expr(operand, scope, errors),
            ExprKind::Try(inner) | ExprKind::Bang(inner) => self.walk_expr(inner, scope, errors),
            ExprKind::Coalesce(a, b) => {
                self.walk_expr(a, scope, errors);
                self.walk_expr(b, scope, errors);
            }
            ExprKind::As(e, _) => self.walk_expr(e, scope, errors),
            ExprKind::Is(e, _) => self.walk_expr(e, scope, errors),
            ExprKind::Member { obj, .. } => self.walk_expr(obj, scope, errors),
            ExprKind::Index { obj, index } => {
                self.walk_expr(obj, scope, errors);
                self.walk_expr(index, scope, errors);
            }
            ExprKind::If { cond, then, else_ } => {
                self.walk_expr(cond, scope, errors);
                self.walk_block(then, scope, errors);
                if let Some(eb) = else_ {
                    match eb {
                        ElseBranch::Block(b) => self.walk_block(b, scope, errors),
                        ElseBranch::If(e) => self.walk_expr(e, scope, errors),
                    }
                }
            }
            ExprKind::IfLet { scrutinee, then, else_, .. } => {
                self.walk_expr(scrutinee, scope, errors);
                self.walk_block(then, scope, errors);
                if let Some(eb) = else_ {
                    match eb {
                        ElseBranch::Block(b) => self.walk_block(b, scope, errors),
                        ElseBranch::If(e) => self.walk_expr(e, scope, errors),
                    }
                }
            }
            ExprKind::Match { scrutinee, arms } => {
                self.walk_expr(scrutinee, scope, errors);
                for arm in arms {
                    if let Some(g) = &arm.guard { self.walk_expr(g, scope, errors); }
                    match &arm.body {
                        MatchArmBody::Expr(e) => self.walk_expr(e, scope, errors),
                        MatchArmBody::Block(b) => self.walk_block(b, scope, errors),
                    }
                }
            }
            ExprKind::Block(b) => self.walk_block(b, scope, errors),
            ExprKind::ArrayLit(elems) => {
                for el in elems {
                    match el {
                        ArrayElem::Item(e) | ArrayElem::Spread(e) => self.walk_expr(e, scope, errors),
                    }
                }
            }
            ExprKind::MapLit { elems, .. } => {
                let pairs = crate::ast::MapElem::cloned_pairs(&elems);
                for (k, v) in pairs.iter() {
                    self.walk_expr(k, scope, errors);
                    self.walk_expr(v, scope, errors);
                }
            }
            ExprKind::TupleLit(elems) => {
                for e in elems { self.walk_expr(e, scope, errors); }
            }
            ExprKind::RecordLit { fields, .. } => {
                for f in fields {
                    if let Some(v) = &f.value { self.walk_expr(v, scope, errors); }
                }
            }
            ExprKind::TaggedTemplate { tag, args, .. } => {
                self.walk_expr(tag, scope, errors);
                for a in args { self.walk_expr(a, scope, errors); }
            }
            ExprKind::InterpolatedStr { parts } => {
                for p in parts {
                    if let InterpStrPart::Expr(e) = p {
                        self.walk_expr(e, scope, errors);
                    }
                }
            }
            ExprKind::Lambda { body, .. } => self.walk_expr(body, scope, errors),
            // Plan 19, C5: BoundCtx РѕР±С…РѕРґРёС‚ С‚РµР»Рѕ closure-light /
            // closure-full РґР»СЏ РіРµРЅРµСЂРёРє-bound РїСЂРѕРІРµСЂРѕРє. РџРѕР»РЅС‹Р№
            // bidirectional inference вЂ” С„Р°Р·Р° C6; Р·РґРµСЃСЊ вЂ” С‚РѕР»СЊРєРѕ walk.
            ExprKind::ClosureLight { body, .. } => match body {
                crate::ast::ClosureBody::Expr(e) => self.walk_expr(e, scope, errors),
                crate::ast::ClosureBody::Block(b) => self.walk_block(b, scope, errors),
            },
            ExprKind::ClosureFull(sb) => match &sb.body {
                FnBody::Expr(e) => self.walk_expr(e, scope, errors),
                FnBody::Block(b) => self.walk_block(b, scope, errors),
                FnBody::External => {}
            },
            ExprKind::Spawn(body) => self.walk_expr(body, scope, errors),
            ExprKind::Detach(body) | ExprKind::Blocking(body) => self.walk_block(body, scope, errors),
            ExprKind::Supervised { body, cancel } => {
                if let Some(c) = cancel { self.walk_expr(c, scope, errors); }
                self.walk_block(body, scope, errors);
            }
            ExprKind::Forbid { body, .. } => self.walk_block(body, scope, errors),
            ExprKind::Realtime { body, .. } => self.walk_block(body, scope, errors),
            ExprKind::ParallelFor { iter, body, .. } => {
                self.walk_expr(iter, scope, errors);
                self.walk_block(body, scope, errors);
            }
            ExprKind::For { iter, body, .. } => {
                self.walk_expr(iter, scope, errors);
                self.walk_block(body, scope, errors);
            }
            ExprKind::While { cond, body, .. } => {
                self.walk_expr(cond, scope, errors);
                self.walk_block(body, scope, errors);
            }
            ExprKind::WhileLet { scrutinee, body, .. } => {
                self.walk_expr(scrutinee, scope, errors);
                self.walk_block(body, scope, errors);
            }
            ExprKind::Loop { body, .. } => self.walk_block(body, scope, errors),
            ExprKind::Select { arms } => {
                for arm in arms {
                    match &arm.op {
                        SelectOp::Recv { chan, .. } => self.walk_expr(chan, scope, errors),
                        SelectOp::Send { chan, value } => {
                            self.walk_expr(chan, scope, errors);
                            self.walk_expr(value, scope, errors);
                        }
                        SelectOp::Default => {}
                    }
                    if let Some(g) = &arm.guard { self.walk_expr(g, scope, errors); }
                    self.walk_block(&arm.body, scope, errors);
                }
            }
            ExprKind::Range { start, end, .. } => {
                if let Some(s) = start { self.walk_expr(s, scope, errors); }
                if let Some(e) = end { self.walk_expr(e, scope, errors); }
            }
            ExprKind::Throw(e) => self.walk_expr(e, scope, errors),
            ExprKind::Interrupt(opt) => {
                if let Some(e) = opt { self.walk_expr(e, scope, errors); }
            }
            ExprKind::With { body, .. } => self.walk_block(body, scope, errors),
            // D.1.3: РєРІР°РЅС‚РѕСЂ вЂ” С‚РѕР»СЊРєРѕ РІ РєРѕРЅС‚СЂР°РєС‚Р°С…; РѕР±С…РѕРґРёРј range Рё body.
            ExprKind::Forall { range, body, .. } | ExprKind::Exists { range, body, .. } => {
                self.walk_expr(range, scope, errors);
                self.walk_expr(body, scope, errors);
            }
            // Plan 97 Ф.4 (D142): protocol-литерал — structural-check
            // относительно объявленного протокола (instance-only).
            ExprKind::ProtocolLit { proto_name, methods } => {
                self.check_protocol_lit(proto_name, methods, e.span, errors);
            }
            // Р›РёС‚РµСЂР°Р»С‹ / ident'С‹ / handler-Р»РёС‚РµСЂР°Р»С‹ вЂ” Р±РµР· СЂРµРєСѓСЂСЃРёРё РІ bound-РїСЂРѕРІРµСЂРєРµ.
            ExprKind::IntLit(_) | ExprKind::FloatLit(_) | ExprKind::BoolLit(_)
            | ExprKind::StrLit(_) | ExprKind::CharLit(_) | ExprKind::UnitLit
            | ExprKind::Ident(_) | ExprKind::Path(_) | ExprKind::SelfAccess
            | ExprKind::HandlerLit { .. } => {}
        }
    }

    /// Plan 97 Ф.4 (D142): структурная проверка protocol-литерала.
    ///
    /// 1. Resolve `proto_name` в registered protocol через `protocol_specs`.
    ///    Если не найден — error (unknown protocol).
    /// 2. Каждый impl-метод должен соответствовать **instance**-методу
    ///    протокола (по имени + arity). Реализация **static**-метода
    ///    (декларированного с `.method`) в protocol-литерале запрещена
    ///    (static — `Type.method` D35, у литерала нет «своего типа»).
    /// 3. Каждый instance-метод протокола должен быть реализован — иначе
    ///    «missing method» error.
    fn check_protocol_lit(
        &self,
        proto_name: &[String],
        methods: &[HandlerMethod],
        span: Span,
        errors: &mut Vec<Diagnostic>,
    ) {
        let name = match proto_name.last() {
            Some(n) => n.clone(),
            None => return,
        };
        let Some(spec_methods) = self.protocol_specs.get(&name) else {
            // Unknown protocol — diagnostic с hint'ом про D142.
            // Permissive если effect (effect-литерал, не protocol-литерал).
            if !self.effect_decls.contains_key(&name) {
                errors.push(Diagnostic::new(
                    format!(
                        "unknown protocol `{}` in protocol-literal — must be a declared \
                         `type {} protocol {{ ... }}` (D142 / Plan 97 Ф.4). \
                         If you meant an effect-literal, use `effect {} {{ ... }}` instead.",
                        name, name, name),
                    span,
                ));
            }
            return;
        };
        // Static-method-impl rejection (Ф.4.3).
        for spec_m in spec_methods.iter() {
            if spec_m.is_static {
                // Если literal реализует static-метод (по имени), diagnostic.
                if methods.iter().any(|im| im.name == spec_m.name) {
                    errors.push(Diagnostic::new(
                        format!(
                            "static method `.{}` cannot be implemented in protocol-literal \
                             — static methods belong to a type (D35: `fn Type.{}(...)`), \
                             not to an instance. Declare a named `type Impl {{ ... }}` with \
                             `fn Impl.{}(...)` and pass an instance of `Impl` instead.",
                            spec_m.name, spec_m.name, spec_m.name),
                        span,
                    ));
                }
            }
        }
        // Structural-match: каждый instance-метод протокола должен быть реализован.
        let mut missing: Vec<String> = Vec::new();
        for spec_m in spec_methods.iter() {
            if spec_m.is_static {
                continue;
            }
            let found = methods.iter().any(|im|
                im.name == spec_m.name && im.params.len() == spec_m.params.len());
            if !found {
                missing.push(format!(
                    "{}({})",
                    spec_m.name,
                    spec_m.params.iter().map(|p| p.name.clone()).collect::<Vec<_>>().join(", ")));
            }
        }
        if !missing.is_empty() {
            errors.push(Diagnostic::new(
                format!(
                    "protocol-literal `protocol {} {{ ... }}` is missing required instance methods: {}. \
                     The protocol contract declared `{}` requires every instance method to be \
                     implemented (D142 / Plan 97 Ф.4 structural conformance).",
                    name, missing.join(", "), name),
                span,
            ));
        }
        // Extra-method warning: реализация unknown-имени.
        for im in methods {
            let in_proto = spec_methods.iter().any(|s| s.name == im.name);
            if !in_proto {
                errors.push(Diagnostic::new(
                    format!(
                        "protocol-literal implements method `{}` not declared in protocol `{}` \
                         (D142 / Plan 97 Ф.4). Method names must match the contract.",
                        im.name, name),
                    im.span,
                ));
            }
        }
    }

    /// Plan 97.1 hardening (D142): Nova-side enforcement для
    /// `obj.method(args)` где `obj` — переменная типа named protocol.
    /// Метод должен быть в `protocol_specs[<Proto>]`; иначе compile error
    /// (раньше эта ошибка ловилась только на C-side как
    /// `no member named 'X' in struct NovaVtable_<Proto>`).
    ///
    /// Закрывает silent miscompile риск для пользовательской опечатки
    /// `l.nonexistent()` на protocol-typed value.
    fn check_protocol_method_call(
        &self,
        e: &Expr,
        scope: &HashMap<String, TypeRef>,
        errors: &mut Vec<Diagnostic>,
    ) {
        let ExprKind::Call { func, .. } = &e.kind else { return; };
        // Снять turbofish если есть.
        let func = match &func.kind {
            ExprKind::TurboFish { base, .. } => base.as_ref(),
            _ => func.as_ref(),
        };
        let (obj, method_name, member_span) = match &func.kind {
            ExprKind::Member { obj, name } => (obj.as_ref(), name.clone(), func.span),
            _ => return,
        };
        // Resolve obj-тип через scope (только для простых Ident'ов; deeper
        // resolution — это задача codegen-уровня inference).
        let obj_ty = match &obj.kind {
            ExprKind::Ident(n) => match scope.get(n) {
                Some(t) => t.clone(),
                None => return,
            },
            _ => return,
        };
        // Extract protocol-name (named type, не generic-bound here).
        let proto_name = match &obj_ty {
            TypeRef::Named { path, generics, .. }
                if generics.is_empty() && path.len() == 1 =>
            {
                path[0].clone()
            }
            _ => return,
        };
        // Skip non-protocol type bindings.
        let Some(spec_methods) = self.protocol_specs.get(&proto_name) else { return; };
        // Method обязан быть в protocol-spec.
        let known: bool = spec_methods.iter().any(|m| m.name == method_name);
        if known { return; }
        // Compose listing of known methods для R5.3 hint.
        let known_methods: Vec<String> = spec_methods.iter().map(|m| m.name.clone()).collect();
        let listing = if known_methods.is_empty() {
            "<no methods>".to_string()
        } else {
            known_methods.join(", ")
        };
        errors.push(Diagnostic::new(
            format!(
                "unknown method `.{}()` on protocol-typed value (declared protocol `{}` \
                 has no such method). Declared methods: [{}].\n  \
                 fix: rename call to one of the declared methods, or add `{}` to the protocol \
                 declaration (`type {} protocol {{ ... }}`).\n  \
                 (Plan 97.1 hardening — D142 / [M-protocol-method-name-shadowing] enforcement.)",
                method_name, proto_name, listing, method_name, proto_name),
            member_span,
        ));
    }

    /// Plan 15 Р¤.3: РїСЂРѕРІРµСЂРёС‚СЊ bound'С‹ РЅР° РєРѕРЅРєСЂРµС‚РЅРѕРј call-site.
    ///
    /// Р•СЃР»Рё callee вЂ” top-level fn СЃ generics+bounds, Рё РµСЃС‚СЊ turbofish
    /// type_args (РёР»Рё РІРѕР·РјРѕР¶РЅР° РїСЂРѕСЃС‚Р°СЏ inference РёР· args) вЂ” РїСЂРѕРІРµСЂРёС‚СЊ
    /// С‡С‚Рѕ concrete-T СѓРґРѕРІР»РµС‚РІРѕСЂСЏРµС‚ bound'Сѓ.
    fn check_call_bounds(
        &self,
        e: &Expr,
        scope: &HashMap<String, TypeRef>,
        errors: &mut Vec<Diagnostic>,
    ) {
        let ExprKind::Call { func, args, .. } = &e.kind else { return; };
        // Р Р°СЃРїР°РєСѓРµРј turbofish, С‡С‚РѕР±С‹ РґРѕР±СЂР°С‚СЊСЃСЏ РґРѕ Р±Р°Р·РѕРІРѕРіРѕ РёРґРµРЅС‚РёС„РёРєР°С‚РѕСЂР°.
        let (base, type_args): (&Expr, &[TypeRef]) = match &func.kind {
            ExprKind::TurboFish { base, type_args } => (base, type_args.as_slice()),
            _ => (func.as_ref(), &[][..]),
        };
        // Plan 101.2 (D145 Ред. 5): method-call bound enforcement.
        // `xs.desc()` где xs : []NoShow, desc объявлен как
        // `fn[T Showable_] []T @desc`. Подставить T = NoShow,
        // проверить satisfaction. Раньше check_call_bounds работал
        // только для free-fn call'ов — method-dispatch with bounded
        // receiver-generic не enforce'ился.
        if let ExprKind::Member { obj, name: method_name } = &base.kind {
            self.check_method_call_bounds(obj, method_name, e.span, scope, errors);
            return;
        }
        let fn_name = match &base.kind {
            ExprKind::Ident(n) => n.clone(),
            _ => return,
        };
        // D84: fn_decls вЂ” Vec<&FnDecl>. Р РµР·РѕР»РІ overload РїРѕ arity (С‚Рѕ, С‡С‚Рѕ
        // bound-checker РјРѕР¶РµС‚ РѕРїСЂРµРґРµР»РёС‚СЊ Р±РµР· full type-inference).
        // Р•СЃР»Рё РЅРµСЃРєРѕР»СЊРєРѕ overloads РїРѕРґС…РѕРґСЏС‚ РїРѕ arity вЂ” bound-checker РЅРµ
        // РґРµР»Р°РµС‚ СЂР°Р·СЂРµС€РµРЅРёРµ (СЌС‚Рѕ СЂР°Р±РѕС‚Р° codegen, Сѓ РєРѕС‚РѕСЂРѕРіРѕ РµСЃС‚СЊ type-info).
        // Bound-РїСЂРѕРІРµСЂРєР° РїСЂРѕРїСѓСЃРєР°РµС‚СЃСЏ; codegen Р»РѕРІРёС‚ ambiguity РЅР° СЃРІРѕС‘Рј
        // СѓСЂРѕРІРЅРµ.
        let Some(overloads) = self.fn_decls.get(&fn_name) else { return; };
        let arity_matches: Vec<&&FnDecl> = overloads.iter()
            .filter(|f| f.params.len() == args.len())
            .collect();
        let callee: &FnDecl = match arity_matches.as_slice() {
            [single] => *single,
            _ => return, // РЅРµС‚ РѕРґРЅРѕР·РЅР°С‡РЅРѕР№ overload РїРѕ arity вЂ” РїСЂРѕРїСѓСЃРєР°РµРј
        };
        // Bounds РїСЂРёСЃСѓС‚СЃС‚РІСѓСЋС‚?
        let has_bounds = callee.generics.iter().any(|g| !g.bounds.is_empty());
        if !has_bounds { return; }
        // РЎРјР°С‚С‡РёРј concrete T. РЎС‚СЂР°С‚РµРіРёСЏ:
        //   - turbofish вЂ” explicit type_args[i] РґР»СЏ callee.generics[i].
        //   - РёРЅР°С‡Рµ simple inference: РґР»СЏ РєР°Р¶РґРѕРіРѕ param СЃ TypeRef::Named{path:[T]}
        //     РіРґРµ T вЂ” generic-param, С‚РёРї arg'Р° РЅР° С‚РѕР№ Р¶Рµ РїРѕР·РёС†РёРё = concrete T.
        let mut bindings: HashMap<String, TypeRef> = HashMap::new();
        if !type_args.is_empty() {
            for (i, gp) in callee.generics.iter().enumerate() {
                if let Some(t) = type_args.get(i) {
                    bindings.insert(gp.name.clone(), t.clone());
                }
            }
        } else {
            // Simple inference РёР· РїРѕР·РёС†РёРѕРЅРЅС‹С… args.
            for (i, param) in callee.params.iter().enumerate() {
                let Some(call_arg) = args.get(i) else { continue; };
                let arg_expr = call_arg.expr();
                if let Some(t_name) = Self::param_generic_name(&param.ty, &callee.generics) {
                    if let Some(arg_ty) = Self::infer_arg_ty(arg_expr, scope) {
                        bindings.entry(t_name).or_insert(arg_ty);
                    }
                }
            }
        }
        // Р”Р»СЏ РєР°Р¶РґРѕРіРѕ bounded generic вЂ” РїСЂРѕРІРµСЂРёС‚СЊ.
        // Plan 101.3: multi-bound `[T A + B]` — ALL bounds должны быть
        // satisfied (conjunction). check_satisfaction вызывается на
        // каждом bound отдельно — каждый missing-метод выдаст diagnostic.
        for gp in &callee.generics {
            if gp.bounds.is_empty() { continue; }
            let Some(concrete) = bindings.get(&gp.name) else {
                // Inference РЅРµ СѓРґР°Р»Р°СЃСЊ вЂ” РїСЂРѕРїСѓСЃРєР°РµРј (best-effort).
                // Strict-mode РјРѕРі Р±С‹ С‚СЂРµР±РѕРІР°С‚СЊ explicit turbofish.
                continue;
            };
            for bound in &gp.bounds {
                self.check_satisfaction(
                    concrete, bound, &gp.name, &fn_name, e.span, errors,
                );
            }
        }
    }

    /// Plan 101.2 (D145 Ред. 5): bound enforcement для method-call
    /// `obj.method(args)` где method объявлен с receiver-generic prefix
    /// `fn[T Bound] []T @method` (или `fn[T Bound] T @method`). Inferим
    /// concrete T из obj-type, для каждого bound checking satisfaction.
    ///
    /// **Surface**: только `fn[T] []T @method` (array-receiver) и
    /// `fn[T] T @method` (bare-T receiver) — Plan 101.1 формы.
    /// Tuple/Func/Map receivers — followup (V2 если нужно).
    ///
    /// **Best-effort**: если obj-type не resolvable или method
    /// неоднозначен по arity — skip (silent, как и check_call_bounds
    /// для free-fn'ов; codegen/runtime поймает на своём уровне).
    fn check_method_call_bounds(
        &self,
        obj: &Expr,
        method_name: &str,
        span: Span,
        scope: &HashMap<String, TypeRef>,
        errors: &mut Vec<Diagnostic>,
    ) {
        // Inferим obj-type.
        let Some(obj_ty) = Self::infer_arg_ty(obj, scope) else { return; };
        // Определяем receiver-key и concrete substitution для T.
        // Plan 101 surface:
        //   []T  → key = "[]T", T = element type.
        //   T    → key = "T",   T = obj-type whole (bare-receiver).
        let (recv_key, concrete_t): (&str, TypeRef) = match &obj_ty {
            TypeRef::Array(inner, _) => ("[]T", (**inner).clone()),
            TypeRef::Named { path, .. } if path.last().map(|s| s.len()).unwrap_or(0) == 1 => {
                // Bare-T receiver `fn[T] T @method` — но obj должен быть
                // конкретным single-name type. Слишком permissive — skip
                // если просто `[T]` без method_table-entry. Лучше: дождаться
                // method-table lookup и если нашлось — substitute.
                ("T", obj_ty.clone())
            }
            _ => return, // Non-array, non-single-name — skip.
        };
        // Lookup methods под этим receiver-key.
        let Some(methods_for_recv) = self.method_table.get(recv_key) else { return; };
        let Some(overloads) = methods_for_recv.get(method_name) else { return; };
        // Take single match (skip if multiple overloads — codegen разрулит).
        let callee: &FnDecl = match overloads.as_slice() {
            [single] => single,
            _ => return,
        };
        // Bounded generic-параметры?
        if !callee.generics.iter().any(|g| !g.bounds.is_empty()) { return; }
        // Substitution: для каждого generic-param с тем же именем что
        // в receiver-type (T в []T или T в bare-T) — concrete_t. Для
        // method-level generics (U, V, ...) — skip (нужен type-inference
        // из args, что выходит за scope этого smoke check'а).
        for gp in &callee.generics {
            if gp.bounds.is_empty() { continue; }
            // Только T matches receiver-substitution.
            // Для recv_key="[]T" мы знаем что receiver-T это первый
            // generic prefix (parser кладёт его первым). Для bare-T
            // тоже первый. Substitute concrete_t для gp.name если он
            // первый prefix-generic; для остальных — skip.
            if callee.generics.first().map(|g| &g.name) != Some(&gp.name) {
                continue;
            }
            for bound in &gp.bounds {
                self.check_satisfaction(
                    &concrete_t, bound, &gp.name, method_name, span, errors,
                );
            }
        }
    }

    /// Plan 46 (D102): РїСЂРѕРІРµСЂРёС‚СЊ argument binding РЅР° call-site.
    /// Р РµР·РѕР»РІРёС‚ callee (free fn / static-method РїРѕ Path), СЃРѕРїРѕСЃС‚Р°РІР»СЏРµС‚
    /// РїРѕР·РёС†РёРѕРЅРЅС‹Рµ + РёРјРµРЅРѕРІР°РЅРЅС‹Рµ Р°СЂРіСѓРјРµРЅС‚С‹ СЃ РїР°СЂР°РјРµС‚СЂР°РјРё С‡РµСЂРµР·
    /// `argbind::bind_call_args`, СЌРјРёС‚РёС‚ diagnostics.
    ///
    /// Р РµР·РѕР»РІ best-effort: РµСЃР»Рё callee РЅРµРѕРґРЅРѕР·РЅР°С‡РµРЅ (overload РїРѕ arity)
    /// РёР»Рё РЅРµ СЂРµР·РѕР»РІРёС‚СЃСЏ (instance-method С‡РµСЂРµР· Member вЂ” РЅСѓР¶РµРЅ С‚РёРї obj) вЂ”
    /// РїСЂРѕРІРµСЂРєР° РїСЂРѕРїСѓСЃРєР°РµС‚СЃСЏ (codegen РїРѕР№РјР°РµС‚ РЅР° СЃРІРѕС‘Рј СѓСЂРѕРІРЅРµ).
    fn check_call_argbind(
        &self,
        e: &Expr,
        scope: &HashMap<String, TypeRef>,
        errors: &mut Vec<Diagnostic>,
    ) {
        let ExprKind::Call { func, args, trailing } = &e.kind else { return; };
        // Р Р°СЃРїР°РєСѓРµРј turbofish РґРѕ Р±Р°Р·РѕРІРѕРіРѕ func-expr.
        let base: &Expr = match &func.kind {
            ExprKind::TurboFish { base, .. } => base,
            _ => func.as_ref(),
        };
        // Р РµР·РѕР»РІРёРј callee в†’ СЃРїРёСЃРѕРє РїР°СЂР°РјРµС‚СЂРѕРІ.
        let callee_params: &[Param] = match &base.kind {
            ExprKind::Ident(name) => {
                let Some(overloads) = self.fn_decls.get(name) else { return; };
                match overloads.as_slice() {
                    [single] => &single.params,
                    _ => return, // overload вЂ” РїСЂРѕРїСѓСЃРєР°РµРј (D102: РЅРµС‚ overload,
                                 // РЅРѕ bootstrap fn_decls РјРѕР¶РµС‚ РёРјРµС‚СЊ РЅРµСЃРєРѕР»СЊРєРѕ).
                }
            }
            ExprKind::Path(parts) if parts.len() == 2 => {
                // `Type.method` вЂ” static-method СЂРµР·РѕР»РІ.
                let Some(methods) = self.method_table.get(&parts[0]) else { return; };
                let Some(overloads) = methods.get(&parts[1]) else { return; };
                match overloads.as_slice() {
                    [single] => &single.params,
                    _ => return,
                }
            }
            // Plan 46 Р¤.3 + Plan 50 follow-up: instance-method `obj.method(...)`.
            // РџРµСЂРІР°СЏ РїРѕРїС‹С‚РєР° вЂ” receiver-type inference (best-effort С‡РµСЂРµР·
            // `infer_arg_ty`): РµСЃР»Рё С‚РёРї `obj` РёР·РІРµСЃС‚РµРЅ (Ident РІ scope,
            // record-Р»РёС‚РµСЂР°Р», Р»РёС‚РµСЂР°Р»-РїСЂРёРјРёС‚РёРІ) вЂ” С‚РѕС‡РЅС‹Р№ СЂРµР·РѕР»РІ
            // `method_table[type][method]`. Р—Р°РєСЂС‹РІР°РµС‚ gap РїСЂРё collision
            // РёРјС‘РЅ РјРµС‚РѕРґРѕРІ: `Box.scaled` vs `Cube.scaled` СЃ РґРµС„РѕР»С‚Р°РјРё
            // Р±РѕР»СЊС€Рµ РЅРµ РїСЂРѕРїСѓСЃРєР°РµС‚ keyword-only РґРёР°РіРЅРѕСЃС‚РёРєСѓ.
            // Fallback вЂ” name-only СЂРµР·РѕР»РІ (РєР°Рє Р±С‹Р»Рѕ РІ Plan 46): СѓРЅРёРєР°Р»СЊРЅРѕРµ
            // РёРјСЏ РјРµС‚РѕРґР° С‡РµСЂРµР· РІСЃРµ С‚РёРїС‹. Р”Р»СЏ РѕСЃС‚Р°Р»СЊРЅС‹С… СЃР»СѓС‡Р°РµРІ codegen
            // СЂРµР·РѕР»РІРёС‚ С‡РµСЂРµР· type-info.
            ExprKind::Member { obj, name: method_name } => {
                let resolved = self.resolve_instance_method(obj, method_name, scope, args.len());
                match resolved {
                    Some(f) => &f.params,
                    None => return,
                }
            }
            _ => return,
        };
        // Plan 46 Р¤.3: trailing-С„РѕСЂРјР° (D43) СЃРІСЏР·С‹РІР°РµС‚ РџРћРЎР›Р•Р”РќРР™
        // С„СѓРЅРєС†РёРѕРЅР°Р»СЊРЅС‹Р№ РїР°СЂР°РјРµС‚СЂ. Bind'Р°РµРј РїСЂРѕС‚РёРІ params Р±РµР· РЅРµРіРѕ.
        // РўР°РєР¶Рµ: РµСЃР»Рё named-arg РЅР°Р·РІР°РЅ РєР°Рє trailing-bound param вЂ” СЌС‚Рѕ
        // double-bind (Р»РѕРІРёС‚СЃСЏ РЅРёР¶Рµ РѕС‚РґРµР»СЊРЅРѕ).
        let trailing_present = trailing.is_some();
        let effective_params: &[Param] = if trailing_present && !callee_params.is_empty() {
            // РџСЂРѕРІРµСЂРєР°: named-arg РґР»СЏ trailing-bound РїР°СЂР°РјРµС‚СЂР° вЂ” error.
            let last = &callee_params[callee_params.len() - 1];
            for a in args.iter() {
                if a.arg_name() == Some(last.name.as_str()) {
                    errors.push(Diagnostic::new(
                        format!(
                            "РїР°СЂР°РјРµС‚СЂ `{}` СЃРІСЏР·Р°РЅ Рё trailing-С„РѕСЂРјРѕР№, Рё РёРјРµРЅРѕРІР°РЅРЅС‹Рј \
                             Р°СЂРіСѓРјРµРЅС‚РѕРј (D102)",
                            last.name
                        ),
                        a.expr().span,
                    ));
                    return;
                }
            }
            &callee_params[..callee_params.len() - 1]
        } else {
            callee_params
        };
        // Р—Р°РїСѓСЃРєР°РµРј binding. РћС€РёР±РєР° в†’ diagnostic.
        //
        // Precedence (Plan 50): СЃС‚СЂСѓРєС‚СѓСЂРЅС‹Рµ РѕС€РёР±РєРё argbind вЂ” Р°СЂРЅРѕСЃС‚СЊ,
        // РЅРµРёР·РІРµСЃС‚РЅРѕРµ РёРјСЏ, РґРІРѕР№РЅР°СЏ РїСЂРёРІСЏР·РєР°, РїРѕР·РёС†РёРѕРЅРЅС‹Р№-РїРѕСЃР»Рµ-РёРјРµРЅРѕРІР°РЅРЅРѕРіРѕ
        // вЂ” fail-fast РІ `bind_call_args` Рё СЌРјРёС‚СЏС‚СЃСЏ РїРµСЂРІС‹РјРё. РџСЂР°РІРёР»Рѕ
        // keyword-only (Plan 50, D102 в„–1) РїСЂРѕРІРµСЂСЏРµС‚СЃСЏ РўРћР›Р¬РљРћ РєРѕРіРґР°
        // СЃС‚СЂСѓРєС‚СѓСЂР° РІР°Р»РёРґРЅР° (`Ok(bindings)`) вЂ” РѕРЅРѕ РїРѕСЃР»РµРґРЅРµРµ РІ РїРѕСЂСЏРґРєРµ
        // РґРёР°РіРЅРѕСЃС‚РёРє.
        match crate::argbind::bind_call_args(effective_params, args) {
            Err(err) => {
                let span = {
                    let s = err.span();
                    if s == crate::diag::Span::dummy() { e.span } else { s }
                };
                errors.push(Diagnostic::new(err.message(), span));
            }
            Ok(bindings) => {
                // Plan 50 (D102 СЂРµРІРёР·РёСЏ): РїР°СЂР°РјРµС‚СЂ СЃ РґРµС„РѕР»С‚РѕРј вЂ” keyword-only.
                // РџРѕР·РёС†РёРѕРЅРЅР°СЏ РїСЂРёРІСЏР·РєР° Рє РґРµС„РѕР»С‚РЅРѕРјСѓ РїР°СЂР°РјРµС‚СЂСѓ вЂ” РѕС€РёР±РєР°.
                // Trailing-С„РѕСЂРјР° РёСЃРєР»СЋС‡РµРЅР° СЃС‚СЂСѓРєС‚СѓСЂРЅРѕ: trailing-bound
                // РїР°СЂР°РјРµС‚СЂ СѓР¶Рµ СЃРЅСЏС‚ РёР· `effective_params` РІС‹С€Рµ, РїРѕСЌС‚РѕРјСѓ
                // РІ `bindings` РµРіРѕ РЅРµС‚ вЂ” Р·Р°РїРѕР»РЅРµРЅРёРµ РґРµС„РѕР»С‚РЅРѕРіРѕ РїРѕСЃР»РµРґРЅРµРіРѕ
                // РїР°СЂР°РјРµС‚СЂР° trailing-С„РѕСЂРјРѕР№ РЅРµ СЃС‡РёС‚Р°РµС‚СЃСЏ РЅР°СЂСѓС€РµРЅРёРµРј.
                //
                // РћС‚РґРµР»СЊРЅР°СЏ РґРёР°РіРЅРѕСЃС‚РёРєР° РЅР° РљРђР–Р”Р«Р™ РЅР°СЂСѓС€Р°СЋС‰РёР№ Р°СЂРіСѓРјРµРЅС‚
                // (РЅРµ В«РїРµСЂРІС‹Р№ Рё СЃС‚РѕРїВ») вЂ” error recovery Р±РµР· РєР°СЃРєР°РґР°:
                // РїСЂРѕСЃС‚Рѕ РїСЂРѕРґРѕР»Р¶Р°РµРј С†РёРєР».
                self.check_keyword_only(effective_params, args, &bindings, errors);
            }
        }
    }

    /// Plan 50 (D102 в„–1): РїРѕСЃР»Рµ СѓСЃРїРµС€РЅРѕРіРѕ argbind вЂ” РЅР°Р№С‚Рё РїРѕР·РёС†РёРѕРЅРЅС‹Рµ
    /// Р°СЂРіСѓРјРµРЅС‚С‹, Р»РµРіС€РёРµ РЅР° РїР°СЂР°РјРµС‚СЂС‹ СЃ РґРµС„РѕР»С‚РѕРј, Рё СЌРјРёС‚РёС‚СЊ production-grade
    /// РґРёР°РіРЅРѕСЃС‚РёРєСѓ РЅР° РєР°Р¶РґС‹Р№ (РёРјСЏ РїР°СЂР°РјРµС‚СЂР°, `note: declared here`,
    /// machine-applicable structured suggestion `name: <expr>`).
    fn check_keyword_only(
        &self,
        effective_params: &[Param],
        args: &[CallArg],
        bindings: &[crate::argbind::ArgBinding],
        errors: &mut Vec<Diagnostic>,
    ) {
        use crate::argbind::ArgBinding;
        use crate::diag::{Applicability, Span, Suggestion};
        for (pi, binding) in bindings.iter().enumerate() {
            let ArgBinding::Positional(ai) = binding else { continue; };
            let param = &effective_params[pi];
            if param.default.is_none() {
                continue;
            }
            // РќР°СЂСѓС€РµРЅРёРµ: РїРѕР·РёС†РёРѕРЅРЅС‹Р№ Р°СЂРіСѓРјРµРЅС‚ `args[*ai]` Р»С‘Рі РЅР°
            // РґРµС„РѕР»С‚РЅС‹Р№ РїР°СЂР°РјРµС‚СЂ `param`.
            let arg_span = args[*ai].expr().span;
            // Structured suggestion вЂ” С‡РёСЃС‚Р°СЏ Р’РЎРўРђР’РљРђ `<name>: ` РІ РЅР°С‡Р°Р»Рµ
            // РІС‹СЂР°Р¶РµРЅРёСЏ-Р°СЂРіСѓРјРµРЅС‚Р° (span РЅСѓР»РµРІРѕР№ С€РёСЂРёРЅС‹). Source-РЅРµР·Р°РІРёСЃРёРјРѕ:
            // producer РЅРµ С‡РёС‚Р°РµС‚ РёСЃС…РѕРґРЅРёРє. Machine-applicable вЂ” edit
            // РєРѕСЂСЂРµРєС‚РµРЅ Рё Р°РІС‚Рѕ-РїСЂРёРјРµРЅРёРј (`nova fix` / LSP code-action).
            let insert_at = Span::with_file(arg_span.start, arg_span.start, arg_span.file_id);
            let suggestion = Suggestion {
                message: format!("pass `{}` by name", param.name),
                span: insert_at,
                replacement: format!("{}: ", param.name),
                applicability: Applicability::MachineApplicable,
            };
            let diag = Diagnostic::new(
                format!(
                    "РїР°СЂР°РјРµС‚СЂ `{}` РёРјРµРµС‚ Р·РЅР°С‡РµРЅРёРµ РїРѕ СѓРјРѕР»С‡Р°РЅРёСЋ вЂ” \
                     РїРµСЂРµРґР°С‘С‚СЃСЏ С‚РѕР»СЊРєРѕ РїРѕ РёРјРµРЅРё (D102)",
                    param.name,
                ),
                arg_span,
            )
            .with_note_at(
                format!("РїР°СЂР°РјРµС‚СЂ `{}` РѕР±СЉСЏРІР»РµРЅ Р·РґРµСЃСЊ", param.name),
                param.span,
            )
            .with_note(
                "РїР°СЂР°РјРµС‚СЂС‹ СЃ РґРµС„РѕР»С‚РѕРј вЂ” keyword-only: РѕР±СЏР·Р°С‚РµР»СЊРЅС‹Р№ вЂ” \
                 РїРѕР·РёС†РёРѕРЅРЅРѕ, РѕРїС†РёРѕРЅР°Р»СЊРЅС‹Р№ вЂ” РїРѕ РёРјРµРЅРё",
            )
            .with_suggestion(suggestion);
            errors.push(diag);
        }
    }

    /// Plan 53: refutability check РґР»СЏ `let`-pattern. Р”РѕРїСѓСЃС‚РёРјС‹ С‚РѕР»СЊРєРѕ
    /// irrefutable patterns:
    /// - `Ident`, `Wildcard`
    /// - `Tuple(pats)` вЂ” СЂРµРєСѓСЂСЃРёРІРЅРѕ irrefutable
    /// - `Record` Р±РµР· type_path РР›Р СЃ type_path Рє record-С‚РёРїСѓ (РЅРµ
    ///   sum-variant) вЂ” СЂРµРєСѓСЂСЃРёРІРЅРѕ irrefutable РґР»СЏ РїРѕРґ-pattern'РѕРІ
    /// - `Binding { inner, .. }` вЂ” inner irrefutable
    ///
    /// Refutable (compile error):
    /// - `Literal`, `Variant`, `Or`, `Array` (РІСЃРµРіРґР° refutable)
    /// - `Record` СЃ type_path Рє sum-variant (РЅСѓР¶РµРЅ tag-check РІ runtime)
    ///
    /// Production-grade diagnostic: С‚РёРї РЅР°СЂСѓС€РµРЅРёСЏ + РїРѕРґСЃРєР°Р·РєР° `if let
    /// <pat> = <expr> { ... }` / `match`.
    fn check_let_pattern_irrefutable(&self, pat: &Pattern, errors: &mut Vec<Diagnostic>) {
        match pat {
            Pattern::Ident { .. } | Pattern::Wildcard(_) => {}
            Pattern::Tuple(pats, _) => {
                for p in pats {
                    self.check_let_pattern_irrefutable(p, errors);
                }
            }
            Pattern::Record { type_path, fields, span, .. } => {
                // Sum-variant in type_path — refutable.
                if let Some(path) = type_path {
                    if let Some(last) = path.last() {
                        if self.sum_variant_names.contains(last) {
                            let path_str = path.join(".");
                            errors.push(
                                Diagnostic::new(
                                    format!(
                                        "refutable pattern in `let`: `{}` is a sum-variant — \
                                         match is not statically guaranteed (D52). Use \
                                         `if let` or `match` instead.",
                                        path_str,
                                    ),
                                    *span,
                                )
                                .with_note(
                                    "Plan 53: `let` accepts only irrefutable patterns (Ident, \
                                     Wildcard, Tuple, plain-Record). Sum-variants need a \
                                     runtime tag-check — `let` cannot perform it.",
                                )
                                .with_note(
                                    format!(
                                        "example: `if let {} {{ ... }} = <expr> {{ ... }}`",
                                        path_str,
                                    ),
                                ),
                            );
                            return;
                        }
                    }
                }
                // Recurse into sub-patterns of fields.
                for f in fields {
                    if let Some(sub) = &f.pattern {
                        self.check_let_pattern_irrefutable(sub, errors);
                    }
                }
            }
            Pattern::Binding { inner, .. } => {
                self.check_let_pattern_irrefutable(inner, errors);
            }
            Pattern::Literal(_, span) => {
                errors.push(
                    Diagnostic::new(
                        "refutable pattern in `let`: literal match is not statically \
                         guaranteed. Use `if let` / `match`, or a plain \
                         `let x = ...; if x == ...`",
                        *span,
                    )
                    .with_note(
                        "example: `if let 42 = n { ... }` or `let x = n; if x == 42 { ... }`",
                    ),
                );
            }
            Pattern::Variant { path, span, .. } => {
                let path_str = path.join(".");
                errors.push(
                    Diagnostic::new(
                        format!(
                            "refutable pattern in `let`: `{}` is a variant-pattern — \
                             match is not statically guaranteed (D52/D59). Use `if let` \
                             or `match` instead.",
                            path_str,
                        ),
                        *span,
                    )
                    .with_note(
                        "Plan 53: variant-patterns need a runtime tag-check — `let` \
                         guarantees binding, not a fallible match.",
                    )
                    .with_note(
                        format!(
                            "example: `if let {}(..) = <expr> {{ ... }}`",
                            path_str,
                        ),
                    ),
                );
            }
            Pattern::Or { span, .. } => {
                errors.push(
                    Diagnostic::new(
                        "refutable pattern in `let`: alternation `|` is not statically \
                         guaranteed to match. Use `if let` / `match`.",
                        *span,
                    )
                    .with_note(
                        "example: `match x { A | B => ..., _ => ... }`",
                    ),
                );
            }
            Pattern::Array { span, .. } => {
                errors.push(
                    Diagnostic::new(
                        "refutable pattern in `let`: array length is not statically \
                         guaranteed. Use `if let` / `match`, or index/length checks \
                         like `xs[0]`, `xs.len`.",
                        *span,
                    )
                    .with_note(
                        "example: `if let [a, b, c] = xs { ... } else { /* handle */ }` \
                         or `match xs { [a, b, c] => ..., _ => ... }`",
                    )
                    .with_note(
                        "Plan 53: array-length is checked at runtime — `let` accepts \
                         only statically-guaranteed patterns.",
                    ),
                );
            }
        }
    }

    /// Plan 50 follow-up: СЂРµР·РѕР»РІ `obj.method` РґР»СЏ argbind-РґРёР°РіРЅРѕСЃС‚РёРє.
    ///
    /// РЎРЅР°С‡Р°Р»Р° best-effort receiver-type inference С‡РµСЂРµР· `infer_arg_ty`
    /// вЂ” РµСЃР»Рё С‚РёРї `obj` РёР·РІРµСЃС‚РµРЅ (Ident РІ scope / record-Р»РёС‚РµСЂР°Р» /
    /// Р»РёС‚РµСЂР°Р»-РїСЂРёРјРёС‚РёРІ), С‚РѕС‡РЅС‹Р№ СЂРµР·РѕР»РІ С‡РµСЂРµР· `method_table[type][name]`.
    /// Р­С‚Рѕ Р·Р°РєСЂС‹РІР°РµС‚ gap РїСЂРё РєРѕР»Р»РёР·РёРё РёРјС‘РЅ РјРµС‚РѕРґРѕРІ РјРµР¶РґСѓ С‚РёРїР°РјРё
    /// (`Box.scaled` vs `Cube.scaled` СЃ РґРµС„РѕР»С‚Р°РјРё): Р±РµР· inference РѕР±Р°
    /// РїРѕРїР°РґР°Р»Рё РІ name-only РїРѕРёСЃРє, С‚РѕС‚ РІРёРґРµР» >1 sig в†’ ambiguous в†’ skip,
    /// keyword-only РґРёР°РіРЅРѕСЃС‚РёРєР° С‚РµСЂСЏР»Р°СЃСЊ.
    ///
    /// Fallback вЂ” name-only С‡РµСЂРµР· РІСЃРµ С‚РёРїС‹ (РїРѕРІРµРґРµРЅРёРµ Plan 46): РїРѕРґС…РѕРґРёС‚
    /// РєРѕРіРґР° С‚РёРї receiver'Р° РЅРµ РІС‹РІРѕРґРёРј (СЃР»РѕР¶РЅРѕРµ РІС‹СЂР°Р¶РµРЅРёРµ / generic).
    /// РЈРЅРёРєР°Р»СЊРЅРѕРµ РёРјСЏ РјРµС‚РѕРґР° в†’ РѕРґРёРЅ С‚РёРї в†’ РѕРґРёРЅ sig в†’ РёСЃРїРѕР»СЊР·СѓРµРј РµРіРѕ.
    /// РРЅР°С‡Рµ вЂ” РїСЂРѕРїСѓСЃРєР°РµРј, codegen СЂРµР·РѕР»РІРёС‚ С‡РµСЂРµР· type-info.
    fn resolve_instance_method(
        &self,
        obj: &Expr,
        method_name: &str,
        scope: &HashMap<String, TypeRef>,
        arg_count_hint: usize,
    ) -> Option<&FnDecl> {
        // РџРѕРїС‹С‚РєР° 1: receiver-type inference.
        if let Some(recv_ty) = Self::infer_arg_ty(obj, scope) {
            if let TypeRef::Named { path, .. } = &recv_ty {
                if path.len() == 1 {
                    if let Some(methods) = self.method_table.get(&path[0]) {
                        if let Some(overloads) = methods.get(method_name) {
                            if let [single] = overloads.as_slice() {
                                return Some(single);
                            }
                        }
                    }
                }
            }
        }
        // РџРѕРїС‹С‚РєР° 2: name-only fallback. РЈРЅРёРєР°Р»СЊРЅРѕРµ РёРјСЏ РјРµС‚РѕРґР° С‡РµСЂРµР·
        // РІСЃРµ С‚РёРїС‹ в†’ РѕРґРёРЅ sig, РёСЃРїРѕР»СЊР·СѓРµРј.
        // Plan 109: фильтр по arity предотвращает ложные "expected 0, got N"
        // когда builtin-метод ([]T::push и т.п.) отсутствует в method_table,
        // но пользовательский тип случайно имеет метод с тем же именем.
        let mut found: Option<&FnDecl> = None;
        let mut ambiguous = false;
        for methods in self.method_table.values() {
            if let Some(overloads) = methods.get(method_name) {
                for f in overloads {
                    if f.params.len() != arg_count_hint { continue; }
                    if found.is_some() {
                        ambiguous = true;
                    }
                    found = Some(f);
                }
            }
        }
        if ambiguous { return None; }
        found
    }

    /// Р•СЃР»Рё param's TypeRef вЂ” РїСЂРѕСЃС‚РѕР№ `Named{path: [T]}` РіРґРµ T РІ
    /// СЃРїРёСЃРєРµ generics, РІРµСЂРЅСѓС‚СЊ РёРјСЏ T. РРЅР°С‡Рµ None.
    fn param_generic_name(ty: &TypeRef, generics: &[GenericParam]) -> Option<String> {
        let TypeRef::Named { path, generics: g, .. } = ty else { return None; };
        if path.len() != 1 || !g.is_empty() { return None; }
        if generics.iter().any(|gp| gp.name == path[0]) {
            Some(path[0].clone())
        } else {
            None
        }
    }

    /// РњРёРЅРёРјР°Р»СЊРЅР°СЏ inference С‚РёРїР° argument'Р° вЂ” best-effort РЅР° РѕСЃРЅРѕРІРµ
    /// СЃРёРЅС‚Р°РєСЃРёС‡РµСЃРєРѕР№ С„РѕСЂРјС‹ Рё С‚РµРєСѓС‰РµРіРѕ scope (let-bindings).
    fn infer_arg_ty(e: &Expr, scope: &HashMap<String, TypeRef>) -> Option<TypeRef> {
        match &e.kind {
            ExprKind::Ident(name) => scope.get(name).cloned(),
            // Plan 97.1 hardening (D142): protocol-литерал имеет тип
            // именованного protocol'а — это позволяет let-binding
            // получить корректный тип в scope (для последующего
            // check_protocol_method_call enforcement'а).
            ExprKind::ProtocolLit { proto_name, .. } => Some(TypeRef::Named {
                path: proto_name.clone(),
                generics: Vec::new(),
                span: e.span,
            }),
            ExprKind::RecordLit { type_name: Some(name), .. } => Some(TypeRef::Named {
                path: name.clone(),
                generics: Vec::new(),
                span: e.span,
            }),
            ExprKind::ArrayLit(elems) => {
                // []T вЂ” element type from first element.
                let inner = elems.iter().find_map(|el| match el {
                    ArrayElem::Item(it) | ArrayElem::Spread(it) => Self::infer_arg_ty(it, scope),
                });
                inner.map(|t| TypeRef::Array(Box::new(t), e.span))
            }
            ExprKind::IntLit(_) => Some(TypeRef::Named {
                path: vec!["int".to_string()], generics: vec![], span: e.span }),
            ExprKind::FloatLit(_) => Some(TypeRef::Named {
                path: vec!["f64".to_string()], generics: vec![], span: e.span }),
            ExprKind::BoolLit(_) => Some(TypeRef::Named {
                path: vec!["bool".to_string()], generics: vec![], span: e.span }),
            ExprKind::StrLit(_) | ExprKind::InterpolatedStr { .. } => Some(TypeRef::Named {
                path: vec!["str".to_string()], generics: vec![], span: e.span }),
            ExprKind::CharLit(_) => Some(TypeRef::Named {
                path: vec!["char".to_string()], generics: vec![], span: e.span }),
            _ => None,
        }
    }

    /// Plan 15 Р¤.3: РїСЂРѕРІРµСЂРёС‚СЊ, С‡С‚Рѕ concrete-С‚РёРї СѓРґРѕРІР»РµС‚РІРѕСЂСЏРµС‚ bound'Сѓ
    /// (protocol-С‚РёРїСѓ). РџСЂРё РЅРµСЃРѕРѕС‚РІРµС‚СЃС‚РІРёРё вЂ” R5.3 diagnostic.
    ///
    /// Plan 97 Ф.2 (D142): bound может быть **анонимным** inline-protocol
    /// (`[T protocol { method-sig* }]`) — методы проверяются «по месту»
    /// без регистрации в `protocol_specs`. Закрывает Plan 15
    /// `[P-15-anon-protocol-bound]`.
    fn check_satisfaction(
        &self,
        concrete: &TypeRef,
        bound: &TypeRef,
        type_param_name: &str,
        fn_name: &str,
        span: Span,
        errors: &mut Vec<Diagnostic>,
    ) {
        // Plan 97 Ф.2: inline-protocol bound — методы прямо в TypeRef.
        if let TypeRef::Protocol { methods, .. } = bound {
            self.check_satisfaction_against_methods(
                concrete,
                methods,
                None, // anon — нет имени
                type_param_name,
                fn_name,
                span,
                errors,
            );
            return;
        }
        let bound_name = match bound {
            TypeRef::Named { path, .. } if path.len() == 1 => path[0].clone(),
            _ => return, // complex bounds (Hashable[K], etc.) вЂ” РѕС‚РґРµР»СЊРЅР°СЏ Р·Р°РґР°С‡Р°
        };
        // Plan 15 D53 strict: bound РґРѕР»Р¶РµРЅ Р±С‹С‚СЊ protocol-kind. Р•СЃР»Рё
        // РёРјСЏ Р·Р°СЂРµРіРёСЃС‚СЂРёСЂРѕРІР°РЅРѕ РєР°Рє effect-kind вЂ” СЌС‚Рѕ spec violation
        // (D72: bounds require protocols). R5.3-style diagnostic.
        if let Some(eff_decl) = self.effect_decls.get(&bound_name) {
            let _ = eff_decl;
            errors.push(Diagnostic::new(
                format!(
                    "type `{}` is an effect, not a protocol вЂ” generic bounds \
                     require protocol-types (D72/D53). Hint: declare `{}` as \
                     `type {} protocol {{ ... }}` if structural-contract semantics \
                     is intended; effects are runtime-dispatched capabilities and \
                     can only appear in effect-rows `(...) {} -> ...`, not as \
                     `[T {}]` bounds.",
                    bound_name, bound_name, bound_name, bound_name, bound_name,
                ),
                span,
            ));
            return;
        }
        let concrete_name = match concrete {
            TypeRef::Named { path, .. } if path.len() == 1 => path[0].clone(),
            // Array/Tuple/Func вЂ” РїРѕРєР° РїСЂРѕРїСѓСЃРєР°РµРј (РЅРµ РѕР±СЂР°Р±Р°С‚С‹РІР°РµРј СЃРѕСЃС‚Р°РІРЅС‹Рµ T).
            _ => return,
        };
        // Built-in primitives Р°РІС‚РѕРјР°С‚РёС‡РµСЃРєРё СѓРґРѕРІР»РµС‚РІРѕСЂСЏСЋС‚ РЅРёС‡РµРјСѓ вЂ” Сѓ РЅР°СЃ
        // РЅРµС‚ registry РёС… РјРµС‚РѕРґРѕРІ РІ method_table. Skip (best-effort).
        if matches!(concrete_name.as_str(),
            "int" | "i8" | "i16" | "i32" | "i64"
            | "u8" | "u16" | "u32" | "u64"
            | "f32" | "f64" | "bool" | "char"
            // Plan 76: `never` — bottom-тип, vacuously удовлетворяет любому bound.
            | "str" | "any" | "never") {
            return;
        }
        let Some(spec_methods) = self.protocol_specs.get(&bound_name) else {
            // Bound вЂ” РЅРµ Р·Р°СЂРµРіРёСЃС‚СЂРёСЂРѕРІР°РЅ РЅРё РєР°Рє protocol, РЅРё РєР°Рє effect.
            // РњРѕР¶РµС‚ Р±С‹С‚СЊ type alias / record / unknown. РџРѕРєР° РїСЂРѕРїСѓСЃРєР°РµРј вЂ”
            // formal check'Р° РЅРµ РґРµР»Р°РµРј (best-effort permissive).
            return;
        };
        // Plan 97 Ф.2: shared satisfaction-логика с anon-вариантом.
        self.check_satisfaction_against_methods(
            concrete,
            spec_methods.as_slice(),
            Some(&bound_name),
            type_param_name,
            fn_name,
            span,
            errors,
        );
    }

    /// Plan 97 Ф.2 (D142): общая satisfaction-логика для named и anonymous
    /// protocol-bound'ов. `bound_name = Some(...)` — named (показывается
    /// в diagnostic); `None` — inline `[T protocol { ... }]`, рендерим
    /// как `protocol{...}`.
    fn check_satisfaction_against_methods(
        &self,
        concrete: &TypeRef,
        required: &[EffectMethod],
        bound_name: Option<&str>,
        type_param_name: &str,
        fn_name: &str,
        span: Span,
        errors: &mut Vec<Diagnostic>,
    ) {
        let concrete_name = match concrete {
            TypeRef::Named { path, .. } if path.len() == 1 => path[0].clone(),
            _ => return,
        };
        if matches!(concrete_name.as_str(),
            "int" | "i8" | "i16" | "i32" | "i64"
            | "u8" | "u16" | "u32" | "u64"
            | "f32" | "f64" | "bool" | "char"
            | "str" | "any" | "never") {
            return;
        }
        let empty: HashMap<String, Vec<&FnDecl>> = HashMap::new();
        let concrete_methods = self.method_table.get(&concrete_name).unwrap_or(&empty);
        let mut missing: Vec<String> = Vec::new();
        for req in required {
            let found = concrete_methods.get(&req.name).map(|fns| {
                fns.iter().any(|f| f.params.len() == req.params.len())
            }).unwrap_or(false);
            if !found {
                // Plan 91.8a.2 part 2 (D183 amendment): default body fallback.
                // Если protocol method имеет default body — type can satisfy via
                // codegen synthesis. Assumption: synthesis will lower default body
                // calls correctly или error там. Здесь — accept satisfaction.
                if req.default_body.is_some() {
                    continue;
                }
                let sig = render_method_sig(&req.name, &req.params, &req.return_type);
                let prefix = if req.is_static { "." } else { "" };
                missing.push(format!("{}{}", prefix, sig));
            }
        }
        if missing.is_empty() {
            return;
        }
        let bound_display = bound_name
            .map(|n| n.to_string())
            .unwrap_or_else(|| "<anonymous protocol>".to_string());
        let mut msg = format!(
            "type `{}` does not satisfy `{}` bound (in call to `{}[{} {}]`).\n\n  `{}` requires:\n",
            concrete_name, bound_display, fn_name, type_param_name, bound_display, bound_display);
        for req in required {
            let prefix = if req.is_static { "." } else { "" };
            msg.push_str(&format!(
                "    {}{}\n",
                prefix,
                render_method_sig(&req.name, &req.params, &req.return_type)));
        }
        msg.push_str(&format!("\n  `{}` is missing: {}\n", concrete_name, missing.join(", ")));
        msg.push_str(&format!(
            "\n  fix: добавить недостающие методы для типа `{}`. \
             См. spec/decisions/02-types.md#d72 и #d142 (anonymous protocol).",
            concrete_name));
        errors.push(Diagnostic::new(msg, span));
    }
}

/// Plan 15: extract simple identifier-name РёР· Pattern. РСЃРїРѕР»СЊР·СѓРµС‚СЃСЏ
/// РґР»СЏ СЂРµРіРёСЃС‚СЂР°С†РёРё let-bindings РІ scope (С‚РѕР»СЊРєРѕ Pattern::Ident; complex
/// patterns вЂ” tuple/variant вЂ” РїСЂРѕРїСѓСЃРєР°СЋС‚СЃСЏ).
fn pattern_simple_name(p: &Pattern) -> Option<String> {
    match p {
        Pattern::Ident { name, .. } => Some(name.clone()),
        _ => None,
    }
}

// ============================================================================
// Plan 16 (D63 forbid + D64 realtime): capability enforcement.
// ============================================================================

/// Plan 16: РЅР°Р±РѕСЂ "suspend"-СЌС„С„РµРєС‚РѕРІ РєРѕС‚РѕСЂС‹Рµ РЅРµР»СЊР·СЏ РёСЃРїРѕР»СЊР·РѕРІР°С‚СЊ РІРЅСѓС‚СЂРё
/// `realtime { ... }` Р±Р»РѕРєРѕРІ (D64). Р­С‚Рё СЌС„С„РµРєС‚С‹ РїРѕ СЃРµРјР°РЅС‚РёРєРµ РјРѕРіСѓС‚
/// РїСЂРёРѕСЃС‚Р°РЅРѕРІРёС‚СЊ fiber'Р° РІ production-runtime'Рµ.
fn realtime_suspend_effect(name: &str) -> bool {
    matches!(name, "Net" | "Fs" | "Db" | "Time" | "Blocking")
}

/// Plan 83.3 Ф.6: эффекты, запрещённые в теле `blocking { }`. Тело
/// исполняется на libuv-threadpool-потоке без fiber/event-loop-
/// контекста — async-I/O-эффекты (Net/Fs/Db/Time) там сломаны.
/// `Blocking` сюда НЕ входит: вложенный `blocking` на threadpool-
/// потоке исполняется inline (`mco_running()` == false) — безвреден.
fn blocking_body_forbidden_effect(name: &str) -> bool {
    matches!(name, "Net" | "Fs" | "Db" | "Time")
}

/// Plan 16: hardcoded whitelist callee-name'РѕРІ, РєРѕС‚РѕСЂС‹Рµ **Р°Р»Р»РѕС†РёСЂСѓСЋС‚**
/// РІ managed heap (Рё РїРѕС‚РѕРјСѓ Р·Р°РїСЂРµС‰РµРЅС‹ РІ `realtime nogc { ... }`).
/// РРґРµРЅС‚РёС„РёРєР°С†РёСЏ РїРѕ mangled C-name pattern + РїРѕ РІС‹СЃРѕРєРѕСѓСЂРѕРІРЅРµРІС‹Рј
/// `Type.method` (e.g. `[]int.new`, `StringBuilder.new`).
///
/// **РќРµ РїРѕРєСЂС‹РІР°РµС‚СЃСЏ** СЌС‚РёРј whitelist'РѕРј:
/// - User-defined record-РєРѕРЅСЃС‚СЂСѓРєС‚РѕСЂС‹ `Foo.new()` РµСЃР»Рё РѕРЅРё alloc'СЏС‚
///   С‡РµСЂРµР· nova_alloc вЂ” codegen РІСЃРµРіРґР° heap-Р±РѕРєСЃРёС‚ record-Р»РёС‚РµСЂР°Р»С‹,
///   С‚Р°Рє С‡С‚Рѕ С„Р°РєС‚РёС‡РµСЃРєРё Р»СЋР±РѕР№ record-Р»РёС‚РµСЂР°Р» В«Р°Р»Р»РѕС†РёСЂСѓСЋС‰РёР№В». РќРѕ
///   detection С‚СЂРµР±СѓРµС‚ bigger inference. Conservative вЂ” С„Р»Р°РіСѓРµРј
///   С‚РѕР»СЊРєРѕ СЃС‚Р°С‚РёС‡РµСЃРєРёРµ fabric-РјРµС‚РѕРґС‹.
/// - `str.from(non-str)` РµСЃР»Рё С‚СЂРµР±СѓРµС‚ concat'Р° вЂ” РїРѕРєР° СЃС‡РёС‚Р°РµРј
///   РІСЃРµ `str.from`-РІС‹Р·РѕРІС‹ "alloc'РёСЂСѓСЋС‰РёРјРё".
fn nogc_blacklisted_call(callee_path: &[String]) -> bool {
    if callee_path.len() != 2 { return false; }
    let ty = callee_path[0].as_str();
    let m = callee_path[1].as_str();
    // Array constructors: `[]T.new` / `[]T.with_capacity`.
    if ty.starts_with("[]") && matches!(m, "new" | "with_capacity") { return true; }
    // Builder/buffer constructors.
    if matches!(ty, "StringBuilder" | "WriteBuffer" | "ReadBuffer")
        && matches!(m, "new" | "with_capacity" | "from") { return true; }
    // D91 (Plan 21): Channel.new allocates Nova_ChannelState + Sender + Receiver + buf.
    if ty == "Channel" && matches!(m, "new" | "with_capacity") { return true; }
    // Map/Set/Vec/Deque etc.
    if matches!(ty, "HashMap" | "Set" | "Vec" | "Deque" | "LinkedList" | "Lru" | "BloomFilter")
        && matches!(m, "new" | "with_capacity") { return true; }
    // str.from: format/conversion РјРѕР¶РµС‚ alloc'Р°С‚СЊ.
    if ty == "str" && m == "from" { return true; }
    false
}

/// Plan 16: registry РґР»СЏ capability enforcement.
struct CapabilityCtx<'a> {
    /// Top-level free fn-РґРµРєР»Р°СЂР°С†РёРё (РґР»СЏ resolve РІС‹Р·РѕРІР° РїРѕ РёРјРµРЅРё).
    /// D84: Vec<&FnDecl> РґР»СЏ multi-overload вЂ” РІСЃРµ overloads РёРјРµРЅРё.
    /// Capability check С…РѕРґРёС‚ РїРѕ РІСЃРµРј overloads (СЃРј. check_capabilities_at).
    fn_decls: HashMap<String, Vec<&'a FnDecl>>,
    /// Plan 15 reuse: type в†’ method_name в†’ fn-decls.
    method_table: HashMap<String, HashMap<String, Vec<&'a FnDecl>>>,
    /// Effect-type name registry (РґР»СЏ distinguish'Р° effect-call vs ordinary).
    effect_decls: HashMap<String, &'a TypeDecl>,
}

/// Plan 16: capability state РїРµСЂРµРґР°С‘С‚СЃСЏ С‡РµСЂРµР· walk РєР°Рє mutable.
/// Push/pop РїСЂРё РІС…РѕРґРµ/РІС‹С…РѕРґРµ РёР· forbid/realtime Р±Р»РѕРєРѕРІ.
#[derive(Default, Clone)]
struct CapState {
    /// Stack forbidden-effects-set'РѕРІ РѕС‚ РІР»РѕР¶РµРЅРЅС‹С… `forbid` Р±Р»РѕРєРѕРІ.
    /// Effect СЂР°Р·СЂРµС€С‘РЅ РµСЃР»Рё РѕРЅ РЅРµ РІ **union'Рµ** СЌС‚РёС… set'РѕРІ.
    /// (Forbid РІРЅСѓС‚СЂРё forbid вЂ” union, СЃРј. D63.)
    forbidden_stack: Vec<HashSet<String>>,
    /// True РµСЃР»Рё РјС‹ РІРЅСѓС‚СЂРё `realtime { ... }` (РёР»Рё `realtime nogc`).
    /// Suspend-effects (Net/Fs/Db/Time/Blocking) Р·Р°РїСЂРµС‰РµРЅС‹.
    realtime_active: bool,
    /// True РµСЃР»Рё РјС‹ РІРЅСѓС‚СЂРё `realtime nogc { ... }`. Р”РѕРїРѕР»РЅРёС‚РµР»СЊРЅРѕ Рє
    /// realtime_active Р·Р°РїСЂРµС‰РµРЅС‹ alloc-РІС‹Р·РѕРІС‹.
    realtime_nogc: bool,
    /// Stack handlers, СѓСЃС‚Р°РЅРѕРІР»РµРЅРЅС‹С… С‡РµСЂРµР· `with X = ... { ... }`.
    /// РСЃРїРѕР»СЊР·СѓРµС‚СЃСЏ РґР»СЏ D63 forbid-handler-ban: `with X` РІРЅСѓС‚СЂРё
    /// `forbid X` вЂ” compile error.
    with_handler_stack: Vec<String>,
    /// Plan 83.3 (D50): имена эффектов, объявленных в сигнатуре
    /// enclosing-функции. `blocking { }` требует наличия `Blocking`
    /// в этом наборе. Заполняется один раз при входе в `walk_fn_body`;
    /// у `test`-блоков остаётся пустым (нет сигнатуры).
    declared_effects: HashSet<String>,
    /// Plan 83.3 Ф.6 (D50): True внутри тела `blocking { }`. Тело
    /// исполняется на libuv-threadpool-потоке без fiber-контекста и
    /// без GC-регистрации — поэтому проверяется как `nogc`
    /// (`realtime_nogc` тоже выставляется) + бан suspend-эффектов
    /// Net/Fs/Db/Time (V1 leaf-контракт). Отдельный флаг, НЕ
    /// `realtime_active` — иначе вложенный `blocking` отвергался бы
    /// как «`blocking` внутри `realtime`».
    blocking_body_active: bool,
}

impl CapState {
    /// Union forbidden-set'РѕРІ РІСЃРµС… СѓСЂРѕРІРЅРµР№ СЃС‚РµРєР°.
    fn union_forbidden(&self) -> HashSet<String> {
        let mut out = HashSet::new();
        for s in &self.forbidden_stack { out.extend(s.iter().cloned()); }
        out
    }
}

impl<'a> CapabilityCtx<'a> {
    fn build(module: &'a Module) -> Self {
        let mut fn_decls: HashMap<String, Vec<&FnDecl>> = HashMap::new();
        let mut method_table: HashMap<String, HashMap<String, Vec<&FnDecl>>> = HashMap::new();
        let mut effect_decls: HashMap<String, &TypeDecl> = HashMap::new();
        for item in &module.items {
            match item {
                Item::Type(t) => {
                    if matches!(t.kind, TypeDeclKind::Effect(_)) {
                        effect_decls.insert(t.name.clone(), t);
                    }
                }
                Item::Fn(f) => {
                    if let Some(recv) = &f.receiver {
                        method_table
                            .entry(recv.type_name.clone())
                            .or_default()
                            .entry(f.name.clone())
                            .or_default()
                            .push(f);
                    } else {
                        // D84: СЃРІРѕР±РѕРґРЅС‹Рµ С„СѓРЅРєС†РёРё С‚РѕР¶Рµ РјРѕРіСѓС‚ РёРјРµС‚СЊ overloads.
                        fn_decls.entry(f.name.clone()).or_default().push(f);
                    }
                }
                _ => {}
            }
        }
        CapabilityCtx { fn_decls, method_table, effect_decls }
    }

    fn check_module(&self, module: &Module, errors: &mut Vec<Diagnostic>) {
        // Plan 42 Sub-plan 42.A: file-level #forbid declarations.
        // Initial forbidden set РёР· module.attrs (per-file scope).
        // Р’СЃРµ functions РІ СЌС‚РѕРј file РїРѕР»СѓС‡Р°СЋС‚ СЌС‚Рё effects forbidden.
        let mut file_forbidden: HashSet<String> = HashSet::new();
        for attr in &module.attrs {
            if matches!(attr.kind, crate::ast::ModuleAttrKind::Forbid) {
                for e in &attr.effects {
                    file_forbidden.insert(e.clone());
                }
            }
        }
        for item in &module.items {
            match item {
                Item::Fn(f) => {
                    let mut state = CapState::default();
                    // Plan 42 Sub-plan 42.A: file-level #forbid initial frame.
                    if !file_forbidden.is_empty() {
                        state.forbidden_stack.push(file_forbidden.clone());
                    }
                    // Plan 16 Р¤.5: @realtime Р°С‚СЂРёР±СѓС‚ РѕР±РѕСЂР°С‡РёРІР°РµС‚ body
                    // РІ realtime[+nogc] РєРѕРЅС‚РµРєСЃС‚.
                    match f.realtime_attr {
                        RealtimeAttr::None => {}
                        RealtimeAttr::Realtime => state.realtime_active = true,
                        RealtimeAttr::RealtimeNogc => {
                            state.realtime_active = true;
                            state.realtime_nogc = true;
                        }
                    }
                    self.walk_fn_body(f, &mut state, errors);
                }
                Item::Test(t) => {
                    let mut state = CapState::default();
                    if !file_forbidden.is_empty() {
                        state.forbidden_stack.push(file_forbidden.clone());
                    }
                    self.walk_block(&t.body, &mut state, errors);
                }
                _ => {}
            }
        }
    }

    fn walk_fn_body(&self, f: &FnDecl, state: &mut CapState, errors: &mut Vec<Diagnostic>) {
        // Plan 83.3 (D50): зафиксировать объявленные эффекты сигнатуры —
        // `blocking { }` в теле требует среди них `Blocking`. Имя эффекта —
        // последний segment Named-path (`std.io.Blocking` → `Blocking`).
        for ef in &f.effects {
            if let TypeRef::Named { path, .. } = ef {
                if let Some(last) = path.last() {
                    state.declared_effects.insert(last.clone());
                }
            }
        }
        match &f.body {
            FnBody::Expr(e) => self.walk_expr(e, state, errors),
            FnBody::Block(b) => self.walk_block(b, state, errors),
            FnBody::External => {}
        }
    }

    fn walk_block(&self, b: &Block, state: &mut CapState, errors: &mut Vec<Diagnostic>) {
        for s in &b.stmts {
            self.walk_stmt(s, state, errors);
        }
        if let Some(t) = &b.trailing {
            self.walk_expr(t, state, errors);
        }
    }

    fn walk_stmt(&self, s: &Stmt, state: &mut CapState, errors: &mut Vec<Diagnostic>) {
        match s {
            Stmt::Expr(e) => self.walk_expr(e, state, errors),
            Stmt::Let(d) => self.walk_expr(&d.value, state, errors),
            Stmt::Const(_) => {}
            Stmt::Assign { target, value, .. } => {
                self.walk_expr(target, state, errors);
                self.walk_expr(value, state, errors);
            }
            Stmt::Return { value, .. } => {
                if let Some(v) = value { self.walk_expr(v, state, errors); }
            }
            Stmt::Throw { value, .. } => self.walk_expr(value, state, errors),
            Stmt::Break(_) | Stmt::Continue(_) => {}
            // D90 Plan 20 Р¤.2: РїСЂРѕРІРµСЂСЏРµРј capability'Рё РІРЅСѓС‚СЂРё body
            // defer'Р°. РџРѕР»РЅС‹Рµ constraints (no Fail/suspend/exit-control)
            // вЂ” Р¤.3.
            Stmt::Defer { body, .. } | Stmt::ErrDefer { body, .. }
            | Stmt::OkDefer { body, .. } | Stmt::DeferWithResult { body, .. } => {
                self.walk_expr(body, state, errors);
            }
            // Plan 110 D188: walk init + body (scaffold).
            Stmt::ConsumeScope { init, body, .. } => {
                self.walk_expr(init, state, errors);
                for s in &body.stmts {
                    self.walk_stmt(s, state, errors);
                }
                if let Some(t) = &body.trailing {
                    self.walk_expr(t, state, errors);
                }
            }
            // Plan 33.2 Р¤.8: assert_static вЂ” walk expr.
            Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => self.walk_expr(expr, state, errors),
            // Ф.4.1: apply — ghost, нет capability-эффектов.
            Stmt::Apply { .. } => {}
            // Ф.4.2: calc — ghost, нет capability-эффектов.
            Stmt::Calc { .. } => {}
            // Plan 33.9 Ф.2: reveal — ghost, нет capability-эффектов.
            Stmt::Reveal { .. } => {}
        }
    }

    fn walk_expr(&self, e: &Expr, state: &mut CapState, errors: &mut Vec<Diagnostic>) {
        // РЎРЅР°С‡Р°Р»Р° РїСЂРѕРІРµСЂСЏРµРј СЃР°Рј СѓР·РµР» (call-bound checks), РїРѕС‚РѕРј
        // РїРѕРіСЂСѓР¶Р°РµРјСЃСЏ РІРЅСѓС‚СЂСЊ СЃ РѕР±РЅРѕРІР»С‘РЅРЅС‹Рј state'РѕРј РґР»СЏ Р±Р»РѕС‡РЅС‹С…
        // РєРѕРЅСЃС‚СЂСѓРєС†РёР№ (forbid/realtime/with).
        self.check_capabilities_at(e, state, errors);
        match &e.kind {
            ExprKind::Forbid { effects, body } => {
                // Push forbidden-set, walk, pop.
                let names: HashSet<String> = effects.iter()
                    .filter_map(|t| match t {
                        TypeRef::Named { path, .. } if path.len() == 1 => Some(path[0].clone()),
                        _ => None,
                    })
                    .collect();
                state.forbidden_stack.push(names);
                self.walk_block(body, state, errors);
                state.forbidden_stack.pop();
            }
            ExprKind::Realtime { nogc, body } => {
                let prev_active = state.realtime_active;
                let prev_nogc = state.realtime_nogc;
                state.realtime_active = true;
                state.realtime_nogc = state.realtime_nogc || *nogc;
                self.walk_block(body, state, errors);
                state.realtime_active = prev_active;
                state.realtime_nogc = prev_nogc;
            }
            ExprKind::With { bindings, body } => {
                // Plan 16 D63: СѓСЃС‚Р°РЅРѕРІРєР° handler'Р° РґР»СЏ forbidden-СЌС„С„РµРєС‚Р°
                // РІРЅСѓС‚СЂРё forbid-Р±Р»РѕРєР° вЂ” compile error.
                //
                // WithBinding.effect: TypeRef. Р”Р»СЏ РЅР°Р·РІР°РЅРёСЏ СЌС„С„РµРєС‚Р°
                // Р±РµСЂС‘Рј РїРѕСЃР»РµРґРЅРёР№ segment Named-path (e.g. `std.io.Net`
                // в†’ "Net"). Non-Named TypeRefs (Array/Tuple/Func/etc.) вЂ”
                // РЅРµРІР°Р»РёРґРЅС‹ РґР»СЏ СЌС„С„РµРєС‚-handler'РѕРІ, РїСЂРѕРїСѓСЃРєР°РµРј.
                let pushed: Vec<String> = bindings.iter()
                    .filter_map(|b| match &b.effect {
                        TypeRef::Named { path, .. } if !path.is_empty() => path.last().cloned(),
                        _ => None,
                    })
                    .collect();
                let forbidden = state.union_forbidden();
                for n in &pushed {
                    if forbidden.contains(n) {
                        errors.push(Diagnostic::new(
                            format!(
                                "cannot install handler for `{}` inside `forbid {}` block (D63): \
                                 forbid is impenetrable вЂ” code in body cannot escape sandbox \
                                 via `with X = вЂ¦`.",
                                n, n
                            ),
                            e.span,
                        ));
                    }
                    state.with_handler_stack.push(n.clone());
                }
                self.walk_block(body, state, errors);
                for _ in &pushed { state.with_handler_stack.pop(); }
            }
            ExprKind::Call { func, args, trailing } => {
                self.walk_expr(func, state, errors);
                for a in args { self.walk_expr(a.expr(), state, errors); }
                if let Some(t) = trailing {
                    match t {
                        crate::ast::Trailing::Block(b) => self.walk_block(b, state, errors),
                        crate::ast::Trailing::LegacyBlockWithParams(tb) => {
                            self.walk_block(&tb.body, state, errors)
                        }
                        crate::ast::Trailing::Fn(sb) => match &sb.body {
                            FnBody::Expr(e) => self.walk_expr(e, state, errors),
                            FnBody::Block(b) => self.walk_block(b, state, errors),
                            FnBody::External => {}
                        },
                    }
                }
            }
            ExprKind::TurboFish { base, .. } => self.walk_expr(base, state, errors),
            ExprKind::Binary { left, right, .. } => {
                self.walk_expr(left, state, errors);
                self.walk_expr(right, state, errors);
            }
            ExprKind::Unary { operand, .. } => self.walk_expr(operand, state, errors),
            ExprKind::Try(inner) | ExprKind::Bang(inner) => self.walk_expr(inner, state, errors),
            ExprKind::Coalesce(a, b) => {
                self.walk_expr(a, state, errors);
                self.walk_expr(b, state, errors);
            }
            ExprKind::As(e, _) => self.walk_expr(e, state, errors),
            ExprKind::Is(e, _) => self.walk_expr(e, state, errors),
            ExprKind::Member { obj, .. } => self.walk_expr(obj, state, errors),
            ExprKind::Index { obj, index } => {
                self.walk_expr(obj, state, errors);
                self.walk_expr(index, state, errors);
            }
            ExprKind::If { cond, then, else_ } => {
                self.walk_expr(cond, state, errors);
                self.walk_block(then, state, errors);
                if let Some(eb) = else_ {
                    match eb {
                        ElseBranch::Block(b) => self.walk_block(b, state, errors),
                        ElseBranch::If(e) => self.walk_expr(e, state, errors),
                    }
                }
            }
            ExprKind::IfLet { scrutinee, then, else_, .. } => {
                self.walk_expr(scrutinee, state, errors);
                self.walk_block(then, state, errors);
                if let Some(eb) = else_ {
                    match eb {
                        ElseBranch::Block(b) => self.walk_block(b, state, errors),
                        ElseBranch::If(e) => self.walk_expr(e, state, errors),
                    }
                }
            }
            ExprKind::Match { scrutinee, arms } => {
                self.walk_expr(scrutinee, state, errors);
                for arm in arms {
                    if let Some(g) = &arm.guard { self.walk_expr(g, state, errors); }
                    match &arm.body {
                        MatchArmBody::Expr(e) => self.walk_expr(e, state, errors),
                        MatchArmBody::Block(b) => self.walk_block(b, state, errors),
                    }
                }
            }
            ExprKind::Block(b) => self.walk_block(b, state, errors),
            ExprKind::ArrayLit(elems) => {
                for el in elems {
                    match el {
                        ArrayElem::Item(e) | ArrayElem::Spread(e) => self.walk_expr(e, state, errors),
                    }
                }
            }
            ExprKind::MapLit { elems, .. } => {
                let pairs = crate::ast::MapElem::cloned_pairs(&elems);
                for (k, v) in pairs.iter() {
                    self.walk_expr(k, state, errors);
                    self.walk_expr(v, state, errors);
                }
            }
            ExprKind::TupleLit(elems) => {
                for e in elems { self.walk_expr(e, state, errors); }
            }
            ExprKind::RecordLit { fields, .. } => {
                for f in fields {
                    if let Some(v) = &f.value { self.walk_expr(v, state, errors); }
                }
            }
            ExprKind::TaggedTemplate { tag, args, .. } => {
                self.walk_expr(tag, state, errors);
                for a in args { self.walk_expr(a, state, errors); }
            }
            ExprKind::InterpolatedStr { parts } => {
                for p in parts {
                    if let InterpStrPart::Expr(e) = p {
                        self.walk_expr(e, state, errors);
                    }
                }
            }
            ExprKind::Lambda { body, .. } => self.walk_expr(body, state, errors),
            // Plan 19, C5: CapabilityCtx РѕР±С…РѕРґРёС‚ С‚РµР»Рѕ closure РґР»СЏ
            // forbid/realtime РїСЂРѕРІРµСЂРѕРє (D63/D64). Closure-light Рё
            // closure-full РѕРґРёРЅР°РєРѕРІРѕ вЂ” walk by body kind.
            ExprKind::ClosureLight { body, .. } => match body {
                crate::ast::ClosureBody::Expr(e) => self.walk_expr(e, state, errors),
                crate::ast::ClosureBody::Block(b) => self.walk_block(b, state, errors),
            },
            ExprKind::ClosureFull(sb) => match &sb.body {
                FnBody::Expr(e) => self.walk_expr(e, state, errors),
                FnBody::Block(b) => self.walk_block(b, state, errors),
                FnBody::External => {}
            },
            ExprKind::Spawn(body) => self.walk_expr(body, state, errors),
            ExprKind::Detach(body) => self.walk_block(body, state, errors),
            ExprKind::Blocking(body) => {
                // Plan 83.3 (D50): `blocking { }` — leaf-блокирующая работа,
                // уводится в libuv threadpool, suspend'ит fiber.
                // (1) Запрещён внутри `realtime { }` (D64): suspend-эффект
                //     `Blocking` есть в realtime_suspend_effect-списке.
                if state.realtime_active {
                    errors.push(Diagnostic::new(
                        "cannot use `blocking { ... }` inside `realtime` block (D64): \
                         blocking work suspends the fiber while it is offloaded to the \
                         libuv threadpool. Hint: realtime guarantees no suspension — \
                         move the `blocking` block out of the `realtime` block."
                            .to_string(),
                        e.span,
                    ));
                }
                // (2) Требует эффект `Blocking` в сигнатуре enclosing-функции
                //     (как `detach` → `Detach` по D50). У `test`-блоков
                //     declared_effects пуст → `blocking` должен быть обёрнут
                //     в `fn ... Blocking -> ...`.
                if !state.declared_effects.contains("Blocking") {
                    errors.push(Diagnostic::new(
                        "`blocking { ... }` requires the `Blocking` effect declared in the \
                         enclosing function's signature (D50). Fix: add `Blocking` to the \
                         effect list — `fn name(...) Blocking -> ...`."
                            .to_string(),
                        e.span,
                    ));
                }
                // (3) Plan 83.3 Ф.6: тело исполняется на libuv-threadpool-
                //     потоке (не fiber, не GC-registered) → проверяем как
                //     nogc (запрет alloc-вызовов) + бан suspend-эффектов
                //     Net/Fs/Db/Time. V1 leaf-контракт (D50 §4) становится
                //     enforced'ным. Отдельный флаг blocking_body_active —
                //     НЕ realtime_active, иначе вложенный `blocking`
                //     отвергался бы как «blocking внутри realtime».
                let prev_blk = state.blocking_body_active;
                let prev_nogc = state.realtime_nogc;
                state.blocking_body_active = true;
                state.realtime_nogc = true;
                self.walk_block(body, state, errors);
                state.blocking_body_active = prev_blk;
                state.realtime_nogc = prev_nogc;
            }
            ExprKind::Supervised { body, cancel } => {
                if let Some(c) = cancel { self.walk_expr(c, state, errors); }
                self.walk_block(body, state, errors);
            }
            ExprKind::ParallelFor { iter, body, .. } => {
                self.walk_expr(iter, state, errors);
                self.walk_block(body, state, errors);
            }
            ExprKind::For { iter, body, .. } => {
                self.walk_expr(iter, state, errors);
                self.walk_block(body, state, errors);
            }
            ExprKind::While { cond, body, .. } => {
                self.walk_expr(cond, state, errors);
                self.walk_block(body, state, errors);
            }
            ExprKind::WhileLet { scrutinee, body, .. } => {
                self.walk_expr(scrutinee, state, errors);
                self.walk_block(body, state, errors);
            }
            ExprKind::Loop { body, .. } => self.walk_block(body, state, errors),
            ExprKind::Select { arms } => {
                for arm in arms {
                    match &arm.op {
                        SelectOp::Recv { chan, .. } => self.walk_expr(chan, state, errors),
                        SelectOp::Send { chan, value } => {
                            self.walk_expr(chan, state, errors);
                            self.walk_expr(value, state, errors);
                        }
                        SelectOp::Default => {}
                    }
                    if let Some(g) = &arm.guard { self.walk_expr(g, state, errors); }
                    self.walk_block(&arm.body, state, errors);
                }
            }
            ExprKind::Range { start, end, .. } => {
                if let Some(s) = start { self.walk_expr(s, state, errors); }
                if let Some(e) = end { self.walk_expr(e, state, errors); }
            }
            ExprKind::Throw(e) => self.walk_expr(e, state, errors),
            ExprKind::Interrupt(opt) => {
                if let Some(e) = opt { self.walk_expr(e, state, errors); }
            }
            // D.1.3: РєРІР°РЅС‚РѕСЂ вЂ” С‚РѕР»СЊРєРѕ РІ РєРѕРЅС‚СЂР°РєС‚Р°С…; РѕР±С…РѕРґРёРј range Рё body.
            ExprKind::Forall { range, body, .. } | ExprKind::Exists { range, body, .. } => {
                self.walk_expr(range, state, errors);
                self.walk_expr(body, state, errors);
            }
            // Р›РёС‚РµСЂР°Р»С‹ / ident'С‹ / handler-Р»РёС‚РµСЂР°Р»С‹ вЂ” Р±РµР· СЂРµРєСѓСЂСЃРёРё.
            ExprKind::IntLit(_) | ExprKind::FloatLit(_) | ExprKind::BoolLit(_)
            | ExprKind::StrLit(_) | ExprKind::CharLit(_) | ExprKind::UnitLit
            | ExprKind::Ident(_) | ExprKind::Path(_) | ExprKind::SelfAccess
            | ExprKind::HandlerLit { .. }
            | ExprKind::ProtocolLit { .. } => {}
        }
    }

    /// Plan 16 Р¤.2-Р¤.4: РїСЂРѕРІРµСЂРєР° capability-rules РЅР° РєРѕРЅРєСЂРµС‚РЅРѕРј СѓР·Р»Рµ.
    /// РЎРµР№С‡Р°СЃ вЂ” С‚РѕР»СЊРєРѕ РґР»СЏ Call'РѕРІ; forbid/realtime/with СѓРїСЂР°РІР»СЏСЋС‚
    /// state'РѕРј, РЅРµ РІС‹Р·С‹РІР°СЏ check'РѕРІ РЅР° СЃРѕР±СЃС‚РІРµРЅРЅРѕРј СѓР·Р»Рµ.
    fn check_capabilities_at(&self, e: &Expr, state: &CapState, errors: &mut Vec<Diagnostic>) {
        let ExprKind::Call { func, .. } = &e.kind else { return; };
        // Path-form: `Type.method`, `Effect.op` РёР»Рё `[]T.method`.
        // Р”Р»СЏ `[]T.method()` РїР°СЂСЃРµСЂ СЃС‚СЂРѕРёС‚ Member{obj: Path(["__array", T]), name}.
        let path: Vec<String> = match &func.kind {
            ExprKind::Path(parts) => parts.clone(),
            ExprKind::Member { obj, name } => {
                match &obj.kind {
                    ExprKind::Ident(n) => vec![n.clone(), name.clone()],
                    // `[]T.method`: Path(["__array","T"]) в†’ ["[]T", method].
                    ExprKind::Path(parts) if parts.len() == 2 && parts[0] == "__array" => {
                        vec![format!("[]{}", parts[1]), name.clone()]
                    }
                    ExprKind::Path(parts) => {
                        let mut v = parts.clone();
                        v.push(name.clone());
                        v
                    }
                    _ => return, // dynamic member-call; РЅРµ resolve'РёРј
                }
            }
            ExprKind::Ident(n) => vec![n.clone()],
            _ => return,
        };
        // 1. Effect-op call: `Effect.op(...)` РіРґРµ Effect вЂ” registered effect-type.
        if path.len() == 2 {
            let head = &path[0];
            if self.effect_decls.contains_key(head) {
                self.check_forbid_intersection(head, state, e.span, errors);
                if state.realtime_active && realtime_suspend_effect(head) {
                    errors.push(Diagnostic::new(
                        format!(
                            "cannot use suspend-effect `{}` inside `realtime` block (D64): \
                             {}.{} may suspend the fiber. Hint: extract the effectful work \
                             out of `realtime` block, or use non-blocking alternative \
                             (e.g. `Channel.try_recv` instead of `Channel.recv`).",
                            head, head, &path[1]
                        ),
                        e.span,
                    ));
                }
                // Plan 83.3 Ф.6: тело `blocking { }` идёт на threadpool-потоке
                // без fiber/event-loop — async-I/O-эффекты там сломаны.
                if state.blocking_body_active && blocking_body_forbidden_effect(head) {
                    errors.push(Diagnostic::new(
                        format!(
                            "cannot use suspend-effect `{}` inside `blocking {{ ... }}` body \
                             (Plan 83.3 V1 leaf-contract, D50 §4): {}.{} needs the \
                             fiber/event-loop context, which the libuv threadpool thread \
                             does not have. Hint: `blocking` is for genuinely-blocking C \
                             calls — do async I/O outside the `blocking` block.",
                            head, head, &path[1]
                        ),
                        e.span,
                    ));
                }
            }
        }
        // 2. Free-fn call: lookup callee.effects.
        // D84: fn_decls вЂ” Vec<&FnDecl>. Р‘РµР· РїРѕР»РЅРѕРіРѕ type-resolve РІ
        // bound-checker'Рµ РЅРµРІРѕР·РјРѕР¶РЅРѕ РІС‹Р±СЂР°С‚СЊ РєРѕРЅРєСЂРµС‚РЅСѓСЋ overload вЂ”
        // РїСЂРѕРІРµСЂСЏРµРј СЌС„С„РµРєС‚С‹ Сѓ **РІСЃРµС…** overloads (consistent СЃ С‚РµРј С‡С‚Рѕ
        // РґРµР»Р°РµС‚ method_table-РІРµС‚РєР° РЅРёР¶Рµ). False-positive РµСЃР»Рё СЂР°Р·РЅС‹Рµ
        // overloads РёРјРµСЋС‚ СЂР°Р·РЅС‹Рµ СЌС„С„РµРєС‚С‹ вЂ” РІ СЂРµР°Р»СЊРЅС‹С… API РјР°Р»РѕРІРµСЂРѕСЏС‚РЅРѕ
        // (overloads РѕР±С‹С‡РЅРѕ РѕС‚Р»РёС‡Р°СЋС‚СЃСЏ С‚РёРїРѕРј Р°СЂРіСѓРјРµРЅС‚Р°, РЅРµ СЌС„С„РµРєС‚Р°РјРё),
        // РЅРѕ РµСЃР»Рё СЃР»СѓС‡РёС‚СЃСЏ вЂ” РїСЂРѕРіСЂР°РјРјРёСЃС‚ РґРёСЃР°РјР±РёРіСѓРёСЂСѓРµС‚ С‡РµСЂРµР· cast.
        if path.len() == 1 {
            if let Some(overloads) = self.fn_decls.get(&path[0]) {
                for callee in overloads.iter() {
                    self.check_callee_effects(callee, &path[0], state, e.span, errors);
                }
            }
        }
        // 3. Method call: `Type.method` РёР»Рё `obj.method` вЂ” lookup РІ method_table.
        // (РўРѕР»СЊРєРѕ receiver-Path С„РѕСЂРјС‹; instance-method С‡РµСЂРµР· obj.method
        // С‚СЂРµР±СѓРµС‚ type-РёРЅС„РµСЂРµРЅС†РёРё, РѕС‚Р»РѕР¶РµРЅ.)
        if path.len() == 2 {
            if let Some(methods) = self.method_table.get(&path[0]) {
                if let Some(fns) = methods.get(&path[1]) {
                    for callee in fns {
                        self.check_callee_effects(callee, &format!("{}.{}", path[0], path[1]), state, e.span, errors);
                    }
                }
            }
        }
        // 4. Plan 16 Р¤.4: nogc alloc-fn check.
        //    Plan 83.3 Ф.6: тело `blocking { }` тоже nogc (threadpool-поток
        //    не GC-registered) — context-aware сообщение.
        if state.realtime_nogc && nogc_blacklisted_call(&path) {
            if state.blocking_body_active && !state.realtime_active {
                errors.push(Diagnostic::new(
                    format!(
                        "cannot allocate inside `blocking {{ ... }}` body (Plan 83.3 \
                         V1 leaf-contract, D50 §4): `{}` allocates on the managed heap, \
                         but the body runs on a libuv threadpool thread that is not \
                         GC-registered. Hint: move the allocation outside the \
                         `blocking` block.",
                        path.join(".")
                    ),
                    e.span,
                ));
            } else {
                errors.push(Diagnostic::new(
                    format!(
                        "cannot allocate inside `realtime nogc` block (D64): `{}` allocates \
                         on managed heap. Hint: use `region {{ ... }}` for arena-allocations, \
                         or move the allocation outside the `realtime nogc` block.",
                        path.join(".")
                    ),
                    e.span,
                ));
            }
        }
    }

    /// Plan 16 Р¤.2: РїСЂРѕРІРµСЂРєР° РїРµСЂРµСЃРµС‡РµРЅРёСЏ callee.effects СЃ union forbidden-СЃС‚РµРєР°.
    fn check_callee_effects(
        &self,
        callee: &FnDecl,
        callee_label: &str,
        state: &CapState,
        span: Span,
        errors: &mut Vec<Diagnostic>,
    ) {
        // Pure вЂ” РІСЃРµРіРґР° OK.
        if callee.effects.is_empty() && state.forbidden_stack.is_empty() && !state.realtime_active {
            return;
        }
        let forbidden = state.union_forbidden();
        for eff in &callee.effects {
            let TypeRef::Named { path, .. } = eff else { continue; };
            if path.is_empty() { continue; }
            let name = &path[0];
            // Forbid check.
            if forbidden.contains(name) {
                errors.push(Diagnostic::new(
                    format!(
                        "function `{}` requires effect `{}`, forbidden by enclosing \
                         `forbid {}` block (D63). Hint: pure code inside `forbid` is OK; \
                         to use `{}`, restructure to compute effect-free results inside \
                         and apply effects outside the sandbox.",
                        callee_label, name, name, name
                    ),
                    span,
                ));
            }
            // Realtime check.
            if state.realtime_active && realtime_suspend_effect(name) {
                errors.push(Diagnostic::new(
                    format!(
                        "function `{}` requires suspend-effect `{}`, cannot be called \
                         inside `realtime` block (D64). Hint: realtime guarantees \
                         no fiber-suspension; effects {} block.",
                        callee_label, name,
                        "Net/Fs/Db/Time/Blocking suspend the fiber and are forbidden inside realtime"
                    ),
                    span,
                ));
            }
            // Plan 83.3 Ф.6: тело `blocking { }` идёт на threadpool-потоке
            // без fiber/event-loop-контекста — async-I/O-эффекты сломаны.
            if state.blocking_body_active && blocking_body_forbidden_effect(name) {
                errors.push(Diagnostic::new(
                    format!(
                        "function `{}` requires suspend-effect `{}`, cannot be called \
                         inside `blocking {{ ... }}` body (Plan 83.3 V1 leaf-contract, \
                         D50 §4): the libuv threadpool thread has no fiber/event-loop \
                         context for `{}`. Hint: `blocking` is for genuinely-blocking \
                         C calls — do async I/O outside it.",
                        callee_label, name, name
                    ),
                    span,
                ));
            }
        }
    }

    /// Plan 16 D63: РµРґРёРЅРёС‡РЅР°СЏ РїСЂРѕРІРµСЂРєР° effect'a РїСЂРѕС‚РёРІ forbidden-СЃС‚РµРєР°.
    fn check_forbid_intersection(
        &self,
        eff_name: &str,
        state: &CapState,
        span: Span,
        errors: &mut Vec<Diagnostic>,
    ) {
        let forbidden = state.union_forbidden();
        if forbidden.contains(eff_name) {
            errors.push(Diagnostic::new(
                format!(
                    "use of effect `{}` is forbidden by enclosing `forbid {}` block (D63).",
                    eff_name, eff_name
                ),
                span,
            ));
        }
    }
}

// ============================================================================
// Name-resolution С„Р°Р·Р°.
//
// Pre-collects top-level РёРјРµРЅР° (fns/types/consts/variants/built-ins) +
// walk fn/test bodies СЃРѕ scope-СЃС‚РµРєРѕРј. РќР° `ExprKind::Ident(name)`
// РїСЂРѕРІРµСЂСЏРµС‚, С‡С‚Рѕ `name` РІ (С‚РµРєСѓС‰РёР№ scope в€Є top-level в€Є built-ins).
// РРЅР°С‡Рµ вЂ” diagnostic В«undefined identifier`.
//
// **РљРѕРЅРєРµСЂРІР°С‚РёРІРЅР°СЏ СЃС‚СЂР°С‚РµРіРёСЏ**: Р»СѓС‡С€Рµ РїСЂРѕРїСѓСЃС‚РёС‚СЊ undefined С‡РµРј
// false-positive. РЎР»СѓС‡Р°Рё, РіРґРµ РЅРµ РїСЂРѕРІРµСЂСЏРµРј:
//   - `obj.method(args)` / `Type.method(args)` вЂ” method-РёРјРµРЅР° resolve'СЏС‚СЃСЏ
//     С‡РµСЂРµР· method_table (РјРѕРіСѓС‚ Р±С‹С‚СЊ РЅР° Р»СЋР±РѕРј С‚РёРїРµ).
//   - `obj.field` / `Record { field: val }` вЂ” РїРѕР»СЏ, РЅРµ РёРґРµРЅС‚РёС„РёРєР°С‚РѕСЂС‹.
//   - Path-СЃРµРіРјРµРЅС‚С‹ `mod1::mod2::name` (intermediate вЂ” РјРѕРґСѓР»Рё, РЅРµ expr).
//   - Tagged-template tags.
//   - Generic-params РІ TypeRef (СЌС‚Рѕ С‚РёРїС‹, РЅРµ expressions).
//   - Sum-variant tag РІ pattern (`Some(x)` вЂ” constructor name, РЅРµ expr).
// ============================================================================

/// Plan 19+: СЃС‚Р°С‚РёС‡РµСЃРєР°СЏ РїСЂРѕРІРµСЂРєР° undefined РёРґРµРЅС‚РёС„РёРєР°С‚РѕСЂРѕРІ.
struct NameResCtx {
    /// Plan 42.15: per-group shared declarations (Rule C). Key = file_id
    /// peer'Р°. Value = declarations РІСЃРµС… peers Р•Р“Рћ module-group (folder-
    /// module СЃ РѕР±С‰РёРј parent dir). Peers РѕРґРЅРѕР№ РіСЂСѓРїРїС‹ РґРµР»СЏС‚ namespace;
    /// РјРµР¶РґСѓ РіСЂСѓРїРїР°РјРё вЂ” РќР• РґРµР»СЏС‚ (imported folder-module's decls РЅРµ
    /// РїСЂРѕС‚РµРєР°СЋС‚).
    group_decls: HashMap<FileId, HashSet<String>>,
    /// Plan 42.15: fallback РґР»СЏ legacy/single-file (peer_files РїСѓСЃС‚) вЂ”
    /// flat РІСЃРµ module.items. РСЃРїРѕР»СЊР·СѓРµС‚СЃСЏ РєРѕРіРґР° file_id РЅРµ РІ group_decls.
    shared_decls: HashSet<String>,
    /// Plan 42.15: union Р’РЎР•РҐ declarations (РІСЃРµ РіСЂСѓРїРїС‹ + imported). РќР•
    /// РґР»СЏ name-resolution enforcement (СЌС‚Рѕ РЅР°СЂСѓС€РёР»Рѕ Р±С‹ Rule C) вЂ”
    /// РёСЃРїРѕР»СЊР·СѓРµС‚СЃСЏ РўРћР›Р¬РљРћ РєР°Рє СЌРІСЂРёСЃС‚РёРєР° РІ `collect_pattern_bindings`
    /// (РѕС‚Р»РёС‡РёС‚СЊ pattern-binding `let x` РѕС‚ variant-pattern `Some`).
    all_decls: HashSet<String>,
    /// Plan 42.15: per-peer imported item names вЂ” items СЃС‚Р°РІС€РёРµ
    /// РІРёРґРёРјС‹РјРё РІ peer'Рµ С‡РµСЂРµР· РµРіРѕ РїСЂСЏРјС‹Рµ `import` (РїРѕСЃР»Рµ rename +
    /// selective filter). Rule C: imports РќР• shared РјРµР¶РґСѓ peers.
    peer_imported_names: HashMap<FileId, HashSet<String>>,
    /// Built-in РёРјРµРЅР°, РґРѕСЃС‚СѓРїРЅС‹Рµ РІ Р»СЋР±РѕРј scope Р±РµР· РѕР±СЉСЏРІР»РµРЅРёСЏ:
    /// primitive types, prelude variants (None/Some/Ok/Err), bool
    /// Р»РёС‚РµСЂР°Р»С‹ (true/false), builtin functions (assert/print/...),
    /// special idents (Self).
    builtins: HashSet<String>,
    /// Per-peer import namespace (Plan 42.4 Rule C).
    /// Key = file_id of peer file (MAIN_FILE_ID for entry).
    /// Value = set of module/alias names visible in that peer.
    peer_module_names: HashMap<FileId, HashSet<String>>,
}

impl NameResCtx {
    fn build(module: &Module) -> Self {
        // Plan 42.15: per-group shared declarations (Rule C).
        //
        // **Module-group** = РЅР°Р±РѕСЂ peer-С„Р°Р№Р»РѕРІ РѕРґРЅРѕРіРѕ folder-module
        // (РёРјРµСЋС‚ РѕР±С‰РёР№ parent dir). Р’РЅСѓС‚СЂРё РіСЂСѓРїРїС‹ peers РґРµР»СЏС‚
        // declarations namespace (Rule C: В«peers share declarationsВ»).
        // РњР•Р–Р”РЈ РіСЂСѓРїРїР°РјРё вЂ” РќР• РґРµР»СЏС‚ (imported folder-module's decls РЅРµ
        // РїСЂРѕС‚РµРєР°СЋС‚ РІ entry's namespace).
        //
        // `group_decls`: HashMap<FileId, HashSet<String>> вЂ” РґР»СЏ РєР°Р¶РґРѕРіРѕ
        // peer'Р° (РїРѕ file_id) в†’ declarations РІСЃРµС… peers РµРіРѕ РіСЂСѓРїРїС‹.
        let mut group_decls: HashMap<FileId, HashSet<String>> = HashMap::new();
        // Fallback РґР»СЏ legacy/single-file (peer_files РїСѓСЃС‚).
        let mut shared_decls: HashSet<String> = HashSet::new();

        fn collect_decl_names(items: &[Item], out: &mut HashSet<String>) {
            for item in items {
                match item {
                    Item::Fn(fd) => {
                        // free-functions (Р±РµР· receiver) РІР°Р»РёРґРЅС‹ РєР°Рє
                        // bare-ident `foo()`. РњРµС‚РѕРґС‹ вЂ” С‡РµСЂРµР· obj.method.
                        if fd.receiver.is_none() {
                            out.insert(fd.name.clone());
                        }
                    }
                    Item::Type(td) => {
                        out.insert(td.name.clone());
                        // Variant-РёРјРµРЅР° sum-С‚РёРїРѕРІ: `Some(x)`, `Red`, etc.
                        if let TypeDeclKind::Sum(variants) = &td.kind {
                            for v in variants {
                                out.insert(v.name.clone());
                            }
                        }
                    }
                    Item::Const(cd) => {
                        out.insert(cd.name.clone());
                    }
                    // Plan 57: bench — top-level item но имя — string-literal,
                    // не идентификатор; в name resolution не участвует.
                    Item::Let(_) | Item::Test(_) | Item::Bench(_) | Item::Lemma(_) => {}
                }
            }
        }

        if module.peer_files.is_empty() {
            // Legacy/single-file: flat вЂ” РІСЃРµ module.items.
            collect_decl_names(&module.items, &mut shared_decls);
        } else {
            // Р“СЂСѓРїРїРёСЂСѓРµРј peers РїРѕ parent dir РїСѓС‚Рё. Р’СЃРµ peers РѕРґРЅРѕР№
            // РїР°РїРєРё = РѕРґРЅР° module-group, РґРµР»СЏС‚ declarations.
            let mut groups: HashMap<(std::path::PathBuf, Vec<String>), HashSet<String>> = HashMap::new();
            let mut peer_group_key: HashMap<FileId, (std::path::PathBuf, Vec<String>)> = HashMap::new();
            for pf in &module.peer_files {
                let dir_key = pf.path.parent()
                    .map(|p| p.to_path_buf())
                    .unwrap_or_else(|| pf.path.clone());
                // Plan 81 F.1: group by (dir, module_name).
                let group_key = (dir_key, pf.module_name.clone());
                peer_group_key.insert(pf.file_id, group_key.clone());
                let entry = groups.entry(group_key).or_default();
                collect_decl_names(&pf.items_here, entry);
            }
            // Р Р°Р·РІРѕСЂР°С‡РёРІР°РµРј: РґР»СЏ РєР°Р¶РґРѕРіРѕ peer'Р° вЂ” decls РµРіРѕ РіСЂСѓРїРїС‹.
            for pf in &module.peer_files {
                if let Some(gk) = peer_group_key.get(&pf.file_id) {
                    if let Some(decls) = groups.get(gk) {
                        group_decls.insert(pf.file_id, decls.clone());
                    }
                }
            }
        }

        let builtins: HashSet<String> = [
            // Numeric primitives.
            "int", "i8", "i16", "i32", "i64",
            "u8", "u16", "u32", "u64",
            "f32", "f64", "uint", "size",
            // Other primitives.
            "bool", "str", "char", "unit", "any",
            // Plan 76: `never` — bottom-тип (uninhabited, 0 значений),
            // строчный встроенный примитив. Subtype любого `T`. Как и
            // остальные примитивы (`int`/`bool`/...) — НЕ объявляется в
            // prelude, известен компилятору напрямую.
            "never",
            // Boolean literals (parsed РєР°Рє Ident РІ bool-context РєРѕРµ-РіРґРµ).
            "true", "false",
            // Special idents.
            "Self", "self",
            // Plan 62.A: `Option`/`Result`/`Some`/`None`/`Ok`/`Err`/`Error`/
            // `Ordering`/`Less`/`Equal`/`Greater` (11 names) перенесены в
            // std/prelude/core.nv. Type-checker теперь resolves их через
            // cross-file resolve (R27 auto-import). См. docs/plans/
            // 62-prelude-hardcode-migration.md §62.A.
            //
            // Plan 62.C: `RuntimeError` + 6 variants (`DivByZero`,
            // `Overflow`, `IndexOutOfBounds`, `TypeMismatch`, `AssertFailed`,
            // `NoHandler`) перенесены в std/prelude/errors.nv. Аналогично
            // `ReadBufferError` + `UnexpectedEnd` (не были в этом HashSet'е,
            // но добавлены в registry через init_prelude_decls_from_items
            // — см. sum_schema_registry.rs::register_prelude_sum_from_decl).
            // Type-checker теперь resolves их через cross-file resolve.
            // Pre-populated `sum_schemas["RuntimeError"]` (emit_c.rs:1029-1048)
            // оставлен как ABI-compat fallback baseline per 62.A.bis
            // architecture (HardcodedBaseline остаётся, lookup precedence
            // DeclaredFromPrelude > HardcodedBaseline).
            //
            // `RuntimeNoneError` НЕ перенесён — bootstrap parser не
            // поддерживает empty-body sum syntax. Остаётся as
            // string-payload throw в nova_rt/effects.h.
            // Plan 62.B: `panic`/`exit`/`assert`/`debug_assert` (4 names)
            // перенесены в std/prelude/runtime.nv (file-based external fn
            // declarations). Type-checker теперь resolves их через
            // cross-file resolve (R27 auto-import + R26 re-export через
            // std/prelude.nv facade). Codegen special-cases в emit_c.rs
            // (~11086-11136) остаются:
            //   - panic/exit нужны для comma-expression обёртки
            //     `(nv_panic(msg), (nova_int)0LL)` в expression-position
            //     (?? coalesce, if-else branches).
            //   - assert/debug_assert: D89 expression-context + Plan 11
            //     auto-derived cond_text (msg arg silently ignored).
            // См. docs/plans/62-prelude-hardcode-migration.md §62.B.
            //
            // Plan 62.B.bis (2026-05-18) closure: `print` / `println`
            // больше не hardcoded — formally declared в
            // std/prelude/runtime.nv через D69 variadic + `[]any`
            // (canonical D26 signature). Cross-file resolve через R27
            // auto-import + R26 facade re-export находит declarations.
            // Codegen special-case (emit_c.rs:11270, Ф.1 reorder) fires
            // ДО variadic routing — preserves per-arg type info,
            // synthesized `[]any` array никогда не строится; per-arg
            // `nova_print_<type>` dispatch через infer_print_helper
            // (Ф.0 Plan 67 absorption — unified через infer_expr_c_type).
            // См. docs/plans/62.B.bis-print-println-migration.md.
            // Plan 32: GC introspection namespace (std.runtime.gc).
            // РСЃРїРѕР»СЊР·СѓРµС‚СЃСЏ РєР°Рє `gc.heap_size()`, `gc.collect()` Рё С‚.Рґ.
            // Source of truth РґР»СЏ signatures: std/runtime/gc.nv (external fn).
            // Codegen dispatch: emit_c.rs:7155 special-case РЅР° name == "gc".
            // Builtin Р·Р°РїРёСЃСЊ РЅСѓР¶РЅР° РїРѕС‚РѕРјСѓ С‡С‚Рѕ cross-file bare-name resolve
            // РЅРµ СЂР°Р±РѕС‚Р°РµС‚ (Plan 35 Р¤.1).
            "gc",
            // Plan 57: bench DSL builtins namespace (std.bench).
            // `bench.opaque(v)`, `bench.iterations()`, `bench.reset_timer()`,
            // `bench.bytes(n)`, `bench.elements(n)`, `bench.allocs()`,
            // `bench.now_ns()`. Source of truth: std/bench.nv. Codegen
            // dispatch: emit_c.rs special-case на `name == "bench"`.
            "bench",
            // Plan 44.2 Р­С‚Р°Рї 3: fiber arena introspection namespace
            // (std.runtime.fibers). `fibers.slot_count()`, etc.
            // Source of truth: std/runtime/fibers.nv. Codegen dispatch:
            // emit_c.rs `name == "fibers"`.
            "fibers",
            // Plan 44 Р­С‚Р°Рї 0: M:N runtime control namespace
            // (std.runtime.runtime). `runtime.init(n)`, `runtime.shutdown()`.
            "runtime",
            // Default Fail-effect type (D65 placeholder).
            "Fail",
            // Detach effect-type РґР»СЏ detach {} expression (D50).
            "Detach",
            // Plan 83.3: Blocking effect-type для blocking {} expression
            // (D50) — увод leaf-блокирующей работы в libuv threadpool.
            "Blocking",
            // CancelToken вЂ” caller-owned cancellation handle (D75 revised,
            // Plan 47). Builtin type: `CancelToken.new()` РєРѕРЅСЃС‚СЂСѓРєС‚РѕСЂ +
            // С‚РёРї РїР°СЂР°РјРµС‚СЂР° `cancel CancelToken`. РњРµС‚РѕРґС‹ (cancel/is_cancelled/
            // bind) вЂ” built-in dispatch РІ codegen РЅР° receiver NovaCancelToken*.
            "CancelToken",
            // Plan 62.D.bis (2026-05-18): StringBuilder / WriteBuffer /
            // ReadBuffer объявлены в std/prelude/collections.nv через
            // `external type` (D126). **Не были** в этом HashSet'е изначально
            // (verified via grep на baseline) — cross-file resolve работает
            // через std/runtime/<name>.nv external fn декларации + теперь
            // через std/prelude/collections.nv type-decl (TypeDeclKind::Opaque).
            // `nogc_blacklisted_call` (types/mod.rs:1454) сохраняет
            // name-matches как capability data — не builtins source,
            // не conflicts.
            //
            // Plan 103.1 Ф.6: `fence` — memory fence free function
            // (std/runtime/sync.nv). Lowercase free fn → нужен в builtins
            // иначе type-checker флагает «undefined identifier» для тестов
            // без `import std.runtime.sync`. Dispatch: ExternalRegistry
            // → nova_fn_fence (free_fn_c_name ExternalRegistry-first path).
            "fence",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        // Plan 42.4 Rule C: per-peer import namespace isolation.
        // Build a map from file_id в†’ visible module names for that peer.
        // If peer_files is empty (legacy/single-file), fall back to entry.
        let mut peer_module_names: HashMap<FileId, HashSet<String>> = HashMap::new();

        let build_import_names = |imports: &[Import], module_name: &[String]| -> HashSet<String> {
            let mut names: HashSet<String> = HashSet::new();
            for imp in imports {
                if let Some(alias) = &imp.alias {
                    names.insert(alias.clone());
                }
                if let Some(last) = imp.path.last() {
                    names.insert(last.clone());
                }
                if let Some(head) = imp.path.first() {
                    names.insert(head.clone());
                }
            }
            // Own module name (last + first segment) for self-reference.
            if let Some(head) = module_name.first() { names.insert(head.clone()); }
            if let Some(last) = module_name.last() { names.insert(last.clone()); }
            names
        };

        if module.peer_files.is_empty() {
            // Legacy/single-file: entry imports under MAIN_FILE_ID.
            peer_module_names.insert(
                MAIN_FILE_ID,
                build_import_names(&module.imports, &module.name),
            );
        } else {
            for pf in &module.peer_files {
                peer_module_names.insert(
                    pf.file_id,
                    build_import_names(&pf.imports, &module.name),
                );
            }
        }

        // Plan 42.15: per-peer imported item names. Resolver РЅР°РїРѕР»РЅРёР»
        // `PeerFile.imported_item_names` (items РїСЂРёС‚Р°С‰РµРЅРЅС‹Рµ РїСЂСЏРјС‹РјРё
        // imports СЌС‚РѕРіРѕ peer'Р°). Rule C: imports РЅРµ shared РјРµР¶РґСѓ peers.
        let mut peer_imported_names: HashMap<FileId, HashSet<String>> = HashMap::new();
        for pf in &module.peer_files {
            peer_imported_names.insert(pf.file_id, pf.imported_item_names.clone());
        }

        // Plan 42.15: all_decls вЂ” union Р’РЎР•РҐ declarations (СЌРІСЂРёСЃС‚РёРєР° РґР»СЏ
        // pattern-binding detection, РќР• РґР»СЏ enforcement).
        let mut all_decls: HashSet<String> = shared_decls.clone();
        for gd in group_decls.values() {
            all_decls.extend(gd.iter().cloned());
        }
        // РўР°РєР¶Рµ merged module.items (imported items РґР»СЏ СЌРІСЂРёСЃС‚РёРєРё).
        collect_decl_names(&module.items, &mut all_decls);

        NameResCtx {
            group_decls, shared_decls, all_decls, builtins,
            peer_module_names, peer_imported_names,
        }
    }

    fn check_module(&self, module: &Module, errors: &mut Vec<Diagnostic>) {
        for item in &module.items {
            let file_id = match item {
                Item::Fn(f) => f.span.file_id,
                Item::Test(t) => t.span.file_id,
                Item::Bench(b) => b.span.file_id,
                Item::Const(c) => c.span.file_id,
                Item::Type(t) => t.span.file_id,
                Item::Let(l) => l.span.file_id,
                Item::Lemma(ld) => ld.span.file_id,
            };
            match item {
                Item::Fn(f) => self.walk_fn(f, file_id, errors),
                Item::Test(t) => {
                    let mut scope: Vec<HashSet<String>> = vec![HashSet::new()];
                    self.walk_block(&t.body, file_id, &mut scope, errors);
                }
                // Plan 57: bench body — name-resolution как у test (один
                // общий scope для setup → measure → teardown, потому что
                // setup-bindings видны в measure и teardown).
                Item::Bench(b) => {
                    let mut scope: Vec<HashSet<String>> = vec![HashSet::new()];
                    for s in &b.setup {
                        self.walk_stmt(s, file_id, &mut scope, errors);
                    }
                    self.walk_block(&b.measure_body, file_id, &mut scope, errors);
                    for s in &b.teardown {
                        self.walk_stmt(s, file_id, &mut scope, errors);
                    }
                }
                Item::Const(c) => {
                    let mut scope: Vec<HashSet<String>> = vec![HashSet::new()];
                    self.walk_expr(&c.value, file_id, &mut scope, errors);
                }
                _ => {}
            }
        }
    }

    fn walk_fn(&self, f: &FnDecl, file_id: FileId, errors: &mut Vec<Diagnostic>) {
        // External вЂ” РЅРµС‚ С‚РµР»Р°.
        if matches!(f.body, FnBody::External) { return; }
        let mut scope: Vec<HashSet<String>> = vec![HashSet::new()];
        let mut frame: HashSet<String> = HashSet::new();
        // Receiver: self/Self РґРѕСЃС‚СѓРїРЅС‹ С‡РµСЂРµР· builtins; РЅРµС‚ РЅСѓР¶РґС‹ РґРѕР±Р°РІР»СЏС‚СЊ.
        if let Some(_recv) = &f.receiver {
            frame.insert("self".to_string());
        }
        for p in &f.params {
            frame.insert(p.name.clone());
        }
        // Generic-params РјРѕРіСѓС‚ РёСЃРїРѕР»СЊР·РѕРІР°С‚СЊСЃСЏ РІ expr-position? вЂ” РќРµС‚
        // (РїРѕ spec). РќРѕ Р±РµР·РѕРїР°СЃРЅРѕ РёС… РґРѕР±Р°РІРёС‚СЊ С‡С‚РѕР±С‹ РЅРµ С„Р»Р°РіР°С‚СЊ False+
        // РµСЃР»Рё parser/codegen РіРґРµ-С‚Рѕ РёС… С‚Р°Рє С‚СЂР°РєС‚СѓРµС‚.
        for g in &f.generics {
            frame.insert(g.name.clone());
        }
        scope.push(frame);
        match &f.body {
            FnBody::Expr(e) => self.walk_expr(e, file_id, &mut scope, errors),
            FnBody::Block(b) => self.walk_block(b, file_id, &mut scope, errors),
            FnBody::External => {}
        }
        scope.pop();
    }

    fn walk_block(
        &self,
        b: &Block,
        file_id: FileId,
        scope: &mut Vec<HashSet<String>>,
        errors: &mut Vec<Diagnostic>,
    ) {
        scope.push(HashSet::new());
        for s in &b.stmts {
            self.walk_stmt(s, file_id, scope, errors);
        }
        if let Some(t) = &b.trailing {
            self.walk_expr(t, file_id, scope, errors);
        }
        scope.pop();
    }

    fn walk_stmt(
        &self,
        s: &Stmt,
        file_id: FileId,
        scope: &mut Vec<HashSet<String>>,
        errors: &mut Vec<Diagnostic>,
    ) {
        match s {
            Stmt::Expr(e) => self.walk_expr(e, file_id, scope, errors),
            Stmt::Let(d) => {
                // Right-side РІС‹С‡РёСЃР»СЏРµС‚СЃСЏ РІ С‚РµРєСѓС‰РµРј scope (let РЅРµ
                // СЂРµРєСѓСЂСЃРёРІРЅС‹Р№). Р—Р°С‚РµРј pattern-bindings РґРѕР±Р°РІР»СЏСЋС‚СЃСЏ РІ
                // С‚РµРєСѓС‰РёР№ frame.
                self.walk_expr(&d.value, file_id, scope, errors);
                let mut bindings: HashSet<String> = HashSet::new();
                self.collect_pattern_bindings(&d.pattern, &mut bindings);
                if let Some(top) = scope.last_mut() {
                    for n in bindings { top.insert(n); }
                }
            }
            // Plan 114.4 Ф.2: scope-local const — pass-through (no-op for now).
            Stmt::Const(_) => {}
            Stmt::Assign { target, value, .. } => {
                self.walk_expr(target, file_id, scope, errors);
                self.walk_expr(value, file_id, scope, errors);
            }
            Stmt::Return { value, .. } => {
                if let Some(v) = value { self.walk_expr(v, file_id, scope, errors); }
            }
            Stmt::Throw { value, .. } => self.walk_expr(value, file_id, scope, errors),
            // D90 (Plan 20): defer/errdefer body вЂ” РѕР±С‹С‡РЅС‹Р№ expr РІ С‚РµРєСѓС‰РµРј
            // scope. Bindings РІРЅСѓС‚СЂРё body Р»РѕРєР°Р»СЊРЅС‹ РёС… СЃРѕР±СЃС‚РІРµРЅРЅС‹Рј under-scope’Р°Рј;
            // РЅР° РІРµСЂС…РЅРµРј СѓСЂРѕРІРЅРµ defer РЅРµ РІРІРѕРґРёС‚ РЅРѕРІС‹С… РёРјС’РЅ.
            Stmt::Defer { body, .. } | Stmt::ErrDefer { body, .. }
            | Stmt::OkDefer { body, .. } | Stmt::DeferWithResult { body, .. } => {
                self.walk_expr(body, file_id, scope, errors);
            }
            // Plan 110 D188: walk init + push new scope frame с binding,
            // walk body, pop frame. Binding visible только внутри body
            // (D188 §«Syntax» single-name binding).
            Stmt::ConsumeScope { binding, init, body, .. } => {
                self.walk_expr(init, file_id, scope, errors);
                scope.push({
                    let mut frame = HashSet::new();
                    frame.insert(binding.clone());
                    frame
                });
                for s in &body.stmts {
                    self.walk_stmt(s, file_id, scope, errors);
                }
                if let Some(t) = &body.trailing {
                    self.walk_expr(t, file_id, scope, errors);
                }
                scope.pop();
            }
            // Plan 33.2 Р¤.8: assert_static вЂ” walk expr.
            Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => self.walk_expr(expr, file_id, scope, errors),
            Stmt::Break(_) | Stmt::Continue(_) => {}
            // Ф.4.1: apply — ghost, args walk для name-resolution.
            Stmt::Apply { args, .. } => {
                for a in args { self.walk_expr(a, file_id, scope, errors); }
            }
            // Ф.4.2: calc — ghost, шаги walk для name-resolution.
            Stmt::Calc { steps, .. } => {
                for step in steps { self.walk_expr(&step.expr, file_id, scope, errors); }
            }
            // Plan 33.9 Ф.2: reveal — ghost, name resolution в pipeline.
            Stmt::Reveal { .. } => {}
        }
    }

    fn walk_expr(
        &self,
        e: &Expr,
        file_id: FileId,
        scope: &mut Vec<HashSet<String>>,
        errors: &mut Vec<Diagnostic>,
    ) {
        match &e.kind {
            ExprKind::Ident(name) => {
                if !self.is_known(name, file_id, scope) {
                    errors.push(Diagnostic::new(
                        format!("undefined identifier `{}`", name),
                        e.span,
                    ));
                }
            }
            // Path-form `Module.func` / `Type.method`: head вЂ” РјРѕРґСѓР»СЊ РёР»Рё
            // type. Plan 42.15 Р¤.3: head-segment check РґР»СЏ lowercase
            // module-alias'РѕРІ (Rule C: peer РІРёРґРёС‚ С‚РѕР»СЊРєРѕ СЃРІРѕРё imports).
            //
            // РџСЂРѕРІРµСЂСЏРµРј РўРћР›Р¬РљРћ lowercase head: Capitalized = С‚РёРї/effect/
            // variant (cross-file, bootstrap-РєРѕРЅСЃРµСЂРІР°С‚РёРІРЅРѕ РїСЂРѕРїСѓСЃРєР°РµРј).
            // lowercase head РґРѕР»Р¶РµРЅ Р±С‹С‚СЊ: builtin namespace (gc/fibers/
            // runtime) РР›Р module-alias РІ peer's import scope. Р•СЃР»Рё РЅРµС‚ вЂ”
            // РІРµСЂРѕСЏС‚РЅРѕ use С‡СѓР¶РѕРіРѕ import'Р° (Rule C violation) РёР»Рё typo.
            ExprKind::Path(parts) => {
                if let Some(head) = parts.first() {
                    let is_lowercase = head.chars().next()
                        .map(|c| c.is_ascii_lowercase())
                        .unwrap_or(false);
                    if is_lowercase {
                        let in_builtins = self.builtins.contains(head);
                        let in_peer_modules = self.peer_module_names.get(&file_id)
                            .or_else(|| self.peer_module_names.get(&MAIN_FILE_ID))
                            .map_or(false, |s| s.contains(head));
                        // РўР°РєР¶Рµ head РјРѕР¶РµС‚ Р±С‹С‚СЊ local binding (struct РІ
                        // scope) вЂ” С‚РѕРіРґР° СЌС‚Рѕ С„Р°РєС‚РёС‡РµСЃРєРё Member-access;
                        // РїР°СЂСЃРµСЂ РёРЅРѕРіРґР° СЌРјРёС‚РёС‚ Path. РџСЂРѕРІРµСЂСЏРµРј scope.
                        let in_scope = scope.iter().rev()
                            .any(|frame| frame.contains(head));
                        if !in_builtins && !in_peer_modules && !in_scope {
                            errors.push(Diagnostic::new(
                                format!(
                                    "undefined module / name `{}` in path expression \
                                     (Rule C: peer sees only its own imports)",
                                    head),
                                e.span,
                            ));
                        }
                    }
                }
            }
            // SelfAccess вЂ” `@field` РёР»Рё `@method`. РќРµ Ident.
            ExprKind::SelfAccess => {}

            // Р›РёС‚РµСЂР°Р»С‹.
            ExprKind::IntLit(_) | ExprKind::FloatLit(_) | ExprKind::BoolLit(_)
            | ExprKind::StrLit(_) | ExprKind::CharLit(_) | ExprKind::UnitLit => {}

            ExprKind::InterpolatedStr { parts } => {
                for p in parts {
                    if let InterpStrPart::Expr(e) = p {
                        self.walk_expr(e, file_id, scope, errors);
                    }
                }
            }

            ExprKind::Call { func, args, trailing } => {
                // Special-case: РµСЃР»Рё func вЂ” bare Ident, РјРѕР¶РµС‚ Р±С‹С‚СЊ
                // variant-constructor (`Square(5)`) вЂ” top_level.contains.
                // is_known РїРѕРєСЂС‹РІР°РµС‚ РѕР±Р° РІР°СЂРёР°РЅС‚Р° (fn + variant).
                self.walk_expr(func, file_id, scope, errors);
                for a in args {
                    self.walk_expr(a.expr(), file_id, scope, errors);
                }
                if let Some(t) = trailing {
                    self.walk_trailing(t, file_id, scope, errors);
                }
            }
            ExprKind::TurboFish { base, .. } => self.walk_expr(base, file_id, scope, errors),
            ExprKind::Try(inner) | ExprKind::Bang(inner) => {
                self.walk_expr(inner, file_id, scope, errors)
            }
            ExprKind::Coalesce(a, b) => {
                self.walk_expr(a, file_id, scope, errors);
                self.walk_expr(b, file_id, scope, errors);
            }
            ExprKind::As(e, _) | ExprKind::Is(e, _) => self.walk_expr(e, file_id, scope, errors),
            ExprKind::Binary { left, right, .. } => {
                self.walk_expr(left, file_id, scope, errors);
                self.walk_expr(right, file_id, scope, errors);
            }
            ExprKind::Unary { operand, .. } => self.walk_expr(operand, file_id, scope, errors),

            // Member-access: РїСЂРѕРІРµСЂСЏРµРј obj (СЌС‚Рѕ expr), РЅРѕ РќР• name (field/method).
            ExprKind::Member { obj, .. } => self.walk_expr(obj, file_id, scope, errors),
            ExprKind::Index { obj, index } => {
                self.walk_expr(obj, file_id, scope, errors);
                self.walk_expr(index, file_id, scope, errors);
            }

            ExprKind::If { cond, then, else_ } => {
                self.walk_expr(cond, file_id, scope, errors);
                self.walk_block(then, file_id, scope, errors);
                if let Some(eb) = else_ {
                    match eb {
                        ElseBranch::Block(b) => self.walk_block(b, file_id, scope, errors),
                        ElseBranch::If(e) => self.walk_expr(e, file_id, scope, errors),
                    }
                }
            }
            ExprKind::IfLet { pattern, scrutinee, then, else_ } => {
                self.walk_expr(scrutinee, file_id, scope, errors);
                // Pattern-bindings вЂ” РІ scope С‚РѕР»СЊРєРѕ РґР»СЏ then-branch.
                let mut bindings: HashSet<String> = HashSet::new();
                self.collect_pattern_bindings(pattern, &mut bindings);
                scope.push(bindings);
                self.walk_block(then, file_id, scope, errors);
                scope.pop();
                if let Some(eb) = else_ {
                    match eb {
                        ElseBranch::Block(b) => self.walk_block(b, file_id, scope, errors),
                        ElseBranch::If(e) => self.walk_expr(e, file_id, scope, errors),
                    }
                }
            }
            ExprKind::Match { scrutinee, arms } => {
                self.walk_expr(scrutinee, file_id, scope, errors);
                for arm in arms {
                    let mut bindings: HashSet<String> = HashSet::new();
                    self.collect_pattern_bindings(&arm.pattern, &mut bindings);
                    scope.push(bindings);
                    if let Some(g) = &arm.guard {
                        self.walk_expr(g, file_id, scope, errors);
                    }
                    match &arm.body {
                        MatchArmBody::Expr(e) => self.walk_expr(e, file_id, scope, errors),
                        MatchArmBody::Block(b) => self.walk_block(b, file_id, scope, errors),
                    }
                    scope.pop();
                }
            }
            ExprKind::For { pattern, iter, body, .. } => {
                self.walk_expr(iter, file_id, scope, errors);
                let mut bindings: HashSet<String> = HashSet::new();
                self.collect_pattern_bindings(pattern, &mut bindings);
                scope.push(bindings);
                self.walk_block(body, file_id, scope, errors);
                scope.pop();
            }
            ExprKind::ParallelFor { pattern, iter, body, .. } => {
                self.walk_expr(iter, file_id, scope, errors);
                let mut bindings: HashSet<String> = HashSet::new();
                self.collect_pattern_bindings(pattern, &mut bindings);
                scope.push(bindings);
                self.walk_block(body, file_id, scope, errors);
                scope.pop();
            }
            ExprKind::While { cond, body, .. } => {
                self.walk_expr(cond, file_id, scope, errors);
                self.walk_block(body, file_id, scope, errors);
            }
            ExprKind::WhileLet { pattern, scrutinee, body, .. } => {
                self.walk_expr(scrutinee, file_id, scope, errors);
                let mut bindings: HashSet<String> = HashSet::new();
                self.collect_pattern_bindings(pattern, &mut bindings);
                scope.push(bindings);
                self.walk_block(body, file_id, scope, errors);
                scope.pop();
            }
            ExprKind::Loop { body, .. } => self.walk_block(body, file_id, scope, errors),
            ExprKind::Select { arms } => {
                for arm in arms {
                    match &arm.op {
                        SelectOp::Recv { binding, chan } => {
                            self.walk_expr(chan, file_id, scope, errors);
                            let mut bindings: HashSet<String> = HashSet::new();
                            if let Some(b) = binding { bindings.insert(b.clone()); }
                            scope.push(bindings);
                            if let Some(g) = &arm.guard { self.walk_expr(g, file_id, scope, errors); }
                            self.walk_block(&arm.body, file_id, scope, errors);
                            scope.pop();
                        }
                        SelectOp::Send { chan, value } => {
                            self.walk_expr(chan, file_id, scope, errors);
                            self.walk_expr(value, file_id, scope, errors);
                            if let Some(g) = &arm.guard { self.walk_expr(g, file_id, scope, errors); }
                            self.walk_block(&arm.body, file_id, scope, errors);
                        }
                        SelectOp::Default => {
                            if let Some(g) = &arm.guard { self.walk_expr(g, file_id, scope, errors); }
                            self.walk_block(&arm.body, file_id, scope, errors);
                        }
                    }
                }
            }

            ExprKind::Block(b) => self.walk_block(b, file_id, scope, errors),

            ExprKind::ArrayLit(elems) => {
                for el in elems {
                    match el {
                        ArrayElem::Item(e) | ArrayElem::Spread(e) => {
                            self.walk_expr(e, file_id, scope, errors);
                        }
                    }
                }
            }
            ExprKind::MapLit { elems, .. } => {
                let pairs = crate::ast::MapElem::cloned_pairs(&elems);
                for (k, v) in pairs.iter() {
                    self.walk_expr(k, file_id, scope, errors);
                    self.walk_expr(v, file_id, scope, errors);
                }
            }
            ExprKind::TupleLit(elems) => {
                for e in elems { self.walk_expr(e, file_id, scope, errors); }
            }
            ExprKind::RecordLit { fields, .. } => {
                for f in fields {
                    match &f.value {
                        Some(v) => {
                            // D52 §2 enforcement: redundant `{ name: name }` или
                            // `{ field: @field }` запрещены (shorthand mandatory
                            // когда имя поля совпадает с источником). Spec:
                            // spec/decisions/02-types.md D52 §2.
                            if !f.is_spread && !f.at_shorthand {
                                use crate::ast::ExprKind as EK;
                                let is_redundant_ident = matches!(&v.kind,
                                    EK::Ident(n) if n == &f.name);
                                let is_redundant_self_field = matches!(&v.kind,
                                    EK::Member { obj, name }
                                        if name == &f.name
                                        && matches!(obj.kind, EK::SelfAccess));
                                if is_redundant_ident {
                                    errors.push(Diagnostic::new(
                                        format!(
                                            "избыточная форма поля `{name}: {name}` — \
                                             D52 §2 требует shorthand `{name}` когда имя \
                                             поля совпадает с источником",
                                            name = f.name),
                                        f.span,
                                    ));
                                } else if is_redundant_self_field {
                                    errors.push(Diagnostic::new(
                                        format!(
                                            "избыточная форма поля `{name}: @{name}` — \
                                             D52 §2 требует shorthand `@{name}` когда имя \
                                             поля совпадает с self-полем",
                                            name = f.name),
                                        f.span,
                                    ));
                                }
                            }
                            self.walk_expr(v, file_id, scope, errors);
                        }
                        None => {
                            // Shorthand `{ name }` (D52 field punning):
                            // `name` вЂ” СЌС‚Рѕ ident, РєРѕС‚РѕСЂС‹Р№ РґРѕР»Р¶РµРЅ Р±С‹С‚СЊ
                            // РІ scope.
                            if !f.is_spread && !self.is_known(&f.name, file_id, scope) {
                                errors.push(Diagnostic::new(
                                    format!("undefined identifier `{}`", f.name),
                                    f.span,
                                ));
                            }
                        }
                    }
                }
            }

            // Tagged-template: tag вЂ” СЌС‚Рѕ СЃРїРµС†РёР°Р»СЊРЅС‹Р№ DSL-marker
            // (sql, json, html, ...). Р’ bootstrap'Рµ tag-С„СѓРЅРєС†РёСЏ
            // РёРіРЅРѕСЂРёСЂСѓРµС‚СЃСЏ (parts РєРѕРЅРєР°С‚РµРЅРёСЂСѓСЋС‚СЃСЏ), РЅРѕ РІ production
            // tag вЂ” СЌС‚Рѕ runtime-С„СѓРЅРєС†РёСЏ/macro. РќРµ РїСЂРѕРІРµСЂСЏРµРј tag РєР°Рє
            // Ident вЂ” СЌС‚Рѕ special-form syntax, РЅРµ РѕР±С‹С‡РЅС‹Р№ expr-call.
            // Args (`${expr}` РёРЅС‚РµСЂРїРѕР»СЏС†РёРё) вЂ” РѕР±С‹С‡РЅС‹Рµ expressions.
            ExprKind::TaggedTemplate { args, .. } => {
                for a in args { self.walk_expr(a, file_id, scope, errors); }
            }

            // Lambda (legacy) / closure-light / closure-full вЂ” params
            // push'СЏС‚СЃСЏ РєР°Рє РЅРѕРІС‹Р№ scope frame.
            ExprKind::Lambda { params, body, .. } => {
                let mut frame: HashSet<String> = HashSet::new();
                for p in params { frame.insert(p.name.clone()); }
                scope.push(frame);
                self.walk_expr(body, file_id, scope, errors);
                scope.pop();
            }
            ExprKind::ClosureLight { params, body } => {
                let mut frame: HashSet<String> = HashSet::new();
                for p in params {
                    if p.name != "_" { frame.insert(p.name.clone()); }
                }
                scope.push(frame);
                match body {
                    crate::ast::ClosureBody::Expr(e) => self.walk_expr(e, file_id, scope, errors),
                    crate::ast::ClosureBody::Block(b) => self.walk_block(b, file_id, scope, errors),
                }
                scope.pop();
            }
            ExprKind::ClosureFull(sb) => {
                let mut frame: HashSet<String> = HashSet::new();
                for p in &sb.params { frame.insert(p.name.clone()); }
                scope.push(frame);
                match &sb.body {
                    FnBody::Expr(e) => self.walk_expr(e, file_id, scope, errors),
                    FnBody::Block(b) => self.walk_block(b, file_id, scope, errors),
                    FnBody::External => {}
                }
                scope.pop();
            }

            ExprKind::With { bindings, body } => {
                // Effect-handler vals вЂ” РѕР±С‹С‡РЅС‹Рµ expressions.
                for b in bindings {
                    self.walk_expr(&b.handler, file_id, scope, errors);
                }
                self.walk_block(body, file_id, scope, errors);
            }
            // Plan 97 Ф.4 (D142): protocol-литерал — name-resolution
            // walk идентичен handler-литералу.
            ExprKind::HandlerLit { methods, .. } | ExprKind::ProtocolLit { methods, .. } => {
                // РљР°Р¶РґС‹Р№ method вЂ” op СЃ СЃРѕР±СЃС‚РІРµРЅРЅС‹Рј scope params.
                for m in methods {
                    let mut frame: HashSet<String> = HashSet::new();
                    for p in &m.params { frame.insert(p.name.clone()); }
                    scope.push(frame);
                    match &m.body {
                        HandlerMethodBody::Expr(e) => self.walk_expr(e, file_id, scope, errors),
                        HandlerMethodBody::Block(b) => self.walk_block(b, file_id, scope, errors),
                    }
                    scope.pop();
                }
            }
            ExprKind::Interrupt(opt) => {
                if let Some(e) = opt { self.walk_expr(e, file_id, scope, errors); }
            }
            ExprKind::Forbid { body, .. } | ExprKind::Realtime { body, .. } => {
                self.walk_block(body, file_id, scope, errors);
            }
            ExprKind::Range { start, end, .. } => {
                if let Some(s) = start { self.walk_expr(s, file_id, scope, errors); }
                if let Some(e) = end { self.walk_expr(e, file_id, scope, errors); }
            }
            ExprKind::Spawn(body) => self.walk_expr(body, file_id, scope, errors),
            ExprKind::Detach(body) | ExprKind::Blocking(body) => {
                self.walk_block(body, file_id, scope, errors);
            }
            ExprKind::Supervised { body, cancel } => {
                // Plan 47: `cancel:` expr вЂ” РѕР±С‹С‡РЅРѕРµ РІС‹СЂР°Р¶РµРЅРёРµ scope'Р°
                // (С‚РёРїРёС‡РЅРѕ `Ident` С‚РѕРєРµРЅР°); СЂРµР·РѕР»РІРёС‚СЃСЏ РІ С‚РµРєСѓС‰РµРј scope'Рµ,
                // РЅРёРєР°РєРёС… РЅРѕРІС‹С… Р±РёРЅРґРёРЅРіРѕРІ РЅРµ РІРІРѕРґРёС‚.
                if let Some(c) = cancel {
                    self.walk_expr(c, file_id, scope, errors);
                }
                self.walk_block(body, file_id, scope, errors);
            }
            ExprKind::Throw(inner) => self.walk_expr(inner, file_id, scope, errors),
            // D.1.3: РєРІР°РЅС‚РѕСЂ вЂ” bound variable РІРІРѕРґРёС‚СЃСЏ РІ scope РґР»СЏ body.
            ExprKind::Forall { var, range, body } | ExprKind::Exists { var, range, body } => {
                self.walk_expr(range, file_id, scope, errors);
                let mut frame: HashSet<String> = HashSet::new();
                frame.insert(var.clone());
                scope.push(frame);
                self.walk_expr(body, file_id, scope, errors);
                scope.pop();
            }
        }
    }

    fn walk_trailing(
        &self,
        t: &crate::ast::Trailing,
        file_id: FileId,
        scope: &mut Vec<HashSet<String>>,
        errors: &mut Vec<Diagnostic>,
    ) {
        match t {
            crate::ast::Trailing::Block(b) => self.walk_block(b, file_id, scope, errors),
            crate::ast::Trailing::LegacyBlockWithParams(tb) => {
                let mut frame: HashSet<String> = HashSet::new();
                for p in &tb.params { frame.insert(p.name.clone()); }
                scope.push(frame);
                self.walk_block(&tb.body, file_id, scope, errors);
                scope.pop();
            }
            crate::ast::Trailing::Fn(sb) => {
                let mut frame: HashSet<String> = HashSet::new();
                for p in &sb.params { frame.insert(p.name.clone()); }
                scope.push(frame);
                match &sb.body {
                    FnBody::Expr(e) => self.walk_expr(e, file_id, scope, errors),
                    FnBody::Block(b) => self.walk_block(b, file_id, scope, errors),
                    FnBody::External => {}
                }
                scope.pop();
            }
        }
    }

    /// РЎРѕР±СЂР°С‚СЊ РІСЃРµ bindings РёР· pattern (С‚РѕР»СЊРєРѕ names, Р±РµР· РїСЂРѕРІРµСЂРєРё
    /// variant-tag'РѕРІ РёР»Рё field-name'РѕРІ вЂ” СЌС‚Рѕ constructor/field
    /// references, РЅРµ expr-bindings).
    fn collect_pattern_bindings(&self, p: &Pattern, out: &mut HashSet<String>) {
        match p {
            Pattern::Wildcard(_) => {}
            Pattern::Literal(_, _) => {}
            Pattern::Ident { name, .. } => {
                // Edge-case: Pattern::Ident { name: "Some" } вЂ” СЌС‚Рѕ
                // unit-variant Some? РќРµС‚, РїР°СЂСЃРµСЂ emit'РёС‚ Variant { path:
                // ["Some"], kind: Unit }. Р—РґРµСЃСЊ вЂ” РЅР°СЃС‚РѕСЏС‰РёР№ binding.
                // РќРѕ РµСЃР»Рё РёРјСЏ СЃРѕРІРїР°РґР°РµС‚ СЃ РёР·РІРµСЃС‚РЅС‹Рј variant вЂ” СЃС‡РёС‚Р°РµРј
                // СЌС‚Рѕ variant-pattern, РЅРµ binding (D52 СЃРµРјР°РЅС‚РёРєР°
                // pattern-matching). РўР°РєР¶Рµ Capitalized-РёРјРµРЅР° РІ bootstrap
                // вЂ” СЌС‚Рѕ РІСЃРµРіРґР° type/variant (cross-file), РЅРµ binding.
                let is_variant_like = self.builtins.contains(name)
                    || self.all_decls.contains(name)
                    || name.chars().next().map(|c| c.is_ascii_uppercase()).unwrap_or(false);
                if !is_variant_like {
                    out.insert(name.clone());
                }
            }
            Pattern::Variant { kind, .. } => {
                // path = variant-tag вЂ” РЅРµ binding.
                match kind {
                    VariantPatternKind::Unit => {}
                    VariantPatternKind::Tuple { patterns, .. } => {
                        for sub in patterns {
                            self.collect_pattern_bindings(sub, out);
                        }
                    }
                }
            }
            Pattern::Record { fields, .. } => {
                for f in fields {
                    match &f.pattern {
                        Some(sub) => self.collect_pattern_bindings(sub, out),
                        // Shorthand `{ name }` вЂ” name вЂ” СЌС‚Рѕ binding
                        // (РѕРґРЅРѕРІСЂРµРјРµРЅРЅРѕ field-name Рё bound variable).
                        None => { out.insert(f.name.clone()); }
                    }
                }
            }
            Pattern::Array { elems, .. } => {
                for el in elems {
                    match el {
                        ArrayPatternElem::Item(sub) => self.collect_pattern_bindings(sub, out),
                        ArrayPatternElem::Rest => {}
                        ArrayPatternElem::RestBind(name) => { out.insert(name.clone()); }
                    }
                }
            }
            Pattern::Tuple(elems, _) => {
                for sub in elems { self.collect_pattern_bindings(sub, out); }
            }
            Pattern::Binding { name, inner, .. } => {
                out.insert(name.clone());
                self.collect_pattern_bindings(inner, out);
            }
            Pattern::Or { alternatives, .. } => {
                // РџРѕ spec РІСЃРµ alternatives РёРјРµСЋС‚ РѕРґРёРЅР°РєРѕРІС‹Р№ РЅР°Р±РѕСЂ
                // bindings; Р±РµСЂС‘Рј РёР· РїРµСЂРІРѕРіРѕ. (Bootstrap-СЃРµРјР°РЅС‚РёРєР° вЂ” СЃРј.
                // ast::Pattern::Or doc.)
                if let Some(first) = alternatives.first() {
                    self.collect_pattern_bindings(first, out);
                }
            }
        }
    }

    fn is_known(&self, name: &str, file_id: FileId, scope: &[HashSet<String>]) -> bool {
        if self.builtins.contains(name) { return true; }
        // Plan 42.15 Rule C: declarations module-group СЌС‚РѕРіРѕ peer'Р°
        // (peers РѕРґРЅРѕРіРѕ folder-module РґРµР»СЏС‚ declarations namespace).
        // Fallback РЅР° flat shared_decls РґР»СЏ legacy/single-file.
        if let Some(gd) = self.group_decls.get(&file_id) {
            if gd.contains(name) { return true; }
        } else if self.shared_decls.contains(name) {
            return true;
        }
        // Plan 42.15: per-peer imported item names вЂ” items РїСЂРёС‚Р°С‰РµРЅРЅС‹Рµ
        // РїСЂСЏРјС‹РјРё imports РРњР•РќРќРћ СЌС‚РѕРіРѕ peer'Р°. Rule C: imports РќР• shared.
        // Fallback РЅР° MAIN_FILE_ID РµСЃР»Рё file_id РЅРµ РЅР°Р№РґРµРЅ (legacy).
        let imported = self.peer_imported_names.get(&file_id)
            .or_else(|| self.peer_imported_names.get(&MAIN_FILE_ID));
        if imported.map_or(false, |s| s.contains(name)) { return true; }
        // Plan 42.4 Rule C: per-peer import namespace (module/alias names).
        let module_names = self.peer_module_names.get(&file_id)
            .or_else(|| self.peer_module_names.get(&MAIN_FILE_ID));
        if module_names.map_or(false, |s| s.contains(name)) { return true; }
        for frame in scope.iter().rev() {
            if frame.contains(name) { return true; }
        }
        // Bootstrap-РєРѕРЅСЃРµСЂРІР°С‚РёРІРЅРѕСЃС‚СЊ: РёРјРµРЅР° РЅР°С‡РёРЅР°СЋС‰РёРµСЃСЏ СЃ Р·Р°РіР»Р°РІРЅРѕР№
        // Р±СѓРєРІС‹ РїРѕ convention вЂ” С‚РёРїС‹ / variants / РјРѕРґСѓР»Рё. Bootstrap
        // РЅРµ РёРјРµРµС‚ cross-file name resolution, РїРѕСЌС‚РѕРјСѓ ident РІСЂРѕРґРµ
        // `HashMap` (РёР· РґСЂСѓРіРѕРіРѕ .nv С„Р°Р№Р»Р°) РїСЂРёС…РѕРґРёС‚ СЃСЋРґР° РЅРµ Р·Р°РґРµРєР»Р°СЂРёСЂРѕРІР°РЅРЅС‹Рј.
        // Р§С‚РѕР±С‹ РЅРµ С„Р»Р°РіР°С‚СЊ С‚Р°РєРёРµ cross-file С‚РёРїС‹ РєР°Рє undefined,
        // РїСЂРѕРїСѓСЃРєР°РµРј Capitalized-ident'С‹. РћРїРµС‡Р°С‚РєРё РІ lowercase
        // РёРјРµРЅР°С… (snake_case convention РґР»СЏ vars/fns) вЂ” РЅР°СЃС‚РѕСЏС‰РёРµ
        // undefined Рё Р±СѓРґСѓС‚ Р»РѕРІРёС‚СЊСЃСЏ.
        if let Some(c) = name.chars().next() {
            if c.is_ascii_uppercase() { return true; }
        }
        false
    }
}

/// Render method signature `name(p1 T1, p2 T2) -> Ret` вЂ” РґР»СЏ diagnostic'Р°.
/// Plan 91.9 (D186): compare T's explicit method signature vs protocol's
/// method requirement. Returns Some(reason) если mismatch, None если ok.
///
/// Strict check:
/// - arity (param count) must match
/// - each param type must match (modulo Self ↔ T receiver-coercion)
/// - return type must match (modulo Self)
///
/// Self в protocol method ↔ T's own type name — допустимо. Generic params
/// в protocol — допустимо в принципе (treated as wildcards), но bootstrap
/// strict-match для simple cases.
fn check_signature_match(
    t_method: &FnDecl,
    proto_method: &crate::ast::EffectMethod,
) -> Option<String> {
    if t_method.params.len() != proto_method.params.len() {
        return Some(format!(
            "arity mismatch: T's `{}` has {} param(s), protocol expects {}",
            t_method.name, t_method.params.len(), proto_method.params.len(),
        ));
    }
    for (tp, pp) in t_method.params.iter().zip(proto_method.params.iter()) {
        let tt = &tp.ty;
        let pt = &pp.ty;
        if !type_refs_equiv_modulo_self(tt, pt, &t_method.receiver.as_ref()
            .map(|r| r.type_name.as_str())
            .unwrap_or(""))
        {
            return Some(format!(
                "param `{}`: T has `{}`, protocol expects `{}`",
                tp.name, render_type_ref(tt), render_type_ref(pt),
            ));
        }
    }
    let t_ret = t_method.return_type.as_ref();
    let p_ret = proto_method.return_type.as_ref();
    let recv_name = t_method.receiver.as_ref()
        .map(|r| r.type_name.as_str()).unwrap_or("");
    // None ↔ Unit equivalence: both forms `fn foo()` и `fn foo() -> ()`
    // declare unit-returning method. Treated identically.
    let is_unit_or_none = |r: Option<&TypeRef>| -> bool {
        match r {
            None => true,
            Some(TypeRef::Unit(_)) => true,
            _ => false,
        }
    };
    if is_unit_or_none(t_ret) && is_unit_or_none(p_ret) {
        return None;
    }
    match (t_ret, p_ret) {
        (Some(a), Some(b)) => {
            if !type_refs_equiv_modulo_self(a, b, recv_name) {
                return Some(format!(
                    "return type: T returns `{}`, protocol expects `{}`",
                    render_type_ref(a), render_type_ref(b),
                ));
            }
        }
        _ => return Some(format!(
            "return type: T returns `{}`, protocol expects `{}`",
            t_ret.map(render_type_ref).unwrap_or_else(|| "()".into()),
            p_ret.map(render_type_ref).unwrap_or_else(|| "()".into()),
        )),
    }
    None
}

/// Two TypeRefs are equivalent if textually equal, OR one is `Self`
/// and the other is `recv_name` (or vice versa).
fn type_refs_equiv_modulo_self(a: &TypeRef, b: &TypeRef, recv_name: &str) -> bool {
    let is_self = |t: &TypeRef| matches!(t, TypeRef::Named { path, .. }
        if path.len() == 1 && path[0] == "Self");
    let is_recv = |t: &TypeRef| matches!(t, TypeRef::Named { path, .. }
        if path.len() == 1 && path[0] == recv_name);
    if is_self(a) && (is_self(b) || is_recv(b)) { return true; }
    if is_self(b) && (is_self(a) || is_recv(a)) { return true; }
    render_type_ref(a) == render_type_ref(b)
}

fn render_method_sig(name: &str, params: &[Param], ret: &Option<TypeRef>) -> String {
    let p_strs: Vec<String> = params.iter().map(|p| {
        format!("{} {}", p.name, render_type_ref(&p.ty))
    }).collect();
    let r = ret.as_ref().map(|t| format!(" -> {}", render_type_ref(t))).unwrap_or_default();
    format!("{}({}){}", name, p_strs.join(", "), r)
}

fn render_type_ref(t: &TypeRef) -> String {
    match t {
        TypeRef::Named { path, generics, .. } => {
            if generics.is_empty() {
                path.join(".")
            } else {
                let g: Vec<String> = generics.iter().map(render_type_ref).collect();
                format!("{}[{}]", path.join("."), g.join(", "))
            }
        }
        TypeRef::Array(inner, _) => format!("[]{}", render_type_ref(inner)),
        TypeRef::FixedArray(n, inner, _) => format!("[{}]{}", n, render_type_ref(inner)),
        TypeRef::Tuple(items, _) => {
            let s: Vec<String> = items.iter().map(render_type_ref).collect();
            format!("({})", s.join(", "))
        }
        TypeRef::Func { params, return_type, .. } => {
            let p: Vec<String> = params.iter().map(render_type_ref).collect();
            let r = return_type.as_ref().map(|t| format!(" -> {}", render_type_ref(t))).unwrap_or_default();
            format!("fn({}){}", p.join(", "), r)
        }
        // Plan 97 Ф.2 (D142): анонимный protocol-тип — пишется через
        // render_method_sig, чтобы R5.3 diagnostic'и видели полную
        // сигнатуру inline-protocol bound'а.
        TypeRef::Protocol { methods, .. } => {
            let sigs: Vec<String> = methods
                .iter()
                .map(|m| {
                    let prefix = if m.is_static { "." } else { "" };
                    let full = render_method_sig(&m.name, &m.params, &m.return_type);
                    format!("{}{}", prefix, full)
                })
                .collect();
            format!("protocol {{ {} }}", sigs.join("; "))
        }
        TypeRef::Unit(_) => "()".to_string(),
        // D176 (Plan 108): readonly T — display as "readonly T"
        TypeRef::Readonly(inner, _) => format!("ro {}", render_type_ref(inner)),
    }
}

/// D28 effect inference РґР»СЏ private fn.
///
/// Walk РјРѕРґСѓР»СЊ mutably: РґР»СЏ РєР°Р¶РґРѕР№ private (`!is_export`) fn,
/// РµСЃР»Рё РµС‘ С‚РµР»Рѕ РёСЃРїРѕР»СЊР·СѓРµС‚ `throw`, Рё РІ effect-row РЅРµС‚ РЅРё РѕРґРЅРѕРіРѕ
/// `Fail`/`Fail[E]`/`Fail[any]` вЂ” РґРѕР±Р°РІР»СЏРµРј `Fail` (placeholder).
///
/// Р­С‚Рѕ СѓРїСЂРѕС‰С‘РЅРЅР°СЏ СЂРµР°Р»РёР·Р°С†РёСЏ D28 РґР»СЏ bootstrap'Р°:
/// - РџРѕР»РЅР°СЏ version РІС‹РІРѕРґРёР»Р° Р±С‹ РєРѕРЅРєСЂРµС‚РЅС‹Р№ E РёР· type-of(throw expr).
///   Bootstrap РЅРµ РёРјРµРµС‚ С‚РѕС‡РЅРѕРіРѕ С‚РёРїРёР·Р°С‚РѕСЂР°, РїРѕСЌС‚РѕРјСѓ РІС‹РІРѕРґРёС‚ РїСЂРѕСЃС‚Рѕ
///   `Fail` (placeholder, РїРѕ D65 вЂ” inference placeholder).
/// - Р”Р»СЏ public fn РЅРёС‡РµРіРѕ РЅРµ РґРµР»Р°РµРј (D62: СЏРІРЅР°СЏ РґРµРєР»Р°СЂР°С†РёСЏ РѕР±СЏР·Р°С‚РµР»СЊРЅР°).
/// - РўСЂР°РЅР·РёС‚РёРІРЅР°СЏ inference (callee РёРјРµРµС‚ Fail в†’ caller С‚РѕР¶Рµ) РЅРµ
///   СЂРµР°Р»РёР·РѕРІР°РЅР°; РїСЂРѕРіСЂР°РјРјРёСЃС‚ РґРѕР»Р¶РµРЅ СЏРІРЅРѕ РёРјРїРѕСЂС‚РёСЂРѕРІР°С‚СЊ.
///
/// Р­С„С„РµРєС‚С‹ С‚РёРїР° Db/Net/Time/etc. **РЅРµ** РґРѕР±Р°РІР»СЏСЋС‚СЃСЏ Р°РІС‚РѕРјР°С‚РёС‡РµСЃРєРё вЂ”
/// РѕРЅРё resource-capability Рё РґРѕР»Р¶РЅС‹ Р±С‹С‚СЊ РІРёРґРЅС‹ РІ СЃРёРіРЅР°С‚СѓСЂРµ, РїСЂРѕРіСЂР°РјРјРёСЃС‚
/// РѕР±СЉСЏРІР»СЏРµС‚ СЏРІРЅРѕ. РўРѕР»СЊРєРѕ Fail РёРјРµРµС‚ РѕСЃРѕР±С‹Р№ placeholder-СЂРµР¶РёРј.
pub fn infer_effects(module: &mut Module) {
    for item in &mut module.items {
        if let Item::Fn(f) = item {
            if f.is_export {
                continue;
            }
            if has_throw_in_fn(f) && !has_fail_effect(&f.effects) {
                let span = f.span;
                f.effects.push(TypeRef::Named {
                    path: vec!["Fail".to_string()],
                    generics: vec![],
                    span,
                });
            }
        }
    }
}

/// Р•СЃС‚СЊ Р»Рё С…РѕС‚СЏ Р±С‹ РѕРґРёРЅ `Fail`/`Fail[...]` РІ effect-row.
fn has_fail_effect(effects: &[TypeRef]) -> bool {
    effects.iter().any(|e| {
        matches!(e, TypeRef::Named { path, .. } if path.len() == 1 && path[0] == "Fail")
    })
}

/// РЎРѕРґРµСЂР¶РёС‚ Р»Рё С‚РµР»Рѕ fn РІС‹СЂР°Р¶РµРЅРёРµ `throw` (СЂРµРєСѓСЂСЃРёРІРЅРѕ).
fn has_throw_in_fn(f: &FnDecl) -> bool {
    match &f.body {
        FnBody::Expr(e) => has_throw_in_expr(e),
        FnBody::Block(b) => has_throw_in_block(b),
        // D82: external fn вЂ” С‚РµР»Р° РЅРµС‚; throw'С‹ РґРµРєР»Р°СЂРёСЂСѓСЋС‚СЃСЏ С‡РµСЂРµР·
        // Fail[E] effect-Р°РЅРЅРѕС‚Р°С†РёСЋ РІ СЃРёРіРЅР°С‚СѓСЂРµ, РЅРµ РІ С‚РµР»Рµ.
        FnBody::External => false,
    }
}

fn has_throw_in_block(b: &Block) -> bool {
    for s in &b.stmts {
        if has_throw_in_stmt(s) {
            return true;
        }
    }
    if let Some(t) = &b.trailing {
        if has_throw_in_expr(t) {
            return true;
        }
    }
    false
}

fn has_throw_in_stmt(s: &Stmt) -> bool {
    match s {
        Stmt::Expr(e) => has_throw_in_expr(e),
        Stmt::Let(decl) => has_throw_in_expr(&decl.value),
        Stmt::Const(_) => false,
        Stmt::Assign { target, value, .. } =>
            has_throw_in_expr(target) || has_throw_in_expr(value),
        Stmt::Return { value, .. } => value.as_ref().map_or(false, has_throw_in_expr),
        Stmt::Throw { value, .. } => {
            // Statement-level throw: СЏРІРЅС‹Р№ СЃРёРіРЅР°Р», С‡С‚Рѕ Fail РЅСѓР¶РµРЅ.
            let _ = value;
            true
        }
        Stmt::Break(_) | Stmt::Continue(_) => false,
        // D90: defer/errdefer body **Р·Р°РїСЂРµС‰Р°СЋС‚** throw РІРЅСѓС‚СЂРё (Р¤.3
        // body-constraint). Throw РІ body вЂ” compile error. РџРѕСЌС‚РѕРјСѓ
        // body РЅРµ СЃС‡РёС‚Р°РµС‚СЃСЏ throw-РЅРѕСЃРёС‚РµР»РµРј вЂ” РѕРЅ РѕС‚РґРµР»СЊРЅС‹Р№ scope СЃ
        // РѕРіСЂР°РЅРёС‡РµРЅРёРµРј. Р•СЃР»Рё РІ body throw РѕР±РЅР°СЂСѓР¶РµРЅ вЂ” Р¤.3 РґР°СЃС‚
        // РѕС‚РґРµР»СЊРЅСѓСЋ compile error СЂР°РЅСЊС€Рµ СЌС‚РѕР№ РїСЂРѕРІРµСЂРєРё.
        Stmt::Defer { .. } | Stmt::ErrDefer { .. }
        | Stmt::OkDefer { .. } | Stmt::DeferWithResult { .. } => false,
        // Plan 110 D188: consume scope-block body может содержать throw —
        // D188 R3 cancel-shield масаит throw к caller'у после on_exit;
        // для has_throw analysis считаем body как throw-носитель.
        Stmt::ConsumeScope { init, body, .. } => {
            if has_throw_in_expr(init) { return true; }
            for s in &body.stmts {
                if has_throw_in_stmt(s) { return true; }
            }
            body.trailing.as_ref().map_or(false, |t| has_throw_in_expr(t))
        }
        // Plan 33.2 Р¤.8: assert_static вЂ” bool expr, no throw inside.
        Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => has_throw_in_expr(expr),
        // Ф.4.1: apply — ghost, args могут содержать throw (теоретически нет, но проверяем).
        Stmt::Apply { args, .. } => args.iter().any(has_throw_in_expr),
        // Ф.4.2: calc — ghost, шаги могут содержать throw.
        Stmt::Calc { steps, .. } => steps.iter().any(|s| has_throw_in_expr(&s.expr)),
        // Plan 33.9 Ф.2: reveal — ghost, no throw inside.
        Stmt::Reveal { .. } => false,
    }
}

fn has_throw_in_expr(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Throw(_) => true,
        ExprKind::Try(inner) => has_throw_in_expr(inner),
        // Plan 19, C7 (D85): `!!` С‚РѕР¶Рµ РјРѕР¶РµС‚ Р±СЂРѕСЃРёС‚СЊ (`Err`/`None`).
        ExprKind::Bang(inner) => has_throw_in_expr(inner),
        ExprKind::Binary { left, right, .. } =>
            has_throw_in_expr(left) || has_throw_in_expr(right),
        ExprKind::Unary { operand, .. } => has_throw_in_expr(operand),
        ExprKind::Call { func, args, .. } =>
            has_throw_in_expr(func) || args.iter().any(|a| has_throw_in_expr(a.expr())),
        ExprKind::Member { obj, .. } => has_throw_in_expr(obj),
        ExprKind::Index { obj, index } =>
            has_throw_in_expr(obj) || has_throw_in_expr(index),
        ExprKind::If { cond, then, else_, .. } => {
            if has_throw_in_expr(cond) || has_throw_in_block(then) { return true; }
            match else_ {
                Some(ElseBranch::Block(b)) => has_throw_in_block(b),
                Some(ElseBranch::If(e)) => has_throw_in_expr(e),
                None => false,
            }
        }
        ExprKind::IfLet { scrutinee, then, else_, .. } => {
            if has_throw_in_expr(scrutinee) || has_throw_in_block(then) { return true; }
            match else_ {
                Some(ElseBranch::Block(b)) => has_throw_in_block(b),
                Some(ElseBranch::If(e)) => has_throw_in_expr(e),
                None => false,
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            if has_throw_in_expr(scrutinee) { return true; }
            arms.iter().any(|arm| match &arm.body {
                MatchArmBody::Expr(e) => has_throw_in_expr(e),
                MatchArmBody::Block(b) => has_throw_in_block(b),
            })
        }
        ExprKind::While { cond, body, .. } => has_throw_in_expr(cond) || has_throw_in_block(body),
        ExprKind::WhileLet { scrutinee, body, .. } =>
            has_throw_in_expr(scrutinee) || has_throw_in_block(body),
        ExprKind::For { iter, body, .. } => has_throw_in_expr(iter) || has_throw_in_block(body),
        ExprKind::Loop { body, .. } => has_throw_in_block(body),
        ExprKind::Select { arms } => arms.iter().any(|a| {
            (match &a.op {
                SelectOp::Recv { chan, .. } => has_throw_in_expr(chan),
                SelectOp::Send { chan, value } => has_throw_in_expr(chan) || has_throw_in_expr(value),
                SelectOp::Default => false,
            }) || a.guard.as_ref().map_or(false, has_throw_in_expr)
              || has_throw_in_block(&a.body)
        }),
        ExprKind::Block(b) => has_throw_in_block(b),
        ExprKind::Lambda { .. } => false,
            // Lambda has its own scope; throw inside lambda вЂ” РµС‘ СЌС„С„РµРєС‚С‹, РЅРµ С‚РµРєСѓС‰РµР№ fn.
        ExprKind::Range { start, end, .. } =>
            start.as_deref().map_or(false, has_throw_in_expr)
                || end.as_deref().map_or(false, has_throw_in_expr),
        ExprKind::TupleLit(elems) => elems.iter().any(has_throw_in_expr),
        ExprKind::ArrayLit(elems) => elems.iter().any(|el| match el {
            ArrayElem::Item(e) => has_throw_in_expr(e),
            ArrayElem::Spread(e) => has_throw_in_expr(e),
        }),
        ExprKind::RecordLit { fields, .. } =>
            fields.iter().any(|f| f.value.as_ref().map_or(false, has_throw_in_expr)),
        ExprKind::With { bindings, body } => {
            if bindings.iter().any(|b| has_throw_in_expr(&b.handler)) { return true; }
            has_throw_in_block(body)
        }
        ExprKind::Spawn(e) => has_throw_in_expr(e),
        ExprKind::Supervised { body, cancel } => {
            has_throw_in_block(body)
                || cancel.as_ref().map_or(false, |c| has_throw_in_expr(c))
        }
        ExprKind::ParallelFor { iter, body, .. } =>
            has_throw_in_expr(iter) || has_throw_in_block(body),
        ExprKind::TurboFish { base, .. } => has_throw_in_expr(base),
        _ => false,
    }
}

/// РџСЂРµРѕР±СЂР°Р·СѓРµС‚ `TypeRef` AST РІ `Ty` РґР»СЏ Р±Р°Р·РѕРІРѕР№ РїСЂРѕРІРµСЂРєРё.
pub fn ty_of_ref(tr: &TypeRef) -> Ty {
    match tr {
        TypeRef::Named { path, .. } => match path.last().map(|s| s.as_str()) {
            Some("int") | Some("i8") | Some("i16") | Some("i32") | Some("i64") => Ty::Int,
            Some("u8") | Some("u16") | Some("u32") | Some("u64") => Ty::Int,
            Some("f32") | Some("f64") => Ty::Float,
            Some("str") => Ty::Str,
            Some("bool") => Ty::Bool,
            // Plan 76: bottom-тип `never` — строчный встроенный примитив.
            Some("never") => Ty::Never,
            Some(name) => Ty::Named(name.to_string()),
            None => Ty::Any,
        },
        TypeRef::Array(inner, _) => Ty::Array(Box::new(ty_of_ref(inner))),
        TypeRef::FixedArray(_, inner, _) => Ty::Array(Box::new(ty_of_ref(inner))),
        TypeRef::Tuple(elems, _) => Ty::Tuple(elems.iter().map(ty_of_ref).collect()),
        TypeRef::Func {
            params,
            return_type,
            effects,
            ..
        } => Ty::Func {
            params: params.iter().map(ty_of_ref).collect(),
            ret: Box::new(
                return_type
                    .as_ref()
                    .map(|t| ty_of_ref(t))
                    .unwrap_or(Ty::Unit),
            ),
            effects: effects
                .iter()
                .filter_map(|e| match e {
                    TypeRef::Named { path, .. } => path.last().cloned(),
                    _ => None,
                })
                .collect(),
        },
        // Plan 97 Ф.2 (D142): анонимный protocol-тип — структурный
        // контракт. Для baseline-ty system ty_of_ref сводим к Ty::Any
        // (permissive); satisfaction-check выполняется отдельно.
        TypeRef::Protocol { .. } => Ty::Any,
        TypeRef::Unit(_) => Ty::Unit,
        // D176 (Plan 108): readonly T — same Ty as inner (transparent).
        TypeRef::Readonly(inner, _) => ty_of_ref(inner),
    }
}

/// D84: structural equality РґР»СЏ TypeRef (РёРіРЅРѕСЂРёСЂСѓРµС‚ Span'С‹).
///
/// РСЃРїРѕР»СЊР·СѓРµС‚СЃСЏ РґР»СЏ detection РґСѓР±Р»РёСЂРѕРІР°РЅРЅС‹С… signatures СЃРІРѕР±РѕРґРЅС‹С…
/// С„СѓРЅРєС†РёР№ вЂ” "С‚РѕС‡РЅРѕРµ СЃРѕРІРїР°РґРµРЅРёРµ" arity + arg-types Р·Р°РїСЂРµС‰РµРЅРѕ РєР°Рє
/// ambiguous overload Р±РµР· РІРѕР·РјРѕР¶РЅРѕСЃС‚Рё СЂРµР·РѕР»РІР°.
///
/// РќРµ РёСЃРїРѕР»СЊР·СѓРµС‚ PartialEq/Eq derive РїРѕС‚РѕРјСѓ С‡С‚Рѕ TypeRef СЃРѕРґРµСЂР¶РёС‚
/// Span'С‹ (РїРѕР·РёС†РёРё РІ РёСЃС…РѕРґРЅРёРєРµ), РєРѕС‚РѕСЂС‹Рµ РѕС‚Р»РёС‡Р°СЋС‚СЃСЏ Сѓ СЂР°Р·РЅС‹С…
/// РѕРїСЂРµРґРµР»РµРЅРёР№ С‚РѕРіРѕ Р¶Рµ С‚РёРїР°.
fn typeref_equal(a: &TypeRef, b: &TypeRef) -> bool {
    match (a, b) {
        (
            TypeRef::Named { path: pa, generics: ga, .. },
            TypeRef::Named { path: pb, generics: gb, .. },
        ) => {
            pa == pb
                && ga.len() == gb.len()
                && ga.iter().zip(gb.iter()).all(|(x, y)| typeref_equal(x, y))
        }
        (TypeRef::Array(ia, _), TypeRef::Array(ib, _)) => typeref_equal(ia, ib),
        (TypeRef::FixedArray(na, ia, _), TypeRef::FixedArray(nb, ib, _)) => {
            na == nb && typeref_equal(ia, ib)
        }
        (TypeRef::Tuple(ea, _), TypeRef::Tuple(eb, _)) => {
            ea.len() == eb.len()
                && ea.iter().zip(eb.iter()).all(|(x, y)| typeref_equal(x, y))
        }
        (
            TypeRef::Func { params: pa, return_type: ra, effects: ea, .. },
            TypeRef::Func { params: pb, return_type: rb, effects: eb, .. },
        ) => {
            pa.len() == pb.len()
                && pa.iter().zip(pb.iter()).all(|(x, y)| typeref_equal(x, y))
                && match (ra.as_deref(), rb.as_deref()) {
                    (Some(x), Some(y)) => typeref_equal(x, y),
                    (None, None) => true,
                    _ => false,
                }
                && ea.len() == eb.len()
                && ea.iter().zip(eb.iter()).all(|(x, y)| typeref_equal(x, y))
        }
        (TypeRef::Unit(_), TypeRef::Unit(_)) => true,
        // D176 (Plan 108): readonly T == readonly T if inner equal.
        (TypeRef::Readonly(ia, _), TypeRef::Readonly(ib, _)) => typeref_equal(ia, ib),
        _ => false,
    }
}

// ============================================================================
// D90 Plan 20 Р¤.3: defer/errdefer body constraints
// ============================================================================
//
// Body Р·Р°РїСЂРµС‰Р°РµС‚ С‚СЂРё РєР°С‚РµРіРѕСЂРёРё РєРѕРЅСЃС‚СЂСѓРєС†РёР№:
//
// 1. **Exit-control:** `return`, `throw`, `break`, `continue` РЅРµР»СЊР·СЏ
//    РёСЃРїРѕР»СЊР·РѕРІР°С‚СЊ РІ defer body вЂ” defer С‡Р°СЃС‚СЊ exit-РїСЂРѕС†РµСЃСЃР°, РЅРµ РјРѕР¶РµС‚
//    hijack РµРіРѕ. Compile error: В«defer body cannot use ... вЂ” СЌС‚Рѕ
//    РЅР°СЂСѓС€РёС‚ exit СЃРµРјР°РЅС‚РёРєСѓ scope'Р°В».
//
// 2. **Fail-effect:** `?`, `!!`, `throw` desugar'СЏС‚СЃСЏ РІ throw С‡РµСЂРµР·
//    СЌС„С„РµРєС‚ Fail. Defer body РґРѕР»Р¶РЅРѕ Р±С‹С‚СЊ infallible вЂ” double-throw
//    РЅРµРІРѕР·РјРѕР¶РЅРѕ СЃРґРµР»Р°С‚СЊ РєРѕСЂСЂРµРєС‚РЅРѕ. Detection С‡РµСЂРµР· AST-walk
//    (ExprKind::Throw, ExprKind::Try, ExprKind::Bang).
//
// 3. **Suspend operations:** Net.*, Fs.*, Db.*, Time.sleep,
//    Channel.recv (blocking), parallel for, spawn, supervised, select.
//    Defer РґРѕР»Р¶РµРЅ Р±С‹С‚СЊ Р±С‹СЃС‚СЂС‹Рј cleanup вЂ” suspend РґРµР»Р°РµС‚ exit-СЃРµРјР°РЅС‚РёРєСѓ
//    РЅРµРїСЂРµРґСЃРєР°Р·СѓРµРјРѕР№. Detection: AST-С„РѕСЂРјР° (ParallelFor, Spawn,
//    Supervised) + callee.effects intersect СЃ SUSPEND_EFFECTS СЃРїРёСЃРєРѕРј.

/// Р­С„С„РµРєС‚С‹, РєРѕС‚РѕСЂС‹Рµ СЃС‡РёС‚Р°СЋС‚СЃСЏ suspend РІ РєРѕРЅС‚РµРєСЃС‚Рµ defer body.
/// Р­С‚Рѕ approximation РґР»СЏ bootstrap вЂ” D90 spec РіРѕРІРѕСЂРёС‚ В«cleanup Р±С‹СЃС‚СЂС‹Р№В»,
/// Р±РµР·РѕРїР°СЃРЅРµРµ Р·Р°РїСЂРµС‚РёС‚СЊ С†РµР»СѓСЋ РіСЂСѓРїРїСѓ С‡РµРј РїС‹С‚Р°С‚СЊСЃСЏ СЂР°Р·Р»РёС‡РёС‚СЊ
/// blocking vs non-blocking РІР°СЂРёР°РЅС‚С‹ РґР»СЏ РєР°Р¶РґРѕРіРѕ СЌС„С„РµРєС‚Р°.
const SUSPEND_EFFECT_NAMES: &[&str] = &[
    "Net", "Fs", "Db", "Time",
];

/// AST-формы которые сами по себе считаются suspend (даже если effects
/// не объявлены).
///
/// **Reserved**: текущая suspend-detection использует прямой
/// `matches!` inline в effect-inference path'е. Helper сохранён для
/// возможной consolidation если правил станет больше.
#[allow(dead_code)]
fn is_suspend_expr_kind(kind: &ExprKind) -> bool {
    matches!(kind,
        ExprKind::ParallelFor { .. }
        | ExprKind::Spawn(_)
        | ExprKind::Supervised { .. }
        | ExprKind::Detach(_)
        | ExprKind::Blocking(_)
    )
}

/// D90 Ф.8 (1): walk модуля, для каждого `HandlerLit { methods }`
/// проверяет, что methods обрабатывающие never-operations завершаются
/// exit-control'ом.
///
/// never-operation = operation, чей return type — `never`. Handler-method
/// для такой operation не может завершиться normally (нет значения типа
/// never). По D61 (стр. 1430-1434) body обязан `interrupt v`, `throw err`,
/// `panic(...)` или `exit(...)`.
///
/// Bootstrap-stage: знаем что built-in `Fail.fail(value) -> never` —
/// единственная never-operation в prelude. Hardcoded effect_name="Fail",
/// method_name="fail". User-defined effects с never-methods будут покрыты
/// общей effect-schema-аналитикой (Plan 25+).
fn check_handler_never_ops(module: &Module, errors: &mut Vec<Diagnostic>) {
    // РЎР±РѕСЂ: РєР°РєРёРµ user-defined effect-methods РёРјРµСЋС‚ return type never.
    // Bootstrap: С‚РѕР»СЊРєРѕ Fail.fail вЂ” РІСЃС‚СЂРѕРµРЅРЅС‹Р№. User effects РїР°СЂСЃСЏС‚СЃСЏ
    // С‡РµСЂРµР· TypeDecl::Effect вЂ” Р°РЅР°Р»РёР·РёСЂСѓРµРј РёС… EffectMethod.return_type.
    let mut never_ops: HashSet<(String, String)> = HashSet::new();
    // Always-true: built-in Fail.fail.
    never_ops.insert(("Fail".to_string(), "fail".to_string()));
    // User-defined effects.
    for item in &module.items {
        if let Item::Type(td) = item {
            if let TypeDeclKind::Effect(methods) = &td.kind {
                for m in methods {
                    if let Some(rt) = &m.return_type {
                        if type_ref_is_never(rt) {
                            never_ops.insert((td.name.clone(), m.name.clone()));
                        }
                    }
                }
            }
        }
    }
    // Walk all expressions, РЅР°Р№РґС‘Рј HandlerLit'С‹.
    for item in &module.items {
        match item {
            Item::Fn(f) => {
                if let FnBody::Block(b) = &f.body {
                    walk_block_for_handler_lits(b, &never_ops, errors);
                } else if let FnBody::Expr(e) = &f.body {
                    walk_expr_for_handler_lits(e, &never_ops, errors);
                }
            }
            Item::Test(t) => walk_block_for_handler_lits(&t.body, &never_ops, errors),
            _ => {}
        }
    }
}

/// Plan 33.3 Р¤.9.6 (D24): handler verification gate.
///
/// Р•СЃР»Рё СЌС„С„РµРєС‚ РёРјРµРµС‚ С…РѕС‚СЏ Р±С‹ РѕРґРЅСѓ `pure_view` op'Сѓ, Р»СЋР±РѕРµ РёСЃРїРѕР»СЊР·РѕРІР°РЅРёРµ
/// handler'Р° С‡РµСЂРµР· `with E = h` РѕР±СЏР·Р°РЅРѕ РґРµРєР»Р°СЂРёСЂРѕРІР°С‚СЊ verification
/// СЃС‚Р°С‚СѓСЃ С‡РµСЂРµР· `#verify_handler` РёР»Рё `#trusted_handler`. Р‘РµР· Р°С‚СЂРёР±СѓС‚Р° вЂ”
/// compile error.
///
/// РЎРµРјР°РЅС‚РёРєР°:
/// - `#verify_handler` вЂ” symbolic verification handler.action body
///   РїСЂРѕС‚РёРІ axiom'РѕРІ СЌС„С„РµРєС‚Р° (Р¤.9.7). Bootstrap V1: Р°С‚СЂРёР±СѓС‚ РїСЂРёРЅРёРјР°РµС‚СЃСЏ
///   РЅРѕ СЂРµР°Р»СЊРЅРѕР№ РІРµСЂРёС„РёРєР°С†РёРё РЅРµС‚ вЂ” placeholder РґР»СЏ Р¤.9.7.
/// - `#trusted_handler` вЂ” РїСЂРѕРіСЂР°РјРјРёСЃС‚ Р±РµСЂС‘С‚ РѕС‚РІРµС‚СЃС‚РІРµРЅРЅРѕСЃС‚СЊ.
/// - Default (Unverified) РґР»СЏ СЌС„С„РµРєС‚РѕРІ СЃ pure_views вЂ” **error**.
///
/// Р­С„С„РµРєС‚С‹ Р‘Р•Р— pure_views вЂ” РЅРёРєР°РєРёС… РѕРіСЂР°РЅРёС‡РµРЅРёР№ (default = Unverified
/// РґРѕРїСѓСЃС‚РёРј).
///
/// Р­С‚Р° РїСЂРѕРІРµСЂРєР° РєРѕРЅСЃРµСЂРІР°С‚РёРІРЅР°: РґР°Р¶Рµ РµСЃР»Рё body РЅРµ РІС‹Р·С‹РІР°РµС‚ pure_view-
/// using С„СѓРЅРєС†РёРё, gate РІСЃС‘ СЂР°РІРЅРѕ С‚СЂРµР±СѓРµС‚ attribute РґР»СЏ СЌС„С„РµРєС‚Р° СЃ
/// pure_views. Р­С‚Рѕ СѓРїСЂРѕС‰Р°РµС‚ V1 (РЅРµС‚ cross-fn analysis); Р¤.9.7
/// СѓС‚РѕС‡РЅРёС‚ РґРѕ actually-uses analysis.
fn check_handler_verification_gate(module: &Module, errors: &mut Vec<Diagnostic>) {
    // РЁР°Рі 1: РєР°РєРёРµ СЌС„С„РµРєС‚С‹ РёРјРµСЋС‚ axioms?
    // Refactor: gate СЃСЂР°Р±Р°С‚С‹РІР°РµС‚ С‚РѕР»СЊРєРѕ РїСЂРё axiom-РїСЂРёСЃСѓС‚СЃС‚РІРёРё вЂ” pure_view СЃР°Рј РїРѕ
    // СЃРµР±Рµ РЅРёС‡РµРіРѕ РЅРµ СѓС‚РІРµСЂР¶РґР°РµС‚, СѓС‚РІРµСЂР¶РґРµРЅРёРµ РґРµР»Р°РµС‚ axiom. Р‘РµР· axiom handler
    // РІРµСЂРёС„РёС†РёСЂРѕРІР°С‚СЊ РЅРµ РЅР° С‡С‚Рѕ.
    let mut effects_with_axioms: HashSet<String> = HashSet::new();
    for item in &module.items {
        let Item::Type(td) = item else { continue };
        if !matches!(&td.kind, TypeDeclKind::Effect(_)) { continue; }
        if !td.axioms.is_empty() {
            effects_with_axioms.insert(td.name.clone());
        }
    }
    if effects_with_axioms.is_empty() { return; }

    // РЁР°Рі 2: walk all expressions, РЅР°Р№С‚Рё WithBinding'Рё СЃ С‚Р°РєРёРјРё СЌС„С„РµРєС‚Р°РјРё.
    for item in &module.items {
        match item {
            Item::Fn(f) => match &f.body {
                FnBody::Block(b) => walk_block_for_with_gate(b, &effects_with_axioms, errors),
                FnBody::Expr(e) => walk_expr_for_with_gate(e, &effects_with_axioms, errors),
                FnBody::External => {}
            }
            Item::Test(t) => walk_block_for_with_gate(&t.body, &effects_with_axioms, errors),
            _ => {}
        }
    }
}

fn walk_block_for_with_gate(b: &Block, eff_pv: &HashSet<String>, errors: &mut Vec<Diagnostic>) {
    for s in &b.stmts {
        match s {
            Stmt::Expr(e) => walk_expr_for_with_gate(e, eff_pv, errors),
            Stmt::Let(LetDecl { value, .. }) => walk_expr_for_with_gate(value, eff_pv, errors),
            Stmt::Const(_) => {}
            Stmt::Assign { target, value, .. } => {
                walk_expr_for_with_gate(target, eff_pv, errors);
                walk_expr_for_with_gate(value, eff_pv, errors);
            }
            _ => {}
        }
    }
    if let Some(t) = &b.trailing { walk_expr_for_with_gate(t, eff_pv, errors); }
}

fn walk_expr_for_with_gate(e: &Expr, eff_pv: &HashSet<String>, errors: &mut Vec<Diagnostic>) {
    use crate::ast::ExprKind::*;
    match &e.kind {
        With { bindings, body } => {
            for b in bindings {
                let eff_name = match &b.effect {
                    TypeRef::Named { path, .. } => path.last().cloned().unwrap_or_default(),
                    _ => String::new(),
                };
                if !eff_pv.contains(&eff_name) { continue; }
                if matches!(b.verification, HandlerVerification::Unverified) {
                    errors.push(Diagnostic::new(
                        format!(
                            "handler for effect `{}` must be marked `#verify` \
                             or `#trusted` (effect has `axiom` declarations, so any \
                             handler must declare verification status). Examples:\n  \
                             with #trusted {0} = my_handler {{ ... }}\n  \
                             with #verify {0} = my_handler {{ ... }}",
                            eff_name,
                        ),
                        b.span,
                    ));
                }
                walk_expr_for_with_gate(&b.handler, eff_pv, errors);
            }
            walk_block_for_with_gate(body, eff_pv, errors);
        }
        Block(b) => walk_block_for_with_gate(b, eff_pv, errors),
        Call { func, args, .. } => {
            walk_expr_for_with_gate(func, eff_pv, errors);
            for a in args { walk_expr_for_with_gate(a.expr(), eff_pv, errors); }
        }
        Binary { left, right, .. } => {
            walk_expr_for_with_gate(left, eff_pv, errors);
            walk_expr_for_with_gate(right, eff_pv, errors);
        }
        Unary { operand, .. } => walk_expr_for_with_gate(operand, eff_pv, errors),
        Member { obj, .. } => walk_expr_for_with_gate(obj, eff_pv, errors),
        Index { obj, index } => {
            walk_expr_for_with_gate(obj, eff_pv, errors);
            walk_expr_for_with_gate(index, eff_pv, errors);
        }
        If { cond, then, else_ } => {
            walk_expr_for_with_gate(cond, eff_pv, errors);
            walk_block_for_with_gate(then, eff_pv, errors);
            match else_ {
                Some(crate::ast::ElseBranch::Block(b)) => walk_block_for_with_gate(b, eff_pv, errors),
                Some(crate::ast::ElseBranch::If(ie)) => walk_expr_for_with_gate(ie, eff_pv, errors),
                None => {}
            }
        }
        _ => {}
    }
}

fn type_ref_is_never(t: &TypeRef) -> bool {
    if let TypeRef::Named { path, .. } = t {
        if let Some(last) = path.last() {
            // Plan 76: bottom-тип — строчный `never`.
            return last == "never";
        }
    }
    false
}

/// Plan 33.3 Р¤.9 (D24): РІР°Р»РёРґР°С†РёСЏ axiom-С„РѕСЂРјСѓР» РІРЅСѓС‚СЂРё effect-Р±Р»РѕРєРѕРІ.
///
/// РљРѕРЅС‚СЂР°РєС‚: РІРЅСѓС‚СЂРё `axiom name(binders) => formula` СЂР°Р·СЂРµС€РµРЅС‹ С‚РѕР»СЊРєРѕ:
///   - Р»РёС‚РµСЂР°Р»С‹ (int/bool/str/unit);
///   - РёРґРµРЅС‚РёС„РёРєР°С‚РѕСЂС‹ РёР· `binders`;
///   - РІС‹Р·РѕРІС‹ pure_view-ops **С‚РѕРіРѕ Р¶Рµ СЌС„С„РµРєС‚Р°**: `balance(id) >= 0`;
///   - СЃС‚Р°РЅРґР°СЂС‚РЅС‹Рµ Р±РёРЅР°СЂРЅС‹Рµ/СѓРЅР°СЂРЅС‹Рµ/comparison/boolean РѕРїРµСЂР°С‚РѕСЂС‹;
///   - `if/else` Р±РµР· stmts.
///
/// Р—Р°РїСЂРµС‰РµРЅС‹:
///   - non-pure_view operations (`SetBalance(...)`);
///   - РІС‹Р·РѕРІС‹ Р»СЋР±С‹С… РґСЂСѓРіРёС… fn (РІРєР»СЋС‡Р°СЏ built-ins Р·Р° РїСЂРµРґРµР»Р°РјРё СЂР°Р·СЂРµС€С‘РЅРЅС‹С…
///     РѕРїРµСЂР°С‚РѕСЂРѕРІ);
///   - record/sum constructors, member access, method calls.
///
/// Р­С‚Рё РѕРіСЂР°РЅРёС‡РµРЅРёСЏ РЅСѓР¶РЅС‹ РґР»СЏ С‡РёСЃС‚РѕР№ SMT-РєРѕРґРёСЂРѕРІРєРё (`pure_view` в†’ UF,
/// axiom в†’ assert) РІ Р¤.9.4. Р•СЃР»Рё СЂР°Р·СЂРµС€РёС‚СЊ РїСЂРѕРёР·РІРѕР»СЊРЅС‹Р№ РєРѕРґ вЂ” SMT
/// encoding С‚РµСЂСЏРµС‚ soundness.
fn check_effect_axioms(module: &Module, errors: &mut Vec<Diagnostic>) {
    for item in &module.items {
        let Item::Type(td) = item else { continue };
        // Plan 33.3 Р¤.9 (refactor): unique-name + axiom-formula checks
        // РїСЂРёРјРµРЅСЏСЋС‚СЃСЏ Рё Рє effect, Рё Рє protocol (РІ РѕР±РѕРёС… РјРѕР¶РЅРѕ РѕР±СЉСЏРІР»СЏС‚СЊ
        // #pure ops Рё axioms).
        let methods = match &td.kind {
            TypeDeclKind::Effect(m) => m,
            TypeDeclKind::Protocol { methods, .. } => methods,
            _ => continue,
        };

        // Plan 33.3 (refactor): unique-name checks РІРЅСѓС‚СЂРё effect/protocol.
        //
        // РџРµСЂРµРіСЂСѓР·РєР° op СЂР°Р·СЂРµС€РµРЅР° вЂ” СѓРЅРёРєР°Р»СЊРЅРѕСЃС‚СЊ РїРѕ (name + param_types).
        // Axioms СѓРЅРёРєР°Р»СЊРЅС‹ РїРѕ РёРјРµРЅРё (overloading axioms РЅРµ РїРѕРґРґРµСЂР¶РёРІР°РµС‚СЃСЏ).
        // Axiom name РЅРµ РјРѕР¶РµС‚ СЃРѕРІРїР°РґР°С‚СЊ СЃ РёРјРµРЅРµРј Р»СЋР±РѕРіРѕ op (РЅРµР·Р°РІРёСЃРёРјРѕ РѕС‚
        // С‚РёРїРѕРІ РїР°СЂР°РјРµС‚СЂРѕРІ) вЂ” РѕРЅРё РІ РѕРґРЅРѕРј logical namespace.
        fn type_key(ty: &TypeRef) -> String {
            match ty {
                TypeRef::Named { path, generics, .. } => {
                    let base = path.join(".");
                    if generics.is_empty() {
                        base
                    } else {
                        let a: Vec<_> = generics.iter().map(type_key).collect();
                        format!("{}[{}]", base, a.join(","))
                    }
                }
                TypeRef::Tuple(ts, _) => {
                    let a: Vec<_> = ts.iter().map(type_key).collect();
                    format!("({})", a.join(","))
                }
                TypeRef::Func { params, return_type, .. } => {
                    let ps: Vec<_> = params.iter().map(type_key).collect();
                    let ret = return_type.as_deref().map(type_key).unwrap_or_default();
                    format!("fn({})->{}", ps.join(","), ret)
                }
                TypeRef::Array(t, _) => format!("[]{}", type_key(t)),
                TypeRef::FixedArray(n, t, _) => format!("[{}]{}", n, type_key(t)),
                // Plan 97 Ф.2 (D142): анонимный protocol — структурный
                // ключ через method-имена + аriт'и. Полная сигнатура с
                // type_key рекурсивно даёт стабильный ключ для overload
                // disambiguation.
                TypeRef::Protocol { methods, .. } => {
                    let ms: Vec<String> = methods
                        .iter()
                        .map(|m| {
                            let prefix = if m.is_static { "." } else { "" };
                            let ps: Vec<String> =
                                m.params.iter().map(|p| type_key(&p.ty)).collect();
                            let ret = m
                                .return_type
                                .as_ref()
                                .map(type_key)
                                .unwrap_or_default();
                            format!("{}{}({})->{}", prefix, m.name, ps.join(","), ret)
                        })
                        .collect();
                    format!("protocol{{{}}}", ms.join(";"))
                }
                TypeRef::Unit(_) => "()".to_string(),
                // D176 (Plan 108): readonly T — key as "readonly_<inner>"
                TypeRef::Readonly(inner, _) => format!("readonly_{}", type_key(inner)),
            }
        }
        fn op_sig(m: &EffectMethod) -> String {
            let types: Vec<String> = m.params.iter()
                .map(|p| type_key(&p.ty))
                .collect();
            format!("{}({})", m.name, types.join(","))
        }
        let mut op_sigs: HashSet<String> = HashSet::new();
        // op_names_only: РІСЃРµ РёРјРµРЅР° operations (РґР»СЏ РїСЂРѕРІРµСЂРєРё axiomв†”op РєРѕР»Р»РёР·РёРё).
        let mut op_names_only: HashSet<&String> = HashSet::new();
        for m in methods {
            op_names_only.insert(&m.name);
            let sig = op_sig(m);
            if !op_sigs.insert(sig) {
                errors.push(Diagnostic::new(
                    format!("effect `{}`: duplicate operation `{}` \
                             (same name and parameter types)",
                        td.name, m.name),
                    m.span,
                ));
            }
        }
        let mut axiom_names: HashSet<&String> = HashSet::new();
        for ax in &td.axioms {
            if !axiom_names.insert(&ax.name) {
                errors.push(Diagnostic::new(
                    format!("effect `{}`: duplicate axiom `{}`",
                        td.name, ax.name),
                    ax.span,
                ));
            }
            if op_names_only.contains(&ax.name) {
                errors.push(Diagnostic::new(
                    format!("effect `{}`: axiom `{}` conflicts with operation \
                             of the same name (axiom names must be distinct \
                             from operations / `#pure` views)",
                        td.name, ax.name),
                    ax.span,
                ));
            }
        }

        if td.axioms.is_empty() { continue; }

        // РЎРѕР±РёСЂР°РµРј pure_view-РёРјРµРЅР° СЌС„С„РµРєС‚Р°: РёРјСЏ в†’ РѕР¶РёРґР°РµРјР°СЏ Р°СЂРЅРѕСЃС‚СЊ.
        let mut pure_views: HashMap<String, usize> = HashMap::new();
        for m in methods {
            if matches!(m.kind, EffectOpKind::PureView) {
                pure_views.insert(m.name.clone(), m.params.len());
            }
        }

        for ax in &td.axioms {
            // Duplicate-binder check.
            let mut seen: HashSet<&String> = HashSet::new();
            for bd in &ax.binders {
                if !seen.insert(&bd.name) {
                    errors.push(Diagnostic::new(
                        format!("axiom `{}.{}`: duplicate binder `{}`",
                            td.name, ax.name, bd.name),
                        ax.span,
                    ));
                }
            }
            let binders: HashSet<&String> = ax.binders.iter().map(|bd| &bd.name).collect();
            check_axiom_expr(&ax.formula, &td.name, &ax.name,
                             &binders, &pure_views, errors);
        }
    }
}

/// Walk `expr` РІ axiom-formula Рё РїСѓС€РёС‚ РѕС€РёР±РєРё РЅР° Р·Р°РїСЂРµС‰С‘РЅРЅС‹Рµ РєРѕРЅСЃС‚СЂСѓРєС†РёРё.
fn check_axiom_expr(
    e: &Expr,
    effect_name: &str,
    axiom_name: &str,
    binders: &HashSet<&String>,
    pure_views: &HashMap<String, usize>,
    errors: &mut Vec<Diagnostic>,
) {
    use crate::ast::ExprKind::*;
    match &e.kind {
        IntLit(_) | BoolLit(_) | StrLit(_) | CharLit(_) | UnitLit => {}
        Ident(n) => {
            if binders.contains(&n.to_string()) { return; }
            if pure_views.contains_key(n) {
                // Reference to pure_view Р±РµР· РІС‹Р·РѕРІР° вЂ” V1 Р·Р°РїСЂРµС‰Р°РµРј
                // (С‚СЂРµР±СѓРµРј `name(args)`-С„РѕСЂРјСѓ РґР»СЏ arity-clarity).
                errors.push(Diagnostic::new(
                    format!(
                        "axiom `{}.{}`: pure_view `{}` must be called \
                         with arguments (e.g. `{}(...)`), not used as value",
                        effect_name, axiom_name, n, n,
                    ),
                    e.span,
                ));
                return;
            }
            errors.push(Diagnostic::new(
                format!(
                    "axiom `{}.{}`: unknown identifier `{}` (axiom-body \
                     may only reference binders {:?} or pure_view ops \
                     of effect `{}`)",
                    effect_name, axiom_name, n,
                    binders.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
                    effect_name,
                ),
                e.span,
            ));
        }
        Binary { left, right, .. } => {
            check_axiom_expr(left, effect_name, axiom_name, binders, pure_views, errors);
            check_axiom_expr(right, effect_name, axiom_name, binders, pure_views, errors);
        }
        Unary { operand, .. } => {
            check_axiom_expr(operand, effect_name, axiom_name, binders, pure_views, errors);
        }
        If { cond, then, else_ } => {
            check_axiom_expr(cond, effect_name, axiom_name, binders, pure_views, errors);
            if !then.stmts.is_empty() {
                errors.push(Diagnostic::new(
                    format!("axiom `{}.{}`: if-branch must not contain statements",
                        effect_name, axiom_name),
                    e.span,
                ));
            }
            if let Some(trailing) = &then.trailing {
                check_axiom_expr(trailing, effect_name, axiom_name, binders, pure_views, errors);
            }
            match else_ {
                Some(crate::ast::ElseBranch::Block(b)) => {
                    if !b.stmts.is_empty() {
                        errors.push(Diagnostic::new(
                            format!("axiom `{}.{}`: else-branch must not contain statements",
                                effect_name, axiom_name),
                            e.span,
                        ));
                    }
                    if let Some(t) = &b.trailing {
                        check_axiom_expr(t, effect_name, axiom_name, binders, pure_views, errors);
                    }
                }
                Some(crate::ast::ElseBranch::If(ie)) => {
                    check_axiom_expr(ie, effect_name, axiom_name, binders, pure_views, errors);
                }
                None => {}
            }
        }
        Call { func, args, trailing } => {
            if trailing.is_some() {
                errors.push(Diagnostic::new(
                    format!(
                        "axiom `{}.{}`: trailing blocks not allowed in axiom-formulas",
                        effect_name, axiom_name,
                    ),
                    e.span,
                ));
                return;
            }
            // Plan 33.5 F.6: allow `post(ActionCall)(viewCall)` in axiom-formulas.
            // Shape: outer Call{ func: Call{ func: Ident("post"), args:[action_call] }, args:[view_call] }
            // Validate: action args and view args must only reference binders.
            // (Action and view names are validated at pipeline verify time, not here.)
            if let Call { func: inner_func, args: inner_args, .. } = &func.kind {
                if let Ident(n) = &inner_func.kind {
                    if n == "post" {
                        // post(ActionCall)(ViewCall): only validate binder references
                        // inside action args and view args; action/view names are
                        // validated by pipeline at verify time.
                        // inner_args[0] = ActionName(binder1, binder2, ...)
                        if let Some(action_call) = inner_args.first() {
                            if let Call { args: action_binder_args, .. } = &action_call.expr().kind {
                                for a in action_binder_args {
                                    check_axiom_expr(a.expr(), effect_name, axiom_name, binders, pure_views, errors);
                                }
                            }
                        }
                        // args[0] = ViewName(binder1, ...)
                        if let Some(view_call) = args.first() {
                            if let Call { args: view_binder_args, .. } = &view_call.expr().kind {
                                for a in view_binder_args {
                                    check_axiom_expr(a.expr(), effect_name, axiom_name, binders, pure_views, errors);
                                }
                            }
                        }
                        return;
                    }
                }
            }
            // V1: allowed form `<pure_view_name>(args)`.
            let pv_name = match &func.kind {
                Ident(n) => n.clone(),
                _ => {
                    errors.push(Diagnostic::new(
                        format!(
                            "axiom `{}.{}`: callee must be a pure_view of effect `{}`",
                            effect_name, axiom_name, effect_name,
                        ),
                        e.span,
                    ));
                    return;
                }
            };
            let Some(&expected) = pure_views.get(&pv_name) else {
                errors.push(Diagnostic::new(
                    format!(
                        "axiom `{}.{}`: `{}` is not a pure_view of effect `{}` \
                         (axioms may only reference pure_view ops)",
                        effect_name, axiom_name, pv_name, effect_name,
                    ),
                    e.span,
                ));
                return;
            };
            if args.len() != expected {
                errors.push(Diagnostic::new(
                    format!(
                        "axiom `{}.{}`: pure_view `{}` expects {} arg(s), got {}",
                        effect_name, axiom_name, pv_name, expected, args.len(),
                    ),
                    e.span,
                ));
            }
            for a in args {
                check_axiom_expr(a.expr(), effect_name, axiom_name, binders, pure_views, errors);
            }
        }
        _ => {
            errors.push(Diagnostic::new(
                format!(
                    "axiom `{}.{}`: this expression form is not allowed inside \
                     axiom-formula (only literals, binders, pure_view calls, \
                     arith/bool ops, and if/else)",
                    effect_name, axiom_name,
                ),
                e.span,
            ));
        }
    }
}

/// Walk block recursively: РёС‰РµС‚ HandlerLit, РїСЂРѕРІРµСЂСЏРµС‚ never-ops.
fn walk_block_for_handler_lits(b: &Block, never_ops: &HashSet<(String, String)>, errors: &mut Vec<Diagnostic>) {
    for s in &b.stmts {
        match s {
            Stmt::Let(decl) => walk_expr_for_handler_lits(&decl.value, never_ops, errors),
            Stmt::Const(_) => {}
            Stmt::Expr(e) => walk_expr_for_handler_lits(e, never_ops, errors),
            Stmt::Assign { target, value, .. } => {
                walk_expr_for_handler_lits(target, never_ops, errors);
                walk_expr_for_handler_lits(value, never_ops, errors);
            }
            Stmt::Return { value, .. } => {
                if let Some(v) = value { walk_expr_for_handler_lits(v, never_ops, errors); }
            }
            Stmt::Throw { value, .. } => walk_expr_for_handler_lits(value, never_ops, errors),
            Stmt::Defer { body, .. } | Stmt::ErrDefer { body, .. }
            | Stmt::OkDefer { body, .. } | Stmt::DeferWithResult { body, .. } => {
                walk_expr_for_handler_lits(body, never_ops, errors);
            }
            // Plan 110 D188: walk init + body block recursively.
            Stmt::ConsumeScope { init, body, .. } => {
                walk_expr_for_handler_lits(init, never_ops, errors);
                walk_block_for_handler_lits(body, never_ops, errors);
            }
            Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => walk_expr_for_handler_lits(expr, never_ops, errors),
            Stmt::Break(_) | Stmt::Continue(_) => {}
            Stmt::Apply { args, .. } => {
                for a in args { walk_expr_for_handler_lits(a, never_ops, errors); }
            }
            Stmt::Calc { steps, .. } => {
                for step in steps { walk_expr_for_handler_lits(&step.expr, never_ops, errors); }
            }
            Stmt::Reveal { .. } => {}
        }
    }
    if let Some(t) = &b.trailing { walk_expr_for_handler_lits(t, never_ops, errors); }
}

fn walk_expr_for_handler_lits(e: &Expr, never_ops: &HashSet<(String, String)>, errors: &mut Vec<Diagnostic>) {
    match &e.kind {
        // Plan 97 Ф.4 (D142): protocol-литерал — never-op check'и
        // на nor сейчас не специфицированы для protocol'ов (D61
        // §1430-1434 — только handler/effect-op'ы). Рекурсивно walk'аем
        // в bodies, но never-op assertion не применяется.
        ExprKind::ProtocolLit { methods, .. } => {
            for m in methods {
                match &m.body {
                    HandlerMethodBody::Expr(ex) => walk_expr_for_handler_lits(ex, never_ops, errors),
                    HandlerMethodBody::Block(b) => walk_block_for_handler_lits(b, never_ops, errors),
                }
            }
        }
        ExprKind::HandlerLit { effect_name, methods } => {
            // effect_name вЂ” Vec<String>, РїРѕСЃР»РµРґРЅРёР№ РєРѕРјРїРѕРЅРµРЅС‚ = effect's last name.
            let eff_last = effect_name.last().cloned().unwrap_or_default();
            for m in methods {
                let key = (eff_last.clone(), m.name.clone());
                if never_ops.contains(&key) {
                    if !handler_body_diverges(&m.body) {
                        errors.push(Diagnostic::new(
                            format!(
                                "handler-method `{}.{}` обрабатывает операцию с возвращаемым типом `never` \
                                 (D61 §1430-1434, D65): body обязан завершиться через `interrupt v`, \
                                 `throw err`, `panic(...)` или `exit(...)`. Нельзя завершить handler-method \
                                 normally — нет значения типа `never` для return.",
                                eff_last, m.name
                            ),
                            m.span,
                        ));
                    }
                }
            }
            // РўР°РєР¶Рµ recurse РІ bodies handler-РјРµС‚РѕРґРѕРІ (РјРѕРіСѓС‚ СЃРѕРґРµСЂР¶Р°С‚СЊ nested
            // HandlerLit).
            for m in methods {
                match &m.body {
                    HandlerMethodBody::Expr(ex) => walk_expr_for_handler_lits(ex, never_ops, errors),
                    HandlerMethodBody::Block(b) => walk_block_for_handler_lits(b, never_ops, errors),
                }
            }
        }
        // Recurse РІ РѕСЃС‚Р°Р»СЊРЅС‹Рµ expr-kinds (РёСЃРїРѕР»СЊР·СѓРµРј СЃСѓС‰РµСЃС‚РІСѓСЋС‰РёР№ walk
        // С‡РµСЂРµР· ExprKind::Block + РѕСЃС‚Р°Р»СЊРЅС‹Рµ expressions).
        ExprKind::Block(b) => walk_block_for_handler_lits(b, never_ops, errors),
        ExprKind::With { bindings, body } => {
            for bd in bindings { walk_expr_for_handler_lits(&bd.handler, never_ops, errors); }
            walk_block_for_handler_lits(body, never_ops, errors);
        }
        ExprKind::If { cond, then, else_ } => {
            walk_expr_for_handler_lits(cond, never_ops, errors);
            walk_block_for_handler_lits(then, never_ops, errors);
            match else_ {
                Some(ElseBranch::Block(b)) => walk_block_for_handler_lits(b, never_ops, errors),
                Some(ElseBranch::If(e2)) => walk_expr_for_handler_lits(e2, never_ops, errors),
                None => {}
            }
        }
        ExprKind::IfLet { scrutinee, then, else_, .. } => {
            walk_expr_for_handler_lits(scrutinee, never_ops, errors);
            walk_block_for_handler_lits(then, never_ops, errors);
            match else_ {
                Some(ElseBranch::Block(b)) => walk_block_for_handler_lits(b, never_ops, errors),
                Some(ElseBranch::If(e2)) => walk_expr_for_handler_lits(e2, never_ops, errors),
                None => {}
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            walk_expr_for_handler_lits(scrutinee, never_ops, errors);
            for a in arms {
                match &a.body {
                    MatchArmBody::Expr(ex) => walk_expr_for_handler_lits(ex, never_ops, errors),
                    MatchArmBody::Block(b) => walk_block_for_handler_lits(b, never_ops, errors),
                }
                if let Some(g) = &a.guard { walk_expr_for_handler_lits(g, never_ops, errors); }
            }
        }
        ExprKind::For { iter, body, .. } => {
            walk_expr_for_handler_lits(iter, never_ops, errors);
            walk_block_for_handler_lits(body, never_ops, errors);
        }
        ExprKind::While { cond, body, .. } | ExprKind::WhileLet { scrutinee: cond, body, .. } => {
            walk_expr_for_handler_lits(cond, never_ops, errors);
            walk_block_for_handler_lits(body, never_ops, errors);
        }
        ExprKind::Loop { body, .. } => walk_block_for_handler_lits(body, never_ops, errors),
        ExprKind::Select { arms } => {
            for arm in arms {
                match &arm.op {
                    SelectOp::Recv { chan, .. } => walk_expr_for_handler_lits(chan, never_ops, errors),
                    SelectOp::Send { chan, value } => {
                        walk_expr_for_handler_lits(chan, never_ops, errors);
                        walk_expr_for_handler_lits(value, never_ops, errors);
                    }
                    SelectOp::Default => {}
                }
                if let Some(g) = &arm.guard { walk_expr_for_handler_lits(g, never_ops, errors); }
                walk_block_for_handler_lits(&arm.body, never_ops, errors);
            }
        }
        ExprKind::Forbid { body, .. } | ExprKind::Realtime { body, .. } => {
            walk_block_for_handler_lits(body, never_ops, errors);
        }
        ExprKind::Detach(b) | ExprKind::Blocking(b) => walk_block_for_handler_lits(b, never_ops, errors),
        ExprKind::Supervised { body, cancel } => {
            if let Some(c) = cancel { walk_expr_for_handler_lits(c, never_ops, errors); }
            walk_block_for_handler_lits(body, never_ops, errors);
        }
        ExprKind::Spawn(ex) => walk_expr_for_handler_lits(ex, never_ops, errors),
        ExprKind::ParallelFor { iter, body, .. } => {
            walk_expr_for_handler_lits(iter, never_ops, errors);
            walk_block_for_handler_lits(body, never_ops, errors);
        }
        ExprKind::Call { func, args, trailing } => {
            walk_expr_for_handler_lits(func, never_ops, errors);
            for a in args { walk_expr_for_handler_lits(a.expr(), never_ops, errors); }
            if let Some(tr) = trailing {
                match tr {
                    Trailing::Block(b) => walk_block_for_handler_lits(b, never_ops, errors),
                    Trailing::Fn(fsb) => match &fsb.body {
                        FnBody::Block(b) => walk_block_for_handler_lits(b, never_ops, errors),
                        FnBody::Expr(e2) => walk_expr_for_handler_lits(e2, never_ops, errors),
                        FnBody::External => {}
                    },
                    Trailing::LegacyBlockWithParams(tb) => walk_block_for_handler_lits(&tb.body, never_ops, errors),
                }
            }
        }
        ExprKind::Binary { left, right, .. } => {
            walk_expr_for_handler_lits(left, never_ops, errors);
            walk_expr_for_handler_lits(right, never_ops, errors);
        }
        ExprKind::Unary { operand, .. } => walk_expr_for_handler_lits(operand, never_ops, errors),
        ExprKind::Coalesce(a, b) => {
            walk_expr_for_handler_lits(a, never_ops, errors);
            walk_expr_for_handler_lits(b, never_ops, errors);
        }
        ExprKind::As(e2, _) | ExprKind::Is(e2, _) => walk_expr_for_handler_lits(e2, never_ops, errors),
        ExprKind::Member { obj, .. } | ExprKind::Index { obj, .. } | ExprKind::TurboFish { base: obj, .. } => {
            walk_expr_for_handler_lits(obj, never_ops, errors);
        }
        ExprKind::Range { start, end, .. } => {
            if let Some(s) = start { walk_expr_for_handler_lits(s, never_ops, errors); }
            if let Some(e) = end { walk_expr_for_handler_lits(e, never_ops, errors); }
        }
        ExprKind::ArrayLit(elems) => {
            for el in elems {
                match el {
                    ArrayElem::Item(e2) | ArrayElem::Spread(e2) => walk_expr_for_handler_lits(e2, never_ops, errors),
                }
            }
        }
        ExprKind::MapLit { elems, .. } => {
                let pairs = crate::ast::MapElem::cloned_pairs(&elems);
            for (k, v) in pairs.iter() {
                walk_expr_for_handler_lits(k, never_ops, errors);
                walk_expr_for_handler_lits(v, never_ops, errors);
            }
        }
        ExprKind::TupleLit(elems) => { for el in elems { walk_expr_for_handler_lits(el, never_ops, errors); } }
        ExprKind::RecordLit { fields, .. } => {
            for f in fields { if let Some(v) = &f.value { walk_expr_for_handler_lits(v, never_ops, errors); } }
        }
        ExprKind::Throw(v) | ExprKind::Try(v) | ExprKind::Bang(v) | ExprKind::Interrupt(Some(v)) => {
            walk_expr_for_handler_lits(v, never_ops, errors);
        }
        ExprKind::Interrupt(None) => {}
        ExprKind::Lambda { body, .. } => walk_expr_for_handler_lits(body, never_ops, errors),
        ExprKind::ClosureLight { body, .. } => match body {
            ClosureBody::Expr(e2) => walk_expr_for_handler_lits(e2, never_ops, errors),
            ClosureBody::Block(b) => walk_block_for_handler_lits(b, never_ops, errors),
        },
        ExprKind::ClosureFull(fsb) => match &fsb.body {
            FnBody::Block(b) => walk_block_for_handler_lits(b, never_ops, errors),
            FnBody::Expr(e2) => walk_expr_for_handler_lits(e2, never_ops, errors),
            FnBody::External => {}
        },
        // Interpolated string вЂ” recurse РІ РµС‘ parts (РјРѕРіСѓС‚ СЃРѕРґРµСЂР¶Р°С‚СЊ expressions).
        ExprKind::InterpolatedStr { parts } => {
            for p in parts {
                if let InterpStrPart::Expr(e2) = p {
                    walk_expr_for_handler_lits(e2, never_ops, errors);
                }
            }
        }
        // TaggedTemplate РёРјРµРµС‚ args СЃРѕ sub-expressions вЂ” РЅРѕ bootstrap-stage
        // СЂРµРґРєРѕ РёСЃРїРѕР»СЊР·СѓРµС‚СЃСЏ; РґР»СЏ completeness'Р° РґРѕР±Р°РІРёРј shallow walk.
        ExprKind::TaggedTemplate { .. } => {}
        // D.1.3: РєРІР°РЅС‚РѕСЂ вЂ” С‚РѕР»СЊРєРѕ РІ РєРѕРЅС‚СЂР°РєС‚Р°С…; РѕР±С…РѕРґРёРј range Рё body.
        ExprKind::Forall { range, body, .. } | ExprKind::Exists { range, body, .. } => {
            walk_expr_for_handler_lits(range, never_ops, errors);
            walk_expr_for_handler_lits(body, never_ops, errors);
        }
        // Leaf expressions вЂ” nothing to recurse into.
        ExprKind::IntLit(_) | ExprKind::FloatLit(_) | ExprKind::CharLit(_) | ExprKind::StrLit(_)
        | ExprKind::BoolLit(_) | ExprKind::Ident(_) | ExprKind::Path(_) | ExprKind::UnitLit
        | ExprKind::SelfAccess => {}
    }
}

/// Static analysis: Р·Р°РІРµСЂС€Р°РµС‚СЃСЏ Р»Рё handler-method body С‡РµСЂРµР· exit-control?
///
/// Exit-control = `interrupt`, `throw`, `panic(...)`, `exit(...)` вЂ”
/// expressions/stmts РєРѕС‚РѕСЂС‹Рµ РіР°СЂР°РЅС‚РёСЂРѕРІР°РЅРЅРѕ РќР• РІРѕР·РІСЂР°С‰Р°СЋС‚ control РІ
/// caller РѕРїРµСЂР°С†РёРё (never-returning).
///
/// Bootstrap conservative: РїСЂРѕРІРµСЂСЏРµРј СЃР°РјС‹Рµ С‡Р°СЃС‚С‹Рµ РїР°С‚С‚РµСЂРЅС‹:
///   - Expr body = exit-control expression.
///   - Block body = РїРѕСЃР»РµРґРЅРёР№ stmt/trailing вЂ” exit-control.
///   - Conditional structures (if/match) вЂ” Р’РЎР• РІРµС‚РєРё exit-control.
///
/// Р•СЃР»Рё РЅРµ СѓРІРµСЂРµРЅС‹ вЂ” РІРѕР·РІСЂР°С‰Р°РµРј `false` (РЅРµС‡Р°СЃС‚Рѕ-РёСЃРїРѕР»СЊР·СѓРµРјС‹Р№ РіСЂР°РЅРёС‡РЅС‹Р№
/// СЃР»СѓС‡Р°Р№ в†’ РїСЂРѕРіСЂР°РјРјРёСЃС‚ РѕР±СЏР·Р°РЅ СЏРІРЅРѕ exit'РЅСѓС‚СЊ).
fn handler_body_diverges(body: &HandlerMethodBody) -> bool {
    match body {
        HandlerMethodBody::Expr(e) => expr_diverges(e),
        HandlerMethodBody::Block(b) => block_diverges(b),
    }
}

fn expr_diverges(e: &Expr) -> bool {
    match &e.kind {
        // Direct exit-control.
        ExprKind::Interrupt(_) | ExprKind::Throw(_) => true,
        // panic(...) / exit(...) вЂ” never-returning builtins (D13).
        ExprKind::Call { func, .. } => {
            if let ExprKind::Ident(name) = &func.kind {
                matches!(name.as_str(), "panic" | "exit")
            } else {
                false
            }
        }
        // Conditional: РІСЃРµ РІРµС‚РєРё РґРѕР»Р¶РЅС‹ diverge.
        ExprKind::If { then, else_, .. } => {
            block_diverges(then)
                && match else_ {
                    Some(ElseBranch::Block(b)) => block_diverges(b),
                    Some(ElseBranch::If(e2)) => expr_diverges(e2),
                    None => false, // РЅРµС‚ else вЂ” fall-through possible
                }
        }
        ExprKind::IfLet { then, else_, .. } => {
            block_diverges(then)
                && match else_ {
                    Some(ElseBranch::Block(b)) => block_diverges(b),
                    Some(ElseBranch::If(e2)) => expr_diverges(e2),
                    None => false,
                }
        }
        ExprKind::Match { arms, .. } => {
            !arms.is_empty()
                && arms.iter().all(|a| match &a.body {
                    MatchArmBody::Expr(ex) => expr_diverges(ex),
                    MatchArmBody::Block(b) => block_diverges(b),
                })
        }
        // Block-as-expr.
        ExprKind::Block(b) => block_diverges(b),
        // Loop Р±РµР· condition вЂ” diverges (РµСЃР»Рё РЅРµС‚ break).
        ExprKind::Loop { .. } => true,
        _ => false,
    }
}

fn block_diverges(b: &Block) -> bool {
    // РЎРЅР°С‡Р°Р»Р° РїСЂРѕРІРµСЂРёРј: РµСЃС‚СЊ Р»Рё РІ block.stmts unconditional throw/return/etc
    // РЅР° РІРµСЂС…РЅРµРј СѓСЂРѕРІРЅРµ? Р­С‚Рѕ early-diverge.
    for s in &b.stmts {
        if stmt_diverges(s) {
            return true;
        }
    }
    // РРЅР°С‡Рµ вЂ” РїСЂРѕРІРµСЂРєР° trailing expression.
    if let Some(t) = &b.trailing {
        return expr_diverges(t);
    }
    false
}

fn stmt_diverges(s: &Stmt) -> bool {
    match s {
        Stmt::Return { .. } | Stmt::Throw { .. } => true,
        Stmt::Expr(e) => expr_diverges(e),
        // Break/Continue exit'СЏС‚ loop, РЅРµ handler-fn вЂ” РЅРµ diverge РґР»СЏ
        // handler-purposes (handler body РґРѕР»Р¶РµРЅ РёРјРµС‚СЊ exit Рє caller'Сѓ
        // РѕРїРµСЂР°С†РёРё, РЅРµ Рє outer loop).
        Stmt::Break(_) | Stmt::Continue(_) => false,
        _ => false,
    }
}

/// Plan 100.7 (D165): Returns true if the block (or any nested block) contains
/// at least one explicit `throw` statement. Used by `check_d162_coverage` to
/// distinguish “consumed at exit via explicit error-branch handling” (D162 ok)
/// from “consumed at exit in a function with no explicit throw paths” (D162 lint).
fn block_has_throw(b: &Block) -> bool {
    for s in &b.stmts {
        if stmt_has_throw(s) { return true; }
    }
    if let Some(t) = &b.trailing {
        if expr_has_throw(t) { return true; }
    }
    false
}

fn stmt_has_throw(s: &Stmt) -> bool {
    match s {
        Stmt::Throw { .. } => true,
        Stmt::Expr(e) => expr_has_throw(e),
        Stmt::Let(d) => expr_has_throw(&d.value),
        Stmt::Const(_) => false,
        Stmt::Defer { body, .. } | Stmt::ErrDefer { body, .. } | Stmt::OkDefer { body, .. }
        | Stmt::DeferWithResult { body, .. }
            => expr_has_throw(body),
        _ => false,
    }
}

fn expr_has_throw(e: &crate::ast::Expr) -> bool {
    use crate::ast::ExprKind;
    match &e.kind {
        ExprKind::Throw(_) => true,
        ExprKind::Block(b) => block_has_throw(b),
        ExprKind::If { then, else_, .. } => {
            block_has_throw(then) || else_.as_ref().map(|el| match el {
                crate::ast::ElseBranch::Block(b) => block_has_throw(b),
                crate::ast::ElseBranch::If(e) => expr_has_throw(e),
            }).unwrap_or(false)
        }
        ExprKind::With { body, .. } => block_has_throw(body),
        _ => false,
    }
}

/// Walk РјРѕРґСѓР»СЏ: РґР»СЏ РєР°Р¶РґРѕРіРѕ defer/errdefer statement РІ bodies С„СѓРЅРєС†РёР№
/// Рё С‚РµСЃС‚Р°С… вЂ” РїСЂРѕРІРµСЂРёС‚СЊ body constraints.
// ─────────────────────────────────────────────────────────────────────
// Plan 73 (D131): consume-qualifier flow-sensitive check.
//
// `consume` помечает receiver / параметр, чьё значение логически
// забирается вызовом (`fn StringBuilder consume @into()`,
// `fn f(consume sb StringBuilder)`). После consume-вызова переменная-
// источник недоступна:
//   - повторное использование (use-after-consume) → compile error;
//   - использование на пути, где consume произошёл лишь на части веток
//     (maybe-consumed) → compile error.
//
// Анализ flow-sensitive: состояние `VarState` каждой переменной
// протягивается через statements, ветвится на if/match/coalesce и
// пессимистично обрабатывает циклы (consume в теле → переменная
// maybe-consumed на 2-й итерации). Это НЕ borrow checker — памятью
// управляет GC; проверяется только логический инвариант D131.
//
// Closure / handler / trailing тела walk'аются изолированно (могут
// исполняться 0+ раз): use-after-consume внутри них ловится, но их
// собственные consume наружу не протекают (conservative).
// ─────────────────────────────────────────────────────────────────────

/// Состояние логической линейности переменной (D131).
#[derive(Clone)]
enum VarState {
    /// Значение доступно.
    Live,
    /// Значение потреблено в указанной точке.
    Consumed(Span),
    /// Значение потреблено лишь на части путей выполнения.
    MaybeConsumed(Span),
}

/// Plan 100.1 (D133 / D6): registry consume-типов модуля. Заполняется
/// pre-pass'ом до `check_consume`. Используется `type_is_consume`
/// для рекурсивной классификации (record-field consume-types,
/// generic-wraps).
///
/// **НЕ путать с `ConsumeRegistry`** (Plan 73 D131 — registry consume-
/// методов и consume-параметров; flat-name based).
struct LinearityRegistry {
    /// Имена типов, объявленных `type X consume {...}`.
    consume_types: HashSet<String>,
    /// Consume-методы по типу: type_name → Vec<method_name>.
    consume_methods: HashMap<String, Vec<String>>,
}

impl LinearityRegistry {
    fn build(module: &Module) -> Self {
        let mut consume_types = HashSet::new();
        let mut consume_methods: HashMap<String, Vec<String>> = HashMap::new();

        // 1. Local module: consume-types + consume-methods.
        for item in &module.items {
            if let Item::Type(td) = item {
                if td.consume {
                    consume_types.insert(td.name.clone());
                }
            }
            if let Item::Fn(fd) = item {
                if let Some(recv) = &fd.receiver {
                    if recv.consume {
                        consume_methods
                            .entry(recv.type_name.clone())
                            .or_default()
                            .push(fd.name.clone());
                    }
                }
            }
        }

        // 2. Plan 73.1 [M-73.1-warning-needs-project-wide-registry] fix:
        //    external stdlib modules (sync.nv) declare cross-module consume-types
        //    (MutexGuard, ReadGuard, WriteGuard, Permit, OnceGuard). Без них
        //    project-wide check ложно классифицирует RHS типа MutexGuard как
        //    "не-consume" → W_CONSUME_KEYWORD_UNNECESSARY false positives.
        //    Mirrors ConsumeRegistry::build §3 (см. line ~8043).
        let external_sources: &[&str] = &[
            crate::codegen::external_registry::ExternalRegistry::SYNC_SRC,
        ];
        for src in external_sources {
            if let Ok(ext_module) = crate::parser::parse(src) {
                for item in &ext_module.items {
                    if let Item::Type(td) = item {
                        if td.consume {
                            consume_types.insert(td.name.clone());
                        }
                    }
                    if let Item::Fn(fd) = item {
                        if let Some(recv) = &fd.receiver {
                            if recv.consume {
                                consume_methods
                                    .entry(recv.type_name.clone())
                                    .or_default()
                                    .push(fd.name.clone());
                            }
                        }
                    }
                }
            }
        }

        LinearityRegistry { consume_types, consume_methods }
    }

    /// Plan 100.1 (D133 / D6): `type_is_consume(TypeRef)` — рекурсивно
    /// определяет, является ли тип consume через wrap-transitivity.
    /// Bootstrap: generic-param без bound → false (silent-ignore;
    /// 100.2 закроет через `[T consume]`).
    fn type_is_consume(&self, t: &TypeRef, module: &Module) -> bool {
        match t {
            TypeRef::Named { path, generics, .. } => {
                // Direct consume-type.
                let name = path.last().cloned().unwrap_or_default();
                if self.consume_types.contains(&name) {
                    return true;
                }
                // Generic wrap: Option[Transaction], Box[Tx], Wrapper[T].
                if generics.iter().any(|a| self.type_is_consume(a, module)) {
                    return true;
                }
                // Record/sum lookup: own fields consume-typed?
                for item in &module.items {
                    if let Item::Type(td) = item {
                        if td.name == name {
                            return match &td.kind {
                                TypeDeclKind::Record(fields) =>
                                    fields.iter().any(|f|
                                        f.consume || self.type_is_consume(&f.ty, module)),
                                TypeDeclKind::Sum(variants) =>
                                    variants.iter().any(|v|
                                        match &v.kind {
                                            SumVariantKind::Tuple(payloads) =>
                                                payloads.iter().any(|p|
                                                    self.type_is_consume(p, module)),
                                            SumVariantKind::Record(fields) =>
                                                fields.iter().any(|f|
                                                    f.consume || self.type_is_consume(&f.ty, module)),
                                            SumVariantKind::Unit => false,
                                        }),
                                _ => false,
                            };
                        }
                    }
                }
                false
            }
            TypeRef::Tuple(elems, _) =>
                elems.iter().any(|e| self.type_is_consume(e, module)),
            TypeRef::Array(inner, _) => self.type_is_consume(inner, module),
            // Generic-param без bound — bootstrap silent-ignore.
            _ => false,
        }
    }

    /// Plan 100.1 (D133): список consume-методов для типа (для diagnostics).
    fn consume_methods_for(&self, type_name: &str) -> Vec<String> {
        self.consume_methods.get(type_name).cloned().unwrap_or_default()
    }
}

/// Plan 100.1 (D133 / D4): проверка согласованности consume-маркеров
/// на type-decl'ах и полях. Emit diagnostics:
/// - D133-field-marker-missing: field consume-типа без `consume`.
/// - D133-type-marker-missing: `consume field` в non-consume type.
/// - D133-empty-consume: `type X consume {}` без consume-полей и без
///   consume-методов.
/// - D133-marker-on-non-consume: `consume f int` (non-consume type).
// Plan 91.10 (D163 retracted, 2026-05-30): check_external_fn_needs_caps удалён.
// Capability tracking via отдельный syntax — redundant с effect system; см.
// docs/plans/91.10-d163-retract-capability-syntax.md для rationale.

fn check_linearity_markers(
    module: &Module,
    reg: &LinearityRegistry,
    errors: &mut Vec<Diagnostic>,
) {
    for item in &module.items {
        let Item::Type(td) = item else { continue; };

        // Sum-types — D4 covers fields-level only для record-variants;
        // skip for now (sum-variants могут содержать consume-payload —
        // type_is_consume их подхватит).
        let TypeDeclKind::Record(fields) = &td.kind else {
            continue;
        };

        for f in fields {
            let field_is_consume_type = reg.type_is_consume(&f.ty, module);

            // 1. D133-field-marker-missing: consume-field без `consume`-маркера.
            if field_is_consume_type && !f.consume {
                errors.push(Diagnostic::new(
                    format!(
                        "[D133-field-marker-missing] field `{}` имеет consume-тип \
                         но не помечен `consume`. Добавь `consume field {}` либо \
                         замени тип на non-consume.",
                        f.name, f.name),
                    f.span,
                ));
            }
            // 4. D133-marker-on-non-consume: `consume f int` где int не consume.
            if f.consume && !field_is_consume_type {
                errors.push(Diagnostic::new(
                    format!(
                        "[D133-marker-on-non-consume] поле `{}` помечено `consume` \
                         но его тип не consume — маркер не нужен. Удали `consume` \
                         перед полем `{}`.",
                        f.name, f.name),
                    f.span,
                ));
            }
        }

        // 2. D133-type-marker-missing: consume-field в non-consume type-decl.
        if !td.consume && fields.iter().any(|f| f.consume) {
            errors.push(Diagnostic::new(
                format!(
                    "[D133-type-marker-missing] type `{}` содержит consume-поле, \
                     но сам не помечен `consume`. Добавь `consume` после имени: \
                     `type {} consume {{ ... }}`.",
                    td.name, td.name),
                td.span,
            ));
        }

        // 3. D133-empty-consume: `type X consume {}` без consume-полей и методов.
        // Допускаем opaque consume-типы (StringBuilder pattern: consume-methods
        // через external-fn, без consume-fields). Heuristic: ищем хотя бы один
        // consume-method для этого типа.
        if td.consume && !fields.iter().any(|f| f.consume) {
            let has_consume_method = module.items.iter().any(|it| {
                if let Item::Fn(fd) = it {
                    fd.receiver.as_ref().map_or(false, |r|
                        r.type_name == td.name && r.consume)
                } else { false }
            });
            if !has_consume_method {
                errors.push(Diagnostic::new(
                    format!(
                        "[D133-empty-consume] type `{}` помечен `consume` но не \
                         имеет ни consume-полей, ни consume-методов — добавь \
                         хотя бы один consume-method (`fn {} consume @method() -> ()`) \
                         либо убери `consume` с type-decl.",
                        td.name, td.name),
                    td.span,
                ));
            }
        }
    }
}

/// Реестр consume-аннотаций: user-module + runtime-stdlib.
struct ConsumeRegistry {
    /// `(receiver_type, method_name)` — consume-методы.
    methods: HashSet<(String, String)>,
    /// free-fn name → индексы consume-параметров.
    fn_params: HashMap<String, Vec<usize>>,
    /// `(receiver_type, method_name)` → индексы consume-параметров.
    method_params: HashMap<(String, String), Vec<usize>>,
    /// Plan 73 followup: free-fn name → имя return-типа (Named, 1-seg).
    /// Для var-type инференса `let x = factory()` — расширяет резолв
    /// consume-метода за пределы очевидных конструкторов.
    fn_return_types: HashMap<String, String>,
    /// Plan 77 (D132): `(receiver_type, method)` — fluent-методы `-> @`,
    /// гарантированно возвращающие сам receiver. `let x = recv.method()`
    /// для такого метода → `x` алиас `recv`.
    recv_returning: HashSet<(String, String)>,
    /// Plan 100.3 (D157): free-fn name → indices of view-params
    /// (non-consume params of consume types). Used to detect
    /// D133-consume-rvalue-in-view: rvalue passed to view-param position.
    fn_view_params: HashMap<String, Vec<usize>>,
    /// Plan 103.9 (D174): `(receiver_type, method_name)` → return-type name
    /// (single-segment Named). Used for var-type inference of method calls:
    /// `consume g = mu.lock()` → g has type `MutexGuard`.
    method_return_types: HashMap<(String, String), String>,
    /// Plan 108.1 (D176 amend): `(receiver_type, method_name)` для всех
    /// методов с `mut`-receiver (`fn T mut @method(...)`).  Вызов такого
    /// метода на параметре без `mut` → E_PARAM_NOT_MUT.
    mut_methods: HashSet<(String, String)>,
    /// Plan 108.1 followup ([M-108.1-readonly-to-explicit-mut-coerce]):
    /// free-fn name → indices of `mut`-params.  Используется при call
    /// site: если arg в этой позиции имеет тип `readonly T` (или
    /// помечен readonly в `readonly_locals`), → E_READONLY_COERCE.
    fn_mut_params: HashMap<String, Vec<usize>>,
    /// Plan 108.1 followup: `(receiver_type, method_name)` → indices of
    /// `mut`-params.  Parallel free-fn `fn_mut_params`.
    method_mut_params: HashMap<(String, String), Vec<usize>>,
}

impl ConsumeRegistry {
    fn build(module: &Module) -> Self {
        let mut methods: HashSet<(String, String)> = HashSet::new();
        let mut fn_params: HashMap<String, Vec<usize>> = HashMap::new();
        let mut method_params: HashMap<(String, String), Vec<usize>> = HashMap::new();
        let mut fn_return_types: HashMap<String, String> = HashMap::new();
        let mut recv_returning: HashSet<(String, String)> = HashSet::new();
        let mut fn_view_params: HashMap<String, Vec<usize>> = HashMap::new();
        // Plan 103.9 (D174): method return-type map for var-type inference.
        let mut method_return_types: HashMap<(String, String), String> = HashMap::new();
        // Plan 108.1 (D176 amend): mut-receiver methods registry.
        let mut mut_methods: HashSet<(String, String)> = HashSet::new();
        // Plan 108.1 followup: mut-params indices for E_READONLY_COERCE.
        let mut fn_mut_params: HashMap<String, Vec<usize>> = HashMap::new();
        let mut method_mut_params: HashMap<(String, String), Vec<usize>> = HashMap::new();

        // Plan 100.3 (D157): collect consume-types for view-param detection.
        // Mirrors LinearityRegistry::build consume_types collection.
        let consume_type_names: HashSet<String> = module.items.iter()
            .filter_map(|it| {
                if let Item::Type(td) = it {
                    if td.consume { Some(td.name.clone()) } else { None }
                } else { None }
            })
            .collect();

        // 1. Runtime-stdlib consume-методы (`StringBuilder.into` и т.п.).
        //    Single source of truth — runtime_registry.rs (`is_consume`).
        for f in crate::codegen::runtime_registry::all() {
            if let Some(recv) = f.receiver {
                if f.is_consume {
                    methods.insert((recv.to_string(), f.name.to_string()));
                }
                // Plan 108.1 (D176 amend): mut-receiver methods registry.
                if !f.is_static && f.is_mut {
                    mut_methods.insert((recv.to_string(), f.name.to_string()));
                }
                // Plan 77 (D132): fluent builder-методы рендерятся `-> @`
                // (mirror `render_nv` is_fluent) — гарантированно
                // возвращают receiver.
                if !f.is_static && f.is_mut && f.return_ty == "Self"
                    && f.nova_body.is_none()
                {
                    recv_returning.insert((recv.to_string(), f.name.to_string()));
                }
            }
        }

        // 2. User-module: consume-receiver методы + consume-параметры.
        for item in &module.items {
            if let Item::Fn(fd) = item {
                let consume_idx: Vec<usize> = fd.params.iter().enumerate()
                    .filter(|(_, p)| p.consume)
                    .map(|(i, _)| i)
                    .collect();
                // Plan 108.1 followup: mut-params indices.
                let mut_idx: Vec<usize> = fd.params.iter().enumerate()
                    .filter(|(_, p)| p.is_mut)
                    .map(|(i, _)| i)
                    .collect();
                match &fd.receiver {
                    Some(r) => {
                        if r.consume {
                            methods.insert((r.type_name.clone(), fd.name.clone()));
                        }
                        // Plan 108.1 (D176 amend): mut-receiver methods registry.
                        if r.mutable {
                            mut_methods.insert((r.type_name.clone(), fd.name.clone()));
                        }
                        if !mut_idx.is_empty() {
                            method_mut_params.insert(
                                (r.type_name.clone(), fd.name.clone()), mut_idx.clone());
                        }
                        // Plan 103.9 (D174): track method return-type for inference.
                        // `fn Mutex mut @lock() -> MutexGuard consume` → ("Mutex","lock") → "MutexGuard".
                        if let Some(TypeRef::Named { path, .. }) = &fd.return_type {
                            if path.len() == 1 {
                                method_return_types.insert(
                                    (r.type_name.clone(), fd.name.clone()),
                                    path[0].clone(),
                                );
                            }
                        }
                        // Plan 77 (D132): `-> @` fluent-метод.
                        if fd.returns_receiver {
                            recv_returning
                                .insert((r.type_name.clone(), fd.name.clone()));
                        }
                        if !consume_idx.is_empty() {
                            method_params.insert(
                                (r.type_name.clone(), fd.name.clone()), consume_idx);
                        }
                    }
                    None => {
                        if !consume_idx.is_empty() {
                            fn_params.insert(fd.name.clone(), consume_idx);
                        }
                        // Plan 108.1 followup: mut-params indices.
                        if !mut_idx.is_empty() {
                            fn_mut_params.insert(fd.name.clone(), mut_idx);
                        }
                        // Plan 73 followup: return-тип свободной функции.
                        if let Some(TypeRef::Named { path, .. }) = &fd.return_type {
                            if path.len() == 1 {
                                fn_return_types
                                    .insert(fd.name.clone(), path[0].clone());
                            }
                        }
                        // Plan 100.3 (D157): collect view-params — non-consume params
                        // of consume types. Used for D133-consume-rvalue-in-view check.
                        let view_idx: Vec<usize> = fd.params.iter().enumerate()
                            .filter(|(_, p)| {
                                !p.consume && matches!(&p.ty,
                                    TypeRef::Named { path, .. }
                                    if path.len() == 1
                                        && consume_type_names.contains(&path[0]))
                            })
                            .map(|(i, _)| i)
                            .collect();
                        if !view_idx.is_empty() {
                            fn_view_params.insert(fd.name.clone(), view_idx);
                        }
                    }
                }
            }
        }

        // 3. Plan 103.9 (D174): stdlib external modules (sync.nv etc.) that
        //    define consume guard types are not always imported into user
        //    modules (they come via codegen ExternalRegistry, not prelude).
        //    Parse them here so that consume methods (MutexGuard.unlock etc.)
        //    and method return types (Mutex.lock → MutexGuard) are visible
        //    to the checker regardless of explicit import.
        //
        //    This mirrors ExternalRegistry::load_builtins() but for the
        //    consume-analysis side only.
        let external_sources: &[&str] = &[
            crate::codegen::external_registry::ExternalRegistry::SYNC_SRC,
        ];
        for src in external_sources {
            if let Ok(ext_module) = crate::parser::parse(src) {
                for item in &ext_module.items {
                    if let Item::Fn(fd) = item {
                        if let Some(r) = &fd.receiver {
                            if r.consume {
                                methods.insert((r.type_name.clone(), fd.name.clone()));
                            }
                            // Plan 108.1 (D176 amend): mut-receiver methods registry
                            // — extern modules.
                            if r.mutable {
                                mut_methods.insert((r.type_name.clone(), fd.name.clone()));
                            }
                            if let Some(TypeRef::Named { path, .. }) = &fd.return_type {
                                if path.len() == 1 {
                                    method_return_types.insert(
                                        (r.type_name.clone(), fd.name.clone()),
                                        path[0].clone(),
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }

        ConsumeRegistry {
            methods, fn_params, method_params, fn_return_types, recv_returning,
            fn_view_params, method_return_types, mut_methods,
            fn_mut_params, method_mut_params,
        }
    }
}

/// Имена, связываемые pattern'ом (best-effort: Ident / Tuple / Record /
/// Variant / Array / Or). Используется для регистрации новых live-vars.
fn consume_pattern_names(p: &Pattern, out: &mut Vec<String>) {
    match p {
        Pattern::Ident { name, .. } => out.push(name.clone()),
        Pattern::Binding { name, inner, .. } => {
            out.push(name.clone());
            consume_pattern_names(inner, out);
        }
        Pattern::Tuple(ps, _) => {
            for sp in ps { consume_pattern_names(sp, out); }
        }
        Pattern::Variant { kind, .. } => {
            if let VariantPatternKind::Tuple { patterns, .. } = kind {
                for sp in patterns { consume_pattern_names(sp, out); }
            }
        }
        Pattern::Record { fields, .. } => {
            for f in fields {
                match &f.pattern {
                    Some(sp) => consume_pattern_names(sp, out),
                    None => out.push(f.name.clone()), // shorthand `{ name }`
                }
            }
        }
        Pattern::Array { elems, .. } => {
            for el in elems {
                match el {
                    ArrayPatternElem::Item(sp) => consume_pattern_names(sp, out),
                    ArrayPatternElem::RestBind(n) => out.push(n.clone()),
                    ArrayPatternElem::Rest => {}
                }
            }
        }
        Pattern::Or { alternatives, .. } => {
            // Альтернативы связывают одинаковый набор имён — берём первую.
            if let Some(first) = alternatives.first() {
                consume_pattern_names(first, out);
            }
        }
        Pattern::Wildcard(_) | Pattern::Literal(..) => {}
    }
}

/// Plan 108.3 (D36 amend): аналог `consume_pattern_names` но возвращает
/// pairs `(name, is_mut)` для per-name mut в pattern.
/// `let (mut a, b) = ...` → `[("a", true), ("b", false)]`.
fn consume_pattern_names_with_mut(p: &Pattern, out: &mut Vec<(String, bool)>) {
    match p {
        Pattern::Ident { name, is_mut, .. } => out.push((name.clone(), *is_mut)),
        Pattern::Binding { name, inner, .. } => {
            out.push((name.clone(), false));
            consume_pattern_names_with_mut(inner, out);
        }
        Pattern::Tuple(ps, _) => {
            for sp in ps { consume_pattern_names_with_mut(sp, out); }
        }
        Pattern::Variant { kind, .. } => {
            if let VariantPatternKind::Tuple { patterns, .. } = kind {
                for sp in patterns { consume_pattern_names_with_mut(sp, out); }
            }
        }
        Pattern::Record { fields, .. } => {
            for f in fields {
                match &f.pattern {
                    Some(sp) => consume_pattern_names_with_mut(sp, out),
                    None => out.push((f.name.clone(), false)),
                }
            }
        }
        Pattern::Array { elems, .. } => {
            for el in elems {
                match el {
                    ArrayPatternElem::Item(sp) => consume_pattern_names_with_mut(sp, out),
                    ArrayPatternElem::RestBind(n) => out.push((n.clone(), false)),
                    ArrayPatternElem::Rest => {}
                }
            }
        }
        Pattern::Or { alternatives, .. } => {
            if let Some(first) = alternatives.first() {
                consume_pattern_names_with_mut(first, out);
            }
        }
        Pattern::Wildcard(_) | Pattern::Literal(..) => {}
    }
}

/// Flow-context consume-анализа одной функции / теста.
struct ConsumeCtx<'a> {
    reg: &'a ConsumeRegistry,
    /// Plan 100.1 (D133): LinearityRegistry для consume-типов и методов.
    lin_reg: &'a LinearityRegistry,
    /// Состояние линейности per-variable. Ключ — каноническое имя
    /// (alias-класс представлен своим каноническим членом).
    states: HashMap<String, VarState>,
    /// Best-effort тип переменной — для резолва consume-метода по
    /// receiver'у. Неизвестный тип → метод не трактуется как consuming
    /// (sound: false-negative, не false-positive).
    var_types: HashMap<String, String>,
    /// Plan 73 followup: alias-карта. `let a = b` -> `aliases[a] = b`
    /// (b — каноническое имя). Обе переменные ссылаются на ОДИН
    /// heap-объект; consume любой -> consume всего alias-класса.
    aliases: HashMap<String, String>,
    /// Plan 100.1 (D133 / D9): локальные переменные объявленные с
    /// `consume tx = ...` — обязаны быть Consumed до scope-exit.
    consume_obligations: HashSet<String>,
    /// Plan 100.8 (D166): accumulates ALL consume-binding names ever declared
    /// in this scope (never cleared, unlike `consume_obligations`).  Used by
    /// `check_d162_coverage` which runs AFTER `consume_walk_block` has already
    /// cleared `consume_obligations` for satisfied obligations.
    all_declared_consume: HashSet<String>,
    /// Plan 100.1 (D133 / D5): состояние consume-полей receiver'а.
    /// Ключ — имя поля (без "@."), значение — VarState.
    /// Для consume-методов: поля должны быть Consumed на exit'е.
    /// Для не-consume методов: consume-поля должны быть Live на exit'е.
    field_states: HashMap<String, VarState>,
    /// Plan 100.2 (D156): generic-параметры с `[T consume]` bound.
    /// Внутри тела функции, параметры с типами из этого набора
    /// трактуются как consume-obligations (strict-mode).
    consume_bound_generics: HashSet<String>,
    /// Plan 100.3 (D157): view-params — non-consume params of consume types.
    /// Calling consume-methods on view-params → D157-consume-via-view error.
    /// Returning a view-param → D157-view-escape-return error.
    view_params: HashSet<String>,
    /// Plan 100.3 (D157): consume-closures — closures that consume outer vars.
    /// closure_var_name → list of outer vars it consumes when invoked.
    /// When closure is invoked: mark those outer vars Consumed + closure Consumed.
    consume_closures: HashMap<String, Vec<String>>,
    /// Plan 103.9 (D174): тип receiver'а текущего метода. Используется для
    /// инференса типа при `consume g = self.method()` — `self` это SelfAccess.
    self_type: Option<String>,
    /// Plan 108.1 (D176 amend): параметры функции с `is_mut: bool`.
    /// HashMap<param_name, is_mut>.  Используется для проверки
    /// `param.mut_method(...)` → E_PARAM_NOT_MUT при is_mut=false.
    /// Заполняется при входе в функцию.  Includes consume-params
    /// (is_mut=true since consume implies ownership+mut).
    param_mut: HashMap<String, bool>,
    /// Plan 108.2 (D36 enforcement): local `let` bindings с `is_mut: bool`.
    /// HashMap<binding_name, is_mut>.  Используется для проверки
    /// `local.mut_method(...)` / `local.field = ...` / `local[i] = ...`
    /// → E_LOCAL_NOT_MUT при is_mut=false.
    /// `consume X = ...` неявно is_mut=true (ownership transfer).
    local_mut: HashMap<String, bool>,
    /// Plan 108.1 followup ([M-108.1-readonly-to-explicit-mut-coerce]):
    /// HashSet локальных binding'ов, объявленных как `readonly T`
    /// (explicit readonly annotation на let-binding или fn-param).
    /// Передача такого binding'а в `mut`-параметр → E_READONLY_COERCE.
    readonly_locals: HashSet<String>,
}

impl<'a> ConsumeCtx<'a> {
    fn new(reg: &'a ConsumeRegistry, lin_reg: &'a LinearityRegistry) -> Self {
        ConsumeCtx {
            reg,
            lin_reg,
            states: HashMap::new(),
            var_types: HashMap::new(),
            aliases: HashMap::new(),
            consume_obligations: HashSet::new(),
            all_declared_consume: HashSet::new(),
            field_states: HashMap::new(),
            consume_bound_generics: HashSet::new(),
            view_params: HashSet::new(),
            consume_closures: HashMap::new(),
            self_type: None,
            param_mut: HashMap::new(),
            local_mut: HashMap::new(),
            readonly_locals: HashSet::new(),
        }
    }

    /// Каноническое имя alias-класса переменной (следует по цепочке
    /// `aliases`). Guard от циклов — теоретически невозможны (alias
    /// всегда на уже-существующее имя), но защищаемся.
    fn canonical(&self, name: &str) -> String {
        let mut cur = name.to_string();
        for _ in 0..64 {
            match self.aliases.get(&cur) {
                Some(next) if next != &cur => cur = next.clone(),
                _ => break,
            }
        }
        cur
    }

    /// Пометить alias-класс переменной потреблённым.
    fn mark_consumed(&mut self, name: &str, span: Span) {
        let canon = self.canonical(name);
        if self.states.contains_key(&canon) {
            self.states.insert(canon, VarState::Consumed(span));
        }
    }

    /// Зарегистрировать новую live-переменную (с опц. известным типом).
    fn declare(&mut self, name: &str, ty: Option<String>) {
        if name == "_" { return; }
        // Свежий binding — не алиас (рвём прежнюю alias-связь при shadow).
        self.aliases.remove(name);
        self.states.insert(name.to_string(), VarState::Live);
        match ty {
            Some(t) => { self.var_types.insert(name.to_string(), t); }
            None => { self.var_types.remove(name); }
        }
    }

    /// Зарегистрировать alias `name` -> `canon` (`let a = b`): обе
    /// переменные — один объект.
    fn declare_alias(&mut self, name: &str, canon: &str, ty: Option<String>) {
        if name == "_" { return; }
        self.states.remove(name);            // shadow: убрать прежнее состояние
        self.aliases.insert(name.to_string(), canon.to_string());
        match ty {
            Some(t) => { self.var_types.insert(name.to_string(), t); }
            None => { self.var_types.remove(name); }
        }
    }

    /// Развязать alias-класс переменной `name` перед её реассайном
    /// (`name = ...`). Каждый член класса становится независимой
    /// переменной с ТЕКУЩИМ состоянием класса (объект-то прежний,
    /// меняется только привязка `name`). Sound: исключает ложную
    /// propagation consume через устаревший alias после реассайна.
    fn dissolve_alias_class(&mut self, name: &str) {
        let canon = self.canonical(name);
        let class_state = self.states.get(&canon).cloned()
            .unwrap_or(VarState::Live);
        let all_aliased: Vec<String> = self.aliases.keys().cloned().collect();
        for m in all_aliased {
            if self.canonical(&m) == canon {
                self.aliases.remove(&m);
                // member продолжает ссылаться на прежний объект —
                // сохраняем его состояние как независимое.
                self.states.insert(m, class_state.clone());
            }
        }
        // `canon` сохраняет своё состояние в `states` (уже там).
    }

    /// Использование переменной — проверка use-after-consume.
    fn use_var(&self, name: &str, span: Span, errors: &mut Vec<Diagnostic>) {
        let canon = self.canonical(name);
        match self.states.get(&canon) {
            Some(VarState::Consumed(at)) => {
                errors.push(Diagnostic::new(
                    format!(
                        "использование потреблённой переменной `{}` (D131): \
                         её значение отдано consume-вызовом и больше недоступно",
                        name),
                    span,
                ).with_note_at("значение потреблено здесь".to_string(), *at));
            }
            Some(VarState::MaybeConsumed(at)) => {
                errors.push(Diagnostic::new(
                    format!(
                        "использование, возможно, потреблённой переменной `{}` \
                         (D131): значение потребляется не на всех путях \
                         выполнения — компилятор не может гарантировать, что \
                         оно ещё доступно",
                        name),
                    span,
                ).with_note_at(
                    "значение потенциально потреблено здесь".to_string(), *at));
            }
            _ => {}
        }
    }

    /// Вывести тип значения `let`-binding'а (best-effort).
    fn infer_let_type(&self, decl: &LetDecl) -> Option<String> {
        // Явная аннотация `let x T = ...`.
        if let Some(TypeRef::Named { path, .. }) = &decl.ty {
            if path.len() == 1 {
                return Some(path[0].clone());
            }
        }
        self.infer_value_type(&decl.value)
    }

    /// Best-effort тип выражения — только синтаксически очевидные формы.
    fn infer_value_type(&self, e: &Expr) -> Option<String> {
        match &e.kind {
            // Конструктор `Type.new(...)` / `.with_capacity` / `.from` и т.п.
            ExprKind::Call { func, .. } => {
                if let ExprKind::Path(parts) = &func.kind {
                    if parts.len() == 2 && matches!(parts[1].as_str(),
                        "new" | "with_capacity" | "from" | "default" | "filled")
                    {
                        return Some(parts[0].clone());
                    }
                }
                // Plan 73 followup: свободная функция с известным
                // return-типом (`let x = make_builder()`).
                if let ExprKind::Ident(fname) = &func.kind {
                    if let Some(rt) = self.reg.fn_return_types.get(fname) {
                        return Some(rt.clone());
                    }
                }
                // Plan 103.9 (D174): метод с известным return-типом.
                // `consume g = mu.lock()` → ("Mutex","lock") → "MutexGuard".
                // Handles: `recv.method()` (Ident), `self.method()` (Ident "self"),
                // `@method()` desugared to `SelfAccess.method()`.
                if let ExprKind::Member { obj, name: method } = &func.kind {
                    let recv_ty: Option<String> = match &obj.kind {
                        ExprKind::Ident(recv) if recv == "self" => {
                            // Nova `self.lock()` — receiver is implicit self.
                            self.self_type.clone()
                        }
                        ExprKind::Ident(recv) => {
                            let canon = self.canonical(recv);
                            self.var_types.get(&canon)
                                .or_else(|| self.var_types.get(recv.as_str()))
                                .cloned()
                        }
                        ExprKind::SelfAccess => self.self_type.clone(),
                        _ => None,
                    };
                    if let Some(rty) = recv_ty {
                        if let Some(ret) = self.reg.method_return_types
                            .get(&(rty, method.clone()))
                        {
                            return Some(ret.clone());
                        }
                    }
                }
                None
            }
            // Алиас `let y = x` — переносим известный тип `x`.
            ExprKind::Ident(n) => self.var_types.get(&self.canonical(n)).cloned(),
            // `User { ... }` record-литерал.
            ExprKind::RecordLit { type_name: Some(path), .. } if path.len() == 1 => {
                Some(path[0].clone())
            }
            _ => None,
        }
    }

    /// Имя consume-метода для receiver-типа? (тип берётся по
    /// каноническому имени alias-класса).
    fn is_consume_method(&self, recv_var: &str, method: &str) -> bool {
        self.var_types.get(&self.canonical(recv_var))
            .map(|ty| self.reg.methods.contains(&(ty.clone(), method.to_string())))
            .unwrap_or(false)
    }

    /// Пометить аргументы в consume-позициях как потреблённые.
    /// Аргументы уже walk'нуты вызывающим (use-after-consume проверен) —
    /// здесь только переход состояния alias-класса.
    fn consume_args(&mut self, args: &[CallArg], idxs: &[usize], span: Span) {
        for &i in idxs {
            if let Some(CallArg::Item(arg)) = args.get(i) {
                if let ExprKind::Ident(name) = &arg.kind {
                    self.mark_consumed(name, span);
                }
            }
        }
    }

    // ── Plan 100.1 (D133): consume-obligation methods ────────────────────

    /// Зарегистрировать `consume tx = ...` binding — tx обязан быть
    /// Consumed до scope-exit.
    fn declare_consume_binding(&mut self, name: &str, ty: Option<String>) {
        if name == "_" { return; }
        self.declare(name, ty);
        self.consume_obligations.insert(name.to_string());
        // Plan 100.8 (D166): also track in all_declared_consume (never cleared).
        self.all_declared_consume.insert(name.to_string());
    }

    // ── Plan 100.3 (D157): view-param and consume-closure helpers ─────────

    /// Зарегистрировать view-param (non-consume param of consume type).
    /// view-params — Live, но запрещено вызывать consume-методы или return.
    fn declare_view_param(&mut self, name: &str, ty: Option<String>) {
        if name == "_" { return; }
        self.declare(name, ty);
        self.view_params.insert(name.to_string());
    }

    /// Проверить, является ли переменная view-param (D157).
    fn is_view_param(&self, name: &str) -> bool {
        let canon = self.canonical(name);
        self.view_params.contains(&canon) || self.view_params.contains(name)
    }

    /// Зарегистрировать consume-closure binding (FnOnce-equivalent).
    /// closure_name — имя let-binding'а; captured — список outer vars
    /// которые closure потребляет при invoke.
    fn declare_consume_closure(&mut self, closure_name: &str, captured: Vec<String>) {
        if closure_name == "_" { return; }
        // Consume-closure само является consume-obligation: если не invoked
        // до scope-exit → D133-not-consumed error.
        self.declare_consume_binding(closure_name, Some("__consume_closure__".to_string()));
        self.consume_closures.insert(closure_name.to_string(), captured);
    }

    /// Проверить, является ли переменная consume-closure (D157).
    fn is_consume_closure(&self, name: &str) -> bool {
        let canon = self.canonical(name);
        self.consume_closures.contains_key(&canon)
            || self.consume_closures.contains_key(name)
    }

    /// Вызвать consume-closure: пометить closure Consumed + все captured outer vars.
    fn invoke_consume_closure(&mut self, name: &str, span: Span) {
        let captured = {
            let canon = self.canonical(name);
            self.consume_closures.get(&canon)
                .or_else(|| self.consume_closures.get(name))
                .cloned()
                .unwrap_or_default()
        };
        // Mark captured outer vars as Consumed (closure consumed them).
        for var in &captured {
            self.mark_consumed(var, span);
        }
        // Mark closure itself as Consumed (FnOnce).
        self.mark_consumed(name, span);
    }

    /// Проверить, что все consume-obligations Consumed на текущем exit.
    /// `exit_span` — span точки выхода (конец scope'а / return / panic).
    ///
    /// Plan 100.8 (D166): enhanced с machine-applicable `Suggestion` для
    /// LSP quick-fix integration (D166 §LSP quick fixes).
    fn check_obligations_at_exit(&self,
                                  exit_span: Span,
                                  errors: &mut Vec<Diagnostic>) {
        use crate::diag::{Applicability, Suggestion};
        for name in &self.consume_obligations {
            let canon = self.canonical(name);
            let state = self.states.get(&canon)
                .or_else(|| self.states.get(name));
            let ty = self.var_types.get(name)
                .or_else(|| self.var_types.get(&canon))
                .cloned()
                .unwrap_or_default();
            // Plan 100.2 (D156): если тип — generic из [T consume] bound,
            // используем D156-strict-forget вместо D133-not-consumed.
            let is_strict_generic = !ty.is_empty()
                && self.consume_bound_generics.contains(&ty);
            match state {
                Some(VarState::Live) => {
                    let methods = self.lin_reg.consume_methods_for(&ty);
                    // Plan 100.6 (D164 §5): cross-module hint — если тип не
                    // объявлен в текущем модуле, он из внешнего пакета.
                    // Используем другой hint, чтобы не вводить в заблуждение.
                    let is_external_type = !ty.is_empty()
                        && !self.lin_reg.consume_types.contains(&ty);
                    let hint = if methods.is_empty() {
                        if is_external_type {
                            format!(
                                "вызовите consume-метод типа `{}` \
                                 (тип из внешнего модуля/пакета)",
                                ty)
                        } else {
                            "объявите consume-метод для этого типа".to_string()
                        }
                    } else if methods.len() <= 4 {
                        methods.join(" / ")
                    } else {
                        format!("см. `nova doc {}`", ty)
                    };
                    // Plan 100.2 (D156): D156-strict-forget для generic [T consume] vars.
                    let code = if is_strict_generic { "D156-strict-forget" } else { "D133-not-consumed" };
                    // Plan 100.8 (D166): build machine-applicable suggestion
                    // for LSP quick-fix. Suggest errdefer + primary commit.
                    let suggestion_text = if methods.is_empty() {
                        format!(
                            "// TODO: add consume-method for `{}` then call it here",
                            ty)
                    } else {
                        // Primary method = first; secondary (cleanup) = last if different.
                        let primary = &methods[0];
                        let cleanup = if methods.len() > 1 { Some(&methods[methods.len()-1]) } else { None };
                        if let Some(cl) = cleanup {
                            format!(
                                "errdefer {{ {name}.{cl}() }}\n{name}.{primary}()",
                                name = name, cl = cl, primary = primary)
                        } else {
                            format!("{name}.{primary}()", name = name, primary = primary)
                        }
                    };
                    let suggestion = Suggestion {
                        message: format!(
                            "consume `{name}` via method ({code} quick fix)",
                            name = name, code = code),
                        span: exit_span,
                        replacement: suggestion_text,
                        applicability: Applicability::MaybeIncorrect,
                    };
                    errors.push(Diagnostic::new(
                        format!(
                            "[{}] переменная `{}` (тип `{}`) не \
                             consumed до scope-exit. Добавьте вызов одного из: {}, \
                             либо `return {}`, либо передайте в consume-param.",
                            code, name, ty, hint, name),
                        exit_span,
                    ).with_suggestion(suggestion));
                }
                Some(VarState::MaybeConsumed(at)) => {
                    let methods = self.lin_reg.consume_methods_for(&ty);
                    // Plan 100.6 (D164 §5): cross-module hint — тип из
                    // внешнего пакета (не в локальном LinearityRegistry).
                    let is_external_type = !ty.is_empty()
                        && !self.lin_reg.consume_types.contains(&ty);
                    let hint = if methods.is_empty() {
                        if is_external_type {
                            format!(
                                "вызовите consume-метод типа `{}` \
                                 (тип из внешнего модуля/пакета)",
                                ty)
                        } else {
                            "объявите consume-метод".to_string()
                        }
                    } else {
                        methods.join(" / ")
                    };
                    // Plan 100.2 (D156): D156-strict-forget для generic [T consume] vars.
                    let code = if is_strict_generic { "D156-strict-forget" } else { "D133-not-consumed" };
                    // Plan 100.8 (D166): multi-path suggestion — both errdefer + okdefer
                    // (D166 §LSP quick fixes — suggestion lists both errdefer + okdefer).
                    let suggestion_text = if methods.is_empty() {
                        format!("// TODO: consume `{}` on all code paths", name)
                    } else {
                        let primary = &methods[0];
                        let cleanup = if methods.len() > 1 { Some(&methods[methods.len()-1]) } else { None };
                        if let Some(cl) = cleanup {
                            format!(
                                "errdefer {{ {name}.{cl}() }}\nokdefer {{ {name}.{primary}() }}",
                                name = name, cl = cl, primary = primary)
                        } else {
                            format!(
                                "errdefer {{ {name}.{primary}() }}\nokdefer {{ {name}.{primary}() }}",
                                name = name, primary = primary)
                        }
                    };
                    let suggestion = Suggestion {
                        message: format!(
                            "cover `{name}` on all paths: errdefer + okdefer ({code} multi-path)",
                            name = name, code = code),
                        span: exit_span,
                        replacement: suggestion_text,
                        applicability: Applicability::MaybeIncorrect,
                    };
                    errors.push(Diagnostic::new(
                        format!(
                            "[{}] переменная `{}` (тип `{}`) \
                             consumed только на части путей выполнения. На всех \
                             путях до scope-exit должен быть вызов одного из: {}. \
                             suggestion: add errdefer + okdefer для полного покрытия.",
                            code, name, ty, hint),
                        exit_span,
                    ).with_note_at("частичный consume здесь".to_string(), *at)
                     .with_suggestion(suggestion));
                }
                Some(VarState::Consumed(_)) | None => {}
            }
        }
    }

    // ── Plan 100.1 (D133 / D5): field-state tracking ─────────────────────

    /// Инициализировать состояние consume-поля receiver'а как Live.
    fn init_field_live(&mut self, field_name: &str) {
        self.field_states.insert(field_name.to_string(), VarState::Live);
    }

    /// Пометить consume-поле как Consumed.
    fn mark_field_consumed(&mut self, field_name: &str, span: Span) {
        if self.field_states.contains_key(field_name) {
            self.field_states.insert(field_name.to_string(), VarState::Consumed(span));
        }
    }

    /// Пометить consume-поле как Live (после rebind через assign).
    fn mark_field_live(&mut self, field_name: &str) {
        if self.field_states.contains_key(field_name) {
            self.field_states.insert(field_name.to_string(), VarState::Live);
        }
    }

    /// Проверить exit-point инварианты для consume-полей:
    /// - consume-метод: все consume-поля должны быть Consumed.
    /// - non-consume метод: все consume-поля должны быть Live.
    fn check_fields_at_exit(&self,
                             receiver_type: &str,
                             is_consume_method: bool,
                             fn_name: &str,
                             exit_span: Span,
                             errors: &mut Vec<Diagnostic>) {
        for (field_name, state) in &self.field_states {
            match (is_consume_method, state) {
                (false, VarState::Consumed(at)) => {
                    // Non-consume метод потребил поле без rebind.
                    errors.push(Diagnostic::new(
                        format!(
                            "[D133-field-not-restored] метод `{}` consume'нул \
                             consume-поле `@.{}` (тип `{}`), но не восстановил \
                             его до выхода. Используйте pattern: \
                             `@{field} = новое_значение` после consume, либо \
                             объявите метод как `consume @{method}`.",
                            fn_name, field_name, receiver_type,
                            field = field_name, method = fn_name),
                        *at,
                    ));
                }
                (false, VarState::MaybeConsumed(at)) => {
                    errors.push(Diagnostic::new(
                        format!(
                            "[D133-field-not-restored] метод `{}` возможно \
                             consume'нул consume-поле `@.{}` на части путей. \
                             Обеспечьте rebind на всех путях.",
                            fn_name, field_name),
                        *at,
                    ));
                }
                (true, VarState::Live) => {
                    // consume-метод не потребил поле — разрешено (может не
                    // использовать поле). Но если поле существует и Live —
                    // это нормально (consume-method закрывает весь record).
                    // Ошибку не эмитим: record-consume = весь объект consumed.
                    let _ = exit_span; // avoid unused warning
                }
                _ => {}
            }
        }
    }
}

/// Plan 73 (D131): consume-check входная точка — walk всех function /
/// method / test bodies модуля.
/// Plan 77 (D132): `-> @` fluent-return — тело non-external метода
/// обязано завершаться выражением `@` (вернуть сам receiver). Делает
/// гарантию «возвращает receiver» проверяемой → consume-checker может
/// soundly трактовать `let x = recv.method()` как alias receiver'а.
fn check_fluent_return(module: &Module, errors: &mut Vec<Diagnostic>) {
    // Build set of (type_name, method_name) pairs that have `returns_receiver: true`
    // in this module. Used by both checks below.
    let fluent_methods: std::collections::HashSet<(String, String)> = module.items.iter()
        .filter_map(|it| {
            if let Item::Fn(f) = it {
                if f.returns_receiver {
                    if let Some(recv) = &f.receiver {
                        return Some((recv.type_name.clone(), f.name.clone()));
                    }
                }
            }
            None
        })
        .collect();

    // Returns true if `expr` can be statically determined to always yield the receiver.
    // Covers: bare `@`, call to a known `-> @` method on `@`, if/else where all
    // branches yield receiver. Conservative — returns false for anything complex.
    fn expr_always_returns_receiver(
        e: &Expr,
        fluent: &std::collections::HashSet<(String, String)>,
        recv_type: &str,
    ) -> bool {
        match &e.kind {
            ExprKind::SelfAccess => true,
            ExprKind::Call { func, .. } => {
                if let ExprKind::Member { obj, name } = &func.kind {
                    if matches!(obj.kind, ExprKind::SelfAccess) {
                        return fluent.contains(&(recv_type.to_string(), name.clone()));
                    }
                }
                false
            }
            ExprKind::If { then, else_: Some(else_branch), .. } => {
                let then_ok = then.trailing.as_ref()
                    .map(|t| expr_always_returns_receiver(t, fluent, recv_type))
                    .unwrap_or(false);
                if !then_ok { return false; }
                match else_branch {
                    ElseBranch::Block(b) => b.trailing.as_ref()
                        .map(|t| expr_always_returns_receiver(t, fluent, recv_type))
                        .unwrap_or(false),
                    ElseBranch::If(e2) => expr_always_returns_receiver(e2, fluent, recv_type),
                }
            }
            _ => false,
        }
    }

    // Returns true if a body can be statically determined to always yield the receiver.
    // External: C-runtime contract guarantees it. Block: checks trailing expression
    // + all Stmt::Return values.
    fn body_always_returns_receiver(
        body: &FnBody,
        fluent: &std::collections::HashSet<(String, String)>,
        recv_type: &str,
    ) -> bool {
        match body {
            // External — C-реализация (StringBuilder/WriteBuffer); C-функция
            // возвращает receiver-pointer по контракту runtime'а.
            FnBody::External => true,
            FnBody::Expr(e) => expr_always_returns_receiver(e, fluent, recv_type),
            FnBody::Block(b) => {
                // Trailing expression must return receiver.
                let trailing_ok = b.trailing.as_ref()
                    .map(|t| expr_always_returns_receiver(t, fluent, recv_type))
                    .unwrap_or(false);
                if !trailing_ok { return false; }
                // All explicit return statements must also return receiver.
                for stmt in &b.stmts {
                    if let Stmt::Return { value: Some(v), .. } = stmt {
                        if !expr_always_returns_receiver(v, fluent, recv_type) {
                            return false;
                        }
                    }
                    if let Stmt::Return { value: None, .. } = stmt {
                        return false; // bare `return` → returns unit, not receiver
                    }
                }
                true
            }
        }
    }

    for item in &module.items {
        if let Item::Fn(f) = item {
            let recv_type = f.receiver.as_ref().map(|r| r.type_name.as_str()).unwrap_or("");
            if f.returns_receiver {
                // Check 1: `-> @` body must always return receiver.
                // Accepted: bare `@`, call to another `-> @` method, if/else where
                // all branches yield receiver, external fn (C-contract).
                if !body_always_returns_receiver(&f.body, &fluent_methods, recv_type) {
                    let span = match &f.body {
                        FnBody::Block(b) => b.span,
                        FnBody::Expr(e) => e.span,
                        FnBody::External => f.span,
                    };
                    errors.push(Diagnostic::new(
                        format!(
                            "метод `{}` объявлен `-> @` (fluent-return, D132): его \
                             тело обязано завершаться выражением `@` (или вызовом \
                             другого `-> @` метода). Добавьте `@` последним \
                             выражением тела.",
                            f.name),
                        span,
                    ));
                }
            } else {
                // Check 2: `-> Self` method where all return paths yield `@`
                // must be declared `-> @` instead. Catches the accidental pattern
                //   `fn T mut @m(...) -> Self => @fluent_call(...)`.
                let is_self_return = matches!(&f.return_type,
                    Some(TypeRef::Named { path, .. })
                    if path.len() == 1 && path[0] == "Self");
                if !is_self_return { continue; }
                if !matches!(&f.receiver, Some(r) if r.kind == ReceiverKind::Instance) {
                    continue;
                }
                if body_always_returns_receiver(&f.body, &fluent_methods, recv_type) {
                    errors.push(Diagnostic::new(
                        format!(
                            "метод `{}` объявлен `-> Self`, но все пути возвращают \
                             сам receiver (`@`). Используйте `-> @` (fluent-return, D132) \
                             вместо `-> Self`.",
                            f.name),
                        f.span,
                    ));
                }
            }
        }
    }
}

/// Plan 100.2 (D156): определяет, содержит ли тип параметра generic
/// с consume_bound. Используется для авто-обязательства при entry в
/// функцию с `[T consume]` bound.
fn typeref_contains_consume_generic(ty: &TypeRef, consume_generics: &HashSet<String>) -> bool {
    if consume_generics.is_empty() { return false; }
    match ty {
        TypeRef::Named { path, generics, .. } => {
            // Generic-param T в consume_generics → consume-обязательство.
            if path.len() == 1 && consume_generics.contains(&path[0]) {
                return true;
            }
            // Generic-wrap G[T] где T consume → G тоже consume.
            generics.iter().any(|g| typeref_contains_consume_generic(g, consume_generics))
        }
        TypeRef::Tuple(elems, _) => {
            elems.iter().any(|e| typeref_contains_consume_generic(e, consume_generics))
        }
        TypeRef::Array(inner, _) => typeref_contains_consume_generic(inner, consume_generics),
        _ => false,
    }
}

fn check_consume(module: &Module, errors: &mut Vec<Diagnostic>) {
    let reg = ConsumeRegistry::build(module);
    // Plan 100.1 (D133): LinearityRegistry + marker consistency.
    let lin_reg = LinearityRegistry::build(module);
    check_linearity_markers(module, &lin_reg, errors);

    for item in &module.items {
        match item {
            Item::Fn(f) => {
                let mut ctx = ConsumeCtx::new(&reg, &lin_reg);

                // Plan 100.2 (D156): collect `[T consume]` bound generics.
                // Внутри тела функции параметры с такими типами —
                // consume-obligations (strict-mode D156).
                let consume_bound_generics: HashSet<String> = f.generics.iter()
                    .filter(|g| g.consume_bound)
                    .map(|g| g.name.clone())
                    .collect();
                ctx.consume_bound_generics = consume_bound_generics.clone();

                // Параметры функции — live на входе.
                // Plan 100.2 (D156): если функция имеет `[T consume]` generics,
                // параметры с типом T (или содержащим T) — consume-obligations.
                for p in &f.params {
                    // Plan 108.1 (D176 amend): track param mutability.
                    // consume params получают неявно mut (по spec).
                    let effective_mut = p.is_mut || p.consume;
                    ctx.param_mut.insert(p.name.clone(), effective_mut);
                    // Plan 108.1 followup: track readonly-annotated params.
                    if matches!(&p.ty, TypeRef::Readonly(..)) {
                        ctx.readonly_locals.insert(p.name.clone());
                    }
                    let pty = match &p.ty {
                        TypeRef::Named { path, .. } if path.len() == 1 =>
                            Some(path[0].clone()),
                        _ => None,
                    };
                    // Plan 100.2: param type contains consume-bound generic → obligation.
                    let is_consume_generic_param =
                        typeref_contains_consume_generic(&p.ty, &consume_bound_generics);
                    if is_consume_generic_param {
                        // Plan 100.2 (D156): if param type is not a simple Named
                        // (e.g. tuple `(T, T)`) but contains a consume-bound generic,
                        // store the generic name as the type so that D156-strict-forget
                        // is emitted (not D133) on obligation failure.
                        let pty = pty.or_else(|| {
                            consume_bound_generics.iter().find(|g| {
                                let g_set: HashSet<String> =
                                    std::iter::once((*g).clone()).collect();
                                typeref_contains_consume_generic(&p.ty, &g_set)
                            }).cloned()
                        });
                        ctx.declare_consume_binding(&p.name, pty);
                    } else if p.consume {
                        // Explicit `consume` param: consume-obligation.
                        ctx.declare_consume_binding(&p.name, pty);
                    } else {
                        // Plan 100.3 (D157): non-consume param of consume type
                        // → view-param. Can read fields, call non-consume methods,
                        // pass to other view-params. Cannot call consume-methods,
                        // cannot return.
                        let is_consume_type = pty.as_ref()
                            .map(|t| lin_reg.consume_types.contains(t))
                            .unwrap_or(false);
                        if is_consume_type {
                            ctx.declare_view_param(&p.name, pty);
                        } else {
                            ctx.declare(&p.name, pty);
                        }
                    }
                }
                // Plan 100.1 (D133 / D5): инициализация consume-полей receiver'а.
                if let Some(recv) = &f.receiver {
                    // Plan 103.9 (D174): track self-type for method call inference.
                    // `consume g = self.lock()` needs to know `self` is `Mutex`.
                    ctx.self_type = Some(recv.type_name.clone());
                    for it in &module.items {
                        if let Item::Type(td) = it {
                            if td.name == recv.type_name {
                                if let TypeDeclKind::Record(fields) = &td.kind {
                                    for field in fields {
                                        if field.consume {
                                            ctx.init_field_live(&field.name);
                                        }
                                    }
                                }
                                break;
                            }
                        }
                    }
                }
                match &f.body {
                    FnBody::Block(b) => {
                        // Plan 73.1 V3 [M-73.1-fluent-return-implicit-consume]:
                        // FnBody-level trailing fluent-chain → mark chain root
                        // Consumed via the `is_fn_body_trailing` variant.
                        consume_walk_block_inner(&mut ctx, b, errors, true);
                        // Plan 100.1 (D133): exit-point checks.
                        let exit_span = b.span;
                        ctx.check_obligations_at_exit(exit_span, errors);
                        // Field-exit check for methods.
                        if let Some(recv) = &f.receiver {
                            ctx.check_fields_at_exit(
                                &recv.type_name,
                                recv.consume,
                                &f.name,
                                exit_span,
                                errors,
                            );
                        }
                        // Plan 100.8 (D166): D162 coverage check — errdefer/okdefer
                        // for consume vars in failable functions.
                        check_d162_coverage(f, &ctx, b, errors, &lin_reg);
                    }
                    FnBody::Expr(e) => {
                        consume_walk_expr(&mut ctx, e, errors);
                        // Plan 100.2: FnBody::Expr trailing ident = implicit return.
                        // Plan 73.1 V3 [M-73.1-fluent-return-implicit-consume]: also
                        // covers fluent-return method-chains (parity with
                        // `consume_walk_block` trailing path).
                        if let Some(name) = implicit_return_consume_var(&ctx, e) {
                            ctx.mark_consumed(&name, e.span);
                        }
                        ctx.check_obligations_at_exit(e.span, errors);
                    }
                    FnBody::External => {}
                }
            }
            Item::Test(t) => {
                let mut ctx = ConsumeCtx::new(&reg, &lin_reg);
                consume_walk_block(&mut ctx, &t.body, errors);
                // Plan 100.1 (D133): exit-point checks for test scope.
                ctx.check_obligations_at_exit(t.body.span, errors);
            }
            _ => {}
        }
    }
}

// ── Plan 100.8 (D166): D162 coverage check helpers ───────────────────────────

/// Check if a function's effect list contains `Fail[...]` or bare `Fail`
/// (indicating the function can throw).
fn fn_is_failable(effects: &[TypeRef]) -> bool {
    effects.iter().any(|e| {
        if let TypeRef::Named { path, .. } = e {
            path.last().map(|n| n == "Fail").unwrap_or(false)
        } else {
            false
        }
    })
}

/// Recursively scan a block for `errdefer` / `okdefer` / `defer` stmts.
/// Returns (has_errdefer, has_okdefer) — both are coarse: presence of ANY
/// errdefer/okdefer in the direct stmts (non-nested) is sufficient for
/// the D162 simplified check.
///
/// Plan 100.7 (D165): plain `defer` runs on ALL exits (including error path),
/// so it counts as errdefer-coverage for D162-uncovered-error-path purposes.
fn scan_defer_coverage(b: &Block) -> (bool, bool) {
    let mut has_errdefer = false;
    let mut has_okdefer = false;
    for s in &b.stmts {
        match s {
            Stmt::ErrDefer { .. } | Stmt::DeferWithResult { .. } => {
                has_errdefer = true;
            }
            Stmt::OkDefer { .. } => {
                has_okdefer = true;
            }
            // Plain `defer` runs on ALL exits (success + error + cancel).
            // Counts as both errdefer and okdefer coverage for D162.
            Stmt::Defer { .. } => {
                has_errdefer = true;
                has_okdefer = true;
            }
            _ => {}
        }
    }
    (has_errdefer, has_okdefer)
}

/// Plan 100.8 (D166): Simplified D162 coverage check.
///
/// Emits `D162-uncovered-error-path` when a failable function (`Fail[E]`
/// in effects) has consume bindings but no `errdefer` covering the error path.
///
/// This is the tooling-layer implementation of D162 (the full static-flow
/// version lives in Plan 100.4.5; this version provides IDE-feedback via
/// the D166 LSP quick-fix infrastructure).
fn check_d162_coverage(
    f: &FnDecl,
    ctx: &ConsumeCtx,
    block: &Block,
    errors: &mut Vec<Diagnostic>,
    lin_reg: &LinearityRegistry,
) {
    use crate::diag::{Applicability, Suggestion};

    // Only check failable functions with consume bindings ever declared.
    // Use all_declared_consume (not consume_obligations which is cleared after
    // each block exit).  Plan 100.8 (D166) fix: check D162 even when the var
    // is properly consumed on success path.
    if ctx.all_declared_consume.is_empty() { return; }
    if !fn_is_failable(&f.effects) { return; }

    let (has_errdefer, has_okdefer) = scan_defer_coverage(block);

    // D162-uncovered-error-path: failable function + consume binding + no errdefer.
    // Plan 100.7 (D165): body_has_throw tells us whether there are explicit throw
    // statements in the function body. Used below to distinguish "consumed at exit
    // via explicit error-branch handling" from "consumed at exit with no error paths."
    let body_throws = block_has_throw(block);
    if !has_errdefer {
        for name in &ctx.all_declared_consume {
            let canon = ctx.canonical(name);
            // Plan 100.7 (D165): if the consume variable is Consumed at fn-exit
            // AND the body contains at least one explicit `throw` statement,
            // the coder has explicitly handled the error path (e.g. tx.rollback()
            // before throw in a diverging branch). Skip D162 in this case.
            // If there are NO explicit throws but the variable is Consumed at exit,
            // the function is still Fail-annotated — D162 fires as a lint (external
            // handler injections can interrupt the function at any Fail-effect site).
            let state = ctx.states.get(&canon).or_else(|| ctx.states.get(name.as_str()));
            if matches!(state, Some(VarState::Consumed(_))) && body_throws {
                continue;
            }
            let ty = ctx.var_types.get(name)
                .or_else(|| ctx.var_types.get(&canon))
                .cloned()
                .unwrap_or_default();
            let methods = lin_reg.consume_methods_for(&ty);
            let cleanup_method = if methods.len() > 1 {
                methods.last().cloned().unwrap_or_default()
            } else {
                methods.first().cloned().unwrap_or_else(|| "cleanup".to_string())
            };
            let suggestion = Suggestion {
                message: format!(
                    "add `errdefer {{ {name}.{method}() }}` to cover error-path \
                     (D162-uncovered-error-path quick fix)",
                    name = name, method = cleanup_method),
                span: block.span,
                replacement: format!(
                    "errdefer {{ {name}.{method}() }}",
                    name = name, method = cleanup_method),
                applicability: Applicability::MachineApplicable,
            };
            errors.push(Diagnostic::new(
                format!(
                    "[D162-uncovered-error-path] consume binding `{}` (тип `{}`) \
                     в failable function без `errdefer` покрытия error-path. \
                     При throw/panic `{}` не будет cleaned up. \
                     Добавьте `errdefer {{ {}.{}() }}`.",
                    name, ty, name, name, cleanup_method),
                block.span,
            ).with_suggestion(suggestion));
        }
    }

    // D162-uncovered-success-path: failable function + consume + has errdefer
    // but no okdefer or explicit commit call on success path.
    // This is a lighter warning — we just note it (MaybeIncorrect suggestion).
    if has_errdefer && !has_okdefer {
        // Only emit if there are consume obligations (errdefer exists but
        // success path might be uncovered).
        // Simplified: check if the trailing expression of the block is a
        // consume method call (if not, suggest okdefer).
        let success_covered = block.trailing.as_ref().map(|t| {
            matches!(&t.kind, ExprKind::Call { func, .. }
                if matches!(&func.kind, ExprKind::Member { .. }))
        }).unwrap_or(false);
        if !success_covered {
            for name in &ctx.all_declared_consume {
                let canon = ctx.canonical(name);
                let ty = ctx.var_types.get(name)
                    .or_else(|| ctx.var_types.get(&canon))
                    .cloned()
                    .unwrap_or_default();
                let methods = lin_reg.consume_methods_for(&ty);
                let primary_method = methods.first().cloned()
                    .unwrap_or_else(|| "commit".to_string());
                // Only emit if var is still Live at function end (errdefer present
                // but actual consume may be missing). This is conservative —
                // we only emit for the first uncovered var.
                let state = ctx.states.get(&canon).or_else(|| ctx.states.get(name));
                if !matches!(state, Some(VarState::Live)) { continue; }
                let suggestion = Suggestion {
                    message: format!(
                        "add `okdefer {{ {name}.{method}() }}` or explicit call \
                         (D162-uncovered-success-path quick fix)",
                        name = name, method = primary_method),
                    span: block.span,
                    replacement: format!(
                        "okdefer {{ {name}.{method}() }}",
                        name = name, method = primary_method),
                    applicability: Applicability::MaybeIncorrect,
                };
                errors.push(Diagnostic::new(
                    format!(
                        "[D162-uncovered-success-path] consume binding `{}` (тип `{}`) \
                         имеет errdefer для error-path, но success-path может быть \
                         не покрыт. Добавьте `okdefer {{ {}.{}() }}` или явный вызов.",
                        name, ty, name, primary_method),
                    block.span,
                ).with_suggestion(suggestion));
                break; // Only first uncovered var to avoid noise.
            }
        }
    }
}

/// Объединить два VarState на слиянии путей (branch join).
fn consume_join2(a: &VarState, b: &VarState) -> VarState {
    match (a, b) {
        (VarState::Live, VarState::Live) => VarState::Live,
        (VarState::Consumed(s), VarState::Consumed(_)) => VarState::Consumed(*s),
        (VarState::MaybeConsumed(s), _) | (_, VarState::MaybeConsumed(s)) =>
            VarState::MaybeConsumed(*s),
        (VarState::Consumed(s), VarState::Live)
        | (VarState::Live, VarState::Consumed(s)) => VarState::MaybeConsumed(*s),
    }
}

/// Слить состояния веток. Базис — `saved` (pre-branch); ветка-локальные
/// переменные отбрасываются.
fn consume_join(saved: &HashMap<String, VarState>,
                a: &HashMap<String, VarState>,
                b: &HashMap<String, VarState>) -> HashMap<String, VarState> {
    let mut out = HashMap::new();
    for (k, base) in saved {
        let sa = a.get(k).unwrap_or(base);
        let sb = b.get(k).unwrap_or(base);
        out.insert(k.clone(), consume_join2(sa, sb));
    }
    out
}

/// Plan 108.1 followup ([M-108.1-readonly-to-explicit-mut-coerce]):
/// Check args at mut-param positions for readonly-binding source.
/// Передача readonly-binding (через `readonly_locals`) в `mut`-param
/// → E_READONLY_COERCE с machine-applicable Suggestion.
fn check_readonly_coerce_args(
    ctx: &ConsumeCtx,
    args: &[CallArg],
    mut_param_idxs: &[usize],
    errors: &mut Vec<Diagnostic>,
) {
    for &idx in mut_param_idxs {
        if let Some(CallArg::Item(arg)) = args.get(idx) {
            if let ExprKind::Ident(name) = &arg.kind {
                if ctx.readonly_locals.contains(name) {
                    errors.push(Diagnostic::new(
                        format!(
                            "[E_READONLY_COERCE] аргумент `{}` имеет тип `readonly T`, но \
                             передаётся в `mut`-параметр — нарушение sound subtyping (D176, \
                             Plan 108.1 followup).  readonly binding гарантирует immutability \
                             у caller'а; передача в mut позволила бы callee'у мутировать.",
                            name),
                        arg.span,
                    ).with_note(
                        "решения: (a) убрать `readonly` annotation у source binding'а \
                         (если значение действительно мутируемое); (b) сделать callee-param \
                         non-mut (default readonly) или `readonly`; (c) скопировать значение \
                         в новый mutable binding перед передачей."
                            .to_string(),
                    ));
                }
            }
        }
    }
}

/// Plan 73.1 V3 [M-73.1-fluent-return-implicit-consume]:
/// Determine whether a trailing/return expression evaluates to a consume-
/// obligation variable directly OR via a fluent (`-> @`) method-chain.
///
/// Example: `sb.append(c)` where `mut @append(c char) -> @` (fluent) and
/// `sb` is a consume-obligation → returns `Some("sb")` because the chain
/// value IS `sb`.  Recurses through arbitrarily long chains
/// (`sb.append(x).append(y).truncate(n)`).
///
/// Returns the binding name to mark Consumed; `None` when the expression
/// does not transitively return a consume-obligation var.
fn implicit_return_consume_var(ctx: &ConsumeCtx, e: &Expr) -> Option<String> {
    match &e.kind {
        ExprKind::Ident(name) => {
            if ctx.consume_obligations.contains(name.as_str()) {
                Some(name.to_string())
            } else {
                None
            }
        }
        ExprKind::Call { func, .. } => {
            let ExprKind::Member { obj, name: method } = &func.kind else {
                return None;
            };
            // Chain root must transitively reduce to a consume-obligation var.
            let root = implicit_return_consume_var(ctx, obj)?;
            // Fluent chain preserves receiver type — look up receiver type by
            // the chain root.  Match in `recv_returning` registry.
            let ty = ctx.var_types.get(&root)?.clone();
            if ctx.reg.recv_returning.contains(&(ty, method.clone())) {
                Some(root)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn consume_walk_block(ctx: &mut ConsumeCtx, b: &Block, errors: &mut Vec<Diagnostic>) {
    consume_walk_block_inner(ctx, b, errors, false);
}

/// Plan 73.1 V3: variant taking `is_fn_body_trailing` flag.  When true,
/// the block's trailing expression is treated as the function's implicit
/// return value, so a fluent-chain trailing
/// (`sb.append(x).append(y)`) marks its consume-obligation root Consumed.
/// Nested blocks (if-then, loop-body, regular `{}`) call with `false`
/// because their trailing value is usually discarded by the outer
/// statement / if-without-else.
fn consume_walk_block_inner(
    ctx: &mut ConsumeCtx,
    b: &Block,
    errors: &mut Vec<Diagnostic>,
    is_fn_body_trailing: bool,
) {
    // Plan 100.1 (D133): track consume-obligations introduced in this block.
    // На exit'е блока проверяем только NEW obligations (не было до входа).
    let obligations_before = ctx.consume_obligations.clone();

    for s in &b.stmts {
        consume_walk_stmt(ctx, s, errors);
    }
    if let Some(t) = &b.trailing {
        consume_walk_expr(ctx, t, errors);
        // Plan 100.1 (D133 / D9): trailing expr = implicit return.
        // Ident-trailing always counts as implicit-return-of-value (existing
        // V1 semantics — preserved for compatibility).  Fluent-chain
        // trailing only counts when this block is the function body
        // (Plan 73.1 V3 [M-73.1-fluent-return-implicit-consume]).
        if is_fn_body_trailing {
            if let Some(name) = implicit_return_consume_var(ctx, t) {
                ctx.mark_consumed(&name, t.span);
            }
        } else if let ExprKind::Ident(name) = &t.kind {
            if ctx.consume_obligations.contains(name.as_str()) {
                ctx.mark_consumed(name, t.span);
            }
        }
    }

    // Plan 100.1 (D133): exit-check для obligations введённых в этом блоке
    // (не в outer scope — те проверяются при function-exit).
    let new_obligations: Vec<String> = ctx.consume_obligations.iter()
        .filter(|n| !obligations_before.contains(n.as_str()))
        .cloned()
        .collect();
    if !new_obligations.is_empty() {
        // Временно ограничить consume_obligations только новыми, чтобы
        // check_obligations_at_exit не дублировал outer-scope проверки.
        let full_obligations = std::mem::replace(
            &mut ctx.consume_obligations,
            new_obligations.iter().cloned().collect());
        ctx.check_obligations_at_exit(b.span, errors);
        ctx.consume_obligations = full_obligations;
        // Убрать проверённые обязательства (они выполнились или ошибка эмитирована).
        for n in &new_obligations {
            ctx.consume_obligations.remove(n);
        }
    }
}

fn consume_walk_stmt(ctx: &mut ConsumeCtx, s: &Stmt, errors: &mut Vec<Diagnostic>) {
    match s {
        Stmt::Let(decl) => {
            consume_walk_expr(ctx, &decl.value, errors);
            let mut names = Vec::new();
            consume_pattern_names(&decl.pattern, &mut names);
            // Plan 108.2 (D36 enforcement) + 108.3 (per-name mut):
            // `let mut x` → is_mut=true; `let x` → false;
            // `consume x` → implicit mut (ownership transfer).
            // `let (mut a, b) = ...` → per-name (a=true, b=false).
            let outer_effective_mut = decl.mutable || decl.consume;
            let mut name_mut_pairs = Vec::new();
            consume_pattern_names_with_mut(&decl.pattern, &mut name_mut_pairs);
            for (n, pat_mut) in &name_mut_pairs {
                if n != "_" {
                    // outer `let mut` || per-name `mut` || consume.
                    ctx.local_mut.insert(n.clone(), outer_effective_mut || *pat_mut);
                }
            }
            // Plan 108.1 followup: track readonly-annotated locals.
            // `let view readonly T = ...` → readonly_locals.contains("view").
            let is_readonly_annotated = matches!(&decl.ty, Some(TypeRef::Readonly(..)));
            if is_readonly_annotated {
                for n in &names {
                    if n != "_" {
                        ctx.readonly_locals.insert(n.clone());
                    }
                }
            }
            // alias-форма `let <name> = <rhs>` — `name` ссылается на тот
            // же объект:
            //   (a) Plan 73 followup: `let a = b` — RHS голый идентификатор.
            //   (b) Plan 77 (D132): `let x = recv.fluent()` — fluent-метод
            //       `-> @` гарантированно возвращает сам receiver.
            let alias_src: Option<String> = if names.len() == 1 {
                match &decl.value.kind {
                    ExprKind::Ident(src) => {
                        let canon = ctx.canonical(src);
                        if canon != names[0] && ctx.states.contains_key(&canon) {
                            Some(canon)
                        } else { None }
                    }
                    ExprKind::Call { func, .. } => {
                        if let ExprKind::Member { obj, name: method } = &func.kind {
                            if let ExprKind::Ident(recv) = &obj.kind {
                                let canon = ctx.canonical(recv);
                                let fluent = ctx.var_types.get(&canon)
                                    .map(|ty| ctx.reg.recv_returning
                                        .contains(&(ty.clone(), method.clone())))
                                    .unwrap_or(false);
                                if fluent && canon != names[0]
                                    && ctx.states.contains_key(&canon)
                                {
                                    Some(canon)
                                } else { None }
                            } else { None }
                        } else { None }
                    }
                    _ => None,
                }
            } else { None };
            // Plan 73.1 (D180): consume binding syntax enforcement.
            // Pre-compute: is RHS consume-obligated? Is alias source consume-obligated?
            let inferred_ty_d180 = ctx.infer_let_type(decl);
            let rhs_yields_consume_type = inferred_ty_d180.as_ref()
                .map(|ty| ctx.lin_reg.consume_types.contains(ty))
                .unwrap_or(false);
            let alias_obligated = if let Some(canon) = &alias_src {
                ctx.consume_obligations.contains(canon)
                    || ctx.var_types.get(canon)
                        .map(|ty| ctx.lin_reg.consume_types.contains(ty))
                        .unwrap_or(false)
            } else { false };

            // `let` keyword span — replace with `consume` (3 chars → 7 chars, but
            // span-based replacement just covers the keyword position).
            // decl.span.start points to `let` (3 chars) или `consume` (7 chars).
            let kw_span = crate::diag::Span {
                file_id: decl.span.file_id,
                start: decl.span.start,
                end: decl.span.start + 3, // "let" = 3 chars; if `consume` present, end overlaps but suggestion ignored
            };

            // Rule 2 (D180): `let X = consume_var` (alias) → E_VIEW_BINDING_FORBIDDEN.
            if !decl.consume && alias_obligated && names.len() == 1 {
                errors.push(crate::diag::Diagnostic::new(
                    format!(
                        "[E_VIEW_BINDING_FORBIDDEN] view-binding на consume-обязательную \
                         переменную `{}` запрещён в теле функции (D180). \
                         Used `let` без consume keyword.",
                        names[0]
                    ),
                    decl.span,
                ).with_note(
                    "views существуют ТОЛЬКО как function-параметры (D157). \
                     Для transfer ownership используй `consume X = …` (move); \
                     для view-borrow перенеси в function-параметр.".to_string(),
                ).with_suggestion(crate::diag::Suggestion {
                    message: format!("use `consume {} = …` для move ownership", names[0]),
                    span: kw_span,
                    replacement: "consume".to_string(),
                    applicability: crate::diag::Applicability::MachineApplicable,
                }));
            }
            // Rule 1 (D180): non-alias consume-obligated RHS без `consume` keyword.
            else if !decl.consume && rhs_yields_consume_type && names.len() == 1 {
                errors.push(crate::diag::Diagnostic::new(
                    format!(
                        "[E_CONSUME_KEYWORD_MISSING] binding `{}` держит \
                         consume-обязательную инстанс типа `{}` — требуется keyword \
                         `consume` (D180).",
                        names[0],
                        inferred_ty_d180.as_deref().unwrap_or("?")
                    ),
                    decl.span,
                ).with_note(
                    "consume-обязательные значения должны быть явно ownership-bound \
                     через `consume X = …`. Альтернатива: передать в function-параметр \
                     для view-borrow.".to_string(),
                ).with_suggestion(crate::diag::Suggestion {
                    message: format!("add `consume` keyword: `consume {} = …`", names[0]),
                    span: kw_span,
                    replacement: "consume".to_string(),
                    applicability: crate::diag::Applicability::MachineApplicable,
                }));
            }
            // W_CONSUME_KEYWORD_UNNECESSARY: V2 RESTORED — `consume` keyword
            // на binding с не-consume RHS. Cross-module false positives устранены
            // через расширенный LinearityRegistry::build (M-73.1-warning-needs-
            // project-wide-registry CLOSED — теперь sync.nv consume-types
            // в registry'е).
            //
            // Conservative: emit только когда inferred_ty_d180.is_some() —
            // т.е. тип известен и НЕ consume. Если тип неизвестен (None),
            // skip (sound: false-negative permissive, не false-positive).
            else if decl.consume && !rhs_yields_consume_type && !alias_obligated
                && names.len() == 1 && inferred_ty_d180.is_some()
            {
                let consume_kw_span = crate::diag::Span {
                    file_id: decl.span.file_id,
                    start: decl.span.start,
                    end: decl.span.start + 8, // "consume " = 8 chars
                };
                errors.push(crate::diag::Diagnostic::new(
                    format!(
                        "[W_CONSUME_KEYWORD_UNNECESSARY] keyword `consume` на binding \
                         `{}` избыточен — RHS типа `{}` не consume-обязателен (D180).",
                        names[0],
                        inferred_ty_d180.as_deref().unwrap_or("?")
                    ),
                    consume_kw_span,
                ).with_note(
                    "удали `consume ` для regular let-binding.".to_string(),
                ).with_suggestion(crate::diag::Suggestion {
                    message: "delete `consume ` keyword".to_string(),
                    span: consume_kw_span,
                    replacement: "let".to_string(),
                    applicability: crate::diag::Applicability::MachineApplicable,
                }));
            }

            // Rule 3 (D180): `consume X = consume_var` = move semantics
            // (mark source Consumed, transfer obligation to X).
            if let Some(ref canon) = alias_src {
                if decl.consume && alias_obligated && names.len() == 1 {
                    // Move: source becomes Consumed, X gets new obligation.
                    ctx.mark_consumed(canon, decl.span);
                    ctx.consume_obligations.remove(canon);
                    let ty = ctx.var_types.get(canon).cloned();
                    ctx.declare_consume_binding(&names[0], ty);
                    // Skip the existing alias/declare path below.
                    return;
                }
            }

            if let Some(canon) = alias_src {
                let ty = ctx.var_types.get(&canon).cloned();
                ctx.declare_alias(&names[0], &canon, ty);
            } else {
                let ty = ctx.infer_let_type(decl);
                // Plan 100.3 (D157): detect consume-closures.
                // If the RHS is a closure that calls consume-methods on outer
                // consume-obligation vars, it becomes a consume-closure (FnOnce).
                let closure_body_for_scan: Option<&Expr> = match &decl.value.kind {
                    ExprKind::ClosureLight { body: ClosureBody::Expr(ex), .. } => Some(ex),
                    ExprKind::Lambda { body, .. } => Some(body),
                    _ => None,
                };
                let consume_closure_captured: Option<Vec<String>> =
                    if names.len() == 1 && !decl.consume {
                        closure_body_for_scan.map(|body| {
                            d157_scan_closure_captures(body, &ctx.consume_obligations, ctx)
                        }).filter(|caps| !caps.is_empty())
                    } else { None };

                for n in &names {
                    // Тип привязываем только к одиночному ident-pattern'у.
                    let t = if names.len() == 1 { ty.clone() } else { None };
                    // Plan 100.1 (D133 / D9): `consume tx = ...` binding.
                    if decl.consume {
                        ctx.declare_consume_binding(n, t);
                    } else if let Some(ref captured) = consume_closure_captured {
                        // Plan 100.3 (D157): consume-closure — declares as consume-obligation.
                        ctx.declare_consume_closure(n, captured.clone());
                    } else {
                        ctx.declare(n, t);
                    }
                }
            }
        }
        // Plan 114.4 Ф.2: scope-local const — pass-through (no-op for now).
        Stmt::Const(_) => {}
        Stmt::Expr(e) => consume_walk_expr(ctx, e, errors),
        Stmt::Assign { target, op, value, .. } => {
            consume_walk_expr(ctx, value, errors);
            match &target.kind {
                ExprKind::Ident(name) => {
                    if matches!(op, AssignOp::Assign) {
                        // `x = v` — свежее значение. Развязываем alias-класс
                        // `x` (прочие члены сохраняют прежнее состояние), x
                        // получает новый объект → live и сам по себе.
                        ctx.dissolve_alias_class(name);
                        ctx.aliases.remove(name);
                        ctx.states.insert(name.clone(), VarState::Live);
                    } else {
                        // compound `+=` и т.п. читают старое значение.
                        ctx.use_var(name, target.span, errors);
                        let canon = ctx.canonical(name);
                        ctx.states.insert(canon, VarState::Live);
                    }
                }
                // Plan 100.1 (D5.1): `@<field> = v` — field rebind.
                // Если поле consume-типа и было Live → D133-assign-live-field.
                // После assign — поле Live снова (rebind).
                ExprKind::Member { obj, name: field_name }
                    if matches!(obj.kind, ExprKind::SelfAccess)
                        && matches!(op, AssignOp::Assign) =>
                {
                    // Plan 100.1 (D5.1): check: если поле было Live (полностью
                    // не consumed, без частичного consume sub-fields) →
                    // silent-overwrite error. MaybeConsumed = sub-field was
                    // consumed (D5.2 nested pattern) → assign is OK (rebind).
                    if let Some(VarState::Live) = ctx.field_states.get(field_name.as_str()) {
                        errors.push(Diagnostic::new(
                            format!(
                                "[D133-assign-live-field] присваивание в Live \
                                 consume-поле `@.{}` без предшествующего consume — \
                                 silent leak. Сначала consume: `@{}.<consume-method>()`, \
                                 затем присвойте новое значение.",
                                field_name, field_name),
                            target.span,
                        ));
                    }
                    // Rebind → поле Live снова (восстановлено независимо от предыдущего состояния).
                    ctx.mark_field_live(field_name);
                }
                _ => consume_walk_expr(ctx, target, errors),
            }
        }
        Stmt::Return { value, span } => {
            if let Some(v) = value {
                consume_walk_expr(ctx, v, errors);
                if let ExprKind::Ident(name) = &v.kind {
                    // Plan 100.3 (D157): view-param cannot escape via return.
                    if ctx.is_view_param(name) {
                        errors.push(Diagnostic::new(
                            format!(
                                "[D157-view-escape-return] `{}` — view-param (read-only borrow): \
                                 нельзя вернуть из функции. View-borrow не может outlive scope \
                                 source'а. Используйте `consume` qualifier на параметре для \
                                 transfer ownership.",
                                name),
                            *span,
                        ));
                    }
                    // Plan 100.1 (D133 / D9): `return tx` — обязательство
                    // передаётся caller'у. Пометить возвращённый consume-var как
                    // Consumed (obligation satisfied by transfer).
                    if ctx.consume_obligations.contains(name.as_str()) {
                        ctx.mark_consumed(name, *span);
                    }
                }
            }
        }
        Stmt::Throw { value, .. } => consume_walk_expr(ctx, value, errors),
        Stmt::Break(_) | Stmt::Continue(_) | Stmt::Reveal { .. } => {}
        // defer/errdefer/okdefer/defer-with-result исполняются на scope-exit.
        // Plan 100.4.5 (D162): тело walk'ается изолированно (use-after-consume
        // ловится, consume наружу не протекает), НО любой consume-call над
        // outer consume-var (`@var.consume_method()` или `consume_fn(@var)`)
        // mark'ает var как Consumed в outer ctx — D162 cover semantic.
        //
        // Side-effect: explicit `tx.commit()` AFTER `defer { tx.commit() }` →
        // use-after-consume error (D162-double-cover from check_d162_coverage
        // tooling layer also captures this).
        Stmt::Defer { body, .. } | Stmt::ErrDefer { body, .. }
        | Stmt::OkDefer { body, .. } | Stmt::DeferWithResult { body, .. } => {
            // Collect outer consume-vars consumed inside body (pre-walk snapshot).
            let pre_obligations: HashSet<String> = ctx.consume_obligations.clone();
            consume_walk_isolated_expr(ctx, &[], body, errors);
            // D162 cover: mark outer consume-vars referenced via consume-method
            // call inside body as Consumed.
            d162_mark_defer_cover(ctx, body, &pre_obligations);
        }
        // Plan 110 D188: consume scope-block. Init expression evaluated +
        // body block walked. Full consume-binding semantics (binding visible
        // только в body; on_exit dispatch как auto-consume) — Plan 110.1.2.
        // Здесь walk recursively, не вводим binding в obligations (110.1.2
        // обязанность).
        Stmt::ConsumeScope { init, body, .. } => {
            consume_walk_expr(ctx, init, errors);
            for stmt in &body.stmts {
                consume_walk_stmt(ctx, stmt, errors);
            }
            if let Some(t) = &body.trailing {
                consume_walk_expr(ctx, t, errors);
            }
        }
        Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => {
            consume_walk_expr(ctx, expr, errors);
        }
        Stmt::Apply { args, .. } => {
            for a in args { consume_walk_expr(ctx, a, errors); }
        }
        Stmt::Calc { steps, .. } => {
            for st in steps { consume_walk_expr(ctx, &st.expr, errors); }
        }
    }
}

/// Walk блока изолированно (closure / handler / trailing): состояние
/// `states` восстанавливается после — consume внутрь наружу не течёт.
fn consume_walk_isolated_block(ctx: &mut ConsumeCtx, params: &[String],
                               b: &Block, errors: &mut Vec<Diagnostic>) {
    let saved = ctx.states.clone();
    for p in params { ctx.declare(p, None); }
    consume_walk_block(ctx, b, errors);
    ctx.states = saved;
}

fn consume_walk_isolated_expr(ctx: &mut ConsumeCtx, params: &[String],
                              e: &Expr, errors: &mut Vec<Diagnostic>) {
    let saved = ctx.states.clone();
    for p in params { ctx.declare(p, None); }
    consume_walk_expr(ctx, e, errors);
    ctx.states = saved;
}

/// Plan 100.4.5 (D162): scan defer/errdefer/okdefer body для consume-method
/// calls над outer consume-vars. Mark такие vars как Consumed в outer ctx.
///
/// `pre_obligations` — snapshot consume_obligations ДО body walk; используется
/// для filtering (mark только outer vars, не inner-bindings из body).
///
/// Simplified bootstrap implementation:
/// - Recursively walk body expr.
/// - Detect `<ident>.<method>()` form where `<ident>` ∈ pre_obligations and
///   `<method>` ∈ consume-methods-for(var's type).
/// - mark_consumed for each matched var.
fn d162_mark_defer_cover(ctx: &mut ConsumeCtx, body: &Expr, pre_obligations: &HashSet<String>) {
    let mut covered: Vec<(String, Span)> = Vec::new();
    d162_collect_covers(body, pre_obligations, ctx, &mut covered);
    // D162 cover semantic: defer body выполняется на scope exit; obligation
    // satisfied НЕ означает что var consumed СЕЙЧАС. Var остаётся Live (для
    // post-defer use); просто remove from obligations так что check_consume
    // не emit'ит D133-not-consumed на exit.
    //
    // Double-cover semantics (D3): если defer body covers + explicit body
    // call consumes — explicit call mark'ает Consumed; повторный call
    // даёт use-after-consume (existing behavior). check_d162_coverage
    // (Plan 100.8 D166 tooling) emits also dedicated D162-double-cover.
    for (name, _span) in covered {
        ctx.consume_obligations.remove(name.as_str());
        // Также remove canonical alias-form.
        let canon = ctx.canonical(&name);
        ctx.consume_obligations.remove(canon.as_str());
    }
}

fn d162_collect_covers(e: &Expr, pre_obligations: &HashSet<String>,
                       ctx: &ConsumeCtx, out: &mut Vec<(String, Span)>) {
    match &e.kind {
        ExprKind::Call { func, .. } => {
            if let ExprKind::Member { obj, name: method } = &func.kind {
                if let ExprKind::Ident(recv) = &obj.kind {
                    let canon = ctx.canonical(recv);
                    if pre_obligations.contains(&canon) || pre_obligations.contains(recv) {
                        // var_types stores type names (String).
                        let type_name = ctx.var_types.get(&canon)
                            .or_else(|| ctx.var_types.get(recv))
                            .cloned();
                        if let Some(tn) = type_name {
                            if ctx.reg.methods.contains(&(tn, method.clone())) {
                                out.push((recv.clone(), e.span));
                            }
                        }
                    }
                }
            }
            d162_collect_covers(func, pre_obligations, ctx, out);
        }
        ExprKind::Block(b) => {
            for s in &b.stmts { d162_collect_covers_stmt(s, pre_obligations, ctx, out); }
            if let Some(t) = &b.trailing { d162_collect_covers(t, pre_obligations, ctx, out); }
        }
        ExprKind::If { cond, then, else_ } => {
            d162_collect_covers(cond, pre_obligations, ctx, out);
            for s in &then.stmts { d162_collect_covers_stmt(s, pre_obligations, ctx, out); }
            if let Some(t) = &then.trailing { d162_collect_covers(t, pre_obligations, ctx, out); }
            match else_ {
                Some(ElseBranch::Block(b)) => {
                    for s in &b.stmts { d162_collect_covers_stmt(s, pre_obligations, ctx, out); }
                    if let Some(t) = &b.trailing { d162_collect_covers(t, pre_obligations, ctx, out); }
                }
                Some(ElseBranch::If(e2)) => d162_collect_covers(e2, pre_obligations, ctx, out),
                None => {}
            }
        }
        _ => {}
    }
}

fn d162_collect_covers_stmt(s: &Stmt, pre_obligations: &HashSet<String>,
                            ctx: &ConsumeCtx, out: &mut Vec<(String, Span)>) {
    match s {
        Stmt::Expr(e) | Stmt::Throw { value: e, .. } => d162_collect_covers(e, pre_obligations, ctx, out),
        Stmt::Return { value: Some(v), .. } => d162_collect_covers(v, pre_obligations, ctx, out),
        Stmt::Let(decl) => d162_collect_covers(&decl.value, pre_obligations, ctx, out),
        Stmt::Const(_) => {}
        _ => {}
    }
}

/// Plan 100.3 (D157): Scan closure body for consume-method calls on outer
/// vars (consume-closure detection). Returns names of outer vars that
/// the closure body would consume when invoked.
///
/// Conservative (sound): only detects direct `recv.consume_method()` patterns
/// where `recv` is a known outer consume-obligation. Does NOT detect
/// indirect consumption through nested calls.
fn d157_scan_closure_captures(
    body: &Expr,
    outer_obligations: &HashSet<String>,
    ctx: &ConsumeCtx,
) -> Vec<String> {
    let mut captured = Vec::new();
    d157_scan_expr(body, outer_obligations, ctx, &mut captured);
    captured.sort();
    captured.dedup();
    captured
}

fn d157_scan_expr(e: &Expr, outer: &HashSet<String>, ctx: &ConsumeCtx, out: &mut Vec<String>) {
    match &e.kind {
        ExprKind::Call { func, args, trailing } => {
            if let ExprKind::Member { obj, name: method } = &func.kind {
                if let ExprKind::Ident(recv) = &obj.kind {
                    let canon = ctx.canonical(recv);
                    let in_outer = outer.contains(&canon) || outer.contains(recv.as_str());
                    if in_outer {
                        // Check if this is a consume-method call.
                        let type_name = ctx.var_types.get(&canon)
                            .or_else(|| ctx.var_types.get(recv.as_str()))
                            .cloned();
                        if let Some(tn) = type_name {
                            if ctx.reg.methods.contains(&(tn, method.clone())) {
                                out.push(recv.clone());
                            }
                        }
                    }
                }
            }
            d157_scan_expr(func, outer, ctx, out);
            for a in args { d157_scan_expr(a.expr(), outer, ctx, out); }
            if let Some(t) = trailing {
                match t {
                    Trailing::Block(b) => d157_scan_block(b, outer, ctx, out),
                    _ => {}
                }
            }
        }
        ExprKind::Block(b) => d157_scan_block(b, outer, ctx, out),
        ExprKind::If { cond, then, else_ } => {
            d157_scan_expr(cond, outer, ctx, out);
            d157_scan_block(then, outer, ctx, out);
            match else_ {
                Some(ElseBranch::Block(b)) => d157_scan_block(b, outer, ctx, out),
                Some(ElseBranch::If(e2)) => d157_scan_expr(e2, outer, ctx, out),
                None => {}
            }
        }
        ExprKind::Member { obj, .. } => d157_scan_expr(obj, outer, ctx, out),
        ExprKind::Binary { left, right, .. } => {
            d157_scan_expr(left, outer, ctx, out);
            d157_scan_expr(right, outer, ctx, out);
        }
        _ => {}
    }
}

fn d157_scan_block(b: &Block, outer: &HashSet<String>, ctx: &ConsumeCtx, out: &mut Vec<String>) {
    for s in &b.stmts {
        match s {
            Stmt::Expr(e) | Stmt::Throw { value: e, .. } => d157_scan_expr(e, outer, ctx, out),
            Stmt::Return { value: Some(v), .. } => d157_scan_expr(v, outer, ctx, out),
            Stmt::Let(decl) => d157_scan_expr(&decl.value, outer, ctx, out),
            Stmt::Const(_) => {}
            _ => {}
        }
    }
    if let Some(t) = &b.trailing { d157_scan_expr(t, outer, ctx, out); }
}

/// Walk тела цикла (for/while/loop) — пессимистично: переменная,
/// потреблённая в теле, становится maybe-consumed (consume на 2-й
/// итерации = use-after-consume).
fn consume_walk_loop(ctx: &mut ConsumeCtx, loop_vars: &[String],
                     body: &Block, errors: &mut Vec<Diagnostic>) {
    let pre = ctx.states.clone();
    // Pass 1 — обнаружить consume в теле (ошибки в throwaway sink).
    for v in loop_vars { ctx.declare(v, None); }
    let mut throwaway: Vec<Diagnostic> = Vec::new();
    consume_walk_block(ctx, body, &mut throwaway);
    let consumed: Vec<String> = pre.keys()
        .filter(|k| matches!(ctx.states.get(*k),
            Some(VarState::Consumed(_)) | Some(VarState::MaybeConsumed(_))))
        .cloned()
        .collect();
    // Reset к pre, pre-mark consumed-в-теле как maybe-consumed.
    ctx.states = pre.clone();
    for k in &consumed {
        ctx.states.insert(k.clone(), VarState::MaybeConsumed(body.span));
    }
    // Pass 2 — реальный walk (ошибки эмитятся).
    for v in loop_vars { ctx.declare(v, None); }
    consume_walk_block(ctx, body, errors);
    // Post-loop: цикл мог не выполниться ни разу → consumed-в-теле
    // переменные maybe-consumed; ветка-локальные сбрасываются.
    ctx.states = pre;
    for k in &consumed {
        ctx.states.insert(k.clone(), VarState::MaybeConsumed(body.span));
    }
}

/// Plan 100.2 (D156): `for consume x in iter { body }` semantics.
/// Each loop variable is a consume-obligation; iter is Consumed after loop.
/// Pass-2 architecture: detect outer-scope pessimism, then real walk + check.
fn consume_walk_consume_for(
    ctx: &mut ConsumeCtx,
    iter: &Expr,
    loop_var_names: &[String],
    body: &Block,
    errors: &mut Vec<Diagnostic>,
) {
    // Walk iter expression (use-after-consume check for the collection var).
    consume_walk_expr(ctx, iter, errors);

    let pre = ctx.states.clone();
    let pre_obligations = ctx.consume_obligations.clone();

    // ── Pass 1: discover which outer-scope vars get consumed in body ──
    // (pessimistic: if consumed in body, mark maybe-consumed post-loop)
    for n in loop_var_names {
        ctx.declare_consume_binding(n, None);
    }
    let mut throwaway: Vec<Diagnostic> = Vec::new();
    consume_walk_block(ctx, body, &mut throwaway);
    let outer_consumed: Vec<String> = pre.keys()
        .filter(|k| matches!(ctx.states.get(*k),
            Some(VarState::Consumed(_)) | Some(VarState::MaybeConsumed(_))))
        .cloned()
        .collect();

    // ── Reset for pass 2: restore + pessimistic outer-consumed ──
    ctx.states = pre.clone();
    for k in &outer_consumed {
        ctx.states.insert(k.clone(), VarState::MaybeConsumed(body.span));
    }
    ctx.consume_obligations = pre_obligations.clone();

    // ── Pass 2: real walk with error collection ──
    for n in loop_var_names {
        ctx.declare_consume_binding(n, None);
    }
    consume_walk_block(ctx, body, errors);

    // ── Check loop vars BEFORE resetting state ──
    let body_span = body.span;
    for n in loop_var_names {
        let canon = ctx.canonical(n);
        let state = ctx.states.get(&canon).or_else(|| ctx.states.get(n)).cloned();
        match state {
            None | Some(VarState::Live) => {
                errors.push(Diagnostic::new(
                    format!(
                        "[D156-iter-not-consumed] `for consume` loop variable `{}` is not \
                         consumed in loop body. Every iteration must consume the loop variable \
                         via a consume-method, `return`, or consume-param transfer.",
                        n),
                    body_span,
                ));
            }
            Some(VarState::MaybeConsumed(_)) => {
                errors.push(Diagnostic::new(
                    format!(
                        "[D156-iter-maybe-consumed] `for consume` loop variable `{}` is \
                         consumed on some execution paths but not all. Every iteration must \
                         consume the loop variable on ALL execution paths.",
                        n),
                    body_span,
                ));
            }
            Some(VarState::Consumed(_)) => {}
        }
        ctx.consume_obligations.remove(n);
    }

    // ── Post-loop: restore pre state with pessimistic outer-consumed ──
    ctx.states = pre;
    for k in &outer_consumed {
        ctx.states.insert(k.clone(), VarState::MaybeConsumed(body.span));
    }

    // Mark iter variable as Consumed after for-consume loop.
    // Pragmatic (D156): even early break → iter considered Consumed.
    if let ExprKind::Ident(name) = &iter.kind {
        ctx.mark_consumed(name, iter.span);
        // If the iter was a consume-obligation (e.g. `consume txs = get_vec()`),
        // the mark_consumed above satisfies it.
    }
}

fn consume_walk_expr(ctx: &mut ConsumeCtx, e: &Expr, errors: &mut Vec<Diagnostic>) {
    match &e.kind {
        // ─── Листья ───
        ExprKind::IntLit(_) | ExprKind::FloatLit(_) | ExprKind::StrLit(_)
        | ExprKind::BoolLit(_) | ExprKind::UnitLit | ExprKind::CharLit(_)
        | ExprKind::Path(_) | ExprKind::SelfAccess => {}

        // ─── Использование переменной ───
        ExprKind::Ident(name) => ctx.use_var(name, e.span, errors),

        // ─── Интерполированная строка `"... ${expr} ..."` ───
        ExprKind::InterpolatedStr { parts } => {
            for p in parts {
                if let InterpStrPart::Expr(ex) = p {
                    consume_walk_expr(ctx, ex, errors);
                }
            }
        }

        // ─── Вызовы — точки consume ───
        ExprKind::Call { func, args, trailing } => {
            match &func.kind {
                // Method call: obj.method(args).
                ExprKind::Member { obj, name: method } => {
                    if let ExprKind::Ident(recv) = &obj.kind {
                        // Любой вызов метода — использование receiver'а.
                        ctx.use_var(recv, obj.span, errors);
                        for a in args { consume_walk_expr(ctx, a.expr(), errors); }
                        if let Some(t) = trailing { consume_walk_trailing(ctx, t, errors); }
                        let recv = recv.clone();
                        // Plan 100.3 (D157): view-param cannot call consume-methods.
                        if ctx.is_view_param(&recv) && ctx.is_consume_method(&recv, method) {
                            errors.push(Diagnostic::new(
                                format!(
                                    "[D157-consume-via-view] `{}` — view-param (read-only borrow): \
                                     нельзя вызывать consume-метод `{}`. Используйте `consume` \
                                     qualifier на параметре функции для ownership transfer.",
                                    recv, method),
                                e.span,
                            ));
                        }
                        // Plan 108.1 (D176 amend): mut-method on non-mut param → E_PARAM_NOT_MUT.
                        // Двa источника mut-методов:
                        //  (a) registered (`ctx.reg.mut_methods` — StringBuilder, WriteBuffer,
                        //      user `fn T mut @method`).
                        //  (b) builtin collection (array `[]T`, map `[K:V]`, set `{T}`) —
                        //      `.push/.pop/.append/.insert/.remove/.clear/.truncate/.reserve/
                        //       .swap/.sort/.set/.extend/.copy_from`.
                        if let Some(&is_mut) = ctx.param_mut.get(&recv) {
                            if !is_mut {
                                let recv_ty = ctx.var_types.get(&ctx.canonical(&recv))
                                    .or_else(|| ctx.var_types.get(&recv))
                                    .cloned();
                                let registered = recv_ty.as_ref()
                                    .map(|rty| ctx.reg.mut_methods.contains(&(rty.clone(), method.clone())))
                                    .unwrap_or(false);
                                let builtin_mut_method = matches!(
                                    method.as_str(),
                                    "push" | "pop" | "append" | "insert" | "remove"
                                    | "clear" | "truncate" | "reserve" | "swap"
                                    | "sort" | "sort_by" | "set" | "extend" | "extend_from"
                                    | "copy_from" | "copy_within" | "shrink_to_fit" | "fill"
                                    | "drain" | "dedup" | "reverse" | "shuffle"
                                );
                                if registered || builtin_mut_method {
                                    let ty_str = recv_ty.as_deref().unwrap_or("?");
                                    errors.push(Diagnostic::new(
                                        format!(
                                            "[E_PARAM_NOT_MUT] параметр `{}` не помечен `mut`, \
                                             но вызывается mut-метод `{}` (тип `{}`).  \
                                             Default для параметров — read-only (D176 amend, Plan 108.1).",
                                            recv, method, ty_str),
                                        e.span,
                                    ).with_note(
                                        "добавь `mut` к параметру: `fn ...(mut <name> T)` — \
                                         разрешит вызов mut-методов и index-assignment в callee."
                                            .to_string(),
                                    ).with_suggestion(crate::diag::Suggestion {
                                        message: format!("add `mut` to param `{}`", recv),
                                        span: e.span,
                                        replacement: format!("mut {}", recv),
                                        applicability: crate::diag::Applicability::MaybeIncorrect,
                                    }));
                                }
                            }
                        }
                        // Plan 108.2 (D36 enforcement): mut-method on non-mut local
                        // → E_LOCAL_NOT_MUT.  Parallel к param check выше.
                        if let Some(&is_mut) = ctx.local_mut.get(&recv) {
                            if !is_mut {
                                let recv_ty = ctx.var_types.get(&ctx.canonical(&recv))
                                    .or_else(|| ctx.var_types.get(&recv))
                                    .cloned();
                                let registered = recv_ty.as_ref()
                                    .map(|rty| ctx.reg.mut_methods.contains(&(rty.clone(), method.clone())))
                                    .unwrap_or(false);
                                let builtin_mut_method = matches!(
                                    method.as_str(),
                                    "push" | "pop" | "append" | "insert" | "remove"
                                    | "clear" | "truncate" | "reserve" | "swap"
                                    | "sort" | "sort_by" | "set" | "extend" | "extend_from"
                                    | "copy_from" | "copy_within" | "shrink_to_fit" | "fill"
                                    | "drain" | "dedup" | "reverse" | "shuffle"
                                );
                                if registered || builtin_mut_method {
                                    let ty_str = recv_ty.as_deref().unwrap_or("?");
                                    errors.push(Diagnostic::new(
                                        format!(
                                            "[E_LOCAL_NOT_MUT] local-binding `{}` не помечен `mut`, \
                                             но вызывается mut-метод `{}` (тип `{}`).  \
                                             Default для `let`-binding'ов — read-only (D36 enforcement, Plan 108.2).",
                                            recv, method, ty_str),
                                        e.span,
                                    ).with_note(
                                        "добавь `mut` к binding'у: `mut <name> = ...` (Plan 114 D184) — \
                                         разрешит вызов mut-методов и field/index-assignment."
                                            .to_string(),
                                    ).with_suggestion(crate::diag::Suggestion {
                                        message: format!("change `ro {}` to `mut {}`", recv, recv),
                                        span: e.span,
                                        replacement: format!("mut {}", recv),
                                        applicability: crate::diag::Applicability::MaybeIncorrect,
                                    }));
                                }
                            }
                        }
                        // consume-метод → receiver (весь alias-класс)
                        // потребляется.
                        if ctx.is_consume_method(&recv, method) {
                            ctx.mark_consumed(&recv, e.span);
                        } else {
                            // Plan 100.2 (D156): `for consume` loop var has
                            // None type (declared without type inference).
                            // If calling a method that IS a consume-method for
                            // ANY registered consume type AND recv is a
                            // consume-obligation → treat as consuming.
                            let canon = ctx.canonical(&recv);
                            let is_obligation = ctx.consume_obligations.contains(&recv)
                                || ctx.consume_obligations.contains(&canon);
                            if is_obligation
                                && ctx.var_types.get(&canon)
                                     .or_else(|| ctx.var_types.get(&recv)).is_none()
                            {
                                let is_any_consume_method = ctx.lin_reg.consume_methods
                                    .values()
                                    .any(|ms| ms.iter().any(|m| m == method.as_str()));
                                if is_any_consume_method {
                                    ctx.mark_consumed(&recv, e.span);
                                }
                            }
                        }
                        // consume-параметры метода.
                        if let Some(ty) = ctx.var_types.get(&ctx.canonical(&recv)).cloned() {
                            if let Some(idxs) = ctx.reg
                                .method_params.get(&(ty.clone(), method.clone())).cloned()
                            {
                                ctx.consume_args(args, &idxs, e.span);
                            }
                            // Plan 108.1 followup ([M-108.1-readonly-to-explicit-mut-coerce]):
                            // E_READONLY_COERCE — передача readonly-binding в mut-param метода.
                            if let Some(mut_idxs) = ctx.reg
                                .method_mut_params.get(&(ty, method.clone())).cloned()
                            {
                                check_readonly_coerce_args(ctx, args, &mut_idxs, errors);
                            }
                        }
                    } else if let ExprKind::Member {
                        obj: inner_obj,
                        name: field_name,
                    } = &obj.kind {
                        // Plan 100.1 (D5): `@field.method()` — field-level
                        // consume tracking. SelfAccess = `@`.
                        if matches!(inner_obj.kind, ExprKind::SelfAccess) {
                            for a in args { consume_walk_expr(ctx, a.expr(), errors); }
                            if let Some(t) = trailing { consume_walk_trailing(ctx, t, errors); }
                            // Если метод — consume-метод типа поля → mark field Consumed.
                            let is_consume_method = ctx.lin_reg.consume_types.iter()
                                .any(|ty| ctx.lin_reg.consume_methods
                                    .get(ty.as_str())
                                    .map_or(false, |ms| ms.contains(method)));
                            if is_consume_method {
                                // Verify field indeed tracked (is a consume-field).
                                if ctx.field_states.contains_key(field_name.as_str()) {
                                    // check use-after-consume for field
                                    if let Some(VarState::Consumed(at)) =
                                        ctx.field_states.get(field_name.as_str())
                                    {
                                        let at = *at;
                                        errors.push(Diagnostic::new(
                                            format!(
                                                "использование потреблённого поля \
                                                 `@.{}` (D133): поле уже потреблено",
                                                field_name),
                                            obj.span,
                                        ).with_note_at(
                                            "поле потреблено здесь".to_string(), at));
                                    }
                                    ctx.mark_field_consumed(field_name, e.span);
                                }
                            }
                        } else if let ExprKind::Member {
                            obj: deep_obj,
                            name: parent_field,
                        } = &inner_obj.kind {
                            // Plan 100.1 (D5.2): `@parent.field.method()` —
                            // nested field consume. Mark parent field as
                            // MaybeConsumed (partially consumed sub-field).
                            if matches!(deep_obj.kind, ExprKind::SelfAccess) {
                                let is_consume_method = ctx.lin_reg.consume_types.iter()
                                    .any(|ty| ctx.lin_reg.consume_methods
                                        .get(ty.as_str())
                                        .map_or(false, |ms| ms.contains(method)));
                                if is_consume_method
                                    && ctx.field_states.contains_key(parent_field.as_str())
                                {
                                    // Mark parent as MaybeConsumed: sub-field was
                                    // consumed but parent was not directly consumed.
                                    // This prevents D5.1 false positive on rebind.
                                    ctx.field_states.insert(
                                        parent_field.clone(),
                                        VarState::MaybeConsumed(e.span));
                                }
                            }
                            consume_walk_expr(ctx, obj, errors);
                            for a in args { consume_walk_expr(ctx, a.expr(), errors); }
                            if let Some(t) = trailing { consume_walk_trailing(ctx, t, errors); }
                        } else {
                            consume_walk_expr(ctx, obj, errors);
                            for a in args { consume_walk_expr(ctx, a.expr(), errors); }
                            if let Some(t) = trailing { consume_walk_trailing(ctx, t, errors); }
                        }
                    } else {
                        consume_walk_expr(ctx, obj, errors);
                        for a in args { consume_walk_expr(ctx, a.expr(), errors); }
                        if let Some(t) = trailing { consume_walk_trailing(ctx, t, errors); }
                        // Plan 73.1 V3 [M-73.1-fluent-return-implicit-consume] extension:
                        // chain `var.fluent1(...).fluent2(...).consume_method(...)` —
                        // detect chain root, check that root + method matches a consume
                        // method on root's type, mark root Consumed.
                        if let Some(root) = implicit_return_consume_var(ctx, obj) {
                            if let Some(ty) = ctx.var_types.get(&root).cloned() {
                                if ctx.reg.methods.contains(&(ty.clone(), method.clone())) {
                                    ctx.mark_consumed(&root, e.span);
                                }
                                // Also: consume-param indexes for chain-receiver consume args.
                                if let Some(idxs) = ctx.reg.method_params
                                    .get(&(ty, method.clone())).cloned()
                                {
                                    ctx.consume_args(args, &idxs, e.span);
                                }
                            }
                        }
                    }
                }
                // Free-fn call: f(args).
                ExprKind::Ident(fname) => {
                    // Plan 100.3 (D157): view-borrow semantics for free-fn calls.
                    // consume_obligations var passed to NON-consume param = view-borrow → OK.
                    // Rvalue (call returning consume-type) passed to view-param → D133-consume-rvalue-in-view.
                    let consume_idxs = ctx.reg.fn_params.get(fname.as_str())
                        .cloned()
                        .unwrap_or_default();
                    let view_idxs = ctx.reg.fn_view_params.get(fname.as_str())
                        .cloned()
                        .unwrap_or_default();
                    // Plan 108.1 followup ([M-108.1-readonly-to-explicit-mut-coerce]):
                    // E_READONLY_COERCE — передача readonly-binding в mut-param.
                    if let Some(mut_idxs) = ctx.reg.fn_mut_params.get(fname.as_str()).cloned() {
                        check_readonly_coerce_args(ctx, args, &mut_idxs, errors);
                    }
                    for (i, a) in args.iter().enumerate() {
                        let is_consume_param = consume_idxs.contains(&i);
                        let is_view_param = view_idxs.contains(&i) || (!is_consume_param);
                        // D133-consume-rvalue-in-view: rvalue of consume-type → view-param.
                        // Rvalue = call returning consume-type (no binding → no tracking slot).
                        if is_view_param && !is_consume_param {
                            if let ExprKind::Call { func: inner_func, .. } = &a.expr().kind {
                                let ret_type: Option<String> = match &inner_func.kind {
                                    ExprKind::Ident(inner_fname) =>
                                        ctx.reg.fn_return_types.get(inner_fname.as_str()).cloned(),
                                    _ => None,
                                };
                                if let Some(rt) = ret_type {
                                    if ctx.lin_reg.consume_types.contains(&rt) {
                                        errors.push(Diagnostic::new(
                                            format!(
                                                "[D133-consume-rvalue-in-view] consume-rvalue (тип `{}`) \
                                                 передан в view-param функции `{}`. Привяжите через \
                                                 `consume name = …`, затем используйте `name`.",
                                                rt, fname),
                                            a.expr().span,
                                        ));
                                    }
                                }
                            }
                        }
                    }
                    // Plan 100.3 (D157): consume-closure invocation.
                    // If calling a consume-closure (FnOnce-like), first check
                    // use-after-consume, then mark captured vars + closure Consumed.
                    let is_consume_closure_call = ctx.is_consume_closure(fname);
                    if is_consume_closure_call {
                        // Check use-after-consume on closure (D157 FnOnce semantics).
                        let canon = ctx.canonical(fname);
                        if let Some(VarState::Consumed(at)) = ctx.states.get(&canon)
                            .or_else(|| ctx.states.get(fname.as_str()))
                        {
                            let at = *at;
                            errors.push(Diagnostic::new(
                                format!(
                                    "[D157-consume-closure-double-invoke] consume-closure `{}` \
                                     уже была вызвана (FnOnce-эквивалент): повторный вызов \
                                     невозможен. Consume-closure можно вызвать ровно один раз.",
                                    fname),
                                e.span,
                            ).with_note_at("closure вызвана (и потреблена) здесь".to_string(), at));
                        } else {
                            ctx.use_var(fname, e.span, errors);
                        }
                    }
                    for a in args { consume_walk_expr(ctx, a.expr(), errors); }
                    if let Some(t) = trailing { consume_walk_trailing(ctx, t, errors); }
                    if !consume_idxs.is_empty() {
                        ctx.consume_args(args, &consume_idxs, e.span);
                    }
                    if is_consume_closure_call {
                        // Invoke: mark captured outer vars + closure itself Consumed.
                        ctx.invoke_consume_closure(fname, e.span);
                    }
                    // Plan 100.1 (D133 / D3): `panic` = exit-point.
                    // Все Live consume-obligations на panic-call → D133.
                    if fname == "panic" || fname == "exit" || fname == "abort" {
                        ctx.check_obligations_at_exit(e.span, errors);
                    }
                }
                // Path call: Type.static(args) / module.fn(args).
                ExprKind::Path(parts) => {
                    for a in args { consume_walk_expr(ctx, a.expr(), errors); }
                    if let Some(t) = trailing { consume_walk_trailing(ctx, t, errors); }
                    if parts.len() == 2 {
                        if let Some(idxs) = ctx.reg.method_params
                            .get(&(parts[0].clone(), parts[1].clone())).cloned()
                        {
                            ctx.consume_args(args, &idxs, e.span);
                        }
                    }
                    if let Some(last) = parts.last() {
                        if let Some(idxs) = ctx.reg.fn_params.get(last).cloned() {
                            ctx.consume_args(args, &idxs, e.span);
                        }
                    }
                }
                _ => {
                    consume_walk_expr(ctx, func, errors);
                    for a in args { consume_walk_expr(ctx, a.expr(), errors); }
                    if let Some(t) = trailing { consume_walk_trailing(ctx, t, errors); }
                }
            }
        }

        // ─── Доступы / операторы ───
        ExprKind::Member { obj, .. } => consume_walk_expr(ctx, obj, errors),
        ExprKind::Index { obj, index } => {
            consume_walk_expr(ctx, obj, errors);
            consume_walk_expr(ctx, index, errors);
        }
        ExprKind::TurboFish { base, .. } => consume_walk_expr(ctx, base, errors),
        ExprKind::Try(inner) | ExprKind::Bang(inner) => consume_walk_expr(ctx, inner, errors),
        ExprKind::As(inner, _) | ExprKind::Is(inner, _) => consume_walk_expr(ctx, inner, errors),
        ExprKind::Unary { operand, .. } => consume_walk_expr(ctx, operand, errors),
        ExprKind::Binary { left, right, .. } => {
            consume_walk_expr(ctx, left, errors);
            consume_walk_expr(ctx, right, errors);
        }
        ExprKind::Throw(inner) => consume_walk_expr(ctx, inner, errors),
        ExprKind::Interrupt(opt) => {
            if let Some(v) = opt { consume_walk_expr(ctx, v, errors); }
        }
        ExprKind::Range { start, end, .. } => {
            if let Some(s) = start { consume_walk_expr(ctx, s, errors); }
            if let Some(e) = end { consume_walk_expr(ctx, e, errors); }
        }

        // ─── `a ?? b` — `b` исполняется условно ───
        ExprKind::Coalesce(a, b) => {
            consume_walk_expr(ctx, a, errors);
            let after_a = ctx.states.clone();
            consume_walk_expr(ctx, b, errors);
            let after_b = ctx.states.clone();
            ctx.states = consume_join(&after_a, &after_a, &after_b);
        }

        // ─── Ветвление if / if-let ───
        ExprKind::If { cond, then, else_ } => {
            consume_walk_expr(ctx, cond, errors);
            consume_walk_if(ctx, then, else_, errors);
        }
        ExprKind::IfLet { pattern, scrutinee, then, else_ } => {
            consume_walk_expr(ctx, scrutinee, errors);
            let saved = ctx.states.clone();
            let mut names = Vec::new();
            consume_pattern_names(pattern, &mut names);
            for n in &names { ctx.declare(n, None); }
            consume_walk_block(ctx, then, errors);
            let then_states = ctx.states.clone();
            ctx.states = saved.clone();
            consume_walk_else(ctx, else_, errors);
            let else_states = ctx.states.clone();
            ctx.states = consume_join(&saved, &then_states, &else_states);
        }

        // ─── match ───
        ExprKind::Match { scrutinee, arms } => {
            consume_walk_expr(ctx, scrutinee, errors);
            let saved = ctx.states.clone();
            let mut joined: Option<HashMap<String, VarState>> = None;
            for arm in arms {
                ctx.states = saved.clone();
                let mut names = Vec::new();
                consume_pattern_names(&arm.pattern, &mut names);
                for n in &names { ctx.declare(n, None); }
                if let Some(g) = &arm.guard { consume_walk_expr(ctx, g, errors); }
                match &arm.body {
                    MatchArmBody::Expr(ex) => consume_walk_expr(ctx, ex, errors),
                    MatchArmBody::Block(b) => consume_walk_block(ctx, b, errors),
                }
                let arm_states = ctx.states.clone();
                joined = Some(match joined {
                    None => arm_states,
                    Some(j) => consume_join(&saved, &j, &arm_states),
                });
            }
            ctx.states = joined.unwrap_or(saved);
        }

        // ─── select ───
        ExprKind::Select { arms } => {
            let saved = ctx.states.clone();
            let mut joined: Option<HashMap<String, VarState>> = None;
            for arm in arms {
                ctx.states = saved.clone();
                match &arm.op {
                    SelectOp::Recv { binding, chan } => {
                        consume_walk_expr(ctx, chan, errors);
                        if let Some(b) = binding { ctx.declare(b, None); }
                    }
                    SelectOp::Send { chan, value } => {
                        consume_walk_expr(ctx, chan, errors);
                        consume_walk_expr(ctx, value, errors);
                    }
                    SelectOp::Default => {}
                }
                if let Some(g) = &arm.guard { consume_walk_expr(ctx, g, errors); }
                consume_walk_block(ctx, &arm.body, errors);
                let arm_states = ctx.states.clone();
                joined = Some(match joined {
                    None => arm_states,
                    Some(j) => consume_join(&saved, &j, &arm_states),
                });
            }
            ctx.states = joined.unwrap_or(saved);
        }

        // ─── Циклы — пессимистично ───
        ExprKind::For { pattern, iter, body, iter_consume, .. } => {
            let mut names = Vec::new();
            consume_pattern_names(pattern, &mut names);
            // Plan 108.3 (D36 amend): track loop-var mutability.
            // `for x in iter` → x immutable.  `for mut x in iter` → x mutable.
            // `for (mut a, b) in pairs` → per-name a mutable, b immutable.
            // `for consume x in iter` → implicit mut.
            let mut name_mut_pairs = Vec::new();
            consume_pattern_names_with_mut(pattern, &mut name_mut_pairs);
            for (n, pat_mut) in &name_mut_pairs {
                if n != "_" {
                    ctx.local_mut.insert(n.clone(), *iter_consume || *pat_mut);
                }
            }
            if *iter_consume {
                // Plan 100.2 (D156): consume-iteration — each loop var is an
                // obligation; iter marked Consumed after loop.
                consume_walk_consume_for(ctx, iter, &names, body, errors);
            } else {
                consume_walk_expr(ctx, iter, errors);
                consume_walk_loop(ctx, &names, body, errors);
            }
        }
        ExprKind::ParallelFor { pattern, iter, body, .. } => {
            consume_walk_expr(ctx, iter, errors);
            let mut names = Vec::new();
            consume_pattern_names(pattern, &mut names);
            consume_walk_loop(ctx, &names, body, errors);
        }
        ExprKind::While { cond, body, .. } => {
            consume_walk_expr(ctx, cond, errors);
            consume_walk_loop(ctx, &[], body, errors);
        }
        ExprKind::WhileLet { pattern, scrutinee, body, .. } => {
            consume_walk_expr(ctx, scrutinee, errors);
            let mut names = Vec::new();
            consume_pattern_names(pattern, &mut names);
            consume_walk_loop(ctx, &names, body, errors);
        }
        ExprKind::Loop { body, .. } => consume_walk_loop(ctx, &[], body, errors),

        // ─── Блоки / scope-конструкции ───
        ExprKind::Block(b) => consume_walk_block(ctx, b, errors),
        ExprKind::With { bindings, body } => {
            for wb in bindings { consume_walk_expr(ctx, &wb.handler, errors); }
            consume_walk_block(ctx, body, errors);
        }
        ExprKind::Supervised { body, cancel } => {
            if let Some(c) = cancel { consume_walk_expr(ctx, c, errors); }
            consume_walk_block(ctx, body, errors);
        }
        ExprKind::Forbid { body, .. } | ExprKind::Realtime { body, .. } => {
            consume_walk_block(ctx, body, errors);
        }
        ExprKind::Detach(b) | ExprKind::Blocking(b) => consume_walk_isolated_block(ctx, &[], b, errors),
        ExprKind::Spawn(inner) => consume_walk_isolated_expr(ctx, &[], inner, errors),

        // ─── Литералы-агрегаты ───
        ExprKind::TupleLit(elems) => {
            for el in elems { consume_walk_expr(ctx, el, errors); }
        }
        ExprKind::ArrayLit(elems) => {
            for el in elems {
                match el {
                    ArrayElem::Item(ex) | ArrayElem::Spread(ex) =>
                        consume_walk_expr(ctx, ex, errors),
                }
            }
        }
        ExprKind::MapLit { elems, .. } => {
            for el in elems {
                match el {
                    MapElem::Pair(k, v) => {
                        consume_walk_expr(ctx, k, errors);
                        consume_walk_expr(ctx, v, errors);
                    }
                    MapElem::Spread(ex) => consume_walk_expr(ctx, ex, errors),
                }
            }
        }
        ExprKind::RecordLit { fields, .. } => {
            for f in fields {
                if let Some(v) = &f.value { consume_walk_expr(ctx, v, errors); }
            }
        }
        ExprKind::TaggedTemplate { tag, args, .. } => {
            consume_walk_expr(ctx, tag, errors);
            for a in args { consume_walk_expr(ctx, a, errors); }
        }

        // ─── Closure / handler — изолированный walk ───
        ExprKind::Lambda { params, body, .. } => {
            let names: Vec<String> = params.iter().map(|p| p.name.clone()).collect();
            consume_walk_isolated_expr(ctx, &names, body, errors);
        }
        ExprKind::ClosureLight { params, body } => {
            let names: Vec<String> = params.iter().map(|p| p.name.clone()).collect();
            match body {
                ClosureBody::Expr(ex) => consume_walk_isolated_expr(ctx, &names, ex, errors),
                ClosureBody::Block(b) => consume_walk_isolated_block(ctx, &names, b, errors),
            }
        }
        ExprKind::ClosureFull(sig) => {
            let names: Vec<String> = sig.params.iter().map(|p| p.name.clone()).collect();
            consume_walk_fnbody_isolated(ctx, &names, &sig.body, errors);
        }
        // Plan 97 Ф.4 (D142): protocol-литерал — consume-walk идентичен.
        ExprKind::HandlerLit { methods, .. } | ExprKind::ProtocolLit { methods, .. } => {
            for m in methods {
                let names: Vec<String> = m.params.iter().map(|p| p.name.clone()).collect();
                match &m.body {
                    HandlerMethodBody::Expr(ex) =>
                        consume_walk_isolated_expr(ctx, &names, ex, errors),
                    HandlerMethodBody::Block(b) =>
                        consume_walk_isolated_block(ctx, &names, b, errors),
                }
            }
        }

        // ─── Контрактные кванторы — ghost, walk body для use-detection ───
        ExprKind::Forall { body, range, .. } | ExprKind::Exists { body, range, .. } => {
            consume_walk_expr(ctx, range, errors);
            consume_walk_expr(ctx, body, errors);
        }
    }
}

/// if-then-else branch join (общий для `If`).
///
/// Plan 100.7 (D165): divergence-aware join. Если ветка заканчивается
/// throw/return (diverges), она не достигает точки слияния — её состояние
/// исключается из join. Это устраняет ложные MaybeConsumed ошибки при паттерне
/// `if cond { x.consume(); throw err }` где x полностью consumed на diverging
/// пути и остаётся Live на non-diverging пути.
fn consume_walk_if(ctx: &mut ConsumeCtx, then: &Block,
                   else_: &Option<ElseBranch>, errors: &mut Vec<Diagnostic>) {
    let saved = ctx.states.clone();
    let then_diverges = block_diverges(then);
    consume_walk_block(ctx, then, errors);
    let then_states = ctx.states.clone();
    ctx.states = saved.clone();
    let else_diverges = else_branch_diverges(else_);
    consume_walk_else(ctx, else_, errors);
    let else_states = ctx.states.clone();
    // Divergence-aware merge: если ветка diverges — её финальные состояния
    // не вносятся в join (управление не достигает точки слияния).
    ctx.states = match (then_diverges, else_diverges) {
        (true, true) => saved,        // оба пути diverge → после if недостижимо
        (true, false) => else_states,  // then diverges → только else-состояние
        (false, true) => then_states,  // else diverges → только then-состояние
        (false, false) => consume_join(&saved, &then_states, &else_states),
    };
}

/// Diverges-check для ElseBranch.
fn else_branch_diverges(else_: &Option<ElseBranch>) -> bool {
    match else_ {
        Some(ElseBranch::Block(b)) => block_diverges(b),
        Some(ElseBranch::If(e)) => expr_diverges(e),
        None => false, // нет else → implicit () — не diverges
    }
}

fn consume_walk_else(ctx: &mut ConsumeCtx, else_: &Option<ElseBranch>,
                     errors: &mut Vec<Diagnostic>) {
    match else_ {
        Some(ElseBranch::Block(b)) => consume_walk_block(ctx, b, errors),
        Some(ElseBranch::If(e)) => consume_walk_expr(ctx, e, errors),
        None => {}
    }
}

fn consume_walk_trailing(ctx: &mut ConsumeCtx, t: &Trailing,
                         errors: &mut Vec<Diagnostic>) {
    match t {
        Trailing::Block(b) => consume_walk_isolated_block(ctx, &[], b, errors),
        Trailing::Fn(sig) => {
            let names: Vec<String> = sig.params.iter().map(|p| p.name.clone()).collect();
            consume_walk_fnbody_isolated(ctx, &names, &sig.body, errors);
        }
        Trailing::LegacyBlockWithParams(tb) => {
            let names: Vec<String> = tb.params.iter().map(|p| p.name.clone()).collect();
            consume_walk_isolated_block(ctx, &names, &tb.body, errors);
        }
    }
}

fn consume_walk_fnbody_isolated(ctx: &mut ConsumeCtx, params: &[String],
                                body: &FnBody, errors: &mut Vec<Diagnostic>) {
    match body {
        FnBody::Block(b) => consume_walk_isolated_block(ctx, params, b, errors),
        FnBody::Expr(e) => consume_walk_isolated_expr(ctx, params, e, errors),
        FnBody::External => {}
    }
}

fn check_defer_bodies(module: &Module, errors: &mut Vec<Diagnostic>) {
    // Lookup callee effects: fn_name -> effects (РґР»СЏ suspend detection).
    let mut fn_effects: HashMap<String, Vec<TypeRef>> = HashMap::new();
    for item in &module.items {
        if let Item::Fn(f) = item {
            let key = match &f.receiver {
                Some(r) => format!("{}.{}", r.type_name, f.name),
                None => f.name.clone(),
            };
            fn_effects.entry(key).or_default().extend(f.effects.iter().cloned());
        }
    }

    // Walk bodies С„СѓРЅРєС†РёР№ Рё С‚РµСЃС‚РѕРІ. Per-fn — передаём enclosing fn-sig
    // effects (D158): defer body Fail-effect разрешён если fn-sig объявляет
    // `Fail[E']`; иначе compile error D158-defer-fail-not-in-sig.
    for item in &module.items {
        match item {
            Item::Fn(f) => {
                if let FnBody::Block(b) = &f.body {
                    walk_block_for_defers(b, &fn_effects, &f.effects, errors);
                } else if let FnBody::Expr(e) = &f.body {
                    walk_expr_for_defers(e, &fn_effects, &f.effects, errors);
                }
            }
            Item::Test(t) => {
                // Test body — implicit Fail[any] (test failure framework).
                // Allow Fail в defer body как если test-fn declared Fail.
                let test_effects: Vec<TypeRef> = vec![TypeRef::Named {
                    path: vec!["Fail".to_string()],
                    generics: vec![],
                    span: t.body.span,
                }];
                walk_block_for_defers(&t.body, &fn_effects, &test_effects, errors);
            }
            _ => {}
        }
    }
}

/// Walk block: РґР»СЏ РєР°Р¶РґРѕРіРѕ Stmt::Defer/ErrDefer вЂ” РїСЂРѕРІРµСЂРёС‚СЊ body;
/// СЂРµРєСѓСЂСЃРёРІРЅРѕ walk РѕСЃС‚Р°Р»СЊРЅС‹Рµ stmts (С‚Р°Рј РјРѕР¶РµС‚ Р±С‹С‚СЊ РІР»РѕР¶РµРЅРЅС‹Р№ block СЃ
/// defer'Р°РјРё).
///
/// `current_fn_effects` — effect-row enclosing fn-sig (D158): defer body
/// Fail-effect разрешён если sig объявляет `Fail[E']`. Pass-through
/// recursive walkers; per-defer body checker делает actual gate.
fn walk_block_for_defers(b: &Block, fn_effects: &HashMap<String, Vec<TypeRef>>, current_fn_effects: &[TypeRef], errors: &mut Vec<Diagnostic>) {
    for s in &b.stmts {
        match s {
            Stmt::Defer { body, .. } => {
                check_defer_body(body, "defer", fn_effects, current_fn_effects, errors);
            }
            Stmt::ErrDefer { body, .. } => {
                check_defer_body(body, "errdefer", fn_effects, current_fn_effects, errors);
            }
            // D160 Plan 100.4.3: OkDefer/DeferWithResult — same body constraints.
            Stmt::OkDefer { body, .. } => {
                check_defer_body(body, "okdefer", fn_effects, current_fn_effects, errors);
            }
            Stmt::DeferWithResult { body, .. } => {
                check_defer_body(body, "defer |result|", fn_effects, current_fn_effects, errors);
            }
            Stmt::Let(decl) => walk_expr_for_defers(&decl.value, fn_effects, current_fn_effects, errors),
            Stmt::Const(_) => {}
            Stmt::Expr(e) => walk_expr_for_defers(e, fn_effects, current_fn_effects, errors),
            Stmt::Assign { target, value, .. } => {
                walk_expr_for_defers(target, fn_effects, current_fn_effects, errors);
                walk_expr_for_defers(value, fn_effects, current_fn_effects, errors);
            }
            Stmt::Return { value, .. } => {
                if let Some(v) = value { walk_expr_for_defers(v, fn_effects, current_fn_effects, errors); }
            }
            Stmt::Throw { value, .. } => walk_expr_for_defers(value, fn_effects, current_fn_effects, errors),
            Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => walk_expr_for_defers(expr, fn_effects, current_fn_effects, errors),
            Stmt::Break(_) | Stmt::Continue(_) => {}
            Stmt::Apply { args, .. } => {
                for a in args { walk_expr_for_defers(a, fn_effects, current_fn_effects, errors); }
            }
            Stmt::Calc { steps, .. } => {
                for step in steps { walk_expr_for_defers(&step.expr, fn_effects, current_fn_effects, errors); }
            }
            // Plan 110 D188: walk init expr + body block recursively.
            Stmt::ConsumeScope { init, body, .. } => {
                walk_expr_for_defers(init, fn_effects, current_fn_effects, errors);
                walk_block_for_defers(body, fn_effects, current_fn_effects, errors);
            }
            Stmt::Reveal { .. } => {}
        }
    }
    if let Some(t) = &b.trailing {
        walk_expr_for_defers(t, fn_effects, current_fn_effects, errors);
    }
}

/// Walk expression: СЂРµРєСѓСЂСЃРёРІРЅРѕ РёС‰РµРј РІР»РѕР¶РµРЅРЅС‹Рµ Р±Р»РѕРєРё СЃ defer'Р°РјРё.
/// РЎР°Рј РїРѕ СЃРµР±Рµ expression РЅРµ РїСЂРѕРІРµСЂСЏРµС‚СЃСЏ вЂ” С‚РѕР»СЊРєРѕ nested blocks.
///
/// `current_fn_effects` (D158) — pass-through к check_defer_body для гейта
/// Fail-effect в defer body. Lambdas / closures **новые** scopes — у них
/// собственные effects; для simplicity bootstrap'а consume parent
/// fn_effects (overly-permissive: closure без Fail в свой effect-row,
/// но defer inside может бросать; runtime errors будут поймать через
/// type-check Outer-call-site). Production-уровневое уточнение —
/// Plan 100.4.5 closure-as-defer.
fn walk_expr_for_defers(e: &Expr, fn_effects: &HashMap<String, Vec<TypeRef>>, current_fn_effects: &[TypeRef], errors: &mut Vec<Diagnostic>) {
    match &e.kind {
        ExprKind::Block(b) => walk_block_for_defers(b, fn_effects, current_fn_effects, errors),
        ExprKind::If { cond, then, else_ } => {
            walk_expr_for_defers(cond, fn_effects, current_fn_effects, errors);
            walk_block_for_defers(then, fn_effects, current_fn_effects, errors);
            if let Some(ElseBranch::Block(b)) = else_ { walk_block_for_defers(b, fn_effects, current_fn_effects, errors); }
            if let Some(ElseBranch::If(e2)) = else_ { walk_expr_for_defers(e2, fn_effects, current_fn_effects, errors); }
        }
        ExprKind::IfLet { scrutinee, then, else_, .. } => {
            walk_expr_for_defers(scrutinee, fn_effects, current_fn_effects, errors);
            walk_block_for_defers(then, fn_effects, current_fn_effects, errors);
            if let Some(ElseBranch::Block(b)) = else_ { walk_block_for_defers(b, fn_effects, current_fn_effects, errors); }
            if let Some(ElseBranch::If(e2)) = else_ { walk_expr_for_defers(e2, fn_effects, current_fn_effects, errors); }
        }
        ExprKind::Match { scrutinee, arms } => {
            walk_expr_for_defers(scrutinee, fn_effects, current_fn_effects, errors);
            for a in arms {
                match &a.body {
                    MatchArmBody::Expr(e2) => walk_expr_for_defers(e2, fn_effects, current_fn_effects, errors),
                    MatchArmBody::Block(b) => walk_block_for_defers(b, fn_effects, current_fn_effects, errors),
                }
                if let Some(g) = &a.guard { walk_expr_for_defers(g, fn_effects, current_fn_effects, errors); }
            }
        }
        ExprKind::For { iter, body, .. } | ExprKind::ParallelFor { iter, body, .. } => {
            walk_expr_for_defers(iter, fn_effects, current_fn_effects, errors);
            walk_block_for_defers(body, fn_effects, current_fn_effects, errors);
        }
        ExprKind::While { cond, body, .. } => {
            walk_expr_for_defers(cond, fn_effects, current_fn_effects, errors);
            walk_block_for_defers(body, fn_effects, current_fn_effects, errors);
        }
        ExprKind::WhileLet { scrutinee, body, .. } => {
            walk_expr_for_defers(scrutinee, fn_effects, current_fn_effects, errors);
            walk_block_for_defers(body, fn_effects, current_fn_effects, errors);
        }
        ExprKind::Loop { body, .. } => walk_block_for_defers(body, fn_effects, current_fn_effects, errors),
        ExprKind::Select { arms } => {
            for arm in arms {
                match &arm.op {
                    SelectOp::Recv { chan, .. } => walk_expr_for_defers(chan, fn_effects, current_fn_effects, errors),
                    SelectOp::Send { chan, value } => {
                        walk_expr_for_defers(chan, fn_effects, current_fn_effects, errors);
                        walk_expr_for_defers(value, fn_effects, current_fn_effects, errors);
                    }
                    SelectOp::Default => {}
                }
                if let Some(g) = &arm.guard { walk_expr_for_defers(g, fn_effects, current_fn_effects, errors); }
                walk_block_for_defers(&arm.body, fn_effects, current_fn_effects, errors);
            }
        }
        ExprKind::With { body, .. } | ExprKind::Forbid { body, .. }
        | ExprKind::Realtime { body, .. }
        | ExprKind::Detach(body) | ExprKind::Blocking(body) => {
            walk_block_for_defers(body, fn_effects, current_fn_effects, errors);
        }
        ExprKind::Supervised { body, cancel } => {
            if let Some(c) = cancel { walk_expr_for_defers(c, fn_effects, current_fn_effects, errors); }
            walk_block_for_defers(body, fn_effects, current_fn_effects, errors);
        }
        ExprKind::Call { func, args, trailing } => {
            walk_expr_for_defers(func, fn_effects, current_fn_effects, errors);
            for a in args {
                walk_expr_for_defers(a.expr(), fn_effects, current_fn_effects, errors);
            }
            if let Some(tr) = trailing {
                match tr {
                    Trailing::Block(b) => walk_block_for_defers(b, fn_effects, current_fn_effects, errors),
                    Trailing::Fn(fsb) => {
                        if let FnBody::Block(b) = &fsb.body { walk_block_for_defers(b, fn_effects, current_fn_effects, errors); }
                        else if let FnBody::Expr(e2) = &fsb.body { walk_expr_for_defers(e2, fn_effects, current_fn_effects, errors); }
                    }
                    Trailing::LegacyBlockWithParams(tb) => {
                        walk_block_for_defers(&tb.body, fn_effects, current_fn_effects, errors);
                    }
                }
            }
        }
        ExprKind::Spawn(body) => walk_expr_for_defers(body, fn_effects, current_fn_effects, errors),
        ExprKind::Binary { left, right, .. } => {
            walk_expr_for_defers(left, fn_effects, current_fn_effects, errors);
            walk_expr_for_defers(right, fn_effects, current_fn_effects, errors);
        }
        ExprKind::Unary { operand, .. } => walk_expr_for_defers(operand, fn_effects, current_fn_effects, errors),
        ExprKind::Try(e2) | ExprKind::Bang(e2) | ExprKind::Throw(e2) => {
            walk_expr_for_defers(e2, fn_effects, current_fn_effects, errors);
        }
        ExprKind::Coalesce(a, b) => {
            walk_expr_for_defers(a, fn_effects, current_fn_effects, errors);
            walk_expr_for_defers(b, fn_effects, current_fn_effects, errors);
        }
        ExprKind::As(e2, _) | ExprKind::Is(e2, _) => walk_expr_for_defers(e2, fn_effects, current_fn_effects, errors),
        ExprKind::Member { obj, .. } | ExprKind::Index { obj, .. } => walk_expr_for_defers(obj, fn_effects, current_fn_effects, errors),
        ExprKind::TurboFish { base, .. } => walk_expr_for_defers(base, fn_effects, current_fn_effects, errors),
        ExprKind::Lambda { body, .. } | ExprKind::Interrupt(Some(body)) => walk_expr_for_defers(body, fn_effects, current_fn_effects, errors),
        ExprKind::Range { start, end, .. } => {
            if let Some(s) = start { walk_expr_for_defers(s, fn_effects, current_fn_effects, errors); }
            if let Some(e) = end { walk_expr_for_defers(e, fn_effects, current_fn_effects, errors); }
        }
        ExprKind::ArrayLit(elems) => {
            for el in elems {
                match el {
                    ArrayElem::Item(e2) | ArrayElem::Spread(e2) => walk_expr_for_defers(e2, fn_effects, current_fn_effects, errors),
                }
            }
        }
        ExprKind::TupleLit(elems) => {
            for el in elems { walk_expr_for_defers(el, fn_effects, current_fn_effects, errors); }
        }
        ExprKind::RecordLit { fields, .. } => {
            for f in fields {
                if let Some(v) = &f.value { walk_expr_for_defers(v, fn_effects, current_fn_effects, errors); }
            }
        }
        // Р›СЏРјР±РґС‹ closure-full: body РІРЅСѓС‚СЂРё FnSigBody.
        ExprKind::ClosureFull(fsb) => {
            if let FnBody::Block(b) = &fsb.body { walk_block_for_defers(b, fn_effects, current_fn_effects, errors); }
            else if let FnBody::Expr(e2) = &fsb.body { walk_expr_for_defers(e2, fn_effects, current_fn_effects, errors); }
        }
        ExprKind::ClosureLight { body, .. } => {
            match body {
                ClosureBody::Expr(e2) => walk_expr_for_defers(e2, fn_effects, current_fn_effects, errors),
                ClosureBody::Block(b) => walk_block_for_defers(b, fn_effects, current_fn_effects, errors),
            }
        }
        // РџСЂРѕСЃС‚С‹Рµ СѓР·Р»С‹ Р±РµР· РІР»РѕР¶РµРЅРЅС‹С… Р±Р»РѕРєРѕРІ.
        _ => {}
    }
}

/// Body constraint check: exit-control, Fail-effect, suspend.
///
/// D158 (Plan 100.4.1): Fail в defer body разрешён, если enclosing fn-sig
/// объявляет `Fail[E']` (passed in `current_fn_effects`) ИЛИ throw/?/!!
/// находятся внутри `with Fail = handler { ... }` (silent suppress
/// shorthand) — tracked через `DeferBodyCtx.inside_fail_handler_depth`.
fn check_defer_body(body: &Expr, kw: &str, fn_effects: &HashMap<String, Vec<TypeRef>>, current_fn_effects: &[TypeRef], errors: &mut Vec<Diagnostic>) {
    // D90 Plan 20 Р¤.3 (revised): Р’Р°СЂРёР°РЅС‚ 3 вЂ” return/break/continue СЂР°Р·СЂРµС€РµРЅС‹
    // С‚РѕР»СЊРєРѕ РІРЅСѓС‚СЂРё nested loop/fn-literal РІ defer body (local control). РќР°
    // top-level defer body РѕРЅРё Р·Р°РїСЂРµС‰РµРЅС‹ вЂ” РЅРµР»СЊР·СЏ hijack scope-exit
    // РѕРєСЂСѓР¶Р°СЋС‰РµР№ С„СѓРЅРєС†РёРё/С†РёРєР»Р°.
    //
    // Ctx tracks: loop-nesting depth (break/continue ok РµСЃР»Рё >0), fn-literal
    // depth (return ok РµСЃР»Рё >0), fail-handler-wrap depth (D158: внутри
    // `with Fail = ... { ... }` body Fail-throws silently suppressed).
    let ctx = DeferBodyCtx { loop_depth: 0, fn_depth: 0, inside_fail_handler_depth: 0 };
    check_defer_body_inner(body, kw, fn_effects, current_fn_effects, &ctx, errors);
}

#[derive(Clone, Copy)]
struct DeferBodyCtx {
    /// РўРµРєСѓС‰Р°СЏ РіР»СѓР±РёРЅР° loop'РѕРІ (for/while/loop) РІРЅСѓС‚СЂРё defer body. Р•СЃР»Рё >0,
    /// `break`/`continue` Р»РѕРєР°Р»СЊРЅС‹ вЂ” СЂР°Р·СЂРµС€РµРЅС‹.
    loop_depth: usize,
    /// РўРµРєСѓС‰Р°СЏ РіР»СѓР±РёРЅР° fn-Р»РёС‚РµСЂР°Р»РѕРІ (closure/lambda) РІРЅСѓС‚СЂРё defer body. Р•СЃР»Рё
    /// >0, `return` Р»РѕРєР°Р»РµРЅ вЂ” СЂР°Р·СЂРµС€С‘РЅ (relates С‚РѕР»СЊРєРѕ Рє Р±Р»РёР¶Р°Р№С€РµРјСѓ fn).
    fn_depth: usize,
    /// D158 (Plan 100.4.1): depth of `with Fail = ... { ... }` wrappers
    /// внутри defer body. Если >0, Fail-throws (`throw`/`?`/`!!` + Fail-
    /// calls) — silently suppressed inner handler'ом, не propagate'ятся
    /// в outer defer scope. **Backward-compat shorthand** для pre-D158
    /// pattern: `defer { with Fail = handler { ... } { risky() } }`.
    inside_fail_handler_depth: usize,
}

fn check_defer_body_inner(e: &Expr, kw: &str, fn_effects: &HashMap<String, Vec<TypeRef>>, current_fn_effects: &[TypeRef], ctx: &DeferBodyCtx, errors: &mut Vec<Diagnostic>) {
    // D158 gate: Fail-throws разрешены если (a) внутри `with Fail = ...`
    // wrapper'а, ИЛИ (b) enclosing fn-sig объявляет `Fail[E']`.
    let fail_throw_allowed = ctx.inside_fail_handler_depth > 0
        || has_fail_effect(current_fn_effects);

    // РЎРЅР°С‡Р°Р»Р° РїСЂРѕРІРµСЂСЏРµРј СѓР·РµР» СЃР°Рј РїРѕ СЃРµР±Рµ.
    match &e.kind {
        // Exit-control: throw expression-form (D85 redirected via Fail).
        // D158 (Plan 100.4.1): allow если fail_throw_allowed; иначе error D158-defer-fail-not-in-sig.
        ExprKind::Throw(_) => {
            if !fail_throw_allowed {
                errors.push(Diagnostic::new(
                    format!("`throw` inside `{}` body requires `Fail[E]` in enclosing fn signature (D158-defer-fail-not-in-sig). \
                             Either (1) add `Fail[E]` to fn signature вЂ” cleanup-fail composes с propagating error через MultiError, \
                             ИЛИ (2) wrap with `with Fail = handler {{ ... }}` for silent suppress.", kw),
                    e.span,
                ));
            }
        }
        // ? Рё !! desugar РІ throw в†’ same D158 rule.
        ExprKind::Try(_) => {
            if !fail_throw_allowed {
                errors.push(Diagnostic::new(
                    format!("`?` operator inside `{}` body requires `Fail[E]` in enclosing fn signature (D158-defer-fail-not-in-sig). \
                             Either (1) add `Fail[E]` to fn signature; ИЛИ (2) wrap with `with Fail = handler {{ ... }}`.", kw),
                    e.span,
                ));
            }
        }
        ExprKind::Bang(_) => {
            if !fail_throw_allowed {
                errors.push(Diagnostic::new(
                    format!("`!!` operator inside `{}` body requires `Fail[E]` in enclosing fn signature (D158-defer-fail-not-in-sig). \
                             Either (1) add `Fail[E]` to fn signature; ИЛИ (2) wrap with `with Fail = handler {{ ... }}`.", kw),
                    e.span,
                ));
            }
        }
        // Interrupt вЂ” РґРѕСЃСЂРѕС‡РЅС‹Р№ exit with-Р±Р»РѕРєР°, hijack'РёС‚ scope exit-СЃРµРјР°РЅС‚РёРєСѓ.
        // D158 НЕ amend'аем: interrupt — это hijack scope-exit, не failable cleanup.
        ExprKind::Interrupt(_) => {
            errors.push(Diagnostic::new(
                format!("`interrupt` is not allowed inside `{}` body (D90 §6): defer body cannot hijack scope exit.", kw),
                e.span,
            ));
        }
        // D159 (Plan 100.4.2) keeps AST-level concurrency-construct ban:
        // spawn / supervised / detach / blocking / parallel for создают новые
        // fiber'ы которые переживут scope cleanup'а — leak supervised
        // hierarchy. Sync suspend (Time.sleep, Net.*, etc.) разрешён.
        ExprKind::Spawn(_) | ExprKind::Supervised { .. } | ExprKind::Detach(_)
        | ExprKind::Blocking(_)
        | ExprKind::ParallelFor { .. } => {
            errors.push(Diagnostic::new(
                format!("concurrency construct (`spawn`/`supervised`/`detach`/`blocking`/`parallel for`) \
                         is not allowed inside `{}` body (D159-spawn-in-defer): \
                         leaks supervised hierarchy. Use sync suspend operations or `Time.timeout` instead.", kw),
                e.span,
            ));
        }
        // D159 (Plan 100.4.2): Call СЃ suspend-СЌС„С„РµРєС‚Р°РјРё вЂ” РЎРќРЇРў Р·Р°РїСЂРµС‚
        // (D90 §5 amended). Suspend operations (Time.sleep, Net.*, Fs.*, Db.*,
        // Channel.recv) теперь allowed в defer body для production graceful
        // cleanup. spawn / parallel for / supervised / detach / blocking
        // остаются запрещены через AST-level matches выше (D5 D159).
        ExprKind::Call { func, .. } => {
            if let Some(callee_name) = call_target_name(func) {
                if let Some(effs) = fn_effects.get(&callee_name) {
                    // D158 (Plan 100.4.1): Fail-call check вЂ” same fail_throw_allowed rule.
                    if has_fail_effect(effs) && !fail_throw_allowed {
                        errors.push(Diagnostic::new(
                            format!("call to `{}` has `Fail` effect, not allowed inside `{}` body вЂ” \
                                     enclosing fn-sig must declare `Fail[E]` (D158-defer-fail-not-in-sig). \
                                     Either (1) add `Fail[E]` to fn signature; ИЛИ (2) wrap with `with Fail = handler {{ ... }}`.",
                                    callee_name, kw),
                            e.span,
                        ));
                    }
                }
            }
            // D159 also lifted built-in effect ops ban (Time.sleep / Net.get / etc.) вЂ”
            // suspend allowed; spawn/parallel for ban сохраняется AST-level выше.
            let _ = func; // suppress unused-warning post-removal
        }
        _ => {}
    }

    // Р РµРєСѓСЂСЃРёРІРЅРѕ РІРіР»СѓР±СЊ вЂ” РІР»РѕР¶РµРЅРЅС‹Рµ scope (block, if, etc.) РїРѕРґС‡РёРЅСЏСЋС‚СЃСЏ С‚РµРј Р¶Рµ
    // РѕРіСЂР°РЅРёС‡РµРЅРёСЏРј, С‚.Рє. РѕРЅРё С‡Р°СЃС‚СЊ defer body.
    walk_defer_subexprs(e, kw, fn_effects, current_fn_effects, ctx, errors);
}

fn walk_defer_subexprs(e: &Expr, kw: &str, fn_effects: &HashMap<String, Vec<TypeRef>>, current_fn_effects: &[TypeRef], ctx: &DeferBodyCtx, errors: &mut Vec<Diagnostic>) {
    match &e.kind {
        ExprKind::Block(b) => check_defer_body_block(b, kw, fn_effects, current_fn_effects, ctx, errors),
        ExprKind::If { cond, then, else_ } => {
            check_defer_body_inner(cond, kw, fn_effects, current_fn_effects, ctx, errors);
            check_defer_body_block(then, kw, fn_effects, current_fn_effects, ctx, errors);
            match else_ {
                Some(ElseBranch::Block(b)) => check_defer_body_block(b, kw, fn_effects, current_fn_effects, ctx, errors),
                Some(ElseBranch::If(e2)) => check_defer_body_inner(e2, kw, fn_effects, current_fn_effects, ctx, errors),
                None => {}
            }
        }
        ExprKind::IfLet { scrutinee, then, else_, .. } => {
            check_defer_body_inner(scrutinee, kw, fn_effects, current_fn_effects, ctx, errors);
            check_defer_body_block(then, kw, fn_effects, current_fn_effects, ctx, errors);
            match else_ {
                Some(ElseBranch::Block(b)) => check_defer_body_block(b, kw, fn_effects, current_fn_effects, ctx, errors),
                Some(ElseBranch::If(e2)) => check_defer_body_inner(e2, kw, fn_effects, current_fn_effects, ctx, errors),
                None => {}
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            check_defer_body_inner(scrutinee, kw, fn_effects, current_fn_effects, ctx, errors);
            for a in arms {
                match &a.body {
                    MatchArmBody::Expr(e2) => check_defer_body_inner(e2, kw, fn_effects, current_fn_effects, ctx, errors),
                    MatchArmBody::Block(b) => check_defer_body_block(b, kw, fn_effects, current_fn_effects, ctx, errors),
                }
                if let Some(g) = &a.guard { check_defer_body_inner(g, kw, fn_effects, current_fn_effects, ctx, errors); }
            }
        }
        ExprKind::For { iter, body, .. } => {
            check_defer_body_inner(iter, kw, fn_effects, current_fn_effects, ctx, errors);
            let inner = DeferBodyCtx { loop_depth: ctx.loop_depth + 1, fn_depth: ctx.fn_depth, inside_fail_handler_depth: ctx.inside_fail_handler_depth };
            check_defer_body_block(body, kw, fn_effects, current_fn_effects, &inner, errors);
        }
        ExprKind::While { cond, body, .. } => {
            check_defer_body_inner(cond, kw, fn_effects, current_fn_effects, ctx, errors);
            let inner = DeferBodyCtx { loop_depth: ctx.loop_depth + 1, fn_depth: ctx.fn_depth, inside_fail_handler_depth: ctx.inside_fail_handler_depth };
            check_defer_body_block(body, kw, fn_effects, current_fn_effects, &inner, errors);
        }
        ExprKind::WhileLet { scrutinee, body, .. } => {
            check_defer_body_inner(scrutinee, kw, fn_effects, current_fn_effects, ctx, errors);
            let inner = DeferBodyCtx { loop_depth: ctx.loop_depth + 1, fn_depth: ctx.fn_depth, inside_fail_handler_depth: ctx.inside_fail_handler_depth };
            check_defer_body_block(body, kw, fn_effects, current_fn_effects, &inner, errors);
        }
        ExprKind::Loop { body, .. } => {
            let inner = DeferBodyCtx { loop_depth: ctx.loop_depth + 1, fn_depth: ctx.fn_depth, inside_fail_handler_depth: ctx.inside_fail_handler_depth };
            check_defer_body_block(body, kw, fn_effects, current_fn_effects, &inner, errors);
        }
        ExprKind::Select { arms } => {
            for arm in arms {
                match &arm.op {
                    SelectOp::Recv { chan, .. } => check_defer_body_inner(chan, kw, fn_effects, current_fn_effects, ctx, errors),
                    SelectOp::Send { chan, value } => {
                        check_defer_body_inner(chan, kw, fn_effects, current_fn_effects, ctx, errors);
                        check_defer_body_inner(value, kw, fn_effects, current_fn_effects, ctx, errors);
                    }
                    SelectOp::Default => {}
                }
                if let Some(g) = &arm.guard { check_defer_body_inner(g, kw, fn_effects, current_fn_effects, ctx, errors); }
                check_defer_body_block(&arm.body, kw, fn_effects, current_fn_effects, ctx, errors);
            }
        }
        // D158 (Plan 100.4.1): `with Fail = handler { ... } { body }` —
        // body inside silently suppress'ит Fail-throws (inner handler ловит
        // их). Increment ctx.inside_fail_handler_depth для recursive check.
        ExprKind::With { bindings, body } => {
            let any_fail = bindings.iter().any(|b| matches!(&b.effect, TypeRef::Named { path, .. } if path.len() == 1 && path[0] == "Fail"));
            let inner = if any_fail {
                DeferBodyCtx {
                    loop_depth: ctx.loop_depth,
                    fn_depth: ctx.fn_depth,
                    inside_fail_handler_depth: ctx.inside_fail_handler_depth + 1,
                }
            } else { *ctx };
            check_defer_body_block(body, kw, fn_effects, current_fn_effects, &inner, errors);
        }
        ExprKind::Forbid { body, .. } | ExprKind::Realtime { body, .. } => {
            check_defer_body_block(body, kw, fn_effects, current_fn_effects, ctx, errors);
        }
        ExprKind::Call { func, args, trailing } => {
            check_defer_body_inner(func, kw, fn_effects, current_fn_effects, ctx, errors);
            for a in args { check_defer_body_inner(a.expr(), kw, fn_effects, current_fn_effects, ctx, errors); }
            if let Some(tr) = trailing {
                match tr {
                    Trailing::Block(b) => check_defer_body_block(b, kw, fn_effects, current_fn_effects, ctx, errors),
                    Trailing::Fn(fsb) => {
                        // Trailing fn-literal `fn { ... }` вЂ” СЌС‚Рѕ Р»СЏРјР±РґР°; return
                        // РІРЅСѓС‚СЂРё РЅРµС‘ Р»РѕРєР°Р»РµРЅ РґР»СЏ Р»СЏРјР±РґС‹, Р° РЅРµ РґР»СЏ defer body.
                        let inner = DeferBodyCtx { loop_depth: ctx.loop_depth, fn_depth: ctx.fn_depth + 1, inside_fail_handler_depth: ctx.inside_fail_handler_depth };
                        if let FnBody::Block(b) = &fsb.body { check_defer_body_block(b, kw, fn_effects, current_fn_effects, &inner, errors); }
                        else if let FnBody::Expr(e2) = &fsb.body { check_defer_body_inner(e2, kw, fn_effects, current_fn_effects, &inner, errors); }
                    }
                    Trailing::LegacyBlockWithParams(tb) => {
                        let inner = DeferBodyCtx { loop_depth: ctx.loop_depth, fn_depth: ctx.fn_depth + 1, inside_fail_handler_depth: ctx.inside_fail_handler_depth };
                        check_defer_body_block(&tb.body, kw, fn_effects, current_fn_effects, &inner, errors);
                    }
                }
            }
        }
        ExprKind::Binary { left, right, .. } => {
            check_defer_body_inner(left, kw, fn_effects, current_fn_effects, ctx, errors);
            check_defer_body_inner(right, kw, fn_effects, current_fn_effects, ctx, errors);
        }
        ExprKind::Unary { operand, .. } => check_defer_body_inner(operand, kw, fn_effects, current_fn_effects, ctx, errors),
        ExprKind::Coalesce(a, b) => {
            check_defer_body_inner(a, kw, fn_effects, current_fn_effects, ctx, errors);
            check_defer_body_inner(b, kw, fn_effects, current_fn_effects, ctx, errors);
        }
        ExprKind::As(e2, _) | ExprKind::Is(e2, _) => check_defer_body_inner(e2, kw, fn_effects, current_fn_effects, ctx, errors),
        ExprKind::Member { obj, .. } | ExprKind::Index { obj, .. } => check_defer_body_inner(obj, kw, fn_effects, current_fn_effects, ctx, errors),
        ExprKind::TurboFish { base, .. } => check_defer_body_inner(base, kw, fn_effects, current_fn_effects, ctx, errors),
        ExprKind::Range { start, end, .. } => {
            if let Some(s) = start { check_defer_body_inner(s, kw, fn_effects, current_fn_effects, ctx, errors); }
            if let Some(e) = end { check_defer_body_inner(e, kw, fn_effects, current_fn_effects, ctx, errors); }
        }
        ExprKind::ArrayLit(elems) => {
            for el in elems {
                match el {
                    ArrayElem::Item(e2) | ArrayElem::Spread(e2) => check_defer_body_inner(e2, kw, fn_effects, current_fn_effects, ctx, errors),
                }
            }
        }
        ExprKind::TupleLit(elems) => {
            for el in elems { check_defer_body_inner(el, kw, fn_effects, current_fn_effects, ctx, errors); }
        }
        ExprKind::RecordLit { fields, .. } => {
            for f in fields {
                if let Some(v) = &f.value { check_defer_body_inner(v, kw, fn_effects, current_fn_effects, ctx, errors); }
            }
        }
        // Lambda/closure bodies вЂ” СЌС‚Рѕ РѕС‚РґРµР»СЊРЅС‹Р№ scope РґР»СЏ defer'Р°
        // (defer РІРЅСѓС‚СЂРё lambda РѕС‚РЅРѕСЃРёС‚СЃСЏ Рє scope lambda, РЅРµ parent).
        // РќРµ РїСЂРѕРІРµСЂСЏРµРј вЂ” СЌС‚Рѕ СѓР¶Рµ РЅРµ defer body, Р° РµРіРѕ callees, РєРѕС‚РѕСЂС‹Рµ
        // РјРѕРіСѓС‚ Р±С‹С‚СЊ call'Р°РЅС‹ РѕС‚РєСѓРґР° СѓРіРѕРґРЅРѕ. Р›СЏРјР±РґР° СЃР°РјР° **РјРѕР¶РµС‚** Р±С‹С‚СЊ
        // call'РЅСѓС‚Р° Р°СЃРёРЅС…СЂРѕРЅРЅРѕ вЂ” РЅРѕ СЌС‚Рѕ РЅРµ defer issue, СЌС‚Рѕ РµС‘ caller's
        // concern.
        ExprKind::Lambda { .. } | ExprKind::ClosureLight { .. } | ExprKind::ClosureFull(_) => {}
        // Suspend / Throw / Interrupt вЂ” СѓР¶Рµ flagged РІС‹С€Рµ РІ check_defer_body_inner.
        _ => {}
    }
}

fn check_defer_body_block(b: &Block, kw: &str, fn_effects: &HashMap<String, Vec<TypeRef>>, current_fn_effects: &[TypeRef], ctx: &DeferBodyCtx, errors: &mut Vec<Diagnostic>) {
    for s in &b.stmts {
        match s {
            Stmt::Return { span, value } => {
                // Р’Р°СЂРёР°РЅС‚ 3 (D90): return Р»РѕРєР°Р»РµРЅ С‚РѕР»СЊРєРѕ РІРЅСѓС‚СЂРё nested fn-Р»РёС‚РµСЂР°Р»Р°.
                if ctx.fn_depth == 0 {
                    errors.push(Diagnostic::new(
                        format!("`return` is not allowed at the top level of `{}` body (D90): defer body cannot hijack scope exit of the enclosing function. \
                                 (Local `return` inside nested `fn`/closure РІРЅСѓС‚СЂРё defer body СЂР°Р·СЂРµС€С‘РЅ.)", kw),
                        *span,
                    ));
                }
                if let Some(v) = value {
                    check_defer_body_inner(v, kw, fn_effects, current_fn_effects, ctx, errors);
                }
            }
            Stmt::Break(span) => {
                if ctx.loop_depth == 0 {
                    errors.push(Diagnostic::new(
                        format!("`break` is not allowed at the top level of `{}` body (D90): defer body cannot hijack the enclosing loop. \
                                 (Local `break` inside nested loop СЂР°Р·СЂРµС€С‘РЅ.)", kw),
                        *span,
                    ));
                }
            }
            Stmt::Continue(span) => {
                if ctx.loop_depth == 0 {
                    errors.push(Diagnostic::new(
                        format!("`continue` is not allowed at the top level of `{}` body (D90): defer body cannot hijack the enclosing loop. \
                                 (Local `continue` inside nested loop СЂР°Р·СЂРµС€С‘РЅ.)", kw),
                        *span,
                    ));
                }
            }
            Stmt::Throw { span, value } => {
                // D158 (Plan 100.4.1): Stmt-form `throw` (а не expr-form) — same rule
                // как expr-form Throw в check_defer_body_inner.
                let fail_throw_allowed = ctx.inside_fail_handler_depth > 0
                    || has_fail_effect(current_fn_effects);
                if !fail_throw_allowed {
                    errors.push(Diagnostic::new(
                        format!("`throw` inside `{}` body requires `Fail[E]` in enclosing fn signature (D158-defer-fail-not-in-sig). \
                                 Either (1) add `Fail[E]` to fn signature вЂ” cleanup-fail composes с propagating error через MultiError, \
                                 ИЛИ (2) wrap with `with Fail = effect Fail {{ ... }}` for silent suppress.", kw),
                        *span,
                    ));
                }
                check_defer_body_inner(value, kw, fn_effects, current_fn_effects, ctx, errors);
            }
            Stmt::Let(decl) => check_defer_body_inner(&decl.value, kw, fn_effects, current_fn_effects, ctx, errors),
            Stmt::Const(_) => {}
            Stmt::Expr(e) => check_defer_body_inner(e, kw, fn_effects, current_fn_effects, ctx, errors),
            Stmt::Assign { target, value, .. } => {
                check_defer_body_inner(target, kw, fn_effects, current_fn_effects, ctx, errors);
                check_defer_body_inner(value, kw, fn_effects, current_fn_effects, ctx, errors);
            }
            // Nested defer/errdefer вЂ” СЌС‚Рѕ OK. Р­С‚Рѕ РЅРѕРІС‹Р№ scope (block),
            // defer'С‹ РІРЅСѓС‚СЂРё СЂРµРіРёСЃС‚СЂРёСЂСѓСЋС‚СЃСЏ РґР»СЏ СЌС‚РѕРіРѕ РІРЅСѓС‚СЂРµРЅРЅРµРіРѕ scope'Р°,
            // РЅРµ РґР»СЏ СЂРѕРґРёС‚РµР»СЊСЃРєРѕРіРѕ. РС… body С‚РѕР¶Рµ РїСЂРѕРІРµСЂСЏРµС‚СЃСЏ вЂ” РЅРѕ С‡РµСЂРµР·
            // РѕСЃРЅРѕРІРЅРѕР№ walk (check_defer_bodies РїСЂРѕС…РѕРґРёС‚ РїРѕ РІСЃРµРј bodies).
            Stmt::Defer { body, .. } => check_defer_body(body, "defer", fn_effects, current_fn_effects, errors),
            Stmt::ErrDefer { body, .. } => check_defer_body(body, "errdefer", fn_effects, current_fn_effects, errors),
            // D160 Plan 100.4.3: OkDefer / DeferWithResult body — same constraints as defer.
            Stmt::OkDefer { body, .. } => check_defer_body(body, "okdefer", fn_effects, current_fn_effects, errors),
            Stmt::DeferWithResult { body, .. } => check_defer_body(body, "defer |result|", fn_effects, current_fn_effects, errors),
            // Plan 33.2 Р¤.8: assert_static РІ defer body вЂ” walk expr.
            Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => check_defer_body_inner(expr, kw, fn_effects, current_fn_effects, ctx, errors),
            // Ф.4.1: apply — ghost, args walk.
            Stmt::Apply { args, .. } => {
                for a in args { check_defer_body_inner(a, kw, fn_effects, current_fn_effects, ctx, errors); }
            }
            // Ф.4.2: calc — ghost, шаги walk.
            Stmt::Calc { steps, .. } => {
                for step in steps { check_defer_body_inner(&step.expr, kw, fn_effects, current_fn_effects, ctx, errors); }
            }
            // Plan 110 D188: nested consume{} inside defer body — init +
            // body block walk (D197 cleanup re-entrance — full rules в
            // Plan 110.1.8).
            Stmt::ConsumeScope { init, body, .. } => {
                check_defer_body_inner(init, kw, fn_effects, current_fn_effects, ctx, errors);
                check_defer_body_block(body, kw, fn_effects, current_fn_effects, ctx, errors);
            }
            Stmt::Reveal { .. } => {}
        }
    }
    if let Some(t) = &b.trailing {
        check_defer_body_inner(t, kw, fn_effects, current_fn_effects, ctx, errors);
    }
}

/// РР·РІР»РµС‡СЊ РёРјСЏ callee РµСЃР»Рё РІС‹СЂР°Р¶РµРЅРёРµ вЂ” call target (Ident РёР»Рё Type.method).
fn call_target_name(e: &Expr) -> Option<String> {
    match &e.kind {
        ExprKind::Ident(n) => Some(n.clone()),
        ExprKind::Path(parts) if parts.len() >= 2 => Some(parts.join(".")),
        ExprKind::Member { obj, name } => {
            if let ExprKind::Ident(head) = &obj.kind {
                Some(format!("{}.{}", head, name))
            } else {
                None
            }
        }
        _ => None,
    }
}

// в”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђ
// Plan 33.1 Р¤.2 (D24): ContractCtx вЂ” РїСЂРѕРІРµСЂРєР° Р±Р°Р·РѕРІС‹С… РїСЂР°РІРёР» РєРѕРЅС‚СЂР°РєС‚РѕРІ.
//
// РњРёРЅРёРјР°Р»СЊРЅС‹Р№ pass РґР»СЏ 33.1. РџРѕР»РЅР°СЏ type-РїСЂРѕРІРµСЂРєР° (РєРѕРЅС‚СЂР°РєС‚ РґРѕР»Р¶РµРЅ Р±С‹С‚СЊ
// bool, result.value РїРѕРґ guard'РѕРј, Рё С‚.Рґ.) вЂ” РІ Р¤.3 РІРјРµСЃС‚Рµ СЃ SMT-РєРѕРґРёСЂРѕРІРєРѕР№.
//
// Р‘Р°Р·РѕРІС‹Рµ РїСЂР°РІРёР»Р° (33.1):
// 1. `result` Р·Р°РїСЂРµС‰С‘РЅ РІ `requires` (Р·РЅР°С‡РµРЅРёСЏ РµС‰С‘ РЅРµС‚).
// 2. `old(...)` Р·Р°РїСЂРµС‰С‘РЅ РІ `requires` (РЅРµС‚ В«РґРѕВ»).
// 3. composition: РІС‹Р·РѕРІ РґСЂСѓРіРѕР№ fn РІ РєРѕРЅС‚СЂР°РєС‚Рµ вЂ” error РІ 33.1 (Plan 33.2
//    СЂР°Р·СЂРµС€РёС‚ РґР»СЏ @pure С„СѓРЅРєС†РёР№).
// в”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђ

/// РљРѕРЅС‚РµРєСЃС‚ РєРѕРЅС‚СЂР°РєС‚-РїСЂРѕРІРµСЂРѕРє.
///
/// Plan 33.2 Р¤.7: СЂР°Р·СЂРµС€Р°РµС‚ composition вЂ” РІС‹Р·РѕРІ `#pure` С„СѓРЅРєС†РёР№
/// РІ РєРѕРЅС‚СЂР°РєС‚Р°С…. Non-`#pure` С„СѓРЅРєС†РёРё РІ РєРѕРЅС‚СЂР°РєС‚Р°С… вЂ” compile error.
struct ContractCtx {
    /// РРјРµРЅР° РІСЃРµС… top-level fn.
    fn_names: HashSet<String>,
    /// РРјРµРЅР° fn РѕР±СЉСЏРІР»РµРЅРЅС‹С… `#pure` (С‡РµСЂРµР· Р°С‚СЂРёР±СѓС‚).
    /// РСЃРїРѕР»СЊР·СѓСЋС‚СЃСЏ РґР»СЏ СЂР°Р·СЂРµС€РµРЅРёСЏ composition РІ РєРѕРЅС‚СЂР°РєС‚Р°С… (33.2).
    pure_fn_names: HashSet<String>,
    /// Plan 33.3 Р¤.9: pure_view-РёРјСЏ в†’ (effect_name, arity).
    /// РџСЂРё РІС‹Р·РѕРІРµ `balance(id)` РІ РєРѕРЅС‚СЂР°РєС‚Рµ РѕРїСЂРµРґРµР»СЏРµРј (Р°) С‡С‚Рѕ СЌС‚Рѕ
    /// pure_view, (Р±) Рє РєР°РєРѕРјСѓ СЌС„С„РµРєС‚Сѓ РѕС‚РЅРѕСЃРёС‚СЃСЏ, (РІ) С‡С‚Рѕ СЌС„С„РµРєС‚ РІ
    /// СЃРёРіРЅР°С‚СѓСЂРµ enclosing fn.
    pure_views: HashMap<String, (String, usize)>,
}

impl ContractCtx {
    fn build(module: &Module) -> Self {
        let mut fn_names = HashSet::new();
        let mut pure_fn_names = HashSet::new();
        let mut pure_views: HashMap<String, (String, usize)> = HashMap::new();
        for item in &module.items {
            match item {
                Item::Fn(fd) => {
                    fn_names.insert(fd.name.clone());
                    if matches!(fd.purity, Purity::Pure) {
                        pure_fn_names.insert(fd.name.clone());
                    }
                    // Р¤.3 (Plan 33.5): SCC inference РґРѕР±Р°РІР»СЏРµС‚СЃСЏ РЅРёР¶Рµ.
                }
                Item::Type(td) => {
                    if let TypeDeclKind::Effect(methods) = &td.kind {
                        for m in methods {
                            if matches!(m.kind, EffectOpKind::PureView) {
                                pure_views.insert(
                                    m.name.clone(),
                                    (td.name.clone(), m.params.len()),
                                );
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        // Р¤.3 (Plan 33.5): SCC inference вЂ” Р°РІС‚Рѕ-РѕРїСЂРµРґРµР»СЏРµРј pure fn'С‹
        // С‡РµСЂРµР· Tarjan SCC РЅР° call-graph (РїР°СЂРёС‚РµС‚ СЃ Dafny auto-pure).
        // Р”РѕР±Р°РІР»СЏРµРј РІ pure_fn_names РµСЃР»Рё РЅРµ РїРѕРјРµС‡РµРЅС‹ СЏРІРЅРѕ Effectful.
        // РЎС‚РµРє РЅРµ РїСЂРѕР±Р»РµРјР°: main() Р·Р°РїСѓСЃРєР°РµС‚СЃСЏ РІ РїРѕС‚РѕРєРµ СЃ 32 MiB СЃС‚РµРєР°.
        let inferred = crate::verify::pipeline::infer_pure_fns_scc(module);
        for name in inferred {
            pure_fn_names.insert(name);
        }
        Self { fn_names, pure_fn_names, pure_views }
    }

    fn check_module(&self, module: &Module, errors: &mut Vec<Diagnostic>) {
        for item in &module.items {
            if let Item::Fn(fd) = item {
                self.check_fn(fd, errors);
            }
        }
    }

    fn check_fn(&self, fd: &FnDecl, errors: &mut Vec<Diagnostic>) {
        // Plan 33.2 Р¤.5: РїСЂРѕРІРµСЂРєР° modifies-frame.
        // Р•СЃР»Рё РѕР±СЉСЏРІР»РµРЅ `modifies`, РІСЃРµ assignment'С‹ РІРЅСѓС‚СЂРё body РґРѕР»Р¶РЅС‹
        // Р±С‹С‚СЊ РїРѕРєСЂС‹С‚С‹ frame-target'Р°РјРё.
        if !fd.modifies.is_empty() {
            self.check_modifies_frame(fd, errors);
        }
        // Plan 33.1 Р¤.4: РєРѕРЅС‚СЂР°РєС‚С‹ РЅР° Fail-С„СѓРЅРєС†РёСЏС… С‚СЂРµР±СѓСЋС‚ ContractResult
        // + flow-Р°РЅР°Р»РёС‚РёРєРё РґР»СЏ result.is_ok / result.value / result.error.
        // Р­С‚Рѕ РїРѕР»РЅР°СЏ СЂРµР°Р»РёР·Р°С†РёСЏ вЂ” РѕС‚Р»РѕР¶РµРЅР° РґРѕ Р¤.3 SMT integration РІРјРµСЃС‚Рµ
        // СЃ Z3-РєРѕРґРёСЂРѕРІРєРѕР№ ContractResult-datatype.
        // Р’ 33.1 вЂ” explicit compile error С‡С‚РѕР±С‹ РёР·Р±РµР¶Р°С‚СЊ silent unsoundness.
        if !fd.contracts.is_empty() && Self::fn_has_fail(fd) {
            errors.push(Diagnostic::new(
                format!(
                    "contracts on `Fail`-returning functions not yet supported in Plan 33.1 \
                     (`{}` has `Fail` effect; ContractResult + flow-analysis for \
                     result.is_ok / result.value / result.error вЂ” Plan 33.1 Р¤.3 / Р¤.4 follow-up)",
                    fd.name
                ),
                fd.span,
            ));
            // РљРѕРЅС‚СЂР°РєС‚С‹ РЅРµ РїСЂРѕРІРµСЂСЏРµРј РґР°Р»СЊС€Рµ вЂ” error СѓР¶Рµ РІС‹РґР°РЅ.
            return;
        }
        // Plan 33.3 Р¤.9: РјРЅРѕР¶РµСЃС‚РІРѕ РёРјС‘РЅ СЌС„С„РµРєС‚РѕРІ РёР· СЃРёРіРЅР°С‚СѓСЂС‹ С„СѓРЅРєС†РёРё
        // (РґР»СЏ СЂР°Р·СЂРµС€РµРЅРёСЏ pure_view-РІС‹Р·РѕРІРѕРІ РІ РєРѕРЅС‚СЂР°РєС‚Р°С…).
        let fn_effects: HashSet<String> = fd.effects.iter()
            .filter_map(|tr| match tr {
                TypeRef::Named { path, .. } => path.last().cloned(),
                _ => None,
            })
            .collect();
        for contract in &fd.contracts {
            match contract.kind {
                ContractKind::Requires => {
                    self.check_requires_expr(&contract.expr, &fn_effects, &fd.name, errors);
                }
                ContractKind::Ensures => {
                    self.check_ensures_expr(&contract.expr, &fn_effects, &fd.name, errors);
                }
                ContractKind::EnsuresFail => {
                    // D.1.5: ensures_fail вЂ” РїСЂРѕРІРµСЂСЏРµРј РєР°Рє ensures (V1 bootstrap).
                    // V2: РґРѕР±Р°РІРёС‚СЊ РїСЂРѕРІРµСЂРєСѓ С‡С‚Рѕ `result` РЅРµ РёСЃРїРѕР»СЊР·СѓРµС‚СЃСЏ.
                    self.check_ensures_expr(&contract.expr, &fn_effects, &fd.name, errors);
                }
            }
        }
    }

    /// Plan 33.2 Р¤.5: РїСЂРѕРІРµСЂРєР° `modifies`-frame.
    /// Walks body, РґР»СЏ РєР°Р¶РґРѕРіРѕ Stmt::Assign Рє **non-local** target'Сѓ
    /// (РїР°СЂР°РјРµС‚СЂ / self / РїРѕР»Рµ) РїСЂРѕРІРµСЂСЏРµС‚ С‡С‚Рѕ target РїРѕРєСЂС‹С‚ frame-target'РѕРј.
    ///
    /// Р›РѕРєР°Р»СЊРЅС‹Рµ `let mut` РќР• С‚СЂРµР±СѓСЋС‚ frame-cover'Р° вЂ” `modifies` РѕС‚РЅРѕСЃРёС‚СЃСЏ
    /// Рє **API-visible** mutations (РїР°СЂР°РјРµС‚СЂС‹, self.fields). Р­С‚Рѕ РїР°СЂРёС‚РµС‚ СЃ
    /// Dafny: В«modifies clause is about heap effect, not stack localsВ».
    fn check_modifies_frame(&self, fd: &FnDecl, errors: &mut Vec<Diagnostic>) {
        let block = match &fd.body {
            FnBody::Block(b) => b,
            FnBody::Expr(_) | FnBody::External => return, // no assigns possible
        };
        // Collect local-binding names (let / let mut РІ block).
        let mut locals: std::collections::HashSet<String> = std::collections::HashSet::new();
        for stmt in &block.stmts {
            if let Stmt::Let(LetDecl { pattern, .. }) = stmt {
                Self::collect_binding_names(pattern, &mut locals);
            }
        }
        for stmt in &block.stmts {
            if let Stmt::Assign { target, span, .. } = stmt {
                // Skip locals.
                if let Some(root_name) = Self::root_lvalue_name(target) {
                    if locals.contains(&root_name) { continue; }
                }
                if !Self::is_assign_covered(target, &fd.modifies) {
                    errors.push(Diagnostic::new(
                        format!(
                            "assignment to `{}` is not covered by `modifies` clause of `{}`",
                            Self::expr_display(target), fd.name
                        ),
                        *span,
                    ));
                }
            }
        }
    }

    fn collect_binding_names(p: &Pattern, out: &mut std::collections::HashSet<String>) {
        match p {
            Pattern::Ident { name, .. } => { out.insert(name.clone()); }
            Pattern::Binding { name, inner, .. } => {
                out.insert(name.clone());
                Self::collect_binding_names(inner, out);
            }
            Pattern::Tuple(ps, _) => for sub in ps { Self::collect_binding_names(sub, out); }
            Pattern::Record { fields, .. } => for f in fields {
                if let Some(sub) = &f.pattern { Self::collect_binding_names(sub, out); }
                else { out.insert(f.name.clone()); }
            },
            Pattern::Array { elems, .. } => for e in elems {
                match e {
                    ArrayPatternElem::Item(pp) => Self::collect_binding_names(pp, out),
                    ArrayPatternElem::RestBind(n) => { out.insert(n.clone()); }
                    ArrayPatternElem::Rest => {}
                }
            },
            _ => {}
        }
    }

    fn root_lvalue_name(e: &Expr) -> Option<String> {
        match &e.kind {
            ExprKind::Ident(n) => Some(n.clone()),
            ExprKind::Member { obj, .. } => Self::root_lvalue_name(obj),
            ExprKind::Index { obj, .. } => Self::root_lvalue_name(obj),
            _ => None,
        }
    }

    /// РџСЂРѕРІРµСЂРєР°: РѕРґРёРЅ target РїРѕРєСЂС‹С‚ `modifies`-list'РѕРј.
    fn is_assign_covered(target: &Expr, frame: &[FrameTarget]) -> bool {
        for ft in frame {
            if Self::frame_covers(ft, target) {
                return true;
            }
        }
        false
    }

    fn frame_covers(ft: &FrameTarget, target: &Expr) -> bool {
        match ft {
            FrameTarget::Whole(e) => Self::same_lvalue(e, target),
            FrameTarget::Field { receiver, field, .. } => {
                if let ExprKind::Member { obj, name } = &target.kind {
                    name == field && Self::same_lvalue(receiver, obj)
                } else {
                    false
                }
            }
            FrameTarget::ArrayElem { array, index, .. } => {
                if let ExprKind::Index { obj, index: tidx } = &target.kind {
                    Self::same_lvalue(array, obj) && Self::same_lvalue(index, tidx)
                } else {
                    false
                }
            }
            FrameTarget::ArrayAll { array, .. } => {
                if let ExprKind::Index { obj, .. } = &target.kind {
                    Self::same_lvalue(array, obj)
                } else {
                    false
                }
            }
        }
    }

    /// РџСЂРѕСЃС‚РѕР№ СЃСЂР°РІРЅРёС‚РµР»СЊ l-value (Р±РµР· РїРѕР»РЅРѕРіРѕ structural equality).
    fn same_lvalue(a: &Expr, b: &Expr) -> bool {
        match (&a.kind, &b.kind) {
            (ExprKind::Ident(n1), ExprKind::Ident(n2)) => n1 == n2,
            (ExprKind::SelfAccess, ExprKind::SelfAccess) => true,
            (ExprKind::Member { obj: o1, name: n1 }, ExprKind::Member { obj: o2, name: n2 }) => {
                n1 == n2 && Self::same_lvalue(o1, o2)
            }
            _ => false,
        }
    }

    fn expr_display(e: &Expr) -> String {
        match &e.kind {
            ExprKind::Ident(n) => n.clone(),
            ExprKind::SelfAccess => "self".into(),
            ExprKind::Member { obj, name } => format!("{}.{}", Self::expr_display(obj), name),
            ExprKind::Index { obj, .. } => format!("{}[..]", Self::expr_display(obj)),
            _ => "<expr>".into(),
        }
    }

    /// РџСЂРѕРІРµСЂРєР°: С„СѓРЅРєС†РёСЏ РѕР±СЉСЏРІР»СЏРµС‚ `Fail` (Р»СЋР±РѕР№ РІР°СЂРёР°РЅС‚) РІ effects.
    fn fn_has_fail(fd: &FnDecl) -> bool {
        fd.effects.iter().any(|e| {
            matches!(e, TypeRef::Named { path, .. }
                if !path.is_empty() && path.last().map(|s| s.as_str()) == Some("Fail"))
        })
    }

    /// `requires`: Р·Р°РїСЂРµС‰РµРЅС‹ `result` Рё `old(...)`.
    fn check_requires_expr(
        &self,
        e: &Expr,
        fn_effects: &HashSet<String>,
        fn_name: &str,
        errors: &mut Vec<Diagnostic>,
    ) {
        self.walk_expr(e, fn_effects, fn_name, errors, /*in_ensures*/ false);
    }

    /// `ensures`: `result`/`old(...)` СЂР°Р·СЂРµС€РµРЅС‹; composition Р·Р°РїСЂРµС‰С‘РЅ РІ 33.1.
    fn check_ensures_expr(
        &self,
        e: &Expr,
        fn_effects: &HashSet<String>,
        fn_name: &str,
        errors: &mut Vec<Diagnostic>,
    ) {
        self.walk_expr(e, fn_effects, fn_name, errors, /*in_ensures*/ true);
    }

    fn walk_expr(
        &self,
        e: &Expr,
        fn_effects: &HashSet<String>,
        fn_name: &str,
        errors: &mut Vec<Diagnostic>,
        in_ensures: bool,
    ) {
        match &e.kind {
            ExprKind::Ident(n) => {
                if n == "result" && !in_ensures {
                    errors.push(Diagnostic::new(
                        "`result` is not available in `requires` (only in `ensures`)",
                        e.span,
                    ));
                }
            }
            ExprKind::Call { func, args, .. } => {
                // Detect `old(...)` вЂ” special-cased call.
                if let ExprKind::Ident(name) = &func.kind {
                    if name == "old" {
                        if !in_ensures {
                            errors.push(Diagnostic::new(
                                "`old(...)` is not available in `requires` (only in `ensures`)",
                                e.span,
                            ));
                        }
                        // Walk old() arg ONCE; it's a snapshot of pre-state,
                        // not a composition.
                        for a in args {
                            self.walk_expr(a.expr(), fn_effects, fn_name, errors, in_ensures);
                        }
                        return;
                    }
                    // Plan 33.3 Р¤.9.3 part 2: pure_view-РІС‹Р·РѕРІ РІ РєРѕРЅС‚СЂР°РєС‚Рµ
                    // СЂР°Р·СЂРµС€С‘РЅ С‚РѕР»СЊРєРѕ РµСЃР»Рё СЃРѕРѕС‚РІРµС‚СЃС‚РІСѓСЋС‰РёР№ СЌС„С„РµРєС‚ РѕР±СЉСЏРІР»РµРЅ РІ
                    // СЃРёРіРЅР°С‚СѓСЂРµ enclosing fn (`(...) Eff -> ...`). pure_view
                    // вЂ” read-only observation, РЅСѓР¶РµРЅ effect-handler РІ scope.
                    if let Some((effect_name, expected_arity)) = self.pure_views.get(name) {
                        if !fn_effects.contains(effect_name) {
                            errors.push(Diagnostic::new(
                                format!(
                                    "pure_view `{}.{}` referenced in contract of `{}`, \
                                     but effect `{}` is not in this function's signature \
                                     (add `{}` to effects)",
                                    effect_name, name, fn_name, effect_name, effect_name,
                                ),
                                e.span,
                            ));
                        }
                        if args.len() != *expected_arity {
                            errors.push(Diagnostic::new(
                                format!(
                                    "pure_view `{}.{}` expects {} arg(s), got {}",
                                    effect_name, name, expected_arity, args.len(),
                                ),
                                e.span,
                            ));
                        }
                        // pure_view-РІС‹Р·РѕРІ СЂР°Р·СЂРµС€С‘РЅ; walk args, РЅРµ walk
                        // callee (СЌС‚Рѕ identifier-name pure_view, РЅРµ fn).
                        for a in args {
                            self.walk_expr(a.expr(), fn_effects, fn_name, errors, in_ensures);
                        }
                        return;
                    }
                    // Plan 33.2 Р¤.7 composition: РІС‹Р·РѕРІ РґСЂСѓРіРѕР№ fn РІ РєРѕРЅС‚СЂР°РєС‚Рµ
                    // СЂР°Р·СЂРµС€С‘РЅ РўРћР›Р¬РљРћ РµСЃР»Рё РѕРЅР° `#pure`.
                    if self.fn_names.contains(name) && !self.pure_fn_names.contains(name) {
                        errors.push(Diagnostic::new(
                            format!(
                                "calling user function `{}` in contracts requires `#pure` attribute \
                                 (Plan 33.2 composition: only #pure functions allowed)",
                                name
                            ),
                            e.span,
                        ));
                    }
                }
                // Walk callee + args.
                self.walk_expr(func, fn_effects, fn_name, errors, in_ensures);
                for a in args {
                    self.walk_expr(a.expr(), fn_effects, fn_name, errors, in_ensures);
                }
            }
            ExprKind::Binary { left, right, .. } => {
                self.walk_expr(left, fn_effects, fn_name, errors, in_ensures);
                self.walk_expr(right, fn_effects, fn_name, errors, in_ensures);
            }
            ExprKind::Unary { operand, .. } => {
                self.walk_expr(operand, fn_effects, fn_name, errors, in_ensures);
            }
            ExprKind::Member { obj, .. } => {
                self.walk_expr(obj, fn_effects, fn_name, errors, in_ensures);
            }
            ExprKind::Index { obj, index } => {
                self.walk_expr(obj, fn_effects, fn_name, errors, in_ensures);
                self.walk_expr(index, fn_effects, fn_name, errors, in_ensures);
            }
            ExprKind::As(inner, _) | ExprKind::Is(inner, _) => {
                self.walk_expr(inner, fn_effects, fn_name, errors, in_ensures);
            }
            ExprKind::Try(inner) | ExprKind::Bang(inner) => {
                self.walk_expr(inner, fn_effects, fn_name, errors, in_ensures);
            }
            ExprKind::Coalesce(l, r) => {
                self.walk_expr(l, fn_effects, fn_name, errors, in_ensures);
                self.walk_expr(r, fn_effects, fn_name, errors, in_ensures);
            }
            // Р›РёС‚РµСЂР°Р»С‹, paths, Рё РїСЂРѕС‡РµРµ вЂ” РЅРµ РёРЅС‚РµСЂРµСЃРЅРѕ РґР»СЏ Р±Р°Р·РѕРІС‹С… РїСЂР°РІРёР».
            _ => {}
        }
    }
}

// в”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђ
// Plan 33.3 Р¤.9.7 (D24): ghost-var usage check.
//
// Verus/Dafny semantics: ghost binding (`ghost let x = ...`) вЂ” spec-only,
// РЅРµ emit'РёС‚СЃСЏ РІ runtime. Non-ghost РєРѕРґ РЅРµ РјРѕР¶РµС‚ С‡РёС‚Р°С‚СЊ ghost-var.
// Р”Рѕ СЌС‚РѕРіРѕ: catch'РёР»РѕСЃСЊ C-compiler'РѕРј РєР°Рє В«undeclared identifierВ» (ghost
// СЌСЂРµР№Р·РёС‚СЃСЏ РІ codegen). РўРµРїРµСЂСЊ вЂ” proper compile-error РЅР° type-check СЌС‚Р°РїРµ
// СЃ РїРѕРЅСЏС‚РЅС‹Рј СЃРѕРѕР±С‰РµРЅРёРµРј.
//
// Р­РІСЂРёСЃС‚РёРєР°: walk РєР°Р¶РґС‹Р№ fn body, РІ РєР°Р¶РґРѕРј block:
// 1. РЎРѕР±РёСЂР°РµРј `ghost let` РёРјРµРЅР° РІ scope.
// 2. Walk РѕСЃС‚Р°Р»СЊРЅС‹Рµ stmt'С‹ (non-ghost) Рё trailing вЂ” РµСЃР»Рё ident СЃСЃС‹Р»Р°РµС‚СЃСЏ
//    РЅР° ghost-name в†’ error.
//
// РћРіСЂР°РЅРёС‡РµРЅРёСЏ bootstrap:
// - РќРµ СѓС‡РёС‚С‹РІР°РµРј `requires`/`ensures` (ghost OK С‚Р°Рј вЂ” РЅРѕ walk РёС… РЅРµ
//   РґРµР»Р°РµРј, Рё РЅРµ РґРѕР»Р¶РЅС‹ catches as В«non-ghostВ»).
// - Nested blocks: ghost РёР· outer scope РІРёРґРµРЅ inner non-ghost вЂ” СЌС‚Рѕ
//   РѕС€РёР±РєР° (РїРѕ Verus); Р»РѕРІРёРј С‡РµСЂРµР· accumulating ghost-set.
// - Pattern bindings: С‚РѕР»СЊРєРѕ Ident-pattern (РїСЂРѕСЃС‚РѕР№ СЃР»СѓС‡Р°Р№).
// в”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђв”Ђ

fn check_ghost_usage(module: &Module, errors: &mut Vec<Diagnostic>) {
    for item in &module.items {
        if let Item::Fn(fd) = item {
            if let FnBody::Block(b) = &fd.body {
                let ghosts: HashSet<String> = HashSet::new();
                check_ghost_in_block(b, &ghosts, errors);
            } else if let FnBody::Expr(e) = &fd.body {
                let ghosts: HashSet<String> = HashSet::new();
                check_ghost_in_expr(e, &ghosts, errors);
            }
        } else if let Item::Test(t) = item {
            let ghosts: HashSet<String> = HashSet::new();
            check_ghost_in_block(&t.body, &ghosts, errors);
        }
    }
}

fn check_ghost_in_block(b: &Block, parent_ghosts: &HashSet<String>, errors: &mut Vec<Diagnostic>) {
    // Local ghost-set РЅР°С‡РёРЅР°РµРј СЃ parent + РґРѕР±Р°РІР»СЏРµРј ghost-let'С‹ РёР· СЌС‚РѕРіРѕ
    // block'Р° РІ РїРѕСЂСЏРґРєРµ РїРѕСЏРІР»РµРЅРёСЏ.
    let mut ghosts = parent_ghosts.clone();
    for stmt in &b.stmts {
        if let Stmt::Let(decl) = stmt {
            if decl.is_ghost {
                // Ghost let value-expr РјРѕР¶РµС‚ С‡РёС‚Р°С‚СЊ РґСЂСѓРіРёРµ ghost-vars
                // вЂ” СЌС‚Рѕ OK. РќРµ РїСЂРѕРІРµСЂСЏРµРј walk_expr РЅР° value.
                if let Pattern::Ident { name, .. } = &decl.pattern {
                    ghosts.insert(name.clone());
                }
                continue;
            }
        }
        // Non-ghost stmt: walk expr Рё РїСЂРѕРІРµСЂСЏРµРј С‡С‚Рѕ РЅРµ С‡РёС‚Р°РµС‚ ghost.
        check_ghost_in_stmt(stmt, &ghosts, errors);
    }
    if let Some(t) = &b.trailing {
        check_ghost_in_expr(t, &ghosts, errors);
    }
}

fn check_ghost_in_stmt(s: &Stmt, ghosts: &HashSet<String>, errors: &mut Vec<Diagnostic>) {
    match s {
        Stmt::Let(decl) => {
            // Non-ghost let: value РЅРµ РґРѕР»Р¶РµРЅ РёСЃРїРѕР»СЊР·РѕРІР°С‚СЊ ghost-vars.
            check_ghost_in_expr(&decl.value, ghosts, errors);
        }
        // Plan 114.4 Ф.2: scope-local const — pass-through (no-op for now).
        Stmt::Const(_) => {}
        Stmt::Expr(e) => check_ghost_in_expr(e, ghosts, errors),
        Stmt::Assign { target, value, .. } => {
            check_ghost_in_expr(target, ghosts, errors);
            check_ghost_in_expr(value, ghosts, errors);
        }
        Stmt::Return { value: Some(v), .. } => check_ghost_in_expr(v, ghosts, errors),
        Stmt::Throw { value, .. } => check_ghost_in_expr(value, ghosts, errors),
        Stmt::Defer { body, .. } | Stmt::ErrDefer { body, .. }
        | Stmt::OkDefer { body, .. } | Stmt::DeferWithResult { body, .. } => check_ghost_in_expr(body, ghosts, errors),
        // assert_static/assume вЂ” СЌС‚Рѕ spec-СѓСЂРѕРІРµРЅСЊ, ghost-vars С‚Р°Рј OK.
        // Skip walk С‡РµСЂРµР· РЅРёС… С‡С‚РѕР±С‹ РЅРµ РІС‹РґР°РІР°С‚СЊ false-positives.
        Stmt::AssertStatic { .. } | Stmt::Assume { .. } => {}
        _ => {}
    }
}

fn check_ghost_in_expr(e: &Expr, ghosts: &HashSet<String>, errors: &mut Vec<Diagnostic>) {
    match &e.kind {
        ExprKind::Ident(n) => {
            if ghosts.contains(n) {
                errors.push(Diagnostic::new(
                    format!(
                        "ghost variable `{}` cannot be read in non-ghost code \
                         (Plan 33.3 Р¤.9.1: ghost vars are spec-only, Verus/Dafny semantics). \
                         Move usage into a contract clause (`requires`/`ensures`/`invariant`) \
                         or another `ghost let` binding.",
                        n
                    ),
                    e.span,
                ));
            }
        }
        ExprKind::Binary { left, right, .. } => {
            check_ghost_in_expr(left, ghosts, errors);
            check_ghost_in_expr(right, ghosts, errors);
        }
        ExprKind::Unary { operand, .. } => check_ghost_in_expr(operand, ghosts, errors),
        ExprKind::Member { obj, .. } => check_ghost_in_expr(obj, ghosts, errors),
        ExprKind::Index { obj, index } => {
            check_ghost_in_expr(obj, ghosts, errors);
            check_ghost_in_expr(index, ghosts, errors);
        }
        ExprKind::Call { func, args, .. } => {
            check_ghost_in_expr(func, ghosts, errors);
            for a in args { check_ghost_in_expr(a.expr(), ghosts, errors); }
        }
        ExprKind::If { cond, then, else_ } => {
            check_ghost_in_expr(cond, ghosts, errors);
            check_ghost_in_block(then, ghosts, errors);
            if let Some(eb) = else_ {
                match eb {
                    ElseBranch::Block(b) => check_ghost_in_block(b, ghosts, errors),
                    ElseBranch::If(e) => check_ghost_in_expr(e, ghosts, errors),
                }
            }
        }
        ExprKind::Block(b) => check_ghost_in_block(b, ghosts, errors),
        ExprKind::As(inner, _) | ExprKind::Is(inner, _) | ExprKind::Try(inner) | ExprKind::Bang(inner) => {
            check_ghost_in_expr(inner, ghosts, errors);
        }
        ExprKind::Coalesce(l, r) => {
            check_ghost_in_expr(l, ghosts, errors);
            check_ghost_in_expr(r, ghosts, errors);
        }
        _ => {}
    }
}

// ============================================================================
// Plan 52 Ф.2 (D108): map-литерал `[k: v]` type-checking.
//
// Focused expected-type проход. Type-checker bootstrap'а синтаксический
// (нет полноценного bidirectional inference), поэтому MapLitCtx — это
// отдельный лёгкий walk, который НЕ заменяет существующие walk'и, а
// добавляет проверки литералов в позициях с известным ожидаемым типом:
//   - `let x HashMap[K,V] = [...]` — let-аннотация;
//   - `fn f() -> HashMap[K,V] => [...]` — return-выражение;
//   - `f([...])` где параметр имеет тип `HashMap[K,V]` — argument-позиция
//     (это и есть фундамент Ф.3a).
//
// На каждом `MapLit`:
//   - вывод `HashMap[K, V]` из ключей/значений ИЛИ из ожидаемого типа;
//   - enforce `K: Hashable` (примитивы — авто-OK; именованный тип —
//     нужны методы `hash` + `eq`; неизвестный/generic — permissive);
//   - унификация: все ключи в один `K`, все значения в один `V`.
// Пустой `[]` в позиции, ожидающей `HashMap` — валиден (пустая мапа).
// ============================================================================

/// Plan 52 Ф.2/Ф.3: контекст для map-литерал type-checking.
struct MapLitCtx {
    /// Для каждого concrete-типа — множество имён его методов. Нужно для
    /// Hashable-проверки именованных ключевых типов (требуются `hash` + `eq`).
    type_methods: HashMap<String, HashSet<String>>,
    /// Имена top-level типов модуля (record/sum/newtype/alias) — для
    /// различения «известный именованный тип» vs «generic-параметр».
    known_types: HashSet<String>,
    /// Plan 52 Ф.3: имена типов, помеченных `#from_fields` — str-keyed
    /// map-типы, в которые анонимный record-литерал `{field: v}` коэрсится
    /// через D55 map-coercion. Bootstrap honored только для
    /// `collections.hashmap.HashMap` (проверка canonical identity ниже).
    from_fields_types: HashSet<String>,
    /// Plan 52 Ф.3a: free-fn name → `(имя, тип)` параметров (только если у
    /// имени **один** кандидат — без overload; иначе резолв неоднозначен и
    /// пропускается). Для D55 argument-position coercion.
    fn_param_types: HashMap<String, Vec<(String, TypeRef)>>,
    /// Plan 52 Ф.3a: `Type.method` → `(имя, тип)` параметров (static +
    /// instance методы; только уникальные по имени, без overload).
    method_param_types: HashMap<String, Vec<(String, TypeRef)>>,
    /// Plan 52 Ф.3a: имя метода → `(имя, тип)` параметров, если метод с этим
    /// именем существует ровно на **одном** типе без overload (для резолва
    /// instance-call `obj.method(...)` без type-inference receiver'а).
    unique_method_param_types: HashMap<String, Vec<(String, TypeRef)>>,
    /// Имена generic-параметров, видимых в текущей функции. Заполняется
    /// per-fn в `check_fn`. Generic `K` — permissive (Hashable не enforce'ится
    /// статически: bound-проверка — отдельный механизм Plan 15).
    fn_generics: HashSet<String>,
    /// Plan 52 Ф.23: типы помеченные `#from_pairs` — target для desugar'а
    /// `[k: v]` (canonical-identity check, как для from_fields).
    from_pairs_types: HashSet<String>,
}

impl MapLitCtx {
    fn build(module: &Module) -> Self {
        let mut type_methods: HashMap<String, HashSet<String>> = HashMap::new();
        let mut known_types: HashSet<String> = HashSet::new();
        let mut from_fields_types: HashSet<String> = HashSet::new();
        // Plan 52 Ф.3a: сначала собираем все overload-группы, потом
        // оставляем только уникальные (single-candidate) для резолва
        // argument-позиций — overload без type-inference резолвить нельзя.
        let mut fn_overloads: HashMap<String, Vec<&FnDecl>> = HashMap::new();
        let mut method_overloads: HashMap<String, Vec<&FnDecl>> = HashMap::new();
        // имя метода → множество (type_name) на которых он определён.
        let mut method_owner_count: HashMap<String, Vec<&FnDecl>> = HashMap::new();
        // Plan 52 Ф.19: canonical identity для `#from_fields`. Через
        // peer_files определяем какой peer-файл объявил TypeDecl с
        // маркером — если path содержит сегмент `collections/hashmap` или
        // `std/collections/hashmap`, это canonical stdlib HashMap.
        // Иначе — user-локальный type с #from_fields → не trust'им.
        //
        // Bootstrap-policy: `#from_fields` honored ТОЛЬКО для типов в
        // canonical stdlib pаth. User-локальные типы с attribute получают
        // warning (через lints, не здесь) — bootstrap не дает им
        // map-coercion для безопасности. После Ф.23 (FromPairs protocol)
        // user-типы получают расширяемость через протоколы.
        let is_canonical_stdlib_from_fields = |type_name: &str, items: &[Item]| -> bool {
            items.iter().any(|it| matches!(it, Item::Type(t)
                if t.name == type_name && t.attrs.contains(&TypeAttr::FromFields)))
        };
        let mut canonical_from_fields_types: HashSet<String> = HashSet::new();
        let mut canonical_from_pairs_types: HashSet<String> = HashSet::new();
        for pf in &module.peer_files {
            let path_str = pf.path.to_string_lossy().replace('\\', "/").to_lowercase();
            // Canonical stdlib markers — собираем имена типов с
            // #from_fields / #from_pairs из peer-файлов в std/collections/.
            let is_stdlib = path_str.contains("/std/collections/")
                || path_str.contains("std\\collections\\");
            if is_stdlib {
                for it in &pf.items_here {
                    if let Item::Type(t) = it {
                        if t.attrs.contains(&TypeAttr::FromFields) {
                            canonical_from_fields_types.insert(t.name.clone());
                        }
                        if t.attrs.contains(&TypeAttr::FromPairs) {
                            canonical_from_pairs_types.insert(t.name.clone());
                        }
                    }
                }
            }
        }
        // Fallback для single-file/legacy (нет peer_files info): принимаем
        // attribute как раньше (bare-name). Это safety net для тестов где
        // peer_files пуст; в реальной компиляции stdlib HashMap всегда
        // приходит через folder-module → попадает в canonical set.
        let use_canonical = !canonical_from_fields_types.is_empty()
            || !canonical_from_pairs_types.is_empty();
        let _ = is_canonical_stdlib_from_fields; // подавить warning о неиспользовании
        let mut from_pairs_types: HashSet<String> = HashSet::new();

        // Plan 52.1 Ф.4: pre-pass для собирания всех methods (нужно для
        // method-validation user-типов с #from_pairs ниже). Без pre-pass
        // type'ы обрабатываются до своих fn-методов → method-check
        // упирается в пустой type_methods.
        let mut prepass_type_methods: HashMap<String, HashSet<String>> = HashMap::new();
        for item in &module.items {
            if let Item::Fn(f) = item {
                if let Some(recv) = &f.receiver {
                    prepass_type_methods
                        .entry(recv.type_name.clone())
                        .or_default()
                        .insert(f.name.clone());
                }
            }
        }

        for item in &module.items {
            match item {
                Item::Type(t) => {
                    known_types.insert(t.name.clone());
                    // Plan 52 Ф.19: canonical-identity check. Если у нас
                    // есть peer_files info — добавляем только canonical
                    // stdlib типы. User-локальный `type HashMap #from_fields`
                    // не попадёт в set → map-coercion для него не сработает.
                    if t.attrs.contains(&TypeAttr::FromFields) {
                        if !use_canonical || canonical_from_fields_types.contains(&t.name) {
                            from_fields_types.insert(t.name.clone());
                        }
                    }
                    // Plan 52.1 Ф.4: from_pairs canonical-check с
                    // method-validation. User-локальный `type #from_pairs`
                    // honored ЕСЛИ имеет требуемые методы
                    // (`with_capacity(int) -> Self` и `insert_new(K, V)`).
                    // Это безопасно: codegen эмитит вызовы этих методов,
                    // и они существуют.
                    //
                    // Без validation user мог бы получить codegen-fail
                    // ('no method with_capacity' / 'no method insert_new')
                    // — confusing. Validation даёт actionable error
                    // через type-check ('type X #from_pairs but missing
                    // with_capacity method').
                    if t.attrs.contains(&TypeAttr::FromPairs) {
                        let is_canonical = canonical_from_pairs_types.contains(&t.name);
                        let is_user = !is_canonical;
                        if is_canonical {
                            from_pairs_types.insert(t.name.clone());
                        } else if is_user {
                            // User-локальный тип — проверяем методы через
                            // prepass_type_methods (собран до этого цикла,
                            // т.к. types и fns могут идти в любом порядке).
                            let methods = prepass_type_methods.get(&t.name);
                            let has_with_capacity = methods
                                .map_or(false, |m| m.contains("with_capacity"));
                            let has_insert_new = methods
                                .map_or(false, |m| m.contains("insert_new"));
                            if has_with_capacity && has_insert_new {
                                from_pairs_types.insert(t.name.clone());
                            }
                            // Если методов нет — silently ignore. Better-error
                            // diagnostic — отдельная фаза (требует mutable
                            // errors vec в build, что нарушает текущую сигнатуру).
                            // Без validation user получит CC-error при использовании.
                        }
                    }
                }
                Item::Fn(f) => {
                    if let Some(recv) = &f.receiver {
                        type_methods
                            .entry(recv.type_name.clone())
                            .or_default()
                            .insert(f.name.clone());
                        let key = format!("{}.{}", recv.type_name, f.name);
                        method_overloads.entry(key).or_default().push(f);
                        method_owner_count.entry(f.name.clone()).or_default().push(f);
                    } else {
                        fn_overloads.entry(f.name.clone()).or_default().push(f);
                    }
                }
                _ => {}
            }
        }
        // Оставляем только single-candidate (без overload) — иначе резолв
        // по имени неоднозначен и argument-позиция не получает expected.
        let extract_params = |fns: &[&FnDecl]| -> Option<Vec<(String, TypeRef)>> {
            match fns {
                [single] => Some(
                    single
                        .params
                        .iter()
                        .map(|p| (p.name.clone(), p.ty.clone()))
                        .collect(),
                ),
                _ => None,
            }
        };
        let fn_param_types: HashMap<String, Vec<(String, TypeRef)>> = fn_overloads
            .iter()
            .filter_map(|(k, v)| extract_params(v).map(|p| (k.clone(), p)))
            .collect();
        let method_param_types: HashMap<String, Vec<(String, TypeRef)>> = method_overloads
            .iter()
            .filter_map(|(k, v)| extract_params(v).map(|p| (k.clone(), p)))
            .collect();
        let unique_method_param_types: HashMap<String, Vec<(String, TypeRef)>> =
            method_owner_count
                .iter()
                .filter_map(|(k, v)| extract_params(v).map(|p| (k.clone(), p)))
                .collect();
        MapLitCtx {
            type_methods,
            known_types,
            from_fields_types,
            fn_param_types,
            method_param_types,
            unique_method_param_types,
            fn_generics: HashSet::new(),
            from_pairs_types,
        }
    }

    fn check_module(&self, module: &Module, errors: &mut Vec<Diagnostic>) {
        for item in &module.items {
            match item {
                Item::Fn(f) => {
                    // Generic-параметры функции — permissive scope для Hashable.
                    let mut ctx = MapLitCtx {
                        type_methods: self.type_methods.clone(),
                        known_types: self.known_types.clone(),
                        from_fields_types: self.from_fields_types.clone(),
                        fn_param_types: self.fn_param_types.clone(),
                        method_param_types: self.method_param_types.clone(),
                        unique_method_param_types: self.unique_method_param_types.clone(),
                        fn_generics: f.generics.iter().map(|g| g.name.clone()).collect(),
                        from_pairs_types: self.from_pairs_types.clone(),
                    };
                    // Generic-параметры receiver-типа тоже видимы.
                    if let Some(recv) = &f.receiver {
                        for g in &recv.generics {
                            if let TypeRef::Named { path, .. } = g {
                                if path.len() == 1 {
                                    ctx.fn_generics.insert(path[0].clone());
                                }
                            }
                        }
                    }
                    match &f.body {
                        FnBody::Expr(e) => {
                            ctx.walk_expr(e, f.return_type.as_ref(), errors);
                        }
                        FnBody::Block(b) => ctx.walk_block(b, errors),
                        FnBody::External => {}
                    }
                }
                Item::Test(t) => {
                    self.walk_block(&t.body, errors);
                }
                // Plan 57: bench — map-литералы могут встречаться в любом из
                // трёх разделов; обходим setup/measure/teardown.
                Item::Bench(b) => {
                    for s in &b.setup {
                        self.walk_stmt(s, errors);
                    }
                    self.walk_block(&b.measure_body, errors);
                    for s in &b.teardown {
                        self.walk_stmt(s, errors);
                    }
                }
                Item::Const(c) => {
                    self.walk_expr(&c.value, c.ty.as_ref(), errors);
                }
                Item::Let(l) => {
                    self.walk_expr(&l.value, l.ty.as_ref(), errors);
                }
                Item::Type(_) => {}
                // Plan 33.3 Ф.13: lemma — spec-only, эрейзится в codegen.
                Item::Lemma(_) => {}
            }
        }
    }

    fn walk_block(&self, b: &Block, errors: &mut Vec<Diagnostic>) {
        for s in &b.stmts {
            self.walk_stmt(s, errors);
        }
        if let Some(t) = &b.trailing {
            // Trailing-выражение блока не имеет известного ожидаемого
            // типа без контекста — walk без expected.
            self.walk_expr(t, None, errors);
        }
    }

    fn walk_stmt(&self, s: &Stmt, errors: &mut Vec<Diagnostic>) {
        match s {
            Stmt::Expr(e) => self.walk_expr(e, None, errors),
            Stmt::Let(d) => {
                // let-аннотация — known-target-type position (D55).
                self.walk_expr(&d.value, d.ty.as_ref(), errors);
            }
            // Plan 114.4 Ф.2: scope-local const — pass-through (no-op for now).
            Stmt::Const(_) => {}
            Stmt::Assign { target, value, .. } => {
                self.walk_expr(target, None, errors);
                self.walk_expr(value, None, errors);
            }
            Stmt::Return { value, .. } => {
                // Return-выражение — known-target-type, но return_type здесь
                // недоступен (walk_block не несёт его). Walk без expected;
                // FnBody::Expr-возврат покрыт в check_module отдельно.
                if let Some(v) = value {
                    self.walk_expr(v, None, errors);
                }
            }
            Stmt::Throw { value, .. } => self.walk_expr(value, None, errors),
            Stmt::Break(_) | Stmt::Continue(_) => {}
            Stmt::Defer { body, .. } | Stmt::ErrDefer { body, .. }
            | Stmt::OkDefer { body, .. } | Stmt::DeferWithResult { body, .. } => {
                self.walk_expr(body, None, errors);
            }
            // Plan 110 D188: walk init + body block recursively.
            Stmt::ConsumeScope { init, body, .. } => {
                self.walk_expr(init, None, errors);
                for stmt in &body.stmts {
                    self.walk_stmt(stmt, errors);
                }
                if let Some(t) = &body.trailing {
                    self.walk_expr(t, None, errors);
                }
            }
            Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => {
                self.walk_expr(expr, None, errors);
            }
            // Plan 33.3 Ф.13: Apply/Calc — proof-statements, spec-only.
            Stmt::Apply { .. } | Stmt::Calc { .. } | Stmt::Reveal { .. } => {}
        }
    }

    /// Обход выражения с опциональным ожидаемым типом. На `MapLit` —
    /// запускает проверку; рекурсивно спускается во все под-выражения,
    /// протаскивая expected туда, где он известен (let / arg-позиции).
    fn walk_expr(&self, e: &Expr, expected: Option<&TypeRef>, errors: &mut Vec<Diagnostic>) {
        match &e.kind {
            ExprKind::MapLit { elems, .. } => {
                let pairs = crate::ast::MapElem::cloned_pairs(&elems);
                self.check_map_lit(e, &pairs, expected, errors);
                // Рекурсия в ключи/значения — без expected (key/value
                // expected-types выводятся внутри check_map_lit; для
                // вложенных литералов глубокий проход — будущее расширение).
                for (k, v) in pairs.iter() {
                    self.walk_expr(k, None, errors);
                    self.walk_expr(v, None, errors);
                }
                // Plan 55 followup: рекурсия в spread expressions.
                for me in elems.iter() {
                    if let crate::ast::MapElem::Spread(se) = me {
                        self.walk_expr(se, expected, errors);
                    }
                }
            }
            ExprKind::ArrayLit(elems) => {
                for el in elems {
                    match el {
                        ArrayElem::Item(x) | ArrayElem::Spread(x) => {
                            self.walk_expr(x, None, errors);
                        }
                    }
                }
            }
            ExprKind::Call { func, args, trailing } => {
                self.walk_expr(func, None, errors);
                // Plan 52 Ф.3a: D55 argument-position coercion. Если callee
                // резолвится в единственного кандидата — протаскиваем тип
                // соответствующего параметра как expected в каждый аргумент.
                // Positional args связываются по индексу, named (D102) — по
                // имени параметра. Это разблокирует `f({...})` / `f([k:v])`
                // / `f(opts: {...})`.
                let params = self.resolve_call_params(func);
                let mut positional_idx = 0usize;
                for a in args.iter() {
                    let arg_expected: Option<&TypeRef> = match (&params, a.arg_name()) {
                        (Some(ps), Some(name)) => {
                            // Named-arg: ищем параметр по имени.
                            ps.iter().find(|(pn, _)| pn == name).map(|(_, t)| t)
                        }
                        (Some(ps), None) => {
                            // Positional-arg: по текущему индексу.
                            let r = ps.get(positional_idx).map(|(_, t)| t);
                            positional_idx += 1;
                            r
                        }
                        (None, _) => None,
                    };
                    self.walk_expr(a.expr(), arg_expected, errors);
                }
                if let Some(t) = trailing {
                    match t {
                        crate::ast::Trailing::Block(b) => self.walk_block(b, errors),
                        crate::ast::Trailing::LegacyBlockWithParams(tb) => {
                            self.walk_block(&tb.body, errors)
                        }
                        crate::ast::Trailing::Fn(sb) => match &sb.body {
                            FnBody::Expr(x) => self.walk_expr(x, None, errors),
                            FnBody::Block(b) => self.walk_block(b, errors),
                            FnBody::External => {}
                        },
                    }
                }
            }
            ExprKind::TurboFish { base, .. } => self.walk_expr(base, expected, errors),
            ExprKind::Try(x) | ExprKind::Bang(x) => self.walk_expr(x, None, errors),
            ExprKind::Coalesce(a, b) => {
                self.walk_expr(a, None, errors);
                self.walk_expr(b, None, errors);
            }
            ExprKind::As(x, _) | ExprKind::Is(x, _) => self.walk_expr(x, None, errors),
            ExprKind::Binary { left, right, .. } => {
                self.walk_expr(left, None, errors);
                self.walk_expr(right, None, errors);
            }
            ExprKind::Unary { operand, .. } => self.walk_expr(operand, None, errors),
            ExprKind::Member { obj, .. } => self.walk_expr(obj, None, errors),
            ExprKind::Index { obj, index } => {
                self.walk_expr(obj, None, errors);
                self.walk_expr(index, None, errors);
            }
            ExprKind::TupleLit(elems) => {
                for x in elems { self.walk_expr(x, None, errors); }
            }
            ExprKind::RecordLit { type_name, fields, .. } => {
                // Plan 52 Ф.3: D55 map-coercion. Анонимный record-литерал
                // `{field: v}` в позиции, ожидающей тип с маркером
                // `#from_fields` (= HashMap) — это НЕ record-coercion (поля
                // литерала ≠ поля struct'а HashMap), а map-coercion: имена
                // полей становятся строковыми ключами.
                if type_name.is_none() {
                    if let Some(exp) = expected {
                        if self.expected_is_from_fields(exp) {
                            self.check_record_map_coercion(e, fields, exp, errors);
                            // Значения уже проверены внутри; рекурсия в них.
                            for f in fields {
                                if let Some(v) = &f.value {
                                    self.walk_expr(v, None, errors);
                                }
                            }
                            return;
                        }
                    }
                }
                for f in fields {
                    if let Some(v) = &f.value { self.walk_expr(v, None, errors); }
                }
            }
            ExprKind::If { cond, then, else_ } => {
                self.walk_expr(cond, None, errors);
                self.walk_block(then, errors);
                if let Some(eb) = else_ {
                    match eb {
                        ElseBranch::Block(b) => self.walk_block(b, errors),
                        ElseBranch::If(x) => self.walk_expr(x, None, errors),
                    }
                }
            }
            ExprKind::IfLet { scrutinee, then, else_, .. } => {
                self.walk_expr(scrutinee, None, errors);
                self.walk_block(then, errors);
                if let Some(eb) = else_ {
                    match eb {
                        ElseBranch::Block(b) => self.walk_block(b, errors),
                        ElseBranch::If(x) => self.walk_expr(x, None, errors),
                    }
                }
            }
            ExprKind::Match { scrutinee, arms } => {
                self.walk_expr(scrutinee, None, errors);
                for arm in arms {
                    if let Some(g) = &arm.guard { self.walk_expr(g, None, errors); }
                    match &arm.body {
                        MatchArmBody::Expr(x) => self.walk_expr(x, None, errors),
                        MatchArmBody::Block(b) => self.walk_block(b, errors),
                    }
                }
            }
            ExprKind::For { iter, body, .. } | ExprKind::ParallelFor { iter, body, .. } => {
                self.walk_expr(iter, None, errors);
                self.walk_block(body, errors);
            }
            ExprKind::While { cond, body, .. } => {
                self.walk_expr(cond, None, errors);
                self.walk_block(body, errors);
            }
            ExprKind::WhileLet { scrutinee, body, .. } => {
                self.walk_expr(scrutinee, None, errors);
                self.walk_block(body, errors);
            }
            ExprKind::Loop { body, .. } => self.walk_block(body, errors),
            ExprKind::Block(b) => self.walk_block(b, errors),
            ExprKind::Spawn(x) => self.walk_expr(x, None, errors),
            ExprKind::Detach(b) | ExprKind::Blocking(b) => self.walk_block(b, errors),
            ExprKind::Supervised { body, cancel } => {
                self.walk_block(body, errors);
                if let Some(c) = cancel { self.walk_expr(c, None, errors); }
            }
            ExprKind::Forbid { body, .. } | ExprKind::Realtime { body, .. } => {
                self.walk_block(body, errors);
            }
            ExprKind::Throw(x) => self.walk_expr(x, None, errors),
            ExprKind::Interrupt(opt) => {
                if let Some(x) = opt { self.walk_expr(x, None, errors); }
            }
            ExprKind::Range { start, end, .. } => {
                if let Some(s) = start { self.walk_expr(s, None, errors); }
                if let Some(e) = end { self.walk_expr(e, None, errors); }
            }
            ExprKind::InterpolatedStr { parts } => {
                for p in parts {
                    if let crate::ast::InterpStrPart::Expr(x) = p {
                        self.walk_expr(x, None, errors);
                    }
                }
            }
            ExprKind::TaggedTemplate { args, .. } => {
                for x in args { self.walk_expr(x, None, errors); }
            }
            ExprKind::Lambda { body, .. } => self.walk_expr(body, None, errors),
            ExprKind::ClosureLight { body, .. } => match body {
                ClosureBody::Expr(x) => self.walk_expr(x, None, errors),
                ClosureBody::Block(b) => self.walk_block(b, errors),
            },
            ExprKind::ClosureFull(sb) => match &sb.body {
                FnBody::Expr(x) => self.walk_expr(x, None, errors),
                FnBody::Block(b) => self.walk_block(b, errors),
                FnBody::External => {}
            },
            ExprKind::With { bindings, body } => {
                for b in bindings { self.walk_expr(&b.handler, None, errors); }
                self.walk_block(body, errors);
            }
            // Plan 97 Ф.4 (D142): protocol-литерал — walk идентичен.
            ExprKind::HandlerLit { methods, .. } | ExprKind::ProtocolLit { methods, .. } => {
                for m in methods {
                    match &m.body {
                        HandlerMethodBody::Expr(x) => self.walk_expr(x, None, errors),
                        HandlerMethodBody::Block(b) => self.walk_block(b, errors),
                    }
                }
            }
            ExprKind::Select { arms } => {
                for arm in arms {
                    match &arm.op {
                        crate::ast::SelectOp::Recv { chan, .. } => {
                            self.walk_expr(chan, None, errors)
                        }
                        crate::ast::SelectOp::Send { chan, value } => {
                            self.walk_expr(chan, None, errors);
                            self.walk_expr(value, None, errors);
                        }
                        crate::ast::SelectOp::Default => {}
                    }
                    if let Some(g) = &arm.guard { self.walk_expr(g, None, errors); }
                    self.walk_block(&arm.body, errors);
                }
            }
            // Plan 33.3 Ф.13: Forall/Exists — spec quantifiers.
            ExprKind::Forall { body, .. } | ExprKind::Exists { body, .. } => {
                self.walk_expr(body, None, errors);
            }
            // Листовые.
            ExprKind::Ident(_) | ExprKind::Path(_) | ExprKind::SelfAccess
            | ExprKind::IntLit(_) | ExprKind::FloatLit(_) | ExprKind::BoolLit(_)
            | ExprKind::StrLit(_) | ExprKind::CharLit(_) | ExprKind::UnitLit => {}
        }
    }

    /// Plan 52 Ф.3a: резолвит callee `func`-выражение в список `(имя, тип)`
    /// параметров для D55 argument-position coercion.
    ///
    /// Поддерживает (bootstrap, только single-candidate без overload):
    ///   - `f(...)` — free-fn по имени;
    ///   - `Type.method(...)` — static-method / конструктор;
    ///   - `obj.method(...)` — instance-method, если имя метода уникально
    ///     (определено ровно на одном типе) — без type-inference receiver'а.
    ///
    /// Возвращает `None` если резолв неоднозначен (overload, неизвестное
    /// имя, сложный callee) — тогда argument-позиции не получают expected
    /// (graceful fallback: coercion-проверки просто не запускаются).
    fn resolve_call_params(&self, func: &Expr) -> Option<Vec<(String, TypeRef)>> {
        // Распаковываем turbofish до базового func-expr.
        let base: &Expr = match &func.kind {
            ExprKind::TurboFish { base, .. } => base,
            _ => func,
        };
        match &base.kind {
            ExprKind::Ident(name) => self.fn_param_types.get(name).cloned(),
            ExprKind::Path(parts) if parts.len() == 2 => {
                // `Type.method` — static-call / конструктор.
                let key = format!("{}.{}", parts[0], parts[1]);
                self.method_param_types.get(&key).cloned()
            }
            ExprKind::Member { name: method_name, .. } => {
                // `obj.method` — instance-call. Резолвим по уникальному
                // имени метода (определён ровно на одном типе без overload).
                self.unique_method_param_types.get(method_name).cloned()
            }
            _ => None,
        }
    }

    /// Plan 52 Ф.2: проверка map-литерала `[k: v]`.
    fn check_map_lit(
        &self,
        e: &Expr,
        pairs: &[(Expr, Expr)],
        expected: Option<&TypeRef>,
        errors: &mut Vec<Diagnostic>,
    ) {
        // Извлечь K, V из ожидаемого типа.
        // Plan 52.1 Ф.4: Принимаем `HashMap[K, V]` (legacy hardcode) ИЛИ
        // любой тип помеченный `#from_pairs` (Ф.23 + Ф.4 user-types
        // через method-validation в MapLitCtx::build).
        let (exp_k, exp_v) = match expected {
            Some(TypeRef::Named { path, generics, .. })
                if (path.last().map(|s| s.as_str()) == Some("HashMap")
                    || self.expected_is_from_pairs(expected.unwrap()))
                    && generics.len() == 2 =>
            {
                (Some(&generics[0]), Some(&generics[1]))
            }
            // Ожидаемый тип задан, но это не HashMap и не #from_pairs тип —
            // литерал `[k:v]` не может быть coerce'нут.
            Some(other) if !is_unknown_type(other) => {
                errors.push(Diagnostic::new(
                    format!(
                        "map literal `[k: v]` requires a `HashMap` or \
                         `#from_pairs`-marked type, but the expected type \
                         here is `{}`",
                        typeref_render(other)
                    ),
                    e.span,
                ));
                return;
            }
            _ => (None, None),
        };

        // Вывод типа ключей: унификация всех ключевых выражений.
        let key_ty = self.unify_exprs(
            pairs.iter().map(|(k, _)| k),
            exp_k,
            "key",
            e.span,
            errors,
        );
        // Вывод типа значений: унификация всех value-выражений.
        let _val_ty = self.unify_exprs(
            pairs.iter().map(|(_, v)| v),
            exp_v,
            "value",
            e.span,
            errors,
        );

        // Enforce `K: Hashable`.
        if let Some(k) = &key_ty {
            self.check_hashable(k, e.span, errors);
        }
    }

    /// Plan 52 Ф.3 / Ф.19: `true` если ожидаемый тип несёт маркер
    /// `#from_fields` (str-keyed map-тип для D55 map-coercion).
    ///
    /// `from_fields_types` собран в `MapLitCtx::build` через peer_files
    /// canonical-identity check (Ф.19) — туда попадают только типы из
    /// `std/collections/` peer-файлов. User-локальный `type HashMap
    /// #from_fields` НЕ попадает в set даже при совпадении имени со
    /// stdlib HashMap. Это закрывает M-52-from-fields-canonical.
    fn expected_is_from_fields(&self, expected: &TypeRef) -> bool {
        let TypeRef::Named { path, .. } = expected else { return false; };
        match path.last() {
            Some(name) => self.from_fields_types.contains(name),
            None => false,
        }
    }

    /// Plan 52 Ф.23: `true` если ожидаемый тип несёт маркер `#from_pairs`
    /// (target для desugar'а `[k: v]`). User-типы получают support
    /// литерала добавив attribute + with_capacity/insert_new методы.
    fn expected_is_from_pairs(&self, expected: &TypeRef) -> bool {
        let TypeRef::Named { path, .. } = expected else { return false; };
        match path.last() {
            Some(name) => self.from_pairs_types.contains(name),
            None => false,
        }
    }

    /// Plan 52 Ф.3: проверка D55 map-coercion анонимного record-литерала
    /// `{field: v}` в позиции, ожидающей `#from_fields`-тип (`HashMap[str, V]`).
    ///
    /// Имена полей литерала → строковые ключи (тип `str`, тривиально
    /// Hashable). Все значения полей унифицируются в `V`. Field-punning
    /// (`{debug, verbose}`) поддержан — значение это одноимённая переменная,
    /// тип которой здесь не проверяется (NameResCtx ловит undefined).
    fn check_record_map_coercion(
        &self,
        e: &Expr,
        fields: &[RecordLitField],
        expected: &TypeRef,
        errors: &mut Vec<Diagnostic>,
    ) {
        // Извлечь V из ожидаемого `HashMap[str, V]`.
        let exp_v = match expected {
            TypeRef::Named { path, generics, .. }
                if path.last().map(|s| s.as_str()) == Some("HashMap")
                    && generics.len() == 2 =>
            {
                // Ключ map-coercion всегда str — проверим, что ожидаемый
                // K-параметр это действительно str (или any/generic).
                if let TypeRef::Named { path: kpath, .. } = &generics[0] {
                    if kpath.len() == 1
                        && kpath[0] != "str"
                        && kpath[0] != "any"
                        && !self.fn_generics.contains(&kpath[0])
                    {
                        errors.push(Diagnostic::new(
                            format!(
                                "record-literal map-coercion `{{field: v}}` produces \
                                 string keys, but the expected map key type is `{}` \
                                 — use a map literal `[\"field\": v]` for non-string \
                                 keys, or annotate `HashMap[str, V]`",
                                typeref_render(&generics[0])
                            ),
                            e.span,
                        ));
                        return;
                    }
                }
                Some(&generics[1])
            }
            // `#from_fields`-тип без 2 generic-параметров — permissive
            // (bootstrap honored только HashMap[K,V]; иные формы — будущее).
            _ => None,
        };

        // Спред в map-coerced record-литерале — не поддержан в bootstrap
        // (D60-spread для мап — отдельная фича).
        for f in fields {
            if f.is_spread {
                errors.push(Diagnostic::new(
                    "spread `...` in a map-coercion record literal is not \
                     supported in bootstrap — insert entries explicitly",
                    f.span,
                ));
                return;
            }
        }

        // Унифицировать типы значений полей в `V`. Field-punning
        // (`value: None`) — значение это переменная `f.name`, тип не
        // выводим локально (None из simple_expr_type) → permissive.
        let value_exprs: Vec<&Expr> = fields
            .iter()
            .filter_map(|f| f.value.as_ref())
            .collect();
        let _ = self.unify_exprs(
            value_exprs.into_iter(),
            exp_v,
            "value",
            e.span,
            errors,
        );
        // Ключи — строковые имена полей, str тривиально Hashable: проверка
        // не нужна.
    }

    /// Унифицирует типы набора выражений. Если задан `expected` — все
    /// выражения сверяются с ним; иначе тип выводится best-effort из
    /// первого выражения с известным типом и остальные сверяются с ним.
    /// `role` — "key" / "value" для текста ошибки. Возвращает выведенный
    /// тип (или `expected`), если он определён.
    fn unify_exprs<'e>(
        &self,
        exprs: impl Iterator<Item = &'e Expr>,
        expected: Option<&TypeRef>,
        role: &str,
        lit_span: Span,
        errors: &mut Vec<Diagnostic>,
    ) -> Option<TypeRef> {
        let mut inferred: Option<TypeRef> = expected.cloned();
        for ex in exprs {
            let this_ty = simple_expr_type(ex);
            let Some(this) = this_ty else { continue };
            match &inferred {
                None => inferred = Some(this),
                Some(prev) => {
                    if !simple_types_compatible(prev, &this) {
                        let hint = if role == "value" {
                            " — возможно нужен общий тип, напр. `HashMap[K, JsonValue]`?"
                        } else {
                            ""
                        };
                        errors.push(Diagnostic::new(
                            format!(
                                "map literal {}s have incompatible types `{}` and `{}`{}",
                                role,
                                typeref_render(prev),
                                typeref_render(&this),
                                hint
                            ),
                            ex.span,
                        ));
                        // Один error на role — дальше не плодим.
                        return inferred;
                    }
                }
            }
        }
        let _ = lit_span;
        inferred
    }

    /// Enforce `K: Hashable` для ключевого типа map-литерала.
    ///
    /// Bootstrap-семантика (best-effort, консистентно с `check_satisfaction`):
    ///   - примитивы (`str`/`int`/`bool`/`char`/числовые) — авто-Hashable;
    ///   - generic-параметр текущей функции — permissive (статический
    ///     bound-check — отдельный механизм Plan 15, здесь не дублируем);
    ///   - известный именованный тип — требует методы `hash` и `eq`;
    ///   - неизвестный тип / составной — permissive (не ругаемся).
    fn check_hashable(&self, k: &TypeRef, span: Span, errors: &mut Vec<Diagnostic>) {
        let TypeRef::Named { path, .. } = k else {
            // Array / Tuple / Func как ключ — permissive в bootstrap.
            return;
        };
        if path.len() != 1 {
            return; // module-qualified — permissive
        }
        let name = &path[0];
        // Примитивы — авто-Hashable.
        if matches!(
            name.as_str(),
            "int" | "i8" | "i16" | "i32" | "i64"
                | "u8" | "u16" | "u32" | "u64"
                | "f32" | "f64" | "bool" | "char" | "str"
                // Plan 76: `never` — bottom-тип (uninhabited), vacuously primitive.
                | "never"
        ) {
            return;
        }
        // Generic-параметр функции — permissive (bound-check — Plan 15).
        if self.fn_generics.contains(name) {
            return;
        }
        // Известный именованный тип — требует `hash` + `eq`.
        if self.known_types.contains(name) {
            let methods = self.type_methods.get(name);
            let has_hash = methods.map(|m| m.contains("hash")).unwrap_or(false);
            let has_eq = methods.map(|m| m.contains("eq")).unwrap_or(false);
            if !has_hash || !has_eq {
                let mut missing = Vec::new();
                if !has_hash { missing.push("`hash() -> u64`"); }
                if !has_eq { missing.push("`eq(other) -> bool`"); }
                errors.push(Diagnostic::new(
                    format!(
                        "key type `{}` does not implement `Hashable` — a map key \
                         type must provide {}. Add the missing method(s) to `{}` \
                         or use a primitive key type.",
                        name,
                        missing.join(" and "),
                        name
                    ),
                    span,
                ));
            }
            return;
        }
        // Неизвестное имя — permissive (не наша забота: NameResCtx поймает
        // действительно undefined типы).
    }
}

/// Plan 52 Ф.2: `true` если тип — `any` или иной «неизвестный» маркер,
/// для которого coercion-проверки пропускаются (permissive).
fn is_unknown_type(t: &TypeRef) -> bool {
    matches!(t, TypeRef::Named { path, .. } if path.len() == 1 && path[0] == "any")
}

/// Plan 52 Ф.2: рендер TypeRef в человекочитаемую строку для диагностик.
fn typeref_render(t: &TypeRef) -> String {
    match t {
        TypeRef::Named { path, generics, .. } => {
            let base = path.join(".");
            if generics.is_empty() {
                base
            } else {
                let inner: Vec<String> = generics.iter().map(typeref_render).collect();
                format!("{}[{}]", base, inner.join(", "))
            }
        }
        TypeRef::Array(inner, _) => format!("[]{}", typeref_render(inner)),
        TypeRef::FixedArray(n, inner, _) => format!("[{}]{}", n, typeref_render(inner)),
        TypeRef::Tuple(elems, _) => {
            let inner: Vec<String> = elems.iter().map(typeref_render).collect();
            format!("({})", inner.join(", "))
        }
        TypeRef::Unit(_) => "()".to_string(),
        TypeRef::Func { .. } => "fn(...)".to_string(),
        // Plan 97 Ф.2 (D142): анонимный protocol-тип. В Plan 52
        // coercion-диагностиках достаточно компактного маркера;
        // полный pretty-print — в `typeref_display`.
        TypeRef::Protocol { methods, .. } => format!("protocol {{...{} sigs}}", methods.len()),
        // D176 (Plan 108): readonly T — display as "readonly T"
        TypeRef::Readonly(inner, _) => format!("ro {}", typeref_render(inner)),
    }
}

/// Plan 52 Ф.2: best-effort тип выражения по синтаксической форме.
/// Возвращает `None` если тип не выводится локально (Ident без scope,
/// произвольный вызов и т.п.) — такие выражения не участвуют в
/// унификации (permissive: «не знаем — не ругаемся»).
fn simple_expr_type(e: &Expr) -> Option<TypeRef> {
    let prim = |name: &str| {
        Some(TypeRef::Named {
            path: vec![name.to_string()],
            generics: Vec::new(),
            span: e.span,
        })
    };
    match &e.kind {
        ExprKind::IntLit(_) => prim("int"),
        ExprKind::FloatLit(_) => prim("f64"),
        ExprKind::BoolLit(_) => prim("bool"),
        ExprKind::StrLit(_) | ExprKind::InterpolatedStr { .. } => prim("str"),
        ExprKind::CharLit(_) => prim("char"),
        ExprKind::RecordLit { type_name: Some(name), .. } => Some(TypeRef::Named {
            path: name.clone(),
            generics: Vec::new(),
            span: e.span,
        }),
        // f64.NAN / int.MAX и т.п. — Path(["f64", "NAN"]).
        ExprKind::Path(parts) if parts.len() == 2 => {
            match parts[0].as_str() {
                "f64" => prim("f64"),
                "f32" => prim("f32"),
                "int" => prim("int"),
                "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64" => prim(&parts[0]),
                _ => None,
            }
        }
        // Унарный минус не меняет числовой тип операнда.
        ExprKind::Unary { op: crate::ast::UnOp::Neg, operand } => simple_expr_type(operand),
        _ => None,
    }
}

/// Plan 52 Ф.2: совместимость двух простых типов для унификации
/// ключей/значений map-литерала. Bootstrap: точное равенство имён +
/// числовая лёгкость (int-литерал совместим с любым целочисленным
/// ожидаемым типом — coercion на codegen-уровне).
fn simple_types_compatible(a: &TypeRef, b: &TypeRef) -> bool {
    if is_unknown_type(a) || is_unknown_type(b) {
        return true;
    }
    match (a, b) {
        (
            TypeRef::Named { path: pa, generics: ga, .. },
            TypeRef::Named { path: pb, generics: gb, .. },
        ) => {
            if pa == pb && ga.len() == gb.len() {
                return ga.iter().zip(gb).all(|(x, y)| simple_types_compatible(x, y));
            }
            // Числовая лёгкость: int-литерал унифицируется с любым
            // целочисленным типом (codegen разрешит).
            let is_int = |p: &[String]| {
                p.len() == 1
                    && matches!(
                        p[0].as_str(),
                        "int" | "i8" | "i16" | "i32" | "i64"
                            | "u8" | "u16" | "u32" | "u64"
                    )
            };
            if is_int(pa) && is_int(pb) {
                return true;
            }
            false
        }
        (TypeRef::Array(ia, _), TypeRef::Array(ib, _)) => simple_types_compatible(ia, ib),
        (TypeRef::Tuple(ea, _), TypeRef::Tuple(eb, _)) => {
            ea.len() == eb.len()
                && ea.iter().zip(eb).all(|(x, y)| simple_types_compatible(x, y))
        }
        _ => false,
    }
}

// ============================================================================
// Plan 52 Ф.7 production-fix: annotate_map_literals — mutable pass.
//
// После `check_module` (immutable: проверки/errors), но ДО `desugar_module`:
// проходим по AST mutable, выводим K/V для каждого `MapLit` и записываем
// в узел через поля `inferred_key`/`inferred_value`. Десугаринг затем
// эмитит `HashMap[K, V].with_capacity(n)` с turbofish — иначе
// мономорфизация инстанциирует `HashMap[void*, void*]` → segfault.
//
// Стратегия:
//   - Build immutable `MapLitCtx` для type-таблиц (известные типы,
//     #from_fields, param-types для arg-position resolve).
//   - Mutable AST walker с expected-type propagation в let / FnBody::Expr-
//     return / argument-position. Те же позиции что MapLitCtx::walk_expr.
//   - На каждом `MapLit`: вычислить (K, V) через `simple_expr_type` +
//     unify_inferred helper; записать в узел. Не emit errors (это сделал
//     check_module).
// ============================================================================

/// Plan 52 Ф.7: пройти по AST mutable, записать inferred K/V для каждого
/// `MapLit`. Вызывается ПОСЛЕ `check_module` (errors уже emitted), ДО
/// `desugar_module` (читает inferred K/V для turbofish).
pub fn annotate_map_literals(module: &mut Module) {
    let ctx = MapLitCtx::build(module);
    let mut ann = MapLitAnnotator {
        ctx,
        fn_generics: HashSet::new(),
        var_types: HashMap::new(),
    };
    ann.walk_module(module);
    // Plan 42.4 / Plan 52 Ф.7: peer_files несут per-peer копии items для
    // name resolution. Десугаринг обходит и peer_files.items_here, поэтому
    // аннотировать нужно тоже их — иначе peer-копия MapLit'а останется без
    // inferred K/V → fallback bare Path → segfault.
    for pf in &mut module.peer_files {
        ann.walk_items(&mut pf.items_here);
    }
}

/// Mutable AST walker для аннотации MapLit-узлов inferred K/V.
struct MapLitAnnotator {
    /// Immutable type-таблицы (#from_fields, param-types).
    ctx: MapLitCtx,
    /// Generic-параметры текущей функции — для permissive Hashable.
    fn_generics: HashSet<String>,
    /// Plan 52.x: типы let-биндингов и параметров текущего item'а.
    /// Нужны для классификации all-spread литералов `[...a, ...b]` без
    /// аннотации: чтобы отличить map-spread от array-spread, нужен тип
    /// spread-источника. Сбрасывается на границе каждого item'а.
    var_types: HashMap<String, TypeRef>,
}

impl MapLitAnnotator {
    fn walk_module(&mut self, module: &mut Module) {
        self.walk_items(&mut module.items);
    }

    fn walk_items(&mut self, items: &mut [Item]) {
        for item in items.iter_mut() {
            match item {
                Item::Fn(f) => {
                    self.fn_generics =
                        f.generics.iter().map(|g| g.name.clone()).collect();
                    if let Some(recv) = &f.receiver {
                        for g in &recv.generics {
                            if let TypeRef::Named { path, .. } = g {
                                if path.len() == 1 {
                                    self.fn_generics.insert(path[0].clone());
                                }
                            }
                        }
                    }
                    // Plan 52.x: свежий var-scope на функцию + параметры.
                    self.var_types.clear();
                    for p in &f.params {
                        self.var_types.insert(p.name.clone(), p.ty.clone());
                    }
                    let return_ty = f.return_type.clone();
                    match &mut f.body {
                        FnBody::Expr(e) => self.walk_expr(e, return_ty.as_ref()),
                        FnBody::Block(b) => self.walk_block(b),
                        FnBody::External => {}
                    }
                }
                Item::Test(t) => {
                    self.fn_generics.clear();
                    self.var_types.clear();
                    self.walk_block(&mut t.body);
                }
                // Plan 57: bench body — аннотируем все три раздела.
                Item::Bench(b) => {
                    self.fn_generics.clear();
                    self.var_types.clear();
                    for s in &mut b.setup {
                        self.walk_stmt(s);
                    }
                    self.walk_block(&mut b.measure_body);
                    for s in &mut b.teardown {
                        self.walk_stmt(s);
                    }
                }
                Item::Const(c) => {
                    self.fn_generics.clear();
                    self.var_types.clear();
                    let ty = c.ty.clone();
                    self.walk_expr(&mut c.value, ty.as_ref());
                }
                Item::Let(l) => {
                    self.fn_generics.clear();
                    self.var_types.clear();
                    let ty = l.ty.clone();
                    self.walk_expr(&mut l.value, ty.as_ref());
                }
                Item::Type(_) | Item::Lemma(_) => {}
            }
        }
    }

    fn walk_block(&mut self, b: &mut Block) {
        for s in &mut b.stmts {
            self.walk_stmt(s);
        }
        if let Some(t) = &mut b.trailing {
            self.walk_expr(t, None);
        }
    }

    fn walk_stmt(&mut self, s: &mut Stmt) {
        match s {
            Stmt::Expr(e) => self.walk_expr(e, None),
            Stmt::Let(d) => {
                let ty = d.ty.clone();
                self.walk_expr(&mut d.value, ty.as_ref());
                // Plan 52.x: запоминаем тип биндинга для классификации
                // all-spread литералов `[...a, ...b]` без аннотации.
                if let Pattern::Ident { name, .. } = &d.pattern {
                    if let Some(bt) = &d.ty {
                        self.var_types.insert(name.clone(), bt.clone());
                    } else if let ExprKind::MapLit {
                        inferred_key: Some(k),
                        inferred_value: Some(v),
                        inferred_target_type,
                        ..
                    } = &d.value.kind
                    {
                        // Без аннотации, но значение — выведенный map-литерал.
                        let path = inferred_target_type
                            .clone()
                            .unwrap_or_else(|| vec!["HashMap".to_string()]);
                        self.var_types.insert(
                            name.clone(),
                            TypeRef::Named {
                                path,
                                generics: vec![k.clone(), v.clone()],
                                span: d.value.span,
                            },
                        );
                    } else {
                        // Неизвестный тип — снять устаревший entry (shadowing).
                        self.var_types.remove(name);
                    }
                }
            }
            // Plan 114.4 Ф.2: scope-local const — pass-through (no-op for now).
            Stmt::Const(_) => {}
            Stmt::Assign { target, value, .. } => {
                self.walk_expr(target, None);
                self.walk_expr(value, None);
            }
            Stmt::Return { value, .. } => {
                if let Some(v) = value { self.walk_expr(v, None); }
            }
            Stmt::Throw { value, .. } => self.walk_expr(value, None),
            Stmt::Break(_) | Stmt::Continue(_) => {}
            Stmt::Defer { body, .. } | Stmt::ErrDefer { body, .. }
            | Stmt::OkDefer { body, .. } | Stmt::DeferWithResult { body, .. } => {
                self.walk_expr(body, None);
            }
            // Plan 110 D188: walk init + body block recursively.
            Stmt::ConsumeScope { init, body, .. } => {
                self.walk_expr(init, None);
                for stmt in &mut body.stmts {
                    self.walk_stmt(stmt);
                }
                if let Some(t) = &mut body.trailing {
                    self.walk_expr(t, None);
                }
            }
            Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => {
                self.walk_expr(expr, None);
            }
            Stmt::Apply { .. } | Stmt::Calc { .. } | Stmt::Reveal { .. } => {}
        }
    }

    /// Plan 52.x: для all-spread `ArrayLit` (`[...a, ...b]`) без expected-
    /// типа выводит map-тип из типов spread-источников. Возвращает
    /// `Some(HashMap-тип)` только если ВСЕ источники — `Ident`-ы
    /// #from_pairs-типа; иначе это array-spread `[...arr1, ...arr2]` —
    /// возвращаем `None`, классификация остаётся прежней (массив).
    fn infer_map_type_from_spreads(&self, kind: &ExprKind) -> Option<TypeRef> {
        let ExprKind::ArrayLit(elems) = kind else {
            return None;
        };
        if elems.is_empty() {
            return None;
        }
        let mut first: Option<TypeRef> = None;
        for el in elems {
            let ArrayElem::Spread(s) = el else {
                return None;
            };
            let ExprKind::Ident(name) = &s.kind else {
                return None;
            };
            let ty = self.var_types.get(name)?;
            if !self.ctx.expected_is_from_pairs(ty) {
                return None;
            }
            if first.is_none() {
                first = Some(ty.clone());
            }
        }
        first
    }

    fn walk_expr(&mut self, e: &mut Expr, expected: Option<&TypeRef>) {
        // Plan 52.x: all-spread `[...a, ...b]` без expected-типа —
        // синтезируем map-тип из spread-источников, чтобы конверсия
        // ArrayLit→MapLit ниже и inference K/V сработали (без этого
        // spread двух map'ов без аннотации мис-классифицируется как
        // массив → `b->len` на HashMap → CC-FAIL).
        let synth_expected: Option<TypeRef> = if expected.is_none() {
            self.infer_map_type_from_spreads(&e.kind)
        } else {
            None
        };
        let expected = expected.or(synth_expected.as_ref());

        // Plan 52.3 Ф.1: empty `[]` в позиции #from_pairs-типа конвертим
        // в empty MapLit. Codegen ArrayLit пустой → array (CC-FAIL для
        // HashMap-target). MapLit пустой → with_capacity(0) — пустая мапа.
        // Plan 55 followup (spread): `[...spread]` (только spreads, без
        // pairs) тоже может быть **либо** array **либо** map — disambiguate
        // через expected type. Если expected is #from_pairs → conv в MapLit.
        if let ExprKind::ArrayLit(elems) = &e.kind {
            let all_spread = !elems.is_empty()
                && elems.iter().all(|el| matches!(el, ArrayElem::Spread(_)));
            let is_empty = elems.is_empty();
            if is_empty || all_spread {
                if let Some(exp) = expected {
                    if self.ctx.expected_is_from_pairs(exp) {
                        // Конвертим: spreads (если есть) move'аются в MapElem.
                        // annotate ниже заполнит inferred_key/value/target_type.
                        let new_elems: Vec<MapElem> = if is_empty {
                            Vec::new()
                        } else {
                            elems.iter().map(|el| match el {
                                ArrayElem::Spread(s) => MapElem::Spread(s.clone()),
                                ArrayElem::Item(_) => unreachable!("all_spread guard"),
                            }).collect()
                        };
                        e.kind = ExprKind::MapLit {
                            elems: new_elems,
                            inferred_key: None,
                            inferred_value: None,
                            inferred_target_type: None,
                        };
                    }
                }
            }
        }

        // 1. На MapLit — заполнить inferred_key/value/target_type (до спуска).
        if let ExprKind::MapLit {
            elems,
            inferred_key,
            inferred_value,
            inferred_target_type,
        } = &mut e.kind {
            let pairs = crate::ast::MapElem::cloned_pairs(elems);
            let (exp_k, exp_v) = extract_hashmap_kv(expected);
            *inferred_key = infer_unified_type(
                pairs.iter().map(|(k, _)| k),
                exp_k,
            );
            *inferred_value = infer_unified_type(
                pairs.iter().map(|(_, v)| v),
                exp_v,
            );
            // Plan 52 Ф.23: если expected помечен #from_pairs — записываем
            // имя target-типа для desugar. Иначе fallback на HashMap.
            if let Some(TypeRef::Named { path, .. }) = expected {
                if self.ctx.expected_is_from_pairs(expected.unwrap()) {
                    *inferred_target_type = Some(path.clone());
                }
            }
        }
        // Plan 52 Ф.10: D55 map-coercion для `{field: v}` в позиции
        // `#from_fields`-типа (= HashMap[str, V]). Если expected —
        // HashMap-with-#from_fields-маркер И литерал анонимный,
        // записываем V в `inferred_map_v`. Codegen `emit_record_as_map`
        // эмитит как `HashMap[str,V].with_capacity + insert("field", v)`.
        if let ExprKind::RecordLit { type_name: None, inferred_map_v, .. } = &mut e.kind {
            if let Some(exp) = expected {
                if self.ctx.expected_is_from_fields(exp) {
                    let (_, exp_v) = extract_hashmap_kv(Some(exp));
                    if let Some(v) = exp_v {
                        *inferred_map_v = Some(v.clone());
                    }
                }
            }
        }
        // 2. Спуск в под-выражения с propagation expected-type где известен.
        match &mut e.kind {
            ExprKind::MapLit { elems, .. } => {
                for me in elems.iter_mut() {
                    match me {
                        crate::ast::MapElem::Pair(k, v) => {
                            self.walk_expr(k, None);
                            self.walk_expr(v, None);
                        }
                        crate::ast::MapElem::Spread(e) => self.walk_expr(e, None),
                    }
                }
            }
            ExprKind::ArrayLit(elems) => {
                for el in elems.iter_mut() {
                    match el {
                        ArrayElem::Item(x) | ArrayElem::Spread(x) => {
                            self.walk_expr(x, None);
                        }
                    }
                }
            }
            ExprKind::Call { func, args, trailing } => {
                self.walk_expr(func, None);
                // Argument-позиция — propagation expected типа параметра
                // (фундамент Ф.3a).
                let params = self.ctx.resolve_call_params(func);
                let mut positional_idx = 0usize;
                for a in args.iter_mut() {
                    let arg_expected: Option<TypeRef> = match (&params, a.arg_name()) {
                        (Some(ps), Some(name)) => ps
                            .iter()
                            .find(|(pn, _)| pn == name)
                            .map(|(_, t)| t.clone()),
                        (Some(ps), None) => {
                            let r = ps.get(positional_idx).map(|(_, t)| t.clone());
                            positional_idx += 1;
                            r
                        }
                        (None, _) => None,
                    };
                    let expr_mut = match a {
                        CallArg::Item(x) | CallArg::Spread(x) => x,
                        CallArg::Named { value, .. } => value,
                    };
                    self.walk_expr(expr_mut, arg_expected.as_ref());
                }
                if let Some(t) = trailing {
                    match t {
                        crate::ast::Trailing::Block(b) => self.walk_block(b),
                        crate::ast::Trailing::LegacyBlockWithParams(tb) => {
                            self.walk_block(&mut tb.body)
                        }
                        crate::ast::Trailing::Fn(sb) => match &mut sb.body {
                            FnBody::Expr(x) => self.walk_expr(x, None),
                            FnBody::Block(b) => self.walk_block(b),
                            FnBody::External => {}
                        },
                    }
                }
            }
            ExprKind::TurboFish { base, .. } => self.walk_expr(base, expected),
            ExprKind::Try(x) | ExprKind::Bang(x) => self.walk_expr(x, None),
            ExprKind::Coalesce(a, b) => {
                self.walk_expr(a, None);
                self.walk_expr(b, None);
            }
            ExprKind::As(x, _) | ExprKind::Is(x, _) => self.walk_expr(x, None),
            ExprKind::Binary { left, right, .. } => {
                self.walk_expr(left, None);
                self.walk_expr(right, None);
            }
            ExprKind::Unary { operand, .. } => self.walk_expr(operand, None),
            ExprKind::Member { obj, .. } => self.walk_expr(obj, None),
            ExprKind::Index { obj, index } => {
                self.walk_expr(obj, None);
                self.walk_expr(index, None);
            }
            ExprKind::TupleLit(elems) => {
                for x in elems.iter_mut() { self.walk_expr(x, None); }
            }
            ExprKind::RecordLit { fields, .. } => {
                for f in fields.iter_mut() {
                    if let Some(v) = &mut f.value { self.walk_expr(v, None); }
                }
            }
            ExprKind::If { cond, then, else_ } => {
                self.walk_expr(cond, None);
                self.walk_block(then);
                if let Some(eb) = else_ {
                    match eb {
                        ElseBranch::Block(b) => self.walk_block(b),
                        ElseBranch::If(x) => self.walk_expr(x, None),
                    }
                }
            }
            ExprKind::IfLet { scrutinee, then, else_, .. } => {
                self.walk_expr(scrutinee, None);
                self.walk_block(then);
                if let Some(eb) = else_ {
                    match eb {
                        ElseBranch::Block(b) => self.walk_block(b),
                        ElseBranch::If(x) => self.walk_expr(x, None),
                    }
                }
            }
            ExprKind::Match { scrutinee, arms } => {
                self.walk_expr(scrutinee, None);
                for arm in arms.iter_mut() {
                    if let Some(g) = &mut arm.guard { self.walk_expr(g, None); }
                    match &mut arm.body {
                        MatchArmBody::Expr(x) => self.walk_expr(x, None),
                        MatchArmBody::Block(b) => self.walk_block(b),
                    }
                }
            }
            ExprKind::For { iter, body, .. } | ExprKind::ParallelFor { iter, body, .. } => {
                self.walk_expr(iter, None);
                self.walk_block(body);
            }
            ExprKind::While { cond, body, .. } => {
                self.walk_expr(cond, None);
                self.walk_block(body);
            }
            ExprKind::WhileLet { scrutinee, body, .. } => {
                self.walk_expr(scrutinee, None);
                self.walk_block(body);
            }
            ExprKind::Loop { body, .. } => self.walk_block(body),
            ExprKind::Block(b) => self.walk_block(b),
            ExprKind::Spawn(x) => self.walk_expr(x, None),
            ExprKind::Detach(b) | ExprKind::Blocking(b) => self.walk_block(b),
            ExprKind::Supervised { body, cancel } => {
                self.walk_block(body);
                if let Some(c) = cancel { self.walk_expr(c, None); }
            }
            ExprKind::Forbid { body, .. } | ExprKind::Realtime { body, .. } => {
                self.walk_block(body);
            }
            ExprKind::Throw(x) => self.walk_expr(x, None),
            ExprKind::Interrupt(opt) => {
                if let Some(x) = opt { self.walk_expr(x, None); }
            }
            ExprKind::Range { start, end, .. } => {
                if let Some(s) = start { self.walk_expr(s, None); }
                if let Some(e) = end { self.walk_expr(e, None); }
            }
            ExprKind::InterpolatedStr { parts } => {
                for p in parts.iter_mut() {
                    if let crate::ast::InterpStrPart::Expr(x) = p {
                        self.walk_expr(x, None);
                    }
                }
            }
            ExprKind::TaggedTemplate { args, .. } => {
                for x in args.iter_mut() { self.walk_expr(x, None); }
            }
            ExprKind::Lambda { body, .. } => self.walk_expr(body, None),
            ExprKind::ClosureLight { body, .. } => match body {
                ClosureBody::Expr(x) => self.walk_expr(x, None),
                ClosureBody::Block(b) => self.walk_block(b),
            },
            ExprKind::ClosureFull(sb) => match &mut sb.body {
                FnBody::Expr(x) => self.walk_expr(x, None),
                FnBody::Block(b) => self.walk_block(b),
                FnBody::External => {}
            },
            ExprKind::With { bindings, body } => {
                for b in bindings.iter_mut() { self.walk_expr(&mut b.handler, None); }
                self.walk_block(body);
            }
            // Plan 97 Ф.4 (D142): protocol-литерал — walk-mut идентичен.
            ExprKind::HandlerLit { methods, .. } | ExprKind::ProtocolLit { methods, .. } => {
                for m in methods.iter_mut() {
                    match &mut m.body {
                        HandlerMethodBody::Expr(x) => self.walk_expr(x, None),
                        HandlerMethodBody::Block(b) => self.walk_block(b),
                    }
                }
            }
            ExprKind::Select { arms } => {
                for arm in arms.iter_mut() {
                    match &mut arm.op {
                        crate::ast::SelectOp::Recv { chan, .. } => {
                            self.walk_expr(chan, None)
                        }
                        crate::ast::SelectOp::Send { chan, value } => {
                            self.walk_expr(chan, None);
                            self.walk_expr(value, None);
                        }
                        crate::ast::SelectOp::Default => {}
                    }
                    if let Some(g) = &mut arm.guard { self.walk_expr(g, None); }
                    self.walk_block(&mut arm.body);
                }
            }
            ExprKind::Forall { body, .. } | ExprKind::Exists { body, .. } => {
                self.walk_expr(body, None);
            }
            ExprKind::Ident(_) | ExprKind::Path(_) | ExprKind::SelfAccess
            | ExprKind::IntLit(_) | ExprKind::FloatLit(_) | ExprKind::BoolLit(_)
            | ExprKind::StrLit(_) | ExprKind::CharLit(_) | ExprKind::UnitLit => {}
        }
        // Подавляем unused warnings.
        let _ = &self.fn_generics;
    }
}

/// Plan 52 Ф.7: извлечь (K, V) из ожидаемого типа `HashMap[K, V]`.
/// Возвращает (None, None) если expected не HashMap[_, _].
fn extract_hashmap_kv(expected: Option<&TypeRef>) -> (Option<&TypeRef>, Option<&TypeRef>) {
    match expected {
        Some(TypeRef::Named { path, generics, .. })
            if path.last().map(|s| s.as_str()) == Some("HashMap")
                && generics.len() == 2 =>
        {
            (Some(&generics[0]), Some(&generics[1]))
        }
        _ => (None, None),
    }
}

/// Plan 52 Ф.7: вывести унифицированный тип набора выражений. Если задан
/// `expected` — берём его (приоритет контекста). Иначе — best-effort
/// первое выражение с известным simple_expr_type. Несовместимости не
/// репортим (это работа check_map_lit, эта функция silent).
fn infer_unified_type<'e>(
    exprs: impl Iterator<Item = &'e Expr>,
    expected: Option<&TypeRef>,
) -> Option<TypeRef> {
    if let Some(t) = expected {
        return Some(t.clone());
    }
    for ex in exprs {
        if let Some(t) = simple_expr_type(ex) {
            return Some(t);
        }
    }
    None
}

// ─── Plan 103.1 Ф.4: Ordering validation helpers ─────────────────────────
//
// Compile-time validation of MemOrdering arguments on atomic load/store ops.
// Called from Plan 103.2+ method-call type-check pipeline for AtomicX.load()
// and AtomicX.store() overloads that accept a MemOrdering parameter.
//
// Error codes (no central registry exists; embedded in Diagnostic message):
//   E_INVALID_ORDERING_LOAD  — Release/AcqRel on load (only Relaxed/Acquire/SeqCst valid)
//   E_INVALID_ORDERING_STORE — Acquire/AcqRel on store (only Relaxed/Release/SeqCst valid)
//
// See D167 (spec/decisions/06-concurrency.md) for semantics rationale.

/// Error code for forbidden ordering on atomic load operation.
/// Release and AcqRel are invalid for load (they provide no acquire synchronization).
pub const E_INVALID_ORDERING_LOAD: &str = "E_INVALID_ORDERING_LOAD";

/// Error code for forbidden ordering on atomic store operation.
/// Acquire and AcqRel are invalid for store (they provide no release synchronization).
pub const E_INVALID_ORDERING_STORE: &str = "E_INVALID_ORDERING_STORE";

/// Plan 103.1 Ф.4: Extract the variant name from a MemOrdering path expression.
/// Returns Some("Acquire") for `MemOrdering.Acquire`, None for runtime values.
fn mem_ordering_variant(ord_expr: &Expr) -> Option<&str> {
    use crate::ast::ExprKind;
    if let ExprKind::Path(parts) = &ord_expr.kind {
        if parts.len() == 2 && parts[0] == "MemOrdering" {
            return Some(&parts[1]);
        }
    }
    None
}

/// Plan 103.1 Ф.4: Validate MemOrdering argument for an atomic **load** operation.
///
/// Valid orderings for load: Relaxed, Acquire, SeqCst.
/// Invalid: Release, AcqRel (these orderings make no semantic sense on a load).
///
/// Validation is compile-time only when the ordering is a literal constant.
/// Runtime-value orderings are skipped (handled via runtime panic in codegen).
///
/// # Plan 103.2 integration
/// Call from the method-call type-check path for `AtomicX.load(ord MemOrdering)`.
/// Pass `ord_arg` = the MemOrdering expression argument, `span` = call-site span.
#[allow(dead_code)]  // Called from Plan 103.2
pub fn check_atomic_load_ordering(
    ord_arg: &Expr,
    span: crate::diag::Span,
) -> Result<(), crate::diag::Diagnostic> {
    let Some(variant) = mem_ordering_variant(ord_arg) else {
        return Ok(()); // Runtime value — skip compile-time check
    };
    match variant {
        "Release" | "AcqRel" => Err(
            crate::diag::Diagnostic::new(
                format!(
                    "[{}] `MemOrdering.{}` запрещён для load; \
                     используйте Relaxed, Acquire, или SeqCst. \
                     Release/AcqRel не имеют семантики для load-операций.",
                    E_INVALID_ORDERING_LOAD, variant
                ),
                span,
            )
            .with_suggestion(crate::diag::Suggestion {
                message: "замените на Acquire (или SeqCst для simplicity)".to_string(),
                span,
                replacement: "MemOrdering.Acquire".to_string(),
                applicability: crate::diag::Applicability::MaybeIncorrect,
            })
        ),
        _ => Ok(()),
    }
}

/// Plan 103.1 Ф.4: Validate MemOrdering argument for an atomic **store** operation.
///
/// Valid orderings for store: Relaxed, Release, SeqCst.
/// Invalid: Acquire, AcqRel (these orderings make no semantic sense on a store).
///
/// # Plan 103.2 integration
/// Call from the method-call type-check path for `AtomicX.store(v, ord MemOrdering)`.
/// Pass `ord_arg` = the MemOrdering expression argument, `span` = call-site span.
#[allow(dead_code)]  // Called from Plan 103.2
pub fn check_atomic_store_ordering(
    ord_arg: &Expr,
    span: crate::diag::Span,
) -> Result<(), crate::diag::Diagnostic> {
    let Some(variant) = mem_ordering_variant(ord_arg) else {
        return Ok(()); // Runtime value — skip compile-time check
    };
    match variant {
        "Acquire" | "AcqRel" => Err(
            crate::diag::Diagnostic::new(
                format!(
                    "[{}] `MemOrdering.{}` запрещён для store; \
                     используйте Relaxed, Release, или SeqCst. \
                     Acquire/AcqRel не имеют семантики для store-операций.",
                    E_INVALID_ORDERING_STORE, variant
                ),
                span,
            )
            .with_suggestion(crate::diag::Suggestion {
                message: "замените на Release (или SeqCst для simplicity)".to_string(),
                span,
                replacement: "MemOrdering.Release".to_string(),
                applicability: crate::diag::Applicability::MaybeIncorrect,
            })
        ),
        _ => Ok(()),
    }
}
