//! Plan 45 Ф.9 — DocTree → JSON renderer (D107 schema v1, MVP subset).
//!
//! MVP: emits валидный D107 schema v1 для базовых полей (modules,
//! items с kind discriminator, signatures, links/doc_tests как пустые
//! массивы). Расширения (stability/deprecation/doc_attrs) — добавляются
//! по мере реализации Ф.3 (doc-attrs); Plan 45 §6 stability rules
//! гарантируют additive-minor.
//!
//! **Deterministic output:**
//! - object-keys в алфавитном порядке (через ручную сериализацию);
//! - arrays отсортированы (modules по `path`, items по `id`,
//!   links/doc_tests по соответствующему id);
//! - `generated_at` опускается при `SOURCE_DATE_EPOCH`.
//!
//! Используем минимальный manual JSON writer (без serde_json) — у
//! проекта уже есть serde+serde_json в depгах parent crate'а; но
//! manual writer проще для гарантии deterministic key-ordering без
//! `BTreeMap` ceremony, и для embedding schema без impl Serialize
//! на public-AST. Когда подключим serde — переключимся, но эта
//! текстовая форма соответствует D107 byte-for-byte.

use crate::doc::doctree::*;
use std::fmt::Write;

pub fn render(tree: &DocTree) -> String {
    render_with_source(tree, None)
}

/// Render с optional source text для line-information. Если `source`
/// предоставлен — `source.line` рассчитывается через byte_to_line_col;
/// иначе — placeholder 1.
pub fn render_with_source(tree: &DocTree, source: Option<&str>) -> String {
    let mut w = JsonWriter::new(source);
    w.begin_object();
    w.field_u32("format_version", tree.format_version);
    // generated_at — opt-in (по умолчанию опускаем для reproducible
    // builds). Источники timestamp'а в порядке приоритета:
    //   1. NOVA_DOC_GENERATED_AT — explicit ISO-8601 string;
    //   2. SOURCE_DATE_EPOCH — Unix epoch seconds (стандарт);
    //   3. опускаем поле (deterministic default).
    // Алфавитный порядок: generated_at (g) < nova_version (n).
    if let Some(ts) = generated_at_value() {
        w.field_str("generated_at", &ts);
    }
    w.field_str(
        "nova_version",
        env!("CARGO_PKG_VERSION"),
    );
    // Ф.22.3 / D107: `source_root` — path к исходнику / workspace root.
    // Plan 45 Ф.23.25: normalize to forward slashes for cross-platform
    // portability. If NOVA_DOC_WORKSPACE_ROOT env var is set, make relative
    // to it using ${WORKSPACE_ROOT}/... placeholder.
    if let Some(root) = &tree.source_root {
        let normalized = normalize_source_root(root);
        w.field_str("source_root", &normalized);
    }
    w.field_array("doc_tests", |w| {
        for t in &tree.doc_tests {
            w.array_object(|w| {
                w.field_null_or_str("from_id", t.from_id.as_deref());
                w.field_str("full_source", &t.full_source);
                w.field_str("id", &t.id);
                w.field_u32("index", t.index);
                w.field_array("modifiers", |w| {
                    let mut mods: Vec<&'static str> =
                        t.modifiers.iter().map(|m| m.as_str()).collect();
                    mods.sort();
                    mods.dedup();
                    for m in mods {
                        w.array_str(m);
                    }
                });
                w.field_str("visible_source", &t.visible_source);
            });
        }
    });
    w.field_array("links", |w| {
        for link in &tree.links {
            w.array_object(|w| {
                w.field_null_or_str("from_id", link.from_id.as_deref());
                w.field_null_or_str("target_id", link.target_id.as_deref());
                // Plan 45 Ф.30.1: external crate-doc URL.
                w.field_null_or_str("target_url", link.target_url.as_deref());
                w.field_str("text", &link.text);
            });
        }
    });
    w.field_array("modules", |w| {
        for m in &tree.modules {
            w.array_object(|w| write_module(w, m));
        }
    });
    w.field_array("items", |w| {
        for m in &tree.modules {
            for it in &m.items {
                w.array_object(|w| write_item(w, it));
            }
        }
    });
    // Plan 45 Ф.25.1: diagnostic warnings (malformed attrs, unknown modifiers,
    // ambiguous links). Sorted, deduped в собирающих passes.
    // Алфавитный порядок: w после v(=visible_source) и source_root(s).
    w.field_array("warnings", |w| {
        for warn in &tree.warnings {
            w.array_object(|w| {
                w.field_str("item_id", &warn.item_id);
                w.field_str("message", &warn.message);
                w.field_str("rule", &warn.rule);
            });
        }
    });
    w.end_object();
    w.finish()
}

fn write_module(w: &mut JsonWriter, m: &DocModule) {
    match &m.deprecation {
        None => w.field_null_or_str("deprecation", None),
        Some(d) => w.field_object("deprecation", |w| {
            w.field_str("note", &d.note);
            w.field_null_or_str("since", d.since.as_deref());
        }),
    }
    w.field_array("doc_attrs", |_| {});
    // Plan 45 Ф.24.16: effect composition matrix (empty array if no effectful fns).
    w.field_array("effect_matrix", |w| {
        for entry in &m.effect_matrix {
            w.array_object(|w| {
                w.field_array("effects", |w| {
                    for eff in &entry.effects {
                        w.array_str(eff);
                    }
                });
                w.field_str("fn_name", &entry.fn_name);
                w.field_str("item_id", &entry.item_id);
            });
        }
    });
    w.field_str(
        "kind",
        match m.kind {
            ModuleKind::Folder => "folder",
            ModuleKind::File => "file",
        },
    );
    w.field_str("name", &m.name);
    w.field_str("path", &m.path.join("."));
    w.field_array("peers", |w| {
        let mut sorted = m.peers.clone();
        sorted.sort();
        for p in &sorted {
            w.array_str(p);
        }
    });
    // Plan 45 Ф.24.17: realtime constraint matrix.
    w.field_array("realtime_matrix", |w| {
        for entry in &m.realtime_matrix {
            w.array_object(|w| {
                w.field_array("forbidden_effects", |w| {
                    for eff in &entry.forbidden_effects {
                        w.array_str(eff);
                    }
                });
                w.field_str("fn_name", &entry.fn_name);
                w.field_str("item_id", &entry.item_id);
                w.field_bool("nogc", entry.nogc);
            });
        }
    });
    w.field_object("source", |w| {
        let span = m.source_span;
        w.field_u32("file_id", span.file_id);
        w.field_u32("line", w.line_of(span.start));
        // Plan 45 Ф.25.3: source URL linking (template-based, opt-in).
        if let Some(url) = source_url_for(&m.path, None, w.line_of(span.start)) {
            w.field_str("url", &url);
        }
    });
    match &m.stability {
        None => w.field_null_or_str("stability", None),
        Some(s) => w.field_object("stability", |w| {
            w.field_null_or_str("feature", s.feature.as_deref());
            w.field_null_or_str("note", s.note.as_deref());
            w.field_null_or_str("since", s.since.as_deref());
            w.field_str("tier", s.tier.as_str());
        }),
    }
    w.field_null_or_str("summary", m.summary.as_deref());
    w.field_null_or_str("description", m.description.as_deref());
}

fn write_item(w: &mut JsonWriter, it: &DocItem) {
    // alphabetical key order
    w.field_array("aliases", |w| {
        for a in &it.aliases {
            w.array_str(a);
        }
    });
    // Plan 45 Ф.23.3 / D63/D64: capabilities — alphabetical:
    // allow_transit (a) < consume (c) < forbid (f) < pure_fn (p) < realtime (r) < realtime_nogc (rn).
    w.field_object("capabilities", |w| {
        // Plan 45 Ф.26.3 / D63: allow_transit (escape hatches из forbid-блока).
        w.field_array("allow_transit", |w| {
            for e in &it.capabilities.allow_transit { w.array_str(e); }
        });
        // Plan 100.8 / D166: consume — must-be-consumed type marker (D133).
        w.field_bool("consume", it.capabilities.consume);
        // Plan 91.9 / D186: impl_protocols — declared opt-in list from
        // #impl(P + Q + ...) annotation. Compiler-verified.
        w.field_array("impl_protocols", |w| {
            for p in &it.capabilities.impl_protocols { w.array_str(p); }
        });
        w.field_array("forbid", |w| {
            for f in &it.capabilities.forbid { w.array_str(f); }
        });
        w.field_bool("pure_fn", it.capabilities.pure_fn);
        w.field_bool("realtime", it.capabilities.realtime);
        w.field_bool("realtime_nogc", it.capabilities.realtime_nogc);
    });
    match &it.deprecation {
        None => w.field_null_or_str("deprecation", None),
        Some(d) => w.field_object("deprecation", |w| {
            w.field_str("note", &d.note);
            w.field_null_or_str("since", d.since.as_deref());
            // Plan 45 Ф.23.6 / D105: until field
            w.field_null_or_str("until", d.until.as_deref());
        }),
    }
    w.field_null_or_str("description", it.description.as_deref());
    w.field_array("doc_attrs", |_| {});
    // Plan 45 Ф.24.11: doc_inline rendering hint for re-exports.
    if it.reexport_from.is_some() {
        w.field_bool("doc_inline", it.doc_inline);
    }
    w.field_null_or_str("doc_test_handlers", it.doc_test_handlers.as_deref());
    w.field_str("id", &it.id);
    w.field_str("kind", item_kind_str(&it.kind));
    // Plan 45 Ф.23.23: back-links — IDs that link to this item.
    if !it.linked_from.is_empty() {
        w.field_array("linked_from", |w| {
            for id in &it.linked_from { w.array_str(id); }
        });
    }
    w.field_str("module_path", &it.module_path.join("."));
    w.field_str("name", &it.name);
    // Plan 45 Ф.24.11: re-export source path (only present for re-exported items).
    w.field_null_or_str("reexport_from", it.reexport_from.as_deref());
    // Plan 45 Ф.24.9: scraped call-site examples (empty array when not scraped).
    w.field_array("scraped_examples", |w| {
        for ex in &it.scraped_examples {
            w.array_object(|w| {
                w.field_str("file", &ex.file);
                w.field_u32("line", ex.line);
                w.field_str("snippet", &ex.snippet);
            });
        }
    });
    w.field_object("sections", |w| {
        for (key, val) in &it.sections {
            w.field_str(key, val);
        }
    });
    w.field_object("source", |w| {
        let span = it.source_span;
        w.field_u32("file_id", span.file_id);
        w.field_u32("line", w.line_of(span.start));
        // Plan 45 Ф.23.11: peer_file attribution (folder-module mode).
        w.field_null_or_str("peer_file", it.peer_file.as_deref());
        // Plan 45 Ф.25.3: source URL linking (template-based, opt-in).
        if let Some(url) = source_url_for(&it.module_path, it.peer_file.as_deref(), w.line_of(span.start)) {
            w.field_str("url", &url);
        }
    });
    match &it.stability {
        None => w.field_null_or_str("stability", None),
        Some(s) => w.field_object("stability", |w| {
            // alphabetical: feature < note < since < tier
            w.field_null_or_str("feature", s.feature.as_deref());
            w.field_null_or_str("note", s.note.as_deref());
            w.field_null_or_str("since", s.since.as_deref());
            w.field_str("tier", s.tier.as_str());
        }),
    }
    w.field_null_or_str("summary", it.summary.as_deref());

    match &it.kind {
        ItemKind::Fn(sig) => {
            w.field_object("signature", |w| write_signature(w, sig));
        }
        ItemKind::Type(def) => {
            w.field_object("definition", |w| write_type_definition(w, def));
        }
        ItemKind::Const { ty, value } => {
            w.field_str("type", ty);
            w.field_str("value", value);
        }
        ItemKind::Effect { methods, axioms, handlers } => {
            // Plan 45 Ф.22.4 / D107: axioms эффекта (D24) — alphabetical.
            w.field_array("axioms", |w| {
                for ax in axioms {
                    w.array_object(|w| {
                        w.field_str("formula", &ax.formula);
                        w.field_str("name", &ax.name);
                    });
                }
            });
            // Plan 45 Ф.26.2 / Ф.23.4: handler matrix — alphabetical перед methods.
            w.field_array("handlers", |w| {
                for h in handlers {
                    w.array_object(|w| {
                        w.field_str("caller_item_id", &h.caller_item_id);
                        w.field_str("kind", &h.kind);
                    });
                }
            });
            w.field_array("methods", |w| {
                for m in methods {
                    w.array_object(|w| {
                        w.field_str("name", &m.name);
                        w.field_array("params", |w| {
                            for p in &m.params {
                                w.array_object(|w| write_param(w, p));
                            }
                        });
                        w.field_str("return_type", &m.return_type);
                    });
                }
            });
        }
        ItemKind::ReExport { .. } => {
            // Re-exports have no extra kind-specific fields; source is in reexport_from.
        }
        ItemKind::Protocol { methods, implementors } => {
            // Plan 45 Ф.23.16: implementors populated in workspace mode.
            w.field_array("implementors", |w| {
                for imp in implementors { w.array_str(imp); }
            });
            w.field_array("methods", |w| {
                for m in methods {
                    w.array_object(|w| {
                        w.field_str("name", &m.name);
                        w.field_array("params", |w| {
                            for p in &m.params {
                                w.array_object(|w| write_param(w, p));
                            }
                        });
                        w.field_str("return_type", &m.return_type);
                    });
                }
            });
        }
    }
}

fn write_signature(w: &mut JsonWriter, sig: &Signature) {
    // Plan 45 Ф.23.1 / D24/D106: contracts из AST (requires/ensures/ensures_fail/decreases).
    // Plan 45 Ф.23.2 / D106: verify_status per-function.
    w.field_object("contracts", |w| {
        let decreases: Vec<&ContractDoc> = sig.contracts.iter()
            .filter(|c| c.kind == "decreases").collect();
        let ensures: Vec<&ContractDoc> = sig.contracts.iter()
            .filter(|c| c.kind == "ensures").collect();
        let ensures_fail: Vec<&ContractDoc> = sig.contracts.iter()
            .filter(|c| c.kind == "ensures_fail").collect();
        let requires: Vec<&ContractDoc> = sig.contracts.iter()
            .filter(|c| c.kind == "requires").collect();
        w.field_array("decreases", |w| {
            for c in &decreases { w.array_str(&c.expr); }
        });
        w.field_array("ensures", |w| {
            for c in &ensures { w.array_str(&c.expr); }
        });
        w.field_array("ensures_fail", |w| {
            for c in &ensures_fail { w.array_str(&c.expr); }
        });
        w.field_array("requires", |w| {
            for c in &requires { w.array_str(&c.expr); }
        });
        // verify_status: {status: "...", counterexample: "..." | null}
        w.field_object("verify_status", |w| {
            match &sig.verify_status {
                VerifyStatus::NotAttempted => {
                    w.field_null_or_str("counterexample", None);
                    w.field_str("status", "not_attempted");
                }
                VerifyStatus::Proven => {
                    w.field_null_or_str("counterexample", None);
                    w.field_str("status", "proven");
                }
                VerifyStatus::HasCounterexample(msg) => {
                    w.field_str("counterexample", msg);
                    w.field_str("status", "has_counterexample");
                }
                VerifyStatus::Timeout => {
                    w.field_null_or_str("counterexample", None);
                    w.field_str("status", "timeout");
                }
            }
        });
    });
    // Plan 45 Ф.23.8: structured effect entries.
    w.field_array("effects", |w| {
        for e in &sig.effects {
            w.array_object(|w| {
                w.field_bool("is_row_var", e.is_row_var);
                w.field_str("name", &e.name);
                w.field_null_or_str("summary", e.summary.as_deref());
                w.field_null_or_str("target_id", e.target_id.as_deref());
            });
        }
    });
    w.field_array("generics", |w| {
        for g in &sig.generics {
            w.array_object(|w| {
                w.field_null_or_str("bound", g.bound.as_deref());
                w.field_null_or_str("default", g.default.as_deref());
                w.field_str("name", &g.name);
            });
        }
    });
    w.field_array("params", |w| {
        for p in &sig.params {
            w.array_object(|w| write_param(w, p));
        }
    });
    w.field_array("raises", |w| {
        for r in &sig.raises {
            w.array_str(r);
        }
    });
    match &sig.receiver {
        Some(r) => w.field_object("receiver", |w| {
            w.field_str(
                "kind",
                match r.kind {
                    ReceiverKind::Instance => "instance",
                    ReceiverKind::Static => "static",
                },
            );
            w.field_bool("mutable", r.mutable);
            w.field_str("type", &r.type_name);
        }),
        None => w.field_null_or_str("receiver", None),
    }
    // Plan 45 Ф.23.22: structural return type.
    write_structural_type(w, &sig.return_type);
    w.field_str("return_type", &sig.return_type);
}

fn write_param(w: &mut JsonWriter, p: &Param) {
    w.field_null_or_str("default", p.default.as_deref());
    w.field_bool("keyword_only", p.keyword_only);
    w.field_str("name", &p.name);
    // Plan 45 Ф.23.22: structural_type alongside source type string.
    write_structural_type(w, &p.ty);
    w.field_str("type", &p.ty);
    w.field_bool("variadic", p.variadic);
}

fn write_type_definition(w: &mut JsonWriter, def: &TypeDefinition) {
    match def {
        TypeDefinition::Record(fields) => {
            w.field_array("fields", |w| {
                for f in fields {
                    w.array_object(|w| {
                        w.field_bool("mutable", f.mutable);
                        w.field_str("name", &f.name);
                        // Plan 124.5 (D220/D222): emit `priv_field` для
                        // tooling-aware consumers (LSP / IDE).
                        w.field_bool("priv_field", f.priv_field);
                        w.field_str("type", &f.ty);
                    });
                }
            });
            w.field_str("kind", "record");
        }
        TypeDefinition::Sum(variants) => {
            w.field_str("kind", "sum");
            w.field_array("variants", |w| {
                for v in variants {
                    w.array_object(|w| {
                        w.field_str("name", &v.name);
                        match &v.payload {
                            VariantPayload::Unit => {
                                w.field_str("payload_kind", "unit");
                            }
                            VariantPayload::Tuple(tys) => {
                                w.field_str("payload_kind", "tuple");
                                w.field_array("types", |w| {
                                    for t in tys {
                                        w.array_str(t);
                                    }
                                });
                            }
                            VariantPayload::Record(fields) => {
                                w.field_array("fields", |w| {
                                    for f in fields {
                                        w.array_object(|w| {
                                            w.field_bool("mutable", f.mutable);
                                            w.field_str("name", &f.name);
                                            w.field_bool("priv_field", f.priv_field);
                                            w.field_str("type", &f.ty);
                                        });
                                    }
                                });
                                w.field_str("payload_kind", "record");
                            }
                        }
                    });
                }
            });
        }
        TypeDefinition::Alias(ty) => {
            w.field_str("aliased_type", ty);
            w.field_str("kind", "alias");
        }
        TypeDefinition::Newtype { inner } => {
            w.field_str("inner_type", inner);
            w.field_str("kind", "newtype");
        }
    }
}

/// Plan 45 Ф.23.22: parse a Nova type string into a structural JSON representation.
/// Emitted alongside every `type` field for LLM consumption.
/// Grammar (simplified): `[]T` = array, `?T` = optional, `(A, B)` = tuple,
/// `fn(A) -> B` = function, `named` = named type.
fn write_structural_type(w: &mut JsonWriter, ty: &str) {
    // Plan 45 Ф.24.6: use Nova parser to get accurate structural type.
    // Fallback to ad-hoc shape detector if parse fails.
    if let Ok(type_ref) = crate::parser::parse_type_str(ty) {
        write_typeref_structural(w, &type_ref);
        return;
    }
    write_structural_type_fallback(w, ty);
}

fn write_typeref_structural(w: &mut JsonWriter, ty: &crate::ast::TypeRef) {
    use crate::ast::TypeRef;
    match ty {
        TypeRef::Unit(_) => {
            w.field_object("structural_type", |w| {
                w.field_str("kind", "unit");
            });
        }
        TypeRef::Named { path, generics, .. } => {
            let name = path.last().cloned().unwrap_or_default();
            if generics.is_empty() {
                w.field_object("structural_type", |w| {
                    w.field_str("kind", "named");
                    w.field_str("name", &name);
                });
            } else {
                let gen_strs: Vec<String> = generics
                    .iter()
                    .map(|g| typeref_to_str(g))
                    .collect();
                w.field_object("structural_type", |w| {
                    w.field_str("kind", "named");
                    w.field_str("name", &name);
                    w.field_array("generics", |w| {
                        for g in &gen_strs {
                            w.array_str(g);
                        }
                    });
                });
            }
        }
        TypeRef::Array(inner, _) => {
            let inner_str = typeref_to_str(inner);
            w.field_object("structural_type", |w| {
                w.field_str("elem", &inner_str);
                w.field_str("kind", "array");
            });
        }
        TypeRef::FixedArray(len, elem, _) => {
            let elem_str = typeref_to_str(elem);
            w.field_object("structural_type", |w| {
                w.field_str("elem", &elem_str);
                w.field_str("kind", "fixed_array");
                w.field_str("len", &len.to_string());
            });
        }
        TypeRef::Tuple(elems, _) => {
            let elems_str: Vec<String> = elems.iter().map(typeref_to_str).collect();
            w.field_object("structural_type", |w| {
                w.field_str("kind", "tuple");
                w.field_array("elems", |w| {
                    for e in &elems_str {
                        w.array_str(e);
                    }
                });
            });
        }
        TypeRef::Func { params, effects, return_type, .. } => {
            let source = super::collector::render_type_for_doc(ty);
            w.field_object("structural_type", |w| {
                w.field_str("kind", "function");
                let _ = (params, effects, return_type);
                w.field_str("source", &source);
            });
        }
        // Plan 97 Ф.2 (D142): анонимный protocol-тип. JSON-схема —
        // kind=anon_protocol + список method-сигнатур (renderiroval'ная
        // строка). Для backward-compat и tooling'а — простой вариант.
        TypeRef::Protocol { methods, .. } => {
            let source = super::collector::render_type_for_doc(ty);
            w.field_object("structural_type", |w| {
                w.field_str("kind", "anon_protocol");
                w.field_str("source", &source);
                w.field_array("methods", |w| {
                    for m in methods {
                        let line = format!(
                            "{}{}/{}",
                            if m.is_static { "." } else { "" },
                            m.name,
                            m.params.len()
                        );
                        w.array_str(&line);
                    }
                });
            });
        }
        // D176 (Plan 108): readonly T — structural type like inner but with readonly kind.
        TypeRef::Readonly(inner, _) => {
            let source = super::collector::render_type_for_doc(ty);
            w.field_object("structural_type", |w| {
                w.field_str("kind", "readonly");
                w.field_str("source", &source);
                // Recurse into inner type for full structural info.
                write_typeref_structural(w, inner);
            });
        }
        // Plan 118 D216 §1 / Plan 118.5: typed pointer `*T` family.
        // V2 canonical form: bare Pointer = `*T` (ro); Mut/Unsafe are wrappers
        // that may wrap Pointer (e.g. `*mut T` = Mut(Pointer(T))) or other types.
        TypeRef::Pointer(inner, _) => {
            let source = super::collector::render_type_for_doc(ty);
            w.field_object("structural_type", |w| {
                w.field_str("kind", "pointer");
                w.field_str("modifier", "ro");
                w.field_str("source", &source);
                write_typeref_structural(w, inner);
            });
        }
        TypeRef::Mut(inner, _) => {
            let source = super::collector::render_type_for_doc(ty);
            w.field_object("structural_type", |w| {
                w.field_str("kind", "mut_wrap");
                w.field_str("source", &source);
                write_typeref_structural(w, inner);
            });
        }
        TypeRef::Unsafe(inner, _) => {
            let source = super::collector::render_type_for_doc(ty);
            w.field_object("structural_type", |w| {
                w.field_str("kind", "unsafe_wrap");
                w.field_str("source", &source);
                write_typeref_structural(w, inner);
            });
        }
    }
}

fn typeref_to_str(ty: &crate::ast::TypeRef) -> String {
    super::collector::render_type_for_doc(ty)
}

fn write_structural_type_fallback(w: &mut JsonWriter, ty: &str) {
    let ty = ty.trim();
    if ty.starts_with("[]") {
        w.field_object("structural_type", |w| {
            w.field_str("kind", "array");
            w.field_str("elem", &ty[2..]);
        });
    } else if ty.starts_with('?') {
        w.field_object("structural_type", |w| {
            w.field_str("kind", "optional");
            w.field_str("inner", &ty[1..]);
        });
    } else if ty.starts_with("fn(") || ty.starts_with("fn (") {
        w.field_object("structural_type", |w| {
            w.field_str("kind", "function");
            w.field_str("source", ty);
        });
    } else if ty == "()" {
        w.field_object("structural_type", |w| {
            w.field_str("kind", "unit");
        });
    } else if ty.starts_with('(') && ty.ends_with(')') {
        w.field_object("structural_type", |w| {
            w.field_str("kind", "tuple");
            w.field_str("source", ty);
        });
    } else {
        w.field_object("structural_type", |w| {
            let base = ty.split('[').next().unwrap_or(ty);
            w.field_str("kind", "named");
            w.field_str("name", base);
            if ty.contains('[') {
                w.field_str("source", ty);
            }
        });
    }
}

fn item_kind_str(k: &ItemKind) -> &'static str {
    match k {
        ItemKind::Fn(_) => "fn",
        ItemKind::Type(_) => "type",
        ItemKind::Const { .. } => "const",
        ItemKind::Effect { .. } => "effect",
        ItemKind::Protocol { .. } => "protocol",
        ItemKind::ReExport { .. } => "reexport",
    }
}

// ── Manual JSON writer ──────────────────────────────────────────────
//
// Гарантирует sorted alphabetical key-order (caller обязан вызывать
// field_* в порядке имён). Производит человекочитаемый 2-space indented
// output, deterministic byte-for-byte.

struct JsonWriter<'src> {
    out: String,
    indent: usize,
    /// Stack of "is this position the first field/elem?" — used to
    /// emit comma separators correctly.
    first_at_depth: Vec<bool>,
    /// Опциональный source text для line-mapping.
    source: Option<&'src str>,
}

impl<'src> JsonWriter<'src> {
    fn new(source: Option<&'src str>) -> Self {
        Self {
            out: String::new(),
            indent: 0,
            first_at_depth: Vec::new(),
            source,
        }
    }
    fn line_of(&self, offset: usize) -> u32 {
        match self.source {
            None => 1,
            Some(src) => crate::diag::byte_to_line_col(src, offset).0 as u32,
        }
    }
    fn finish(self) -> String {
        let mut s = self.out;
        if !s.ends_with('\n') {
            s.push('\n');
        }
        s
    }
    fn write_indent(&mut self) {
        for _ in 0..self.indent {
            self.out.push_str("  ");
        }
    }
    fn comma_if_needed(&mut self) {
        if let Some(first) = self.first_at_depth.last_mut() {
            if *first {
                *first = false;
            } else {
                self.out.push_str(",\n");
            }
        }
    }
    fn begin_object(&mut self) {
        self.out.push_str("{\n");
        self.indent += 1;
        self.first_at_depth.push(true);
    }
    fn end_object(&mut self) {
        self.first_at_depth.pop();
        self.out.push('\n');
        self.indent -= 1;
        self.write_indent();
        self.out.push('}');
    }
    fn begin_array(&mut self) {
        self.out.push_str("[\n");
        self.indent += 1;
        self.first_at_depth.push(true);
    }
    fn end_array(&mut self) {
        self.first_at_depth.pop();
        // если массив был пустой — без переноса
        self.out.push('\n');
        self.indent -= 1;
        self.write_indent();
        self.out.push(']');
    }
    fn write_field_key(&mut self, key: &str) {
        self.comma_if_needed();
        self.write_indent();
        let _ = write!(self.out, "\"{}\": ", json_escape(key));
    }
    fn field_str(&mut self, key: &str, value: &str) {
        self.write_field_key(key);
        let _ = write!(self.out, "\"{}\"", json_escape(value));
    }
    fn field_u32(&mut self, key: &str, value: u32) {
        self.write_field_key(key);
        let _ = write!(self.out, "{}", value);
    }
    fn field_bool(&mut self, key: &str, value: bool) {
        self.write_field_key(key);
        let _ = write!(self.out, "{}", value);
    }
    fn field_null_or_str(&mut self, key: &str, value: Option<&str>) {
        self.write_field_key(key);
        match value {
            None => self.out.push_str("null"),
            Some(s) => {
                let _ = write!(self.out, "\"{}\"", json_escape(s));
            }
        }
    }
    fn field_object<F: FnOnce(&mut Self)>(&mut self, key: &str, f: F) {
        self.write_field_key(key);
        self.begin_object();
        f(self);
        self.end_object();
    }
    fn field_array<F: FnOnce(&mut Self)>(&mut self, key: &str, f: F) {
        self.write_field_key(key);
        self.begin_array();
        f(self);
        self.end_array();
    }
    fn array_str(&mut self, value: &str) {
        self.comma_if_needed();
        self.write_indent();
        let _ = write!(self.out, "\"{}\"", json_escape(value));
    }
    fn array_object<F: FnOnce(&mut Self)>(&mut self, f: F) {
        self.comma_if_needed();
        self.write_indent();
        self.begin_object();
        f(self);
        self.end_object();
    }
}

/// Plan 45 Ф.9: вернуть значение для `generated_at` поля или `None`,
/// если оно должно быть опущено. Поведение:
/// - `NOVA_DOC_GENERATED_AT=<string>` → возвращаем как есть (caller'у
///   решать формат);
/// - `SOURCE_DATE_EPOCH=<seconds>` → конвертируем в простой ISO-8601-
///   подобный формат `YYYY-MM-DDTHH:MM:SSZ`;
/// - иначе → `None` (deterministic by default).
/// Plan 45 Ф.23.25: normalize source_root path for cross-platform portability.
/// - Backslashes → forward slashes.
/// - If NOVA_DOC_WORKSPACE_ROOT env var is set and root starts with that prefix,
///   replaces prefix with `${WORKSPACE_ROOT}` for machine-agnostic output.
fn normalize_source_root(root: &str) -> String {
    let normalized = root.replace('\\', "/");
    if let Ok(ws_root) = std::env::var("NOVA_DOC_WORKSPACE_ROOT") {
        let ws_norm = ws_root.replace('\\', "/");
        if !ws_norm.is_empty() {
            if normalized == ws_norm {
                return "${WORKSPACE_ROOT}".to_string();
            }
            let prefix_slash = format!("{}/", ws_norm);
            if let Some(rest) = normalized.strip_prefix(&prefix_slash) {
                return format!("${{WORKSPACE_ROOT}}/{}", rest);
            }
        }
    }
    normalized
}

/// Plan 45 Ф.25.3 — source URL для item'а.
///
/// Template берётся из env var `NOVA_DOC_SOURCE_URL_TEMPLATE`.
/// Placeholders:
/// - `{path}` — relative path к файлу от source-root (module_path + .nv,
///   или peer_file для folder-modules);
/// - `{line}` — line number (1-indexed).
///
/// Примеры templates:
/// - GitHub: `https://github.com/user/repo/blob/main/{path}#L{line}`
/// - GitLab: `https://gitlab.com/user/repo/-/blob/main/{path}#L{line}`
/// - Codeberg: `https://codeberg.org/user/repo/src/branch/main/{path}#L{line}`
///
/// Возвращает `None` если template не set — поле в JSON omits.
fn source_url_for(module_path: &[String], peer_file: Option<&str>, line: u32) -> Option<String> {
    let template = std::env::var("NOVA_DOC_SOURCE_URL_TEMPLATE").ok()?;
    if template.is_empty() {
        return None;
    }
    // Path: peer_file (если есть, для folder-modules) ИЛИ module_path.join('/') + .nv.
    let path = if let Some(pf) = peer_file {
        // Folder-module case: items'у явно атрибутирован peer-file.
        // Path = module_path/<peer_file>.
        format!("{}/{}", module_path.join("/"), pf)
    } else {
        format!("{}.nv", module_path.join("/"))
    };
    Some(
        template
            .replace("{path}", &path)
            .replace("{line}", &line.to_string()),
    )
}

fn generated_at_value() -> Option<String> {
    if let Ok(ts) = std::env::var("NOVA_DOC_GENERATED_AT") {
        if !ts.is_empty() {
            return Some(ts);
        }
    }
    if let Ok(s) = std::env::var("SOURCE_DATE_EPOCH") {
        if let Ok(secs) = s.parse::<i64>() {
            return Some(epoch_to_iso8601(secs));
        }
    }
    None
}

/// Конвертация Unix epoch в `YYYY-MM-DDTHH:MM:SSZ`. Простая
/// реализация без внешних зависимостей (UTC, no leap-seconds).
fn epoch_to_iso8601(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let hour = rem / 3600;
    let minute = (rem % 3600) / 60;
    let second = rem % 60;
    let (year, month, day) = civil_from_days(days);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hour, minute, second
    )
}

/// Howard Hinnant's `civil_from_days` алгоритм: Unix epoch days →
/// (year, month, day) в григорианском календаре.
fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };
    (year as i32, m as u32, d as u32)
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out
}
