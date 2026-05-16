//! Plan 45 Ф.8 — DocTree → Markdown renderer (production-grade after Ф.23+Ф.25).
//!
//! Один модуль = одна markdown-строка. Items в каноническом порядке (по `id` —
//! DocTree гарантирует sorted). Per-item rendering: signature (для fn) /
//! definition (для type) / value (для const) + summary + description +
//! sections (canonical order через `markdown::split_sections`).
//!
//! **Production features:**
//! - Stability/deprecation/capability badges (Ф.23.3+Ф.23.15)
//! - Intra-doc-link anchor rewriting (Ф.23.14)
//! - `[src]` URL links (Ф.25.3) через thread-local source
//! - Verify-status badges (Ф.24.14: ✅/❌/⏱️/⚠️)
//! - Handler matrix section (Ф.26.2): `#### Handlers` под Effect

use crate::doc::doctree::*;
use std::fmt::Write;

pub fn render(tree: &DocTree) -> String {
    render_with_source(tree, None)
}

/// Plan 45 Ф.25.3 — render с optional source text для точных line numbers
/// в `[src]` URL'ах. Source — это текст single-file (для single-file mode);
/// для workspace mode currently передаём None и используем placeholder.
pub fn render_with_source(tree: &DocTree, source: Option<&str>) -> String {
    // Plan 45 Ф.23.14: build link→anchor map for intra-doc-link rewriting.
    let link_map = build_link_map(&tree.links);
    // Plan 45 Ф.25.3: установить thread-local source для line resolution.
    set_render_source(source);
    let mut out = String::new();
    for module in &tree.modules {
        render_module(module, &link_map, &mut out);
    }
    clear_render_source();
    out
}

// Plan 45 Ф.25.3: thread-local source для byte→line conversion в [src] links.
// Альтернатива — protokol-rewrite render_item signature, но source нужен
// только в одном месте, а render иерархия глубокая.
thread_local! {
    static RENDER_SOURCE: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

fn set_render_source(src: Option<&str>) {
    RENDER_SOURCE.with(|s| *s.borrow_mut() = src.map(|x| x.to_string()));
}

fn clear_render_source() {
    RENDER_SOURCE.with(|s| *s.borrow_mut() = None);
}

fn line_of_byte(byte_offset: u32) -> u32 {
    RENDER_SOURCE.with(|s| {
        let src = s.borrow();
        match src.as_ref() {
            Some(text) => {
                let (line, _col) = crate::diag::byte_to_line_col(text, byte_offset as usize);
                line as u32
            }
            None => 1,
        }
    })
}

/// Build a map from link-text → anchor href for resolved links.
/// Anchor = `#<slug>-<kind>` where slug = last segment of target_id lowercased with `.` → `-`.
fn build_link_map(links: &[DocLink]) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    for l in links {
        if let Some(tid) = &l.target_id {
            // Derive anchor from target_id: "mod.path::Type.method" → "type-method"
            let last = tid.rsplit("::").next().unwrap_or(tid);
            let slug = last.to_lowercase().replace('.', "-");
            map.insert(l.text.clone(), format!("#{}", slug));
        }
    }
    map
}

/// Rewrite `[Name]` intra-doc links in markdown text to `[Name](#anchor)`.
fn rewrite_links(text: &str, link_map: &std::collections::HashMap<String, String>) -> String {
    if link_map.is_empty() || !text.contains('[') {
        return text.to_string();
    }
    let mut result = String::with_capacity(text.len() + 64);
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'[' {
            // Find closing ].
            let mut j = i + 1;
            while j < bytes.len() && bytes[j] != b']' && bytes[j] != b'\n' {
                j += 1;
            }
            if j < bytes.len() && bytes[j] == b']' {
                let inner = &text[i + 1..j];
                let next = bytes.get(j + 1).copied();
                // Only rewrite bare [Name] (not [text](url) or [text][ref]).
                if !matches!(next, Some(b'(') | Some(b'[')) {
                    if let Some(anchor) = link_map.get(inner) {
                        result.push('[');
                        result.push_str(inner);
                        result.push(']');
                        result.push('(');
                        result.push_str(anchor);
                        result.push(')');
                        i = j + 1;
                        continue;
                    }
                }
            }
        }
        result.push(bytes[i] as char);
        i += 1;
    }
    result
}

fn render_module(m: &DocModule, link_map: &std::collections::HashMap<String, String>, out: &mut String) {
    let path = m.path.join(".");
    let _ = writeln!(out, "# {} ({})", m.name, path);
    let _ = writeln!(out);
    if m.kind == ModuleKind::Folder && !m.peers.is_empty() {
        let _ = writeln!(out, "*Folder-module peers:* {}", m.peers.join(", "));
        let _ = writeln!(out);
    }
    if let Some(s) = &m.summary {
        let _ = writeln!(out, "{}", rewrite_links(s, link_map));
        let _ = writeln!(out);
    }
    if let Some(d) = &m.description {
        let _ = writeln!(out, "{}", rewrite_links(d, link_map));
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
        for it in &types { render_item(it, link_map, out); }
    }
    if !effects.is_empty() {
        let _ = writeln!(out, "## Effects");
        let _ = writeln!(out);
        for it in &effects { render_item(it, link_map, out); }
    }
    if !protocols.is_empty() {
        let _ = writeln!(out, "## Protocols");
        let _ = writeln!(out);
        for it in &protocols { render_item(it, link_map, out); }
    }
    if !consts.is_empty() {
        let _ = writeln!(out, "## Constants");
        let _ = writeln!(out);
        for it in &consts { render_item(it, link_map, out); }
    }
    if !fns.is_empty() {
        let _ = writeln!(out, "## Functions");
        let _ = writeln!(out);
        for it in &fns { render_item(it, link_map, out); }
    }
}

fn render_item(it: &DocItem, link_map: &std::collections::HashMap<String, String>, out: &mut String) {
    // Plan 45 Ф.25.3: heading с опциональным `[src]` link.
    let line = line_of_byte(it.source_span.start as u32);
    if let Some(url) = source_url_for_md(&it.module_path, it.peer_file.as_deref(), line) {
        let _ = writeln!(out, "### `{}` [\\[src\\]]({})", it.name, url);
    } else {
        let _ = writeln!(out, "### `{}`", it.name);
    }
    let _ = writeln!(out);
    if let Some(d) = &it.deprecation {
        let since = d.since.as_deref().map(|s| format!(" since {}", s)).unwrap_or_default();
        let until = d.until.as_deref().map(|u| format!(" (until {})", u)).unwrap_or_default();
        let _ = writeln!(out, "> **DEPRECATED{}{}**: {}", since, until, d.note);
        let _ = writeln!(out);
    }
    if let Some(s) = &it.stability {
        let since = s.since.as_deref().map(|v| format!(" since {}", v)).unwrap_or_default();
        // Plan 45 Ф.23.15: show feature/note for unstable/experimental.
        let extra = match s.tier {
            StabilityTier::Unstable => s.feature.as_deref()
                .map(|f| format!("({})", f))
                .unwrap_or_default(),
            StabilityTier::Experimental => s.note.as_deref()
                .map(|n| format!(": \"{}\"", n))
                .unwrap_or_default(),
            StabilityTier::Stable => String::new(),
        };
        let _ = writeln!(out, "> *Stability:* **{}**{}{}", s.tier.as_str(), extra, since);
        let _ = writeln!(out);
    }
    // Plan 45 Ф.23.20: [internal] badge for private items when rendered.
    if it.visibility == Visibility::Private {
        let _ = writeln!(out, "> *[internal]*");
        let _ = writeln!(out);
    }
    // Plan 45 Ф.22.6 / D105: aliases из `#doc_alias(...)` — отображаются
    // как search-hints для IDE. Показываем только если есть.
    if !it.aliases.is_empty() {
        let _ = writeln!(out, "*Also known as:* {}", it.aliases.iter().map(|a| format!("`{}`", a)).collect::<Vec<_>>().join(", "));
        let _ = writeln!(out);
    }
    // Plan 45 Ф.23.3: capability badges над signature.
    {
        let cap = &it.capabilities;
        let mut badges: Vec<String> = Vec::new();
        if cap.realtime_nogc { badges.push("⏱ `realtime nogc`".to_string()); }
        else if cap.realtime { badges.push("⏱ `realtime`".to_string()); }
        if cap.pure_fn { badges.push("🧊 `pure`".to_string()); }
        for f in &cap.forbid { badges.push(format!("🚫 `forbid({})`", f)); }
        // Plan 45 Ф.26.3 / D63: allow_transit badges.
        for e in &cap.allow_transit { badges.push(format!("📤 `allow_transit({})`", e)); }
        if !badges.is_empty() {
            let _ = writeln!(out, "{}", badges.join(" "));
            let _ = writeln!(out);
        }
    }
    // Signature / definition / value.
    match &it.kind {
        ItemKind::Fn(sig) => {
            let _ = writeln!(out, "```nova");
            let _ = writeln!(out, "{}", render_fn_signature(&it.name, sig));
            let _ = writeln!(out, "```");
            let _ = writeln!(out);
            // Plan 45 Ф.24.14: verification badges — detailed Z3 badges.
            // ✅ proven by Z3 / ⚠️ unverified / ❌ counterexample: <desc> / ⏱ timeout.
            let has_contracts = !sig.contracts.is_empty();
            match &sig.verify_status {
                VerifyStatus::Proven => {
                    let _ = writeln!(out, "> ✅ **proven by Z3**");
                    let _ = writeln!(out);
                }
                VerifyStatus::HasCounterexample(desc) => {
                    if desc.is_empty() {
                        let _ = writeln!(out, "> ❌ **counterexample found**");
                    } else {
                        let _ = writeln!(out, "> ❌ **counterexample:** {}", desc);
                    }
                    let _ = writeln!(out);
                }
                VerifyStatus::Timeout => {
                    let _ = writeln!(out, "> ⏱ **verify timeout** (Z3 exceeded limit)");
                    let _ = writeln!(out);
                }
                VerifyStatus::NotAttempted => {
                    // Show ⚠️ unverified only when there are contracts to verify.
                    if has_contracts {
                        let _ = writeln!(out, "> ⚠️ **unverified** (contracts present but not checked)");
                        let _ = writeln!(out);
                    }
                }
            }
            // Plan 45 Ф.23.1: Contracts section.
            if !sig.contracts.is_empty() {
                let _ = writeln!(out, "#### Contracts");
                let _ = writeln!(out);
                for c in &sig.contracts {
                    let _ = writeln!(out, "- `{}` {}", c.kind, c.expr);
                }
                let _ = writeln!(out);
            }
            // Plan 45 Ф.23.8: Effects auto-section.
            let non_fail_effects: Vec<_> = sig.effects.iter()
                .filter(|e| !e.name.starts_with("Fail["))
                .collect();
            if !non_fail_effects.is_empty() {
                let _ = writeln!(out, "#### Effects");
                let _ = writeln!(out);
                for e in &non_fail_effects {
                    if e.is_row_var {
                        let _ = writeln!(out, "- {} *(effect row-variable)*", e.name);
                    } else {
                        let _ = writeln!(out, "- `{}`", e.name);
                    }
                }
                let _ = writeln!(out);
            }
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
        ItemKind::Effect { methods, axioms, handlers } => {
            let _ = writeln!(out, "```nova");
            let _ = writeln!(out, "type {} effect {{", it.name);
            for m in methods {
                let params = m.params.iter().map(render_param).collect::<Vec<_>>().join(", ");
                let _ = writeln!(out, "    fn {}({}) -> {}", m.name, params, m.return_type);
            }
            for ax in axioms {
                let _ = writeln!(out, "    axiom {} => {}", ax.name, ax.formula);
            }
            let _ = writeln!(out, "}}");
            let _ = writeln!(out, "```");
            let _ = writeln!(out);
            // Plan 45 Ф.26.2 / Ф.23.4: handler matrix section.
            if !handlers.is_empty() {
                let _ = writeln!(out, "#### Handlers");
                let _ = writeln!(out);
                for h in handlers {
                    let _ = writeln!(out, "- `{}` ({})", h.caller_item_id, h.kind);
                }
                let _ = writeln!(out);
            }
        }
        ItemKind::Protocol { methods, implementors } => {
            let _ = writeln!(out, "```nova");
            let _ = writeln!(out, "type {} protocol {{", it.name);
            for m in methods {
                let params = m.params.iter().map(render_param).collect::<Vec<_>>().join(", ");
                let _ = writeln!(out, "    fn {}({}) -> {}", m.name, params, m.return_type);
            }
            let _ = writeln!(out, "}}");
            let _ = writeln!(out, "```");
            let _ = writeln!(out);
            // Plan 45 Ф.23.16: implementors section.
            if !implementors.is_empty() {
                let _ = writeln!(out, "#### Implementors");
                let _ = writeln!(out);
                for imp in implementors {
                    let _ = writeln!(out, "- `{}`", imp);
                }
                let _ = writeln!(out);
            }
        }
        // Plan 45 Ф.24.11: re-export rendering.
        ItemKind::ReExport { source } => {
            if it.doc_inline {
                let _ = writeln!(out, "*Inlined re-export of `{}`.*", source);
            } else {
                let _ = writeln!(out, "*Re-exported from [`{}`]({}).*", source, source);
            }
            let _ = writeln!(out);
        }
    }
    if let Some(s) = &it.summary {
        let _ = writeln!(out, "{}", rewrite_links(s, link_map));
        let _ = writeln!(out);
    }
    if let Some(d) = &it.description {
        let _ = writeln!(out, "{}", rewrite_links(d, link_map));
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

/// Plan 45 Ф.25.3 — source URL для MD `[src]` link.
/// Использует тот же template что и JSON renderer (NOVA_DOC_SOURCE_URL_TEMPLATE).
fn source_url_for_md(module_path: &[String], peer_file: Option<&str>, line: u32) -> Option<String> {
    let template = std::env::var("NOVA_DOC_SOURCE_URL_TEMPLATE").ok()?;
    if template.is_empty() {
        return None;
    }
    let path = if let Some(pf) = peer_file {
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
        let effect_names: Vec<&str> = sig.effects.iter().map(|e| e.name.as_str()).collect();
        let _ = write!(s, " {}", effect_names.join(" "));
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
        TypeDefinition::Newtype { inner } => {
            let _ = write!(s, "type {} = newtype {}", name, inner);
        }
    }
    s
}
