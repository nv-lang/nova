//! Plan 45 / D104-D107: `nova doc` documentation tooling.
//!
//! Главные публичные точки:
//! - [`build`]: запускает collector + passes на парсенном `Module`,
//!   возвращает `DocTree` (typed IR для рендеринга).
//! - [`render_markdown`], [`render_json`]: преобразуют `DocTree` в
//!   соответствующий формат.
//!
//! Архитектура — passes-based IR transformation (см. Plan 45 §3).
//! Этот модуль организован как:
//!
//! - `doctree`: типы IR (`DocTree`, `DocItem`, `Signature` и пр.).
//! - `collector`: AST + type-checker info → raw `DocTree`.
//! - `markdown`: парсинг markdown-content (summary extraction,
//!   section recognition).
//! - `render_md` / `render_json`: рендеринг в форматы вывода.
//!
//! **MVP-scope:** Plan 45 MVP (см. §12.0 плана) фокусируется на
//! parser + DocModel + markdown + intra-links + doc-tests +
//! Markdown/JSON renderers + CLI. HTML/man/diff/cache — Plan 45.A.

pub mod doctree;
pub mod collector;
pub mod markdown;
pub mod links;
pub mod doctests;
pub mod schema;
pub mod stability;
pub mod test_runner;
pub mod render_md;
pub mod render_json;

pub use doctree::{DocTree, DocModule, DocItem, ItemKind, Signature, Visibility};

use crate::ast::Module;

/// Plan 45 Ф.4 — Public API: построить `DocTree` из парсенного `Module`.
///
/// **MVP:** только collector (одно прохождение AST → doctree). Passes
/// (`strip_private`, `resolve_intra_doc_links`, `collect_doc_tests`,
/// `propagate_stability` и пр.) — отдельные фазы Plan 45, добавляются
/// инкрементально.
///
/// На вход — уже type-checked `Module` (caller гарантирует, что
/// `types::check_module(&module)` прошёл; collector ассамит, что
/// сигнатуры валидны).
pub fn build(module: &Module) -> DocTree {
    let mut tree = collector::collect(module);
    // `#hide_doc` фильтруется до link-resolution, чтобы ссылки на скрытые
    // items сразу попадали в broken-list.
    strip_hidden_doc(&mut tree);
    links::resolve_intra_doc_links(&mut tree);
    doctests::collect_doc_tests(&mut tree);
    stability::propagate_stability(&mut tree);
    tree
}

/// Plan 45 Ф.12: pass `strip_private` — отбрасывает items с
/// `visibility = Private`. Применять, если `--include-private` НЕ задан.
pub fn strip_private(tree: &mut DocTree) {
    for m in &mut tree.modules {
        m.items.retain(|it| it.visibility == doctree::Visibility::Export);
    }
}

/// Plan 45 Ф.3 / D105: pass `strip_hidden_doc` — отбрасывает items с
/// `#hide_doc` attr. Применяется ВСЕГДА (это explicit сигнал автора).
pub fn strip_hidden_doc(tree: &mut DocTree) {
    for m in &mut tree.modules {
        m.items.retain(|it| !it.hide_doc);
    }
}

/// Plan 45 Ф.8 — DocTree → Markdown.
pub fn render_markdown(tree: &DocTree) -> String {
    render_md::render(tree)
}

/// Plan 45 Ф.9 — DocTree → JSON (D107 schema v1).
pub fn render_json(tree: &DocTree) -> String {
    render_json::render(tree)
}

/// Plan 45 Ф.9 — DocTree → JSON с source text для точной line-info.
pub fn render_json_with_source(tree: &DocTree, source: &str) -> String {
    render_json::render_with_source(tree, Some(source))
}
