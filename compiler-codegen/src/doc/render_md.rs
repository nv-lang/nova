//! Plan 45 Ф.8 — DocTree → Markdown renderer.
//!
//! MVP: один модуль = одна markdown-строка. Items в каноническом
//! порядке (по `id` — DocTree гарантирует sorted). Per-item rendering:
//! signature (для fn) / definition (для type) / value (для const) +
//! summary + description.
//!
//! Style guide (Plan 45 §11.5) — определяет section order; renderer
//! pass'ит через `sections` уже расположенные в правильном порядке
//! (Ф.5 `sections.rs` сделает разбиение; MVP — summary + body).

use crate::doc::doctree::*;
use std::fmt::Write;

pub fn render(tree: &DocTree) -> String {
    let mut out = String::new();
    for module in &tree.modules {
        render_module(module, &mut out);
    }
    out
}

fn render_module(m: &DocModule, out: &mut String) {
    let path = m.path.join(".");
    let _ = writeln!(out, "# {} ({})", m.name, path);
    let _ = writeln!(out);
    if m.kind == ModuleKind::Folder && !m.peers.is_empty() {
        let _ = writeln!(out, "*Folder-module peers:* {}", m.peers.join(", "));
        let _ = writeln!(out);
    }
    if let Some(s) = &m.summary {
        let _ = writeln!(out, "{}", s);
        let _ = writeln!(out);
    }
    if let Some(d) = &m.description {
        let _ = writeln!(out, "{}", d);
        let _ = writeln!(out);
    }

    // Группируем items по kind для удобного чтения.
    let fns: Vec<&DocItem> = m.items.iter()
        .filter(|i| matches!(i.kind, ItemKind::Fn(_)))
        .collect();
    let types: Vec<&DocItem> = m.items.iter()
        .filter(|i| matches!(i.kind, ItemKind::Type(_)))
        .collect();
    let consts: Vec<&DocItem> = m.items.iter()
        .filter(|i| matches!(i.kind, ItemKind::Const { .. }))
        .collect();
    let effects: Vec<&DocItem> = m.items.iter()
        .filter(|i| matches!(i.kind, ItemKind::Effect { .. }))
        .collect();
    let protocols: Vec<&DocItem> = m.items.iter()
        .filter(|i| matches!(i.kind, ItemKind::Protocol { .. }))
        .collect();

    if !types.is_empty() {
        let _ = writeln!(out, "## Types");
        let _ = writeln!(out);
        for it in &types { render_item(it, out); }
    }
    if !effects.is_empty() {
        let _ = writeln!(out, "## Effects");
        let _ = writeln!(out);
        for it in &effects { render_item(it, out); }
    }
    if !protocols.is_empty() {
        let _ = writeln!(out, "## Protocols");
        let _ = writeln!(out);
        for it in &protocols { render_item(it, out); }
    }
    if !consts.is_empty() {
        let _ = writeln!(out, "## Constants");
        let _ = writeln!(out);
        for it in &consts { render_item(it, out); }
    }
    if !fns.is_empty() {
        let _ = writeln!(out, "## Functions");
        let _ = writeln!(out);
        for it in &fns { render_item(it, out); }
    }
}

fn render_item(it: &DocItem, out: &mut String) {
    let _ = writeln!(out, "### `{}`", it.name);
    let _ = writeln!(out);
    if let Some(d) = &it.deprecation {
        let since = d.since.as_deref().map(|s| format!(" (since {})", s)).unwrap_or_default();
        let _ = writeln!(out, "> **DEPRECATED{}**: {}", since, d.note);
        let _ = writeln!(out);
    }
    if let Some(s) = &it.stability {
        let since = s.since.as_deref().map(|v| format!(" since {}", v)).unwrap_or_default();
        let _ = writeln!(out, "> *Stability:* **{}**{}", s.tier.as_str(), since);
        let _ = writeln!(out);
    }
    // Signature / definition / value.
    match &it.kind {
        ItemKind::Fn(sig) => {
            let _ = writeln!(out, "```nova");
            let _ = writeln!(out, "{}", render_fn_signature(&it.name, sig));
            let _ = writeln!(out, "```");
            let _ = writeln!(out);
        }
        ItemKind::Type(def) => {
            let _ = writeln!(out, "```nova");
            let _ = writeln!(out, "{}", render_type_definition(&it.name, def));
            let _ = writeln!(out, "```");
            let _ = writeln!(out);
        }
        ItemKind::Const { ty, value } => {
            let _ = writeln!(out, "```nova");
            let _ = writeln!(out, "const {} {} = {}", it.name, ty, value);
            let _ = writeln!(out, "```");
            let _ = writeln!(out);
        }
        ItemKind::Effect { methods } => {
            let _ = writeln!(out, "```nova");
            let _ = writeln!(out, "type {} effect {{", it.name);
            for m in methods {
                let params = m.params.iter().map(render_param).collect::<Vec<_>>().join(", ");
                let _ = writeln!(out, "    fn {}({}) -> {}", m.name, params, m.return_type);
            }
            let _ = writeln!(out, "}}");
            let _ = writeln!(out, "```");
            let _ = writeln!(out);
        }
        ItemKind::Protocol { methods } => {
            let _ = writeln!(out, "```nova");
            let _ = writeln!(out, "type {} protocol {{", it.name);
            for m in methods {
                let params = m.params.iter().map(render_param).collect::<Vec<_>>().join(", ");
                let _ = writeln!(out, "    fn {}({}) -> {}", m.name, params, m.return_type);
            }
            let _ = writeln!(out, "}}");
            let _ = writeln!(out, "```");
            let _ = writeln!(out);
        }
    }
    if let Some(s) = &it.summary {
        let _ = writeln!(out, "{}", s);
        let _ = writeln!(out);
    }
    if let Some(d) = &it.description {
        let _ = writeln!(out, "{}", d);
        let _ = writeln!(out);
    }
    // Style-guide §11.5 fixed section order.
    const SECTION_ORDER: &[(&str, &str)] = &[
        ("examples", "Examples"),
        ("errors", "Errors"),
        ("panics", "Panics"),
        ("safety", "Safety"),
        ("effects", "Effects"),
        ("contracts", "Contracts"),
        ("since", "Since"),
        ("see also", "See also"),
        ("deprecated", "Deprecated"),
    ];
    for (key, title) in SECTION_ORDER {
        if let Some(body) = it.sections.get(*key) {
            let _ = writeln!(out, "#### {}", title);
            let _ = writeln!(out);
            let _ = writeln!(out, "{}", body);
            let _ = writeln!(out);
        }
    }
}

fn render_fn_signature(name: &str, sig: &Signature) -> String {
    let mut s = String::new();
    s.push_str("fn ");
    if let Some(r) = &sig.receiver {
        match r.kind {
            ReceiverKind::Instance => {
                if r.mutable {
                    let _ = write!(s, "{} mut @{}", r.type_name, name);
                } else {
                    let _ = write!(s, "{} @{}", r.type_name, name);
                }
            }
            ReceiverKind::Static => {
                let _ = write!(s, "{}.{}", r.type_name, name);
            }
        }
    } else {
        s.push_str(name);
    }
    if !sig.generics.is_empty() {
        let g = sig.generics.iter().map(|g| {
            let mut name = g.name.clone();
            if let Some(b) = &g.bound { let _ = write!(name, " {}", b); }
            if let Some(d) = &g.default { let _ = write!(name, " = {}", d); }
            name
        }).collect::<Vec<_>>().join(", ");
        let _ = write!(s, "[{}]", g);
    }
    let params = sig.params.iter().map(render_param).collect::<Vec<_>>().join(", ");
    let _ = write!(s, "({})", params);
    if !sig.effects.is_empty() {
        let _ = write!(s, " {}", sig.effects.join(" "));
    }
    let _ = write!(s, " -> {}", sig.return_type);
    s
}

fn render_param(p: &Param) -> String {
    let mut s = String::new();
    if p.variadic { s.push_str("..."); }
    let _ = write!(s, "{} {}", p.name, p.ty);
    if let Some(d) = &p.default {
        let _ = write!(s, " = {}", d);
    }
    s
}

fn render_type_definition(name: &str, def: &TypeDefinition) -> String {
    let mut s = String::new();
    match def {
        TypeDefinition::Record(fields) => {
            let _ = write!(s, "type {} {{ ", name);
            let fs = fields.iter().map(|f| {
                if f.mutable { format!("mut {} {}", f.name, f.ty) }
                else { format!("{} {}", f.name, f.ty) }
            }).collect::<Vec<_>>().join("; ");
            let _ = write!(s, "{} }}", fs);
        }
        TypeDefinition::Sum(variants) => {
            let _ = write!(s, "type {} =", name);
            for v in variants {
                match &v.payload {
                    VariantPayload::Unit => { let _ = write!(s, "\n    | {}", v.name); }
                    VariantPayload::Tuple(tys) => {
                        let _ = write!(s, "\n    | {}({})", v.name, tys.join(", "));
                    }
                    VariantPayload::Record(fields) => {
                        let fs = fields.iter().map(|f| format!("{} {}", f.name, f.ty)).collect::<Vec<_>>().join("; ");
                        let _ = write!(s, "\n    | {} {{ {} }}", v.name, fs);
                    }
                }
            }
        }
        TypeDefinition::Alias(ty) => {
            let _ = write!(s, "type {} = {}", name, ty);
        }
    }
    s
}
