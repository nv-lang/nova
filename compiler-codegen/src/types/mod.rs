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
                    errors.push(Diagnostic::new(
                        format!(
                            "`external fn` is only allowed in `std.runtime.*` modules \
                             (this module is `{}`); for FFI to external C libraries \
                             a future `extern(\"C\")` keyword will be added (Q-ffi)",
                            module.name.join(".")
                        ),
                        fd.span,
                    ));
                }
            }
            // Plan 62.D.bis (D126): `external type X` — same whitelist as
            // `external fn` per D82. Only `std.runtime.*` / `std.prelude.*`
            // modules can declare opaque types (runtime backing —
            // compiler-versioned artefact, not user-extensible).
            if let Item::Type(td) = item {
                if matches!(td.kind, TypeDeclKind::Opaque) {
                    errors.push(Diagnostic::new(
                        format!(
                            "`external type` is only allowed in `std.runtime.*` / `std.prelude.*` modules \
                             (this module is `{}`); for FFI to external C libraries \
                             a future `extern(\"C\") type` keyword will be added (Q-ffi). \
                             See D126 (spec/decisions/03-syntax.md).",
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
        // Bounds и defaults generic-параметров.
        for g in &fd.generics {
            if let Some(b) = &g.bound {
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
            if let Some(b) = &g.bound {
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
            TypeDeclKind::Effect(methods) | TypeDeclKind::Protocol(methods) => {
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
            Stmt::Defer { body, .. } | Stmt::ErrDefer { body, .. } => {
                self.walk_expr(body, gs, errors);
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
            ExprKind::RecordLit { fields, .. } => {
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
                self.walk_expr(start, gs, errors);
                self.walk_expr(end, gs, errors);
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
            ExprKind::HandlerLit { methods, .. } => {
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
            Stmt::Assign { target, value, .. } => {
                self.f1_expr(target, gs, scope, errors);
                self.f1_expr(value, gs, scope, errors);
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
            Stmt::Defer { body, .. } | Stmt::ErrDefer { body, .. } => {
                self.f1_expr(body, gs, scope, errors);
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
                self.f1_expr(start, gs, scope, errors);
                self.f1_expr(end, gs, scope, errors);
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
            ExprKind::HandlerLit { methods, .. } => {
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
            _ => match self.infer_expr_type(iter, scope)? {
                TypeRef::Array(inner, _)
                | TypeRef::FixedArray(_, inner, _) => Some(*inner),
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
                match self
                    .method_table
                    .get(&parts[0])
                    .and_then(|m| m.get(&parts[1]))
                    .map(|v| v.as_slice())
                {
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
            _ => None,
        }
    }

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
                            TypeDeclKind::Protocol(_)
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
    protocol_specs: HashMap<String, &'a [EffectMethod]>,
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
        let mut protocol_specs = HashMap::new();
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
                        TypeDeclKind::Protocol(methods) => {
                            protocol_specs.insert(t.name.clone(), methods.as_slice());
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
            Stmt::Defer { body, .. } | Stmt::ErrDefer { body, .. } => {
                self.walk_expr(body, scope, errors);
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
                self.walk_expr(start, scope, errors);
                self.walk_expr(end, scope, errors);
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
            // Р›РёС‚РµСЂР°Р»С‹ / ident'С‹ / handler-Р»РёС‚РµСЂР°Р»С‹ вЂ” Р±РµР· СЂРµРєСѓСЂСЃРёРё РІ bound-РїСЂРѕРІРµСЂРєРµ.
            ExprKind::IntLit(_) | ExprKind::FloatLit(_) | ExprKind::BoolLit(_)
            | ExprKind::StrLit(_) | ExprKind::CharLit(_) | ExprKind::UnitLit
            | ExprKind::Ident(_) | ExprKind::Path(_) | ExprKind::SelfAccess
            | ExprKind::HandlerLit { .. } => {}
        }
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
        let fn_name = match &base.kind {
            ExprKind::Ident(n) => n.clone(),
            _ => return, // РјРµС‚РѕРґС‹ Рё С‚.Рї. вЂ” РѕС‚РґРµР»СЊРЅР°СЏ Р·Р°РґР°С‡Р°
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
        let has_bounds = callee.generics.iter().any(|g| g.bound.is_some());
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
        for gp in &callee.generics {
            let Some(bound) = &gp.bound else { continue; };
            let Some(concrete) = bindings.get(&gp.name) else {
                // Inference РЅРµ СѓРґР°Р»Р°СЃСЊ вЂ” РїСЂРѕРїСѓСЃРєР°РµРј (best-effort).
                // Strict-mode РјРѕРі Р±С‹ С‚СЂРµР±РѕРІР°С‚СЊ explicit turbofish.
                continue;
            };
            self.check_satisfaction(
                concrete, bound, &gp.name, &fn_name, e.span, errors,
            );
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
                let resolved = self.resolve_instance_method(obj, method_name, scope);
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
        let mut found: Option<&FnDecl> = None;
        let mut ambiguous = false;
        for methods in self.method_table.values() {
            if let Some(overloads) = methods.get(method_name) {
                for f in overloads {
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
            *spec_methods,
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
            Stmt::Defer { body, .. } | Stmt::ErrDefer { body, .. } => {
                self.walk_expr(body, state, errors);
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
                self.walk_expr(start, state, errors);
                self.walk_expr(end, state, errors);
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
            | ExprKind::HandlerLit { .. } => {}
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
            Stmt::Assign { target, value, .. } => {
                self.walk_expr(target, file_id, scope, errors);
                self.walk_expr(value, file_id, scope, errors);
            }
            Stmt::Return { value, .. } => {
                if let Some(v) = value { self.walk_expr(v, file_id, scope, errors); }
            }
            Stmt::Throw { value, .. } => self.walk_expr(value, file_id, scope, errors),
            // D90 (Plan 20): defer/errdefer body вЂ” РѕР±С‹С‡РЅС‹Р№ expr РІ С‚РµРєСѓС‰РµРј
            // scope. Bindings РІРЅСѓС‚СЂРё body Р»РѕРєР°Р»СЊРЅС‹ РёС… СЃРѕР±СЃС‚РІРµРЅРЅС‹Рј under-scope'Р°Рј;
            // РЅР° РІРµСЂС…РЅРµРј СѓСЂРѕРІРЅРµ defer РЅРµ РІРІРѕРґРёС‚ РЅРѕРІС‹С… РёРјС‘РЅ.
            Stmt::Defer { body, .. } | Stmt::ErrDefer { body, .. } => {
                self.walk_expr(body, file_id, scope, errors);
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
            ExprKind::HandlerLit { methods, .. } => {
                // РљР°Р¶РґС‹Р№ method вЂ” handler-op СЃ СЃРѕР±СЃС‚РІРµРЅРЅС‹Рј scope params.
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
                self.walk_expr(start, file_id, scope, errors);
                self.walk_expr(end, file_id, scope, errors);
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
        Stmt::Defer { .. } | Stmt::ErrDefer { .. } => false,
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
            has_throw_in_expr(start) || has_throw_in_expr(end),
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
            TypeDeclKind::Effect(m) | TypeDeclKind::Protocol(m) => m,
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
            Stmt::Expr(e) => walk_expr_for_handler_lits(e, never_ops, errors),
            Stmt::Assign { target, value, .. } => {
                walk_expr_for_handler_lits(target, never_ops, errors);
                walk_expr_for_handler_lits(value, never_ops, errors);
            }
            Stmt::Return { value, .. } => {
                if let Some(v) = value { walk_expr_for_handler_lits(v, never_ops, errors); }
            }
            Stmt::Throw { value, .. } => walk_expr_for_handler_lits(value, never_ops, errors),
            Stmt::Defer { body, .. } | Stmt::ErrDefer { body, .. } => {
                walk_expr_for_handler_lits(body, never_ops, errors);
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
            walk_expr_for_handler_lits(start, never_ops, errors);
            walk_expr_for_handler_lits(end, never_ops, errors);
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
}

impl ConsumeRegistry {
    fn build(module: &Module) -> Self {
        let mut methods: HashSet<(String, String)> = HashSet::new();
        let mut fn_params: HashMap<String, Vec<usize>> = HashMap::new();
        let mut method_params: HashMap<(String, String), Vec<usize>> = HashMap::new();
        let mut fn_return_types: HashMap<String, String> = HashMap::new();
        let mut recv_returning: HashSet<(String, String)> = HashSet::new();

        // 1. Runtime-stdlib consume-методы (`StringBuilder.into` и т.п.).
        //    Single source of truth — runtime_registry.rs (`is_consume`).
        for f in crate::codegen::runtime_registry::all() {
            if let Some(recv) = f.receiver {
                if f.is_consume {
                    methods.insert((recv.to_string(), f.name.to_string()));
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
                match &fd.receiver {
                    Some(r) => {
                        if r.consume {
                            methods.insert((r.type_name.clone(), fd.name.clone()));
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
                        // Plan 73 followup: return-тип свободной функции.
                        if let Some(TypeRef::Named { path, .. }) = &fd.return_type {
                            if path.len() == 1 {
                                fn_return_types
                                    .insert(fd.name.clone(), path[0].clone());
                            }
                        }
                    }
                }
            }
        }
        ConsumeRegistry {
            methods, fn_params, method_params, fn_return_types, recv_returning,
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

/// Flow-context consume-анализа одной функции / теста.
struct ConsumeCtx<'a> {
    reg: &'a ConsumeRegistry,
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
}

impl<'a> ConsumeCtx<'a> {
    fn new(reg: &'a ConsumeRegistry) -> Self {
        ConsumeCtx {
            reg,
            states: HashMap::new(),
            var_types: HashMap::new(),
            aliases: HashMap::new(),
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
}

/// Plan 73 (D131): consume-check входная точка — walk всех function /
/// method / test bodies модуля.
/// Plan 77 (D132): `-> @` fluent-return — тело non-external метода
/// обязано завершаться выражением `@` (вернуть сам receiver). Делает
/// гарантию «возвращает receiver» проверяемой → consume-checker может
/// soundly трактовать `let x = recv.method()` как alias receiver'а.
fn check_fluent_return(module: &Module, errors: &mut Vec<Diagnostic>) {
    fn tail_is_self(body: &FnBody) -> bool {
        match body {
            // External — C-реализация (StringBuilder/WriteBuffer); C-функция
            // возвращает receiver-pointer по контракту runtime'а.
            FnBody::External => true,
            FnBody::Expr(e) => matches!(e.kind, ExprKind::SelfAccess),
            FnBody::Block(b) => b.trailing.as_ref()
                .map(|t| matches!(t.kind, ExprKind::SelfAccess))
                .unwrap_or(false),
        }
    }
    for item in &module.items {
        if let Item::Fn(f) = item {
            if !f.returns_receiver { continue; }
            if !tail_is_self(&f.body) {
                let span = match &f.body {
                    FnBody::Block(b) => b.span,
                    FnBody::Expr(e) => e.span,
                    FnBody::External => f.span,
                };
                errors.push(Diagnostic::new(
                    format!(
                        "метод `{}` объявлен `-> @` (fluent-return, D132): его \
                         тело обязано завершаться выражением `@` — вернуть сам \
                         receiver. Добавьте `@` последним выражением тела.",
                        f.name),
                    span,
                ));
            }
        }
    }
}

fn check_consume(module: &Module, errors: &mut Vec<Diagnostic>) {
    let reg = ConsumeRegistry::build(module);
    for item in &module.items {
        match item {
            Item::Fn(f) => {
                let mut ctx = ConsumeCtx::new(&reg);
                // Параметры функции — live на входе; consume-параметры
                // внутри тела тоже просто live (consume у caller'а).
                for p in &f.params {
                    let pty = match &p.ty {
                        TypeRef::Named { path, .. } if path.len() == 1 =>
                            Some(path[0].clone()),
                        _ => None,
                    };
                    ctx.declare(&p.name, pty);
                }
                match &f.body {
                    FnBody::Block(b) => consume_walk_block(&mut ctx, b, errors),
                    FnBody::Expr(e) => consume_walk_expr(&mut ctx, e, errors),
                    FnBody::External => {}
                }
            }
            Item::Test(t) => {
                let mut ctx = ConsumeCtx::new(&reg);
                consume_walk_block(&mut ctx, &t.body, errors);
            }
            _ => {}
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

fn consume_walk_block(ctx: &mut ConsumeCtx, b: &Block, errors: &mut Vec<Diagnostic>) {
    for s in &b.stmts {
        consume_walk_stmt(ctx, s, errors);
    }
    if let Some(t) = &b.trailing {
        consume_walk_expr(ctx, t, errors);
    }
}

fn consume_walk_stmt(ctx: &mut ConsumeCtx, s: &Stmt, errors: &mut Vec<Diagnostic>) {
    match s {
        Stmt::Let(decl) => {
            consume_walk_expr(ctx, &decl.value, errors);
            let mut names = Vec::new();
            consume_pattern_names(&decl.pattern, &mut names);
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
            if let Some(canon) = alias_src {
                let ty = ctx.var_types.get(&canon).cloned();
                ctx.declare_alias(&names[0], &canon, ty);
            } else {
                let ty = ctx.infer_let_type(decl);
                for n in &names {
                    // Тип привязываем только к одиночному ident-pattern'у.
                    let t = if names.len() == 1 { ty.clone() } else { None };
                    ctx.declare(n, t);
                }
            }
        }
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
                _ => consume_walk_expr(ctx, target, errors),
            }
        }
        Stmt::Return { value, .. } => {
            if let Some(v) = value { consume_walk_expr(ctx, v, errors); }
        }
        Stmt::Throw { value, .. } => consume_walk_expr(ctx, value, errors),
        Stmt::Break(_) | Stmt::Continue(_) | Stmt::Reveal { .. } => {}
        // defer/errdefer исполняются на scope-exit — walk изолированно
        // (use-after-consume ловится, consume наружу не протекает).
        Stmt::Defer { body, .. } | Stmt::ErrDefer { body, .. } => {
            consume_walk_isolated_expr(ctx, &[], body, errors);
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
                        // consume-метод → receiver (весь alias-класс)
                        // потребляется.
                        if ctx.is_consume_method(&recv, method) {
                            ctx.mark_consumed(&recv, e.span);
                        }
                        // consume-параметры метода.
                        if let Some(ty) = ctx.var_types.get(&ctx.canonical(&recv)).cloned() {
                            if let Some(idxs) = ctx.reg
                                .method_params.get(&(ty, method.clone())).cloned()
                            {
                                ctx.consume_args(args, &idxs, e.span);
                            }
                        }
                    } else {
                        consume_walk_expr(ctx, obj, errors);
                        for a in args { consume_walk_expr(ctx, a.expr(), errors); }
                        if let Some(t) = trailing { consume_walk_trailing(ctx, t, errors); }
                    }
                }
                // Free-fn call: f(args).
                ExprKind::Ident(fname) => {
                    for a in args { consume_walk_expr(ctx, a.expr(), errors); }
                    if let Some(t) = trailing { consume_walk_trailing(ctx, t, errors); }
                    if let Some(idxs) = ctx.reg.fn_params.get(fname).cloned() {
                        ctx.consume_args(args, &idxs, e.span);
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
            consume_walk_expr(ctx, start, errors);
            consume_walk_expr(ctx, end, errors);
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
        ExprKind::For { pattern, iter, body, .. }
        | ExprKind::ParallelFor { pattern, iter, body, .. } => {
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
        ExprKind::HandlerLit { methods, .. } => {
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
fn consume_walk_if(ctx: &mut ConsumeCtx, then: &Block,
                   else_: &Option<ElseBranch>, errors: &mut Vec<Diagnostic>) {
    let saved = ctx.states.clone();
    consume_walk_block(ctx, then, errors);
    let then_states = ctx.states.clone();
    ctx.states = saved.clone();
    consume_walk_else(ctx, else_, errors);
    let else_states = ctx.states.clone();
    ctx.states = consume_join(&saved, &then_states, &else_states);
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

    // Walk bodies С„СѓРЅРєС†РёР№ Рё С‚РµСЃС‚РѕРІ.
    for item in &module.items {
        match item {
            Item::Fn(f) => {
                if let FnBody::Block(b) = &f.body {
                    walk_block_for_defers(b, &fn_effects, errors);
                } else if let FnBody::Expr(e) = &f.body {
                    walk_expr_for_defers(e, &fn_effects, errors);
                }
            }
            Item::Test(t) => {
                walk_block_for_defers(&t.body, &fn_effects, errors);
            }
            _ => {}
        }
    }
}

/// Walk block: РґР»СЏ РєР°Р¶РґРѕРіРѕ Stmt::Defer/ErrDefer вЂ” РїСЂРѕРІРµСЂРёС‚СЊ body;
/// СЂРµРєСѓСЂСЃРёРІРЅРѕ walk РѕСЃС‚Р°Р»СЊРЅС‹Рµ stmts (С‚Р°Рј РјРѕР¶РµС‚ Р±С‹С‚СЊ РІР»РѕР¶РµРЅРЅС‹Р№ block СЃ
/// defer'Р°РјРё).
fn walk_block_for_defers(b: &Block, fn_effects: &HashMap<String, Vec<TypeRef>>, errors: &mut Vec<Diagnostic>) {
    for s in &b.stmts {
        match s {
            Stmt::Defer { body, .. } => {
                check_defer_body(body, /*is_errdefer*/ false, fn_effects, errors);
            }
            Stmt::ErrDefer { body, .. } => {
                check_defer_body(body, /*is_errdefer*/ true, fn_effects, errors);
            }
            Stmt::Let(decl) => walk_expr_for_defers(&decl.value, fn_effects, errors),
            Stmt::Expr(e) => walk_expr_for_defers(e, fn_effects, errors),
            Stmt::Assign { target, value, .. } => {
                walk_expr_for_defers(target, fn_effects, errors);
                walk_expr_for_defers(value, fn_effects, errors);
            }
            Stmt::Return { value, .. } => {
                if let Some(v) = value { walk_expr_for_defers(v, fn_effects, errors); }
            }
            Stmt::Throw { value, .. } => walk_expr_for_defers(value, fn_effects, errors),
            Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => walk_expr_for_defers(expr, fn_effects, errors),
            Stmt::Break(_) | Stmt::Continue(_) => {}
            Stmt::Apply { args, .. } => {
                for a in args { walk_expr_for_defers(a, fn_effects, errors); }
            }
            Stmt::Calc { steps, .. } => {
                for step in steps { walk_expr_for_defers(&step.expr, fn_effects, errors); }
            }
            Stmt::Reveal { .. } => {}
        }
    }
    if let Some(t) = &b.trailing {
        walk_expr_for_defers(t, fn_effects, errors);
    }
}

/// Walk expression: СЂРµРєСѓСЂСЃРёРІРЅРѕ РёС‰РµРј РІР»РѕР¶РµРЅРЅС‹Рµ Р±Р»РѕРєРё СЃ defer'Р°РјРё.
/// РЎР°Рј РїРѕ СЃРµР±Рµ expression РЅРµ РїСЂРѕРІРµСЂСЏРµС‚СЃСЏ вЂ” С‚РѕР»СЊРєРѕ nested blocks.
fn walk_expr_for_defers(e: &Expr, fn_effects: &HashMap<String, Vec<TypeRef>>, errors: &mut Vec<Diagnostic>) {
    match &e.kind {
        ExprKind::Block(b) => walk_block_for_defers(b, fn_effects, errors),
        ExprKind::If { cond, then, else_ } => {
            walk_expr_for_defers(cond, fn_effects, errors);
            walk_block_for_defers(then, fn_effects, errors);
            if let Some(ElseBranch::Block(b)) = else_ { walk_block_for_defers(b, fn_effects, errors); }
            if let Some(ElseBranch::If(e2)) = else_ { walk_expr_for_defers(e2, fn_effects, errors); }
        }
        ExprKind::IfLet { scrutinee, then, else_, .. } => {
            walk_expr_for_defers(scrutinee, fn_effects, errors);
            walk_block_for_defers(then, fn_effects, errors);
            if let Some(ElseBranch::Block(b)) = else_ { walk_block_for_defers(b, fn_effects, errors); }
            if let Some(ElseBranch::If(e2)) = else_ { walk_expr_for_defers(e2, fn_effects, errors); }
        }
        ExprKind::Match { scrutinee, arms } => {
            walk_expr_for_defers(scrutinee, fn_effects, errors);
            for a in arms {
                match &a.body {
                    MatchArmBody::Expr(e2) => walk_expr_for_defers(e2, fn_effects, errors),
                    MatchArmBody::Block(b) => walk_block_for_defers(b, fn_effects, errors),
                }
                if let Some(g) = &a.guard { walk_expr_for_defers(g, fn_effects, errors); }
            }
        }
        ExprKind::For { iter, body, .. } | ExprKind::ParallelFor { iter, body, .. } => {
            walk_expr_for_defers(iter, fn_effects, errors);
            walk_block_for_defers(body, fn_effects, errors);
        }
        ExprKind::While { cond, body, .. } => {
            walk_expr_for_defers(cond, fn_effects, errors);
            walk_block_for_defers(body, fn_effects, errors);
        }
        ExprKind::WhileLet { scrutinee, body, .. } => {
            walk_expr_for_defers(scrutinee, fn_effects, errors);
            walk_block_for_defers(body, fn_effects, errors);
        }
        ExprKind::Loop { body, .. } => walk_block_for_defers(body, fn_effects, errors),
        ExprKind::Select { arms } => {
            for arm in arms {
                match &arm.op {
                    SelectOp::Recv { chan, .. } => walk_expr_for_defers(chan, fn_effects, errors),
                    SelectOp::Send { chan, value } => {
                        walk_expr_for_defers(chan, fn_effects, errors);
                        walk_expr_for_defers(value, fn_effects, errors);
                    }
                    SelectOp::Default => {}
                }
                if let Some(g) = &arm.guard { walk_expr_for_defers(g, fn_effects, errors); }
                walk_block_for_defers(&arm.body, fn_effects, errors);
            }
        }
        ExprKind::With { body, .. } | ExprKind::Forbid { body, .. }
        | ExprKind::Realtime { body, .. }
        | ExprKind::Detach(body) | ExprKind::Blocking(body) => {
            walk_block_for_defers(body, fn_effects, errors);
        }
        ExprKind::Supervised { body, cancel } => {
            if let Some(c) = cancel { walk_expr_for_defers(c, fn_effects, errors); }
            walk_block_for_defers(body, fn_effects, errors);
        }
        ExprKind::Call { func, args, trailing } => {
            walk_expr_for_defers(func, fn_effects, errors);
            for a in args {
                walk_expr_for_defers(a.expr(), fn_effects, errors);
            }
            if let Some(tr) = trailing {
                match tr {
                    Trailing::Block(b) => walk_block_for_defers(b, fn_effects, errors),
                    Trailing::Fn(fsb) => {
                        if let FnBody::Block(b) = &fsb.body { walk_block_for_defers(b, fn_effects, errors); }
                        else if let FnBody::Expr(e2) = &fsb.body { walk_expr_for_defers(e2, fn_effects, errors); }
                    }
                    Trailing::LegacyBlockWithParams(tb) => {
                        walk_block_for_defers(&tb.body, fn_effects, errors);
                    }
                }
            }
        }
        ExprKind::Spawn(body) => walk_expr_for_defers(body, fn_effects, errors),
        ExprKind::Binary { left, right, .. } => {
            walk_expr_for_defers(left, fn_effects, errors);
            walk_expr_for_defers(right, fn_effects, errors);
        }
        ExprKind::Unary { operand, .. } => walk_expr_for_defers(operand, fn_effects, errors),
        ExprKind::Try(e2) | ExprKind::Bang(e2) | ExprKind::Throw(e2) => {
            walk_expr_for_defers(e2, fn_effects, errors);
        }
        ExprKind::Coalesce(a, b) => {
            walk_expr_for_defers(a, fn_effects, errors);
            walk_expr_for_defers(b, fn_effects, errors);
        }
        ExprKind::As(e2, _) | ExprKind::Is(e2, _) => walk_expr_for_defers(e2, fn_effects, errors),
        ExprKind::Member { obj, .. } | ExprKind::Index { obj, .. } => walk_expr_for_defers(obj, fn_effects, errors),
        ExprKind::TurboFish { base, .. } => walk_expr_for_defers(base, fn_effects, errors),
        ExprKind::Lambda { body, .. } | ExprKind::Interrupt(Some(body)) => walk_expr_for_defers(body, fn_effects, errors),
        ExprKind::Range { start, end, .. } => {
            walk_expr_for_defers(start, fn_effects, errors);
            walk_expr_for_defers(end, fn_effects, errors);
        }
        ExprKind::ArrayLit(elems) => {
            for el in elems {
                match el {
                    ArrayElem::Item(e2) | ArrayElem::Spread(e2) => walk_expr_for_defers(e2, fn_effects, errors),
                }
            }
        }
        ExprKind::TupleLit(elems) => {
            for el in elems { walk_expr_for_defers(el, fn_effects, errors); }
        }
        ExprKind::RecordLit { fields, .. } => {
            for f in fields {
                if let Some(v) = &f.value { walk_expr_for_defers(v, fn_effects, errors); }
            }
        }
        // Р›СЏРјР±РґС‹ closure-full: body РІРЅСѓС‚СЂРё FnSigBody.
        ExprKind::ClosureFull(fsb) => {
            if let FnBody::Block(b) = &fsb.body { walk_block_for_defers(b, fn_effects, errors); }
            else if let FnBody::Expr(e2) = &fsb.body { walk_expr_for_defers(e2, fn_effects, errors); }
        }
        ExprKind::ClosureLight { body, .. } => {
            match body {
                ClosureBody::Expr(e2) => walk_expr_for_defers(e2, fn_effects, errors),
                ClosureBody::Block(b) => walk_block_for_defers(b, fn_effects, errors),
            }
        }
        // РџСЂРѕСЃС‚С‹Рµ СѓР·Р»С‹ Р±РµР· РІР»РѕР¶РµРЅРЅС‹С… Р±Р»РѕРєРѕРІ.
        _ => {}
    }
}

/// Body constraint check: exit-control, Fail-effect, suspend.
fn check_defer_body(body: &Expr, is_errdefer: bool, fn_effects: &HashMap<String, Vec<TypeRef>>, errors: &mut Vec<Diagnostic>) {
    let kw = if is_errdefer { "errdefer" } else { "defer" };
    // D90 Plan 20 Р¤.3 (revised): Р’Р°СЂРёР°РЅС‚ 3 вЂ” return/break/continue СЂР°Р·СЂРµС€РµРЅС‹
    // С‚РѕР»СЊРєРѕ РІРЅСѓС‚СЂРё nested loop/fn-literal РІ defer body (local control). РќР°
    // top-level defer body РѕРЅРё Р·Р°РїСЂРµС‰РµРЅС‹ вЂ” РЅРµР»СЊР·СЏ hijack scope-exit
    // РѕРєСЂСѓР¶Р°СЋС‰РµР№ С„СѓРЅРєС†РёРё/С†РёРєР»Р°.
    //
    // Ctx tracks: loop-nesting depth (break/continue ok РµСЃР»Рё >0), fn-literal
    // depth (return ok РµСЃР»Рё >0).
    let ctx = DeferBodyCtx { loop_depth: 0, fn_depth: 0 };
    check_defer_body_inner(body, kw, fn_effects, &ctx, errors);
}

#[derive(Clone, Copy)]
struct DeferBodyCtx {
    /// РўРµРєСѓС‰Р°СЏ РіР»СѓР±РёРЅР° loop'РѕРІ (for/while/loop) РІРЅСѓС‚СЂРё defer body. Р•СЃР»Рё >0,
    /// `break`/`continue` Р»РѕРєР°Р»СЊРЅС‹ вЂ” СЂР°Р·СЂРµС€РµРЅС‹.
    loop_depth: usize,
    /// РўРµРєСѓС‰Р°СЏ РіР»СѓР±РёРЅР° fn-Р»РёС‚РµСЂР°Р»РѕРІ (closure/lambda) РІРЅСѓС‚СЂРё defer body. Р•СЃР»Рё
    /// >0, `return` Р»РѕРєР°Р»РµРЅ вЂ” СЂР°Р·СЂРµС€С‘РЅ (relates С‚РѕР»СЊРєРѕ Рє Р±Р»РёР¶Р°Р№С€РµРјСѓ fn).
    fn_depth: usize,
}

fn check_defer_body_inner(e: &Expr, kw: &str, fn_effects: &HashMap<String, Vec<TypeRef>>, ctx: &DeferBodyCtx, errors: &mut Vec<Diagnostic>) {
    // РЎРЅР°С‡Р°Р»Р° РїСЂРѕРІРµСЂСЏРµРј СѓР·РµР» СЃР°Рј РїРѕ СЃРµР±Рµ.
    match &e.kind {
        // Exit-control: throw expression-form (D85 redirected via Fail).
        ExprKind::Throw(_) => {
            errors.push(Diagnostic::new(
                format!("`throw` is not allowed inside `{}` body (D90): defer body must be infallible вЂ” \
                         it cannot raise errors. If cleanup may fail, wrap with `with Fail = ...` handler.", kw),
                e.span,
            ));
        }
        // ? Рё !! desugar РІ throw в†’ Р·Р°РїСЂРµС‰РµРЅС‹ РїРѕ С‚РѕР№ Р¶Рµ РїСЂРёС‡РёРЅРµ (no Fail).
        ExprKind::Try(_) => {
            errors.push(Diagnostic::new(
                format!("`?` operator is not allowed inside `{}` body (D90): defer body must be infallible вЂ” \
                         `?` requires Fail effect.", kw),
                e.span,
            ));
        }
        ExprKind::Bang(_) => {
            errors.push(Diagnostic::new(
                format!("`!!` operator is not allowed inside `{}` body (D90): defer body must be infallible вЂ” \
                         `!!` requires Fail effect.", kw),
                e.span,
            ));
        }
        // Interrupt вЂ” РґРѕСЃСЂРѕС‡РЅС‹Р№ exit with-Р±Р»РѕРєР°, hijack'РёС‚ scope exit-СЃРµРјР°РЅС‚РёРєСѓ.
        ExprKind::Interrupt(_) => {
            errors.push(Diagnostic::new(
                format!("`interrupt` is not allowed inside `{}` body (D90): defer body cannot hijack scope exit.", kw),
                e.span,
            ));
        }
        // Suspend constructs by AST-form.
        ExprKind::Spawn(_) | ExprKind::Supervised { .. } | ExprKind::Detach(_)
        | ExprKind::Blocking(_)
        | ExprKind::ParallelFor { .. } => {
            errors.push(Diagnostic::new(
                format!("suspend operation (`spawn`/`supervised`/`detach`/`blocking`/`parallel for`) \
                         is not allowed inside `{}` body (D90): defer must be fast cleanup.", kw),
                e.span,
            ));
        }
        // Call СЃ suspend-СЌС„С„РµРєС‚Р°РјРё (callee.effects в€© SUSPEND_EFFECT_NAMES).
        ExprKind::Call { func, .. } => {
            if let Some(callee_name) = call_target_name(func) {
                if let Some(effs) = fn_effects.get(&callee_name) {
                    for ef in effs {
                        if let TypeRef::Named { path, .. } = ef {
                            if let Some(name) = path.last() {
                                if SUSPEND_EFFECT_NAMES.contains(&name.as_str()) {
                                    errors.push(Diagnostic::new(
                                        format!("call to `{}` requires suspend-effect `{}`, not allowed inside `{}` body (D90): \
                                                 defer must be fast cleanup.",
                                                callee_name, name, kw),
                                        e.span,
                                    ));
                                    break;
                                }
                            }
                        }
                    }
                }
            }
            // Also: built-in effect ops `Time.sleep`, `Net.get`, etc. вЂ”
            // РѕР±РЅР°СЂСѓР¶РёРІР°СЋС‚СЃСЏ РїРѕ member-path РїРµСЂРІРѕРіРѕ identifier'Р°.
            if let ExprKind::Member { obj, .. } = &func.kind {
                if let ExprKind::Ident(head) = &obj.kind {
                    if SUSPEND_EFFECT_NAMES.contains(&head.as_str()) {
                        errors.push(Diagnostic::new(
                            format!("operation `{}.{}` (effect `{}`) is not allowed inside `{}` body (D90): \
                                     defer must be fast cleanup.",
                                    head,
                                    match &func.kind { ExprKind::Member { name, .. } => name.as_str(), _ => "" },
                                    head, kw),
                            e.span,
                        ));
                    }
                }
            }
        }
        _ => {}
    }

    // Р РµРєСѓСЂСЃРёРІРЅРѕ РІРіР»СѓР±СЊ вЂ” РІР»РѕР¶РµРЅРЅС‹Рµ scope (block, if, etc.) РїРѕРґС‡РёРЅСЏСЋС‚СЃСЏ С‚РµРј Р¶Рµ
    // РѕРіСЂР°РЅРёС‡РµРЅРёСЏРј, С‚.Рє. РѕРЅРё С‡Р°СЃС‚СЊ defer body.
    walk_defer_subexprs(e, kw, fn_effects, ctx, errors);
}

fn walk_defer_subexprs(e: &Expr, kw: &str, fn_effects: &HashMap<String, Vec<TypeRef>>, ctx: &DeferBodyCtx, errors: &mut Vec<Diagnostic>) {
    match &e.kind {
        ExprKind::Block(b) => check_defer_body_block(b, kw, fn_effects, ctx, errors),
        ExprKind::If { cond, then, else_ } => {
            check_defer_body_inner(cond, kw, fn_effects, ctx, errors);
            check_defer_body_block(then, kw, fn_effects, ctx, errors);
            match else_ {
                Some(ElseBranch::Block(b)) => check_defer_body_block(b, kw, fn_effects, ctx, errors),
                Some(ElseBranch::If(e2)) => check_defer_body_inner(e2, kw, fn_effects, ctx, errors),
                None => {}
            }
        }
        ExprKind::IfLet { scrutinee, then, else_, .. } => {
            check_defer_body_inner(scrutinee, kw, fn_effects, ctx, errors);
            check_defer_body_block(then, kw, fn_effects, ctx, errors);
            match else_ {
                Some(ElseBranch::Block(b)) => check_defer_body_block(b, kw, fn_effects, ctx, errors),
                Some(ElseBranch::If(e2)) => check_defer_body_inner(e2, kw, fn_effects, ctx, errors),
                None => {}
            }
        }
        ExprKind::Match { scrutinee, arms } => {
            check_defer_body_inner(scrutinee, kw, fn_effects, ctx, errors);
            for a in arms {
                match &a.body {
                    MatchArmBody::Expr(e2) => check_defer_body_inner(e2, kw, fn_effects, ctx, errors),
                    MatchArmBody::Block(b) => check_defer_body_block(b, kw, fn_effects, ctx, errors),
                }
                if let Some(g) = &a.guard { check_defer_body_inner(g, kw, fn_effects, ctx, errors); }
            }
        }
        ExprKind::For { iter, body, .. } => {
            check_defer_body_inner(iter, kw, fn_effects, ctx, errors);
            let inner = DeferBodyCtx { loop_depth: ctx.loop_depth + 1, fn_depth: ctx.fn_depth };
            check_defer_body_block(body, kw, fn_effects, &inner, errors);
        }
        ExprKind::While { cond, body, .. } => {
            check_defer_body_inner(cond, kw, fn_effects, ctx, errors);
            let inner = DeferBodyCtx { loop_depth: ctx.loop_depth + 1, fn_depth: ctx.fn_depth };
            check_defer_body_block(body, kw, fn_effects, &inner, errors);
        }
        ExprKind::WhileLet { scrutinee, body, .. } => {
            check_defer_body_inner(scrutinee, kw, fn_effects, ctx, errors);
            let inner = DeferBodyCtx { loop_depth: ctx.loop_depth + 1, fn_depth: ctx.fn_depth };
            check_defer_body_block(body, kw, fn_effects, &inner, errors);
        }
        ExprKind::Loop { body, .. } => {
            let inner = DeferBodyCtx { loop_depth: ctx.loop_depth + 1, fn_depth: ctx.fn_depth };
            check_defer_body_block(body, kw, fn_effects, &inner, errors);
        }
        ExprKind::Select { arms } => {
            for arm in arms {
                match &arm.op {
                    SelectOp::Recv { chan, .. } => check_defer_body_inner(chan, kw, fn_effects, ctx, errors),
                    SelectOp::Send { chan, value } => {
                        check_defer_body_inner(chan, kw, fn_effects, ctx, errors);
                        check_defer_body_inner(value, kw, fn_effects, ctx, errors);
                    }
                    SelectOp::Default => {}
                }
                if let Some(g) = &arm.guard { check_defer_body_inner(g, kw, fn_effects, ctx, errors); }
                check_defer_body_block(&arm.body, kw, fn_effects, ctx, errors);
            }
        }
        ExprKind::With { body, .. } | ExprKind::Forbid { body, .. }
        | ExprKind::Realtime { body, .. } => {
            check_defer_body_block(body, kw, fn_effects, ctx, errors);
        }
        ExprKind::Call { func, args, trailing } => {
            check_defer_body_inner(func, kw, fn_effects, ctx, errors);
            for a in args { check_defer_body_inner(a.expr(), kw, fn_effects, ctx, errors); }
            if let Some(tr) = trailing {
                match tr {
                    Trailing::Block(b) => check_defer_body_block(b, kw, fn_effects, ctx, errors),
                    Trailing::Fn(fsb) => {
                        // Trailing fn-literal `fn { ... }` вЂ” СЌС‚Рѕ Р»СЏРјР±РґР°; return
                        // РІРЅСѓС‚СЂРё РЅРµС‘ Р»РѕРєР°Р»РµРЅ РґР»СЏ Р»СЏРјР±РґС‹, Р° РЅРµ РґР»СЏ defer body.
                        let inner = DeferBodyCtx { loop_depth: ctx.loop_depth, fn_depth: ctx.fn_depth + 1 };
                        if let FnBody::Block(b) = &fsb.body { check_defer_body_block(b, kw, fn_effects, &inner, errors); }
                        else if let FnBody::Expr(e2) = &fsb.body { check_defer_body_inner(e2, kw, fn_effects, &inner, errors); }
                    }
                    Trailing::LegacyBlockWithParams(tb) => {
                        let inner = DeferBodyCtx { loop_depth: ctx.loop_depth, fn_depth: ctx.fn_depth + 1 };
                        check_defer_body_block(&tb.body, kw, fn_effects, &inner, errors);
                    }
                }
            }
        }
        ExprKind::Binary { left, right, .. } => {
            check_defer_body_inner(left, kw, fn_effects, ctx, errors);
            check_defer_body_inner(right, kw, fn_effects, ctx, errors);
        }
        ExprKind::Unary { operand, .. } => check_defer_body_inner(operand, kw, fn_effects, ctx, errors),
        ExprKind::Coalesce(a, b) => {
            check_defer_body_inner(a, kw, fn_effects, ctx, errors);
            check_defer_body_inner(b, kw, fn_effects, ctx, errors);
        }
        ExprKind::As(e2, _) | ExprKind::Is(e2, _) => check_defer_body_inner(e2, kw, fn_effects, ctx, errors),
        ExprKind::Member { obj, .. } | ExprKind::Index { obj, .. } => check_defer_body_inner(obj, kw, fn_effects, ctx, errors),
        ExprKind::TurboFish { base, .. } => check_defer_body_inner(base, kw, fn_effects, ctx, errors),
        ExprKind::Range { start, end, .. } => {
            check_defer_body_inner(start, kw, fn_effects, ctx, errors);
            check_defer_body_inner(end, kw, fn_effects, ctx, errors);
        }
        ExprKind::ArrayLit(elems) => {
            for el in elems {
                match el {
                    ArrayElem::Item(e2) | ArrayElem::Spread(e2) => check_defer_body_inner(e2, kw, fn_effects, ctx, errors),
                }
            }
        }
        ExprKind::TupleLit(elems) => {
            for el in elems { check_defer_body_inner(el, kw, fn_effects, ctx, errors); }
        }
        ExprKind::RecordLit { fields, .. } => {
            for f in fields {
                if let Some(v) = &f.value { check_defer_body_inner(v, kw, fn_effects, ctx, errors); }
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

fn check_defer_body_block(b: &Block, kw: &str, fn_effects: &HashMap<String, Vec<TypeRef>>, ctx: &DeferBodyCtx, errors: &mut Vec<Diagnostic>) {
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
                    check_defer_body_inner(v, kw, fn_effects, ctx, errors);
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
            Stmt::Throw { span, .. } => {
                errors.push(Diagnostic::new(
                    format!("`throw` is not allowed inside `{}` body (D90): defer body must be infallible.", kw),
                    *span,
                ));
            }
            Stmt::Let(decl) => check_defer_body_inner(&decl.value, kw, fn_effects, ctx, errors),
            Stmt::Expr(e) => check_defer_body_inner(e, kw, fn_effects, ctx, errors),
            Stmt::Assign { target, value, .. } => {
                check_defer_body_inner(target, kw, fn_effects, ctx, errors);
                check_defer_body_inner(value, kw, fn_effects, ctx, errors);
            }
            // Nested defer/errdefer вЂ” СЌС‚Рѕ OK. Р­С‚Рѕ РЅРѕРІС‹Р№ scope (block),
            // defer'С‹ РІРЅСѓС‚СЂРё СЂРµРіРёСЃС‚СЂРёСЂСѓСЋС‚СЃСЏ РґР»СЏ СЌС‚РѕРіРѕ РІРЅСѓС‚СЂРµРЅРЅРµРіРѕ scope'Р°,
            // РЅРµ РґР»СЏ СЂРѕРґРёС‚РµР»СЊСЃРєРѕРіРѕ. РС… body С‚РѕР¶Рµ РїСЂРѕРІРµСЂСЏРµС‚СЃСЏ вЂ” РЅРѕ С‡РµСЂРµР·
            // РѕСЃРЅРѕРІРЅРѕР№ walk (check_defer_bodies РїСЂРѕС…РѕРґРёС‚ РїРѕ РІСЃРµРј bodies).
            Stmt::Defer { body, .. } => check_defer_body(body, false, fn_effects, errors),
            Stmt::ErrDefer { body, .. } => check_defer_body(body, true, fn_effects, errors),
            // Plan 33.2 Р¤.8: assert_static РІ defer body вЂ” walk expr.
            Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => check_defer_body_inner(expr, kw, fn_effects, ctx, errors),
            // Ф.4.1: apply — ghost, args walk.
            Stmt::Apply { args, .. } => {
                for a in args { check_defer_body_inner(a, kw, fn_effects, ctx, errors); }
            }
            // Ф.4.2: calc — ghost, шаги walk.
            Stmt::Calc { steps, .. } => {
                for step in steps { check_defer_body_inner(&step.expr, kw, fn_effects, ctx, errors); }
            }
            Stmt::Reveal { .. } => {}
        }
    }
    if let Some(t) = &b.trailing {
        check_defer_body_inner(t, kw, fn_effects, ctx, errors);
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
        Stmt::Expr(e) => check_ghost_in_expr(e, ghosts, errors),
        Stmt::Assign { target, value, .. } => {
            check_ghost_in_expr(target, ghosts, errors);
            check_ghost_in_expr(value, ghosts, errors);
        }
        Stmt::Return { value: Some(v), .. } => check_ghost_in_expr(v, ghosts, errors),
        Stmt::Throw { value, .. } => check_ghost_in_expr(value, ghosts, errors),
        Stmt::Defer { body, .. } | Stmt::ErrDefer { body, .. } => check_ghost_in_expr(body, ghosts, errors),
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
            Stmt::Defer { body, .. } | Stmt::ErrDefer { body, .. } => {
                self.walk_expr(body, None, errors);
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
                self.walk_expr(start, None, errors);
                self.walk_expr(end, None, errors);
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
            ExprKind::HandlerLit { methods, .. } => {
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
            Stmt::Assign { target, value, .. } => {
                self.walk_expr(target, None);
                self.walk_expr(value, None);
            }
            Stmt::Return { value, .. } => {
                if let Some(v) = value { self.walk_expr(v, None); }
            }
            Stmt::Throw { value, .. } => self.walk_expr(value, None),
            Stmt::Break(_) | Stmt::Continue(_) => {}
            Stmt::Defer { body, .. } | Stmt::ErrDefer { body, .. } => {
                self.walk_expr(body, None);
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
                self.walk_expr(start, None);
                self.walk_expr(end, None);
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
            ExprKind::HandlerLit { methods, .. } => {
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
