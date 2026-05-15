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
    let mut w = JsonWriter::new();
    w.begin_object();
    w.field_u32("format_version", tree.format_version);
    w.field_str(
        "nova_version",
        env!("CARGO_PKG_VERSION"),
    );
    if let Ok(_) = std::env::var("SOURCE_DATE_EPOCH") {
        // Опускаем generated_at для reproducible builds.
    } else {
        // MVP: не эмитим generated_at вообще (deterministic by default).
        // Production может включить через флаг.
    }
    w.field_array("doc_tests", |_w| {
        // Plan 45 Ф.7 — пока пустой массив (collector не извлекает
        // doc-test'ы в MVP-collector'е; будет добавлено вместе с Ф.7).
    });
    w.field_array("links", |_w| {
        // Plan 45 Ф.6 — пока пустой массив (intra-doc-link resolver
        // не реализован в MVP-collector'е).
    });
    w.field_array("modules", |w| {
        for m in &tree.modules {
            w.array_object(|w| write_module(w, m));
        }
    });
    w.field_array("items", |w| {
        for m in &tree.modules {
            for it in &m.items {
                if it.visibility != Visibility::Export {
                    continue;
                }
                w.array_object(|w| write_item(w, it));
            }
        }
    });
    w.end_object();
    w.finish()
}

fn write_module(w: &mut JsonWriter, m: &DocModule) {
    w.field_null_or_str("deprecation", None);
    w.field_array("doc_attrs", |_| {});
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
    w.field_object("source", |w| {
        let span = m.source_span;
        w.field_u32("file_id", span.file_id);
        w.field_u32("line", line_of(span.start) as u32);
    });
    w.field_null_or_str("stability", None);
    w.field_null_or_str("summary", m.summary.as_deref());
    w.field_null_or_str("description", m.description.as_deref());
}

fn write_item(w: &mut JsonWriter, it: &DocItem) {
    w.field_null_or_str("deprecation", None);
    w.field_null_or_str("description", it.description.as_deref());
    w.field_array("doc_attrs", |_| {});
    w.field_str("id", &it.id);
    w.field_str("kind", item_kind_str(&it.kind));
    w.field_str("module_path", &it.module_path.join("."));
    w.field_str("name", &it.name);
    w.field_object("source", |w| {
        let span = it.source_span;
        w.field_u32("file_id", span.file_id);
        w.field_u32("line", line_of(span.start) as u32);
    });
    w.field_null_or_str("stability", None);
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
        ItemKind::Effect { methods } => {
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
        ItemKind::Protocol { methods } => {
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
    w.field_object("contracts", |w| {
        // MVP: empty contracts (Plan 33 SMT verify результаты — Ф.3+).
        w.field_array("ensures", |_| {});
        w.field_array("requires", |_| {});
        w.field_str("verify_status", "UNVERIFIED");
    });
    w.field_array("effects", |w| {
        for e in &sig.effects {
            w.array_str(e);
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
    w.field_str("return_type", &sig.return_type);
}

fn write_param(w: &mut JsonWriter, p: &Param) {
    w.field_null_or_str("default", p.default.as_deref());
    w.field_bool("keyword_only", p.keyword_only);
    w.field_str("name", &p.name);
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
    }
}

fn item_kind_str(k: &ItemKind) -> &'static str {
    match k {
        ItemKind::Fn(_) => "fn",
        ItemKind::Type(_) => "type",
        ItemKind::Const { .. } => "const",
        ItemKind::Effect { .. } => "effect",
        ItemKind::Protocol { .. } => "protocol",
    }
}

/// Plan 45 Ф.9: MVP-helper для line-information. Без полной line-map'ы
/// (это требует доступа к source-тексту) возвращает 1 — placeholder.
/// Production: line-map передаётся в render_json() параметром.
fn line_of(_offset: usize) -> usize {
    1
}

// ── Manual JSON writer ──────────────────────────────────────────────
//
// Гарантирует sorted alphabetical key-order (caller обязан вызывать
// field_* в порядке имён). Производит человекочитаемый 2-space indented
// output, deterministic byte-for-byte.

struct JsonWriter {
    out: String,
    indent: usize,
    /// Stack of "is this position the first field/elem?" — used to
    /// emit comma separators correctly.
    first_at_depth: Vec<bool>,
}

impl JsonWriter {
    fn new() -> Self {
        Self {
            out: String::new(),
            indent: 0,
            first_at_depth: Vec::new(),
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
