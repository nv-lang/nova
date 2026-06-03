//! Plan 45 Ф.4 — collector: AST → DocTree.
//!
//! Production-grade pipeline (после Ф.21-Ф.27):
//! - `collect()` / `collect_workspace()` — single-file / workspace mode entry.
//! - Дополнительные passes (отдельные модули): `strip_private`,
//!   `propagate_stability`, `resolve_intra_doc_links`, `collect_doc_tests`,
//!   `run_lints`, `populate_handler_matrix`, `populate_verify_status`,
//!   `infer_contracts_from_doctests`. Pipeline orchestration в `doc/mod.rs::build`.
//! - Module-level `#forbid` propagates в каждый item.capabilities (Ф.24.1).
//! - Implementors second-pass (workspace mode) opt-in через
//!   `NOVA_DOC_EXPERIMENTAL_IMPLEMENTORS=1` (Ф.24.3, false-positive guard).

use crate::ast::{
    ConstDecl, ContractKind, DocAttr, FnDecl, Item, Module, ModuleAttrKind, Purity,
    RealtimeAttr, TypeDecl, TypeDeclKind,
};
use crate::doc::doctree::*;

/// Plan 45 Р¤.3 / D105: СЂР°СЃРїР°СЂСЃРµРЅРЅС‹Рµ doc-attrs в†’ СЃС‚СЂСѓРєС‚СѓСЂРёСЂРѕРІР°РЅРЅС‹Рµ
/// РїРѕР»СЏ DocItem. Р’РѕР·РІСЂР°С‰Р°РµС‚СЃСЏ tuple РґР»СЏ РїРµСЂ-call site РїСЂРёРјРµРЅРµРЅРёСЏ.
struct ExtractedDocAttrs {
    deprecation: Option<Deprecation>,
    stability: Option<Stability>,
    aliases: Vec<String>,
    hide_doc: bool,
    summary_override: Option<String>,
    doc_test_handlers: Option<String>,
}

fn extract_doc_attrs(attrs: &[DocAttr]) -> ExtractedDocAttrs {
    let mut deprecation: Option<Deprecation> = None;
    let mut aliases: Vec<String> = Vec::new();
    let mut hide_doc = false;
    let mut summary_override: Option<String> = None;
    let mut doc_test_handlers: Option<String> = None;

    // Pass 1: СЃРѕР±СЂР°С‚СЊ explicit `#since(...)` (order-independent СЃ tier'РѕРј).
    let mut explicit_since: Option<String> = None;
    for a in attrs {
        if let DocAttr::Since(v) = a {
            explicit_since = Some(v.clone());
        }
    }

    // Pass 2: tier + РѕСЃС‚Р°Р»СЊРЅС‹Рµ Р°С‚СЂРёР±СѓС‚С‹.
    let mut tier: Option<StabilityTier> = None;
    let mut tier_since: Option<String> = None;
    let mut tier_feature: Option<String> = None;
    let mut tier_note: Option<String> = None;
    for a in attrs {
        match a {
            DocAttr::Deprecated { since, note, until } => {
                deprecation = Some(Deprecation {
                    note: note.clone().unwrap_or_default(),
                    since: since.clone().or_else(|| explicit_since.clone()),
                    until: until.clone(),
                });
            }
            DocAttr::Since(_) => {}
            DocAttr::Stable { since } => {
                tier = Some(StabilityTier::Stable);
                tier_since = since.clone().or_else(|| explicit_since.clone());
            }
            DocAttr::Unstable { feature } => {
                tier = Some(StabilityTier::Unstable);
                tier_since = explicit_since.clone();
                tier_feature = feature.clone();
            }
            DocAttr::Experimental { note } => {
                tier = Some(StabilityTier::Experimental);
                tier_since = explicit_since.clone();
                tier_note = note.clone();
            }
            DocAttr::HideDoc => hide_doc = true,
            DocAttr::DocAlias(xs) => aliases.extend(xs.iter().cloned()),
            DocAttr::DocInline | DocAttr::DocNoInline => {}
            DocAttr::DocSummary(s) => summary_override = Some(s.clone()),
            DocAttr::DocSection(_) => {}
            DocAttr::DocTestHandlers(p) => doc_test_handlers = Some(p.clone()),
        }
    }
    let stability = match tier {
        Some(t) => Some(Stability {
            tier: t,
            since: tier_since,
            feature: tier_feature,
            note: tier_note,
        }),
        None => explicit_since.as_ref().map(|v| Stability {
            tier: if is_post_1_0_version(v) {
                StabilityTier::Stable
            } else {
                StabilityTier::Unstable
            },
            since: Some(v.clone()),
            feature: None,
            note: None,
        }),
    };
    aliases.sort();
    aliases.dedup();
    ExtractedDocAttrs {
        deprecation,
        stability,
        aliases,
        hide_doc,
        summary_override,
        doc_test_handlers,
    }
}

fn is_post_1_0_version(v: &str) -> bool {
    let v = v.trim().trim_start_matches('v');
    let major = v.split('.').next().unwrap_or("0");
    matches!(major.parse::<u32>(), Ok(n) if n >= 1)
}

/// Plan 45 Р¤.4 вЂ" РїРѕСЃС‚СЂРѕРёС‚СЊ `DocTree` РёР· РїР°СЂСЃРµРЅРЅРѕРіРѕ, type-checked `Module`.
///
/// РџРѕРІРµРґРµРЅРёРµ MVP:
/// - РћРґРёРЅ module в†’ РѕРґРёРЅ `DocModule`.
/// - Items: `Fn` / `Type` / `Const` СЃРѕР±РёСЂР°СЋС‚СЃСЏ. `Effect` / `Protocol`
///   С‡РµСЂРµР· `TypeDeclKind::Effect` / `TypeDeclKind::Protocol`.
/// - Visibility: `is_export = true` в†’ `Export`, РёРЅР°С‡Рµ `Private`.
///   РџРѕ РґРµС„РѕР»С‚Сѓ filter вЂ" Export-only; flag `--include-private` (Plan 45
///   Р¤.12) РїРµСЂРµРєР»СЋС‡Р°РµС‚ (РЅР° collector-СѓСЂРѕРІРЅРµ РІСЃС‘ СЃРѕР±РёСЂР°РµС‚СЃСЏ; filter вЂ" РІ
///   renderer'Рµ).
/// - Module summary: РёР· `module.doc` (`//!` inner) + Р»СЋР±С‹С… `#doc "..."`
///   module-attr (D101).
pub fn collect(module: &Module) -> DocTree {
    let mut tree = DocTree::new();
    tree.modules.push(collect_one(module));
    tree
}

/// Plan 45 Р¤.21.7: workspace mode вЂ" СЃРѕР±СЂР°С‚СЊ РјРЅРѕРіРѕРјРѕРґСѓР»СЊРЅС‹Р№ DocTree.
/// Modules СѓР¶Рµ type-checked + effects-inferred caller'РѕРј. РџРѕСЂСЏРґРѕРє modules
/// РІ tree.modules вЂ" СЃРѕСЂС‚РёСЂСѓРµС‚СЃСЏ РїРѕ `path` РґР»СЏ РґРµС‚РµСЂРјРёРЅРёР·РјР°.
pub fn collect_workspace(modules: &[Module]) -> DocTree {
    let mut tree = DocTree::new();
    for m in modules {
        tree.modules.push(collect_one(m));
    }
    // Deterministic order: РїРѕ path.
    tree.modules.sort_by(|a, b| a.path.cmp(&b.path));

    // Plan 45 Р¤.24.3: implementors only when opt-in (structural matching has false-positives).
    // Populate only when NOVA_DOC_EXPERIMENTAL_IMPLEMENTORS=1.
    let experimental_implementors = std::env::var("NOVA_DOC_EXPERIMENTAL_IMPLEMENTORS")
        .map(|v| v == "1")
        .unwrap_or(false);

    if experimental_implementors {
        // Plan 45 Р¤.24.2: use BTreeMap/BTreeSet for deterministic iteration order.
        let mut proto_methods: Vec<(String, std::collections::BTreeSet<String>)> = Vec::new();
        for m in &tree.modules {
            for it in &m.items {
                if let ItemKind::Protocol { methods, .. } = &it.kind {
                    let names: std::collections::BTreeSet<String> =
                        methods.iter().map(|m| m.name.clone()).collect();
                    if !names.is_empty() {
                        proto_methods.push((it.id.clone(), names));
                    }
                }
            }
        }
        if !proto_methods.is_empty() {
            let mut type_methods: std::collections::BTreeMap<String, std::collections::BTreeSet<String>> =
                std::collections::BTreeMap::new();
            for m in &tree.modules {
                for it in &m.items {
                    if let ItemKind::Fn(sig) = &it.kind {
                        if let Some(recv) = &sig.receiver {
                            let type_id = format!("{}::{}", m.path.join("."), recv.type_name);
                            let method_name = it.id.rsplit('.').next().unwrap_or(&it.name).to_string();
                            type_methods.entry(type_id).or_default().insert(method_name);
                        }
                    }
                }
            }
            for m in &mut tree.modules {
                for it in &mut m.items {
                    if let ItemKind::Protocol { implementors, .. } = &mut it.kind {
                        if let Some((_, required)) = proto_methods.iter().find(|(id, _)| id == &it.id) {
                            let found: Vec<String> = type_methods
                                .iter()
                                .filter(|(_, meths)| required.iter().all(|r| meths.contains(r)))
                                .map(|(type_id, _)| type_id.clone())
                                .collect();
                            // already sorted (BTreeMap iteration is ordered)
                            *implementors = found;
                        }
                    }
                }
            }
        }
    }

    tree
}

/// Helper: РѕРґРёРЅ Module в†’ DocModule.
fn collect_one(module: &Module) -> DocModule {
    let module_path = module.name.clone();
    // Module-level documentation: РєРѕРЅС†Р°С‚ `//!` (inner doc) + РІСЃРµ
    // `#doc "..."` module-attr СЃС‚СЂРѕРєРё.
    let mut module_doc_parts: Vec<String> = Vec::new();
    for attr in &module.attrs {
        if let ModuleAttrKind::Doc(s) = &attr.kind {
            module_doc_parts.push(s.clone());
        }
    }
    if let Some(inner) = &module.doc {
        module_doc_parts.push(inner.content.clone());
    }
    let module_doc_content = if module_doc_parts.is_empty() {
        String::new()
    } else {
        module_doc_parts.join("\n\n")
    };
    let (module_summary, module_description) =
        crate::doc::markdown::extract_summary(&module_doc_content);

    // Plan 45 Р¤.24.1: extract module-level #forbid effects to propagate into items.
    let mut module_forbid: Vec<String> = Vec::new();
    for attr in &module.attrs {
        if let ModuleAttrKind::Forbid = &attr.kind {
            for eff in &attr.effects {
                if !module_forbid.contains(eff) {
                    module_forbid.push(eff.clone());
                }
            }
        }
    }
    module_forbid.sort();

    let mut items: Vec<DocItem> = Vec::new();
    for item in &module.items {
        match item {
            Item::Fn(f) => items.push(collect_fn(&module_path, f, &module_forbid)),
            Item::Type(t) => items.push(collect_type(&module_path, t)),
            Item::Const(c) => items.push(collect_const(&module_path, c)),
            // Plan 57: bench-декларации не документируются (как test/lemma).
            Item::Let(_) | Item::Test(_) | Item::Bench(_) | Item::Lemma(_) => {}
        }
    }
    // Plan 45 Р¤.24.11: collect re-exported items (`export import X.{Foo}`) as DocItems.
    // `#doc_inline` в†’ doc_inline=true (render inline), `#doc_no_inline` в†’ false (render as link).
    // Default: doc_inline=false (show "Re-exported from вЂ¦" link, like rustdoc default).
    for imp in &module.imports {
        if !imp.is_export {
            continue;
        }
        let source_module = imp.path.join(".");
        // Determine inline hint from doc_attrs.
        let doc_inline = imp.doc_attrs.iter().any(|a| matches!(a, crate::ast::DocAttr::DocInline));
        // no_inline explicitly overrides inline.
        let doc_no_inline = imp.doc_attrs.iter().any(|a| matches!(a, crate::ast::DocAttr::DocNoInline));
        let effective_inline = doc_inline && !doc_no_inline;

        if let Some(selective_items) = &imp.items {
            for sel in selective_items {
                let local_name = sel.alias.as_ref().unwrap_or(&sel.name).clone();
                let reexport_from = format!("{}::{}", source_module, sel.name);
                let id = format!("{}::{}", module_path.join("."), local_name);
                items.push(DocItem {
                    id,
                    module_path: module_path.to_vec(),
                    name: local_name,
                    visibility: Visibility::Export,
                    summary: None,
                    description: None,
                    sections: std::collections::BTreeMap::new(),
                    deprecation: None,
                    stability: None,
                    aliases: Vec::new(),
                    hide_doc: false,
                    doc_test_handlers: None,
                    capabilities: Capabilities::default(),
                    kind: ItemKind::ReExport { source: reexport_from.clone() },
                    source_span: imp.span,
                    peer_file: None,
                    linked_from: Vec::new(),
                    reexport_from: Some(reexport_from),
                    doc_inline: effective_inline,
                    scraped_examples: Vec::new(),
                });
            }
        } else {
            // Whole-module re-export: `export import X` вЂ" emit a module-level re-export marker.
            let local_name = imp.alias.as_ref().unwrap_or(imp.path.last().unwrap_or(&String::new())).clone();
            let reexport_from = source_module.clone();
            let id = format!("{}::{}", module_path.join("."), local_name);
            items.push(DocItem {
                id,
                module_path: module_path.to_vec(),
                name: local_name,
                visibility: Visibility::Export,
                summary: None,
                description: None,
                sections: std::collections::BTreeMap::new(),
                deprecation: None,
                stability: None,
                aliases: Vec::new(),
                hide_doc: false,
                doc_test_handlers: None,
                capabilities: Capabilities::default(),
                kind: ItemKind::ReExport { source: reexport_from.clone() },
                source_span: imp.span,
                peer_file: None,
                linked_from: Vec::new(),
                reexport_from: Some(reexport_from),
                doc_inline: effective_inline,
                scraped_examples: Vec::new(),
            });
        }
    }
    items.sort_by(|a, b| a.id.cmp(&b.id));

    let module_name = module_path.last().cloned().unwrap_or_default();
    let kind = if module.peer_files.len() > 1 {
        ModuleKind::Folder
    } else {
        ModuleKind::File
    };
    let peers: Vec<String> = module
        .peer_files
        .iter()
        .filter_map(|pf| pf.path.file_name().map(|s| s.to_string_lossy().into_owned()))
        .collect();

    // Plan 45 Ф.22.1 / D105: module-level doc-attrs.
    let mod_attrs = extract_doc_attrs(&module.doc_attrs);

    // Plan 45 Ф.24.16: effect composition matrix — collect exported fns with effects.
    let effect_matrix: Vec<crate::doc::doctree::EffectMatrixEntry> = {
        let mut entries = Vec::new();
        for item in &items {
            if item.visibility != Visibility::Export {
                continue;
            }
            if let ItemKind::Fn(sig) = &item.kind {
                if !sig.effects.is_empty() {
                    let effect_names: Vec<String> = sig.effects.iter().map(|e| e.name.clone()).collect();
                    entries.push(crate::doc::doctree::EffectMatrixEntry {
                        item_id: item.id.clone(),
                        fn_name: item.name.clone(),
                        effects: effect_names,
                    });
                }
            }
        }
        entries
    };
    // Plan 45 Ф.24.17: realtime constraint matrix — collect @realtime fns.
    let realtime_matrix: Vec<crate::doc::doctree::RealtimeConstraintEntry> = {
        let mut entries = Vec::new();
        for item in &items {
            if item.visibility != Visibility::Export {
                continue;
            }
            if item.capabilities.realtime {
                entries.push(crate::doc::doctree::RealtimeConstraintEntry {
                    item_id: item.id.clone(),
                    fn_name: item.name.clone(),
                    nogc: item.capabilities.realtime_nogc,
                    forbidden_effects: item.capabilities.forbid.clone(),
                });
            }
        }
        entries
    };

    // Plan 71 / D127: пути source-файлов для fixture-exemption logic в doc::lints.
    // Folder-modules — все peers; single-file — один путь.
    let source_paths: Vec<std::path::PathBuf> = module
        .peer_files
        .iter()
        .map(|pf| pf.path.clone())
        .collect();

    DocModule {
        path: module_path,
        name: module_name,
        kind,
        peers,
        summary: module_summary,
        description: module_description,
        deprecation: mod_attrs.deprecation,
        stability: mod_attrs.stability,
        hide_doc: mod_attrs.hide_doc,
        items,
        effect_matrix,
        realtime_matrix,
        source_span: module.span,
        source_paths,
    }
}

fn collect_fn(module_path: &[String], f: &FnDecl, module_forbid: &[String]) -> DocItem {
    let module_str = module_path.join(".");
    let id = match &f.receiver {
        Some(r) => format!("{}::{}.{}", module_str, r.type_name, f.name),
        None => format!("{}::{}", module_str, f.name),
    };
    let (summary_auto, description, sections) = crate::doc::doctree::split_doc(&f.doc);
    let visibility = if f.is_export {
        Visibility::Export
    } else {
        Visibility::Private
    };
    let signature = build_signature(f);
    let attrs = extract_doc_attrs(&f.doc_attrs);
    let summary = attrs.summary_override.clone().or(summary_auto);
    let capabilities = Capabilities {
        realtime: matches!(f.realtime_attr, RealtimeAttr::Realtime | RealtimeAttr::RealtimeNogc),
        realtime_nogc: matches!(f.realtime_attr, RealtimeAttr::RealtimeNogc),
        pure_fn: matches!(f.purity, Purity::Pure),
        // Plan 45 Ф.24.1: propagate module-level #forbid into each item.
        forbid: module_forbid.to_vec(),
        // Plan 45 Ф.26.3 / D63: allow_transit пока всегда empty —
        // parser не поддерживает `#allow_transit X` attribute. Schema-stable
        // placeholder для будущего D63-completion (Plan 16/45 follow-up).
        allow_transit: Vec::new(),
        // Plan 100.8 / D166: consume is a type-level property; fns never carry it.
        consume: false,
        // Plan 91.9 / D186: impl_protocols — type-only property, empty for fns.
        impl_protocols: Vec::new(),
    };
    DocItem {
        id,
        module_path: module_path.to_vec(),
        name: f.name.clone(),
        visibility,
        summary,
        description,
        sections,
        deprecation: attrs.deprecation,
        stability: attrs.stability,
        aliases: attrs.aliases,
        hide_doc: attrs.hide_doc,
        doc_test_handlers: attrs.doc_test_handlers,
        capabilities,
        kind: ItemKind::Fn(signature),
        source_span: f.span,
        peer_file: None,
        linked_from: Vec::new(),
        reexport_from: None,
        doc_inline: false,
        scraped_examples: Vec::new(),
    }
}

fn collect_type(module_path: &[String], t: &TypeDecl) -> DocItem {
    let module_str = module_path.join(".");
    let id = format!("{}::{}", module_str, t.name);
    let (summary_auto, description, sections) = crate::doc::doctree::split_doc(&t.doc);
    let visibility = if t.is_export {
        Visibility::Export
    } else {
        Visibility::Private
    };
    let attrs = extract_doc_attrs(&t.doc_attrs);
    let summary = attrs.summary_override.clone().or(summary_auto);
    let kind = match &t.kind {
        TypeDeclKind::Record(fields) => {
            let record_fields = fields
                .iter()
                .map(|f| RecordField {
                    name: f.name.clone(),
                    ty: render_type(&f.ty),
                    mutable: f.mutable,
                    priv_field: f.priv_field,
                })
                .collect();
            ItemKind::Type(TypeDefinition::Record(record_fields))
        }
        TypeDeclKind::Sum(variants) => {
            let sum_variants = variants
                .iter()
                .map(|v| SumVariant {
                    name: v.name.clone(),
                    payload: match &v.kind {
                        crate::ast::SumVariantKind::Unit => VariantPayload::Unit,
                        crate::ast::SumVariantKind::Tuple(ts) => VariantPayload::Tuple(
                            ts.iter().map(render_type).collect(),
                        ),
                        crate::ast::SumVariantKind::Record(fs) => {
                            VariantPayload::Record(
                                fs.iter()
                                    .map(|f| RecordField {
                                        name: f.name.clone(),
                                        ty: render_type(&f.ty),
                                        mutable: f.mutable,
                                        priv_field: f.priv_field,
                                    })
                                    .collect(),
                            )
                        }
                    },
                })
                .collect();
            ItemKind::Type(TypeDefinition::Sum(sum_variants))
        }
        TypeDeclKind::Alias(ty) => ItemKind::Type(TypeDefinition::Alias(render_type(ty))),
        // Plan 45 Ф.23.10 / D107: Newtype — отдельный variant (см. ниже после Protocol).
        // ВНИМАНИЕ: предыдущая строка делала `Newtype → Alias` (MVP-stub до Ф.23.10).
        // Этот arm удалён — теперь правильная ветка `TypeDeclKind::Newtype` ниже.
        TypeDeclKind::Effect(methods) => {
            // Plan 45 Р¤.22.4 / D107: axioms РЅР° СѓСЂРѕРІРЅРµ ItemKind::Effect,
            // РЅРµ per-method (axiom СЃСЃС‹Р»Р°РµС‚СЃСЏ РЅР° СЌС„С„РµРєС‚ С†РµР»РёРєРѕРј).
            let axioms: Vec<EffectAxiomDoc> = t.axioms.iter().map(|ax| EffectAxiomDoc {
                name: ax.name.clone(),
                formula: render_expr(&ax.formula),
            }).collect();
            let sigs = methods
                .iter()
                .map(|m| EffectMethodSig {
                    name: m.name.clone(),
                    params: m
                        .params
                        .iter()
                        .map(|p| Param {
                            name: p.name.clone(),
                            ty: render_type(&p.ty),
                            default: p.default.as_ref().map(render_expr),
                            variadic: p.is_variadic,
                            keyword_only: p.default.is_some(),
                        })
                        .collect(),
                    return_type: m
                        .return_type
                        .as_ref()
                        .map(render_type)
                        .unwrap_or_else(|| "()".to_string()),
                })
                .collect();
            // Plan 45 Ф.26.2: handlers populated post-collection в
            // `collect_handlers` pass (workspace mode). Default empty.
            ItemKind::Effect { methods: sigs, axioms, handlers: Vec::new() }
        }
        TypeDeclKind::Protocol { methods, .. } => {
            let sigs = methods
                .iter()
                .map(|m| ProtocolMethodSig {
                    name: m.name.clone(),
                    params: m
                        .params
                        .iter()
                        .map(|p| Param {
                            name: p.name.clone(),
                            ty: render_type(&p.ty),
                            default: p.default.as_ref().map(render_expr),
                            variadic: p.is_variadic,
                            keyword_only: p.default.is_some(),
                        })
                        .collect(),
                    return_type: m
                        .return_type
                        .as_ref()
                        .map(render_type)
                        .unwrap_or_else(|| "()".to_string()),
                })
                .collect();
            ItemKind::Protocol { methods: sigs, implementors: Vec::new() }
        }
        // Plan 120 (D215): named tuple — render like a record for doc purposes.
        TypeDeclKind::NamedTuple(fields) => {
            let record_fields = fields
                .iter()
                .map(|f| RecordField {
                    name: f.name.clone(),
                    ty: render_type(&f.ty),
                    mutable: false,
                    priv_field: f.priv_field,
                })
                .collect();
            ItemKind::Type(TypeDefinition::Record(record_fields))
        }
        TypeDeclKind::Newtype(inner_ty) => {
            ItemKind::Type(TypeDefinition::Newtype {
                inner: render_type(inner_ty),
            })
        }
        // Plan 62.D.bis (D126, 2026-05-18): `external type X` — opaque type
        // (StringBuilder, WriteBuffer, ReadBuffer, future Channel[T]).
        // Doc surface — type alias к "opaque (runtime-implemented)" — fallback
        // на newtype-like display с pseudo-target "opaque". `nova doc`
        // покажет declaration + doc-comment + generic params + visibility.
        // Future enhancement: dedicated `TypeDefinition::Opaque` variant
        // в doc tree для richer rendering ("Implementation: runtime —
        // nova_rt/<name>.h" line). Bootstrap — alias-style для simplicity.
        TypeDeclKind::Opaque => ItemKind::Type(TypeDefinition::Alias("opaque".to_string())),
    };
    // Plan 100.8 / D166: propagate consume marker from TypeDecl into Capabilities.
    // Plan 91.9 / D186: propagate impl_protocols list (renderers show
    // `implements: P, Q, R` line в type summary).
    let capabilities = Capabilities {
        consume: t.consume,
        impl_protocols: t.impl_protocols.clone(),
        ..Capabilities::default()
    };
    DocItem {
        id,
        module_path: module_path.to_vec(),
        name: t.name.clone(),
        visibility,
        summary,
        description,
        sections,
        deprecation: attrs.deprecation,
        stability: attrs.stability,
        aliases: attrs.aliases,
        hide_doc: attrs.hide_doc,
        doc_test_handlers: attrs.doc_test_handlers,
        capabilities,
        kind,
        source_span: t.span,
        peer_file: None,
        linked_from: Vec::new(),
        reexport_from: None,
        doc_inline: false,
        scraped_examples: Vec::new(),
    }
}

fn collect_const(module_path: &[String], c: &ConstDecl) -> DocItem {
    let module_str = module_path.join(".");
    let id = format!("{}::{}", module_str, c.name);
    let (summary_auto, description, sections) = crate::doc::doctree::split_doc(&c.doc);
    let visibility = if c.is_export {
        Visibility::Export
    } else {
        Visibility::Private
    };
    let attrs = extract_doc_attrs(&c.doc_attrs);
    let summary = attrs.summary_override.clone().or(summary_auto);
    let ty = c
        .ty
        .as_ref()
        .map(render_type)
        .unwrap_or_else(|| "_".to_string());
    DocItem {
        id,
        module_path: module_path.to_vec(),
        name: c.name.clone(),
        visibility,
        summary,
        description,
        sections,
        deprecation: attrs.deprecation,
        stability: attrs.stability,
        aliases: attrs.aliases,
        hide_doc: attrs.hide_doc,
        doc_test_handlers: attrs.doc_test_handlers,
        capabilities: Capabilities::default(),
        kind: ItemKind::Const {
            ty,
            value: render_expr(&c.value),
        },
        source_span: c.span,
        peer_file: None,
        linked_from: Vec::new(),
        reexport_from: None,
        doc_inline: false,
        scraped_examples: Vec::new(),
    }
}

fn build_signature(f: &FnDecl) -> Signature {
    let receiver = f.receiver.as_ref().map(|r| Receiver {
        type_name: r.type_name.clone(),
        kind: match r.kind {
            crate::ast::ReceiverKind::Instance => ReceiverKind::Instance,
            crate::ast::ReceiverKind::Static => ReceiverKind::Static,
        },
        mutable: r.mutable,
    });
    let generics = f
        .generics
        .iter()
        .map(|g| GenericParam {
            name: g.name.clone(),
            // Plan 101.3: doc render — собираем все bounds в один
            // строковый формат `A + B` (Rust-style). Empty = None.
            bound: if g.bounds.is_empty() {
                None
            } else {
                Some(
                    g.bounds
                        .iter()
                        .map(render_type)
                        .collect::<Vec<_>>()
                        .join(" + "),
                )
            },
            default: g.default.as_ref().map(render_type),
        })
        .collect();
    let params = f
        .params
        .iter()
        .map(|p| Param {
            name: p.name.clone(),
            ty: render_type(&p.ty),
            default: p.default.as_ref().map(render_expr),
            variadic: p.is_variadic,
            keyword_only: p.default.is_some(),
        })
        .collect();
    let return_type = f
        .return_type
        .as_ref()
        .map(render_type)
        .unwrap_or_else(|| "()".to_string());
    // Effect-row: structured entries РґР»СЏ Р¤.23.8/23.9.
    let mut effects: Vec<EffectEntry> = f.effects.iter().map(|eff| {
        // Plan 45 Р¤.23.9: row-variables вЂ" single uppercase letter Р±РµР· generics.
        let is_row_var = if let crate::ast::TypeRef::Named { path, generics, .. } = eff {
            path.len() == 1 && generics.is_empty()
                && path[0].len() == 1
                && path[0].chars().next().map_or(false, |c| c.is_uppercase())
        } else {
            false
        };
        let name = if is_row_var {
            if let crate::ast::TypeRef::Named { path, .. } = eff {
                format!("({})", path[0])
            } else {
                render_effect(eff)
            }
        } else {
            render_effect(eff)
        };
        EffectEntry {
            name,
            target_id: None,
            summary: None,
            is_row_var,
        }
    }).collect();
    effects.sort_by(|a, b| a.name.cmp(&b.name));
    effects.dedup_by(|a, b| a.name == b.name);
    // Raises: РІС‹С‚Р°С‰РёС‚СЊ РёР· `Fail[X]` РІ effect-row.
    let mut raises: Vec<String> = Vec::new();
    for eff in &f.effects {
        if let Some(inner) = extract_fail_inner(eff) {
            raises.push(inner);
        }
    }
    raises.sort();
    raises.dedup();
    // Plan 45 Р¤.23.1 / D24/D106: contracts РёР· AST.
    let mut contracts: Vec<ContractDoc> = Vec::new();
    for c in &f.contracts {
        let kind = match c.kind {
            ContractKind::Requires => "requires",
            ContractKind::Ensures => "ensures",
            ContractKind::EnsuresFail => "ensures_fail",
        };
        contracts.push(ContractDoc {
            kind: kind.to_string(),
            expr: render_expr(&c.expr),
        });
    }
    if let Some(dec) = &f.decreases {
        contracts.push(ContractDoc {
            kind: "decreases".to_string(),
            expr: render_expr(dec),
        });
    }
    Signature {
        receiver,
        generics,
        params,
        return_type,
        effects,
        raises,
        contracts,
        verify_status: VerifyStatus::NotAttempted,
    }
}

/// РњРёРЅРёРјР°Р»СЊРЅС‹Р№ pretty-print TypeRef РІ Nova source. **MVP-РїСЂРѕСЃС‚РѕР№** вЂ"
/// РґР»СЏ РїРѕРїСѓР»СЏСЂРЅС‹С… С„РѕСЂРј; СЃР»РѕР¶РЅС‹Рµ СЃР»СѓС‡Р°Рё РјРѕРіСѓС‚ РѕРєСЂСѓРіР»СЏС‚СЊСЃСЏ (best-effort
/// СЃС‚СЂРѕРєРѕРІРѕРµ РїСЂРµРґСЃС‚Р°РІР»РµРЅРёРµ).
/// Plan 45 Р¤.24.6: public re-export of render_type for use in render_json.
pub fn render_type_for_doc(ty: &crate::ast::TypeRef) -> String {
    render_type(ty)
}

fn render_type(ty: &crate::ast::TypeRef) -> String {
    use crate::ast::TypeRef;
    match ty {
        TypeRef::Named { path, generics, .. } => {
            let base = path.join(".");
            if generics.is_empty() {
                base
            } else {
                let g = generics
                    .iter()
                    .map(render_type)
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{}[{}]", base, g)
            }
        }
        TypeRef::Array(inner, _) => format!("[]{}", render_type(inner)),
        TypeRef::FixedArray(len, elem, _) => format!("[{}]{}", len, render_type(elem)),
        TypeRef::Tuple(elems, _) => {
            let inner = elems
                .iter()
                .map(render_type)
                .collect::<Vec<_>>()
                .join(", ");
            format!("({})", inner)
        }
        TypeRef::Func {
            params,
            effects,
            return_type,
            ..
        } => {
            let p = params
                .iter()
                .map(render_type)
                .collect::<Vec<_>>()
                .join(", ");
            let eff = if effects.is_empty() {
                String::new()
            } else {
                let es = effects.iter().map(render_type).collect::<Vec<_>>().join(" ");
                format!(" {}", es)
            };
            let r = return_type
                .as_ref()
                .map(|t| render_type(t))
                .unwrap_or_else(|| "()".to_string());
            format!("fn({}){} -> {}", p, eff, r)
        }
        // Plan 97 Ф.2 (D142): анонимный protocol-тип — `protocol { sig* }`.
        // Render — упрощённый, методы перечисляются через `;`. Полный
        // pretty-print с эффектами/generics — задача render_method_sig.
        TypeRef::Protocol { methods, .. } => {
            let sigs: Vec<String> = methods
                .iter()
                .map(|m| {
                    let prefix = if m.is_static { "." } else { "" };
                    let params = m
                        .params
                        .iter()
                        .map(|p| format!("{} {}", p.name, render_type(&p.ty)))
                        .collect::<Vec<_>>()
                        .join(", ");
                    let ret = m
                        .return_type
                        .as_ref()
                        .map(|t| format!(" -> {}", render_type(t)))
                        .unwrap_or_default();
                    format!("{}{}({}){}", prefix, m.name, params, ret)
                })
                .collect();
            format!("protocol {{ {} }}", sigs.join("; "))
        }
        TypeRef::Unit(_) => "()".to_string(),
        // D176 (Plan 108) / Plan 114 D184: ro T — display as "ro T"
        TypeRef::Readonly(inner, _) => format!("ro {}", render_type(inner)),
        // Plan 118 D216 §1 / Plan 118.5: typed pointer `*T` family.
        // V2 canonical form: Mut/Unsafe wrap Pointer, e.g. `Mut(Pointer(T))` = `*mut T`.
        // Bare Pointer (no Mut/Unsafe wrapper) = `*T` (read-only).
        TypeRef::Pointer(inner, _) => format!("*{}", render_type(inner)),
        TypeRef::Mut(inner, _) => match inner.as_ref() {
            TypeRef::Pointer(p_inner, _) => format!("*mut {}", render_type(p_inner)),
            _ => format!("mut {}", render_type(inner)),
        },
        TypeRef::Unsafe(inner, _) => match inner.as_ref() {
            TypeRef::Pointer(p_inner, _) => format!("*unsafe {}", render_type(p_inner)),
            _ => format!("unsafe {}", render_type(inner)),
        },
    }
}

/// Effect вЂ" СЌС‚Рѕ `TypeRef` (РѕР±С‹С‡РЅРѕ `Named`). Render С‡РµСЂРµР· `render_type`.
fn render_effect(eff: &crate::ast::TypeRef) -> String {
    render_type(eff)
}

/// РР·РІР»РµС‡СЊ РёРјСЏ `X` РёР· effect-row СЌР»РµРјРµРЅС‚Р° `Fail[X]` (РґР»СЏ `raises`-СЃРїРёСЃРєР°).
/// Р’РѕР·РІСЂР°С‰Р°РµС‚ `None`, РµСЃР»Рё СЌР»РµРјРµРЅС‚ РЅРµ `Fail[...]`.
fn extract_fail_inner(eff: &crate::ast::TypeRef) -> Option<String> {
    use crate::ast::TypeRef;
    if let TypeRef::Named { path, generics, .. } = eff {
        if path.len() == 1 && path[0] == "Fail" && !generics.is_empty() {
            return Some(render_type(&generics[0]));
        }
    }
    None
}

/// Pretty-print expression в Nova source (для contracts JSON/MD).
///
/// **Plan 45 Ф.28.1:** делегирует в shared util `crate::ast::pretty::print_expr`.
/// Раньше был own implementation (Ф.27.2 расширение). Shared util — единая
/// pretty-print logic, переиспользуема в diag/spec.
fn render_expr(e: &crate::ast::Expr) -> String {
    crate::ast::pretty::print_expr(e)
}

// Plan 45 Ф.29.1: render_expr_legacy removed (Ф.28.1 soak finished).
// Shared util `crate::ast::pretty::print_expr` полностью покрывает все cases.

