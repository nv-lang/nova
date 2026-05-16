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
pub mod lints;
pub mod schema;
pub mod stability;
pub mod test_runner;
pub mod render_md;
pub mod render_json;
pub mod scraper;

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
    // Plan 45 Ф.23.2 / D106: verify_status per-fn wired through Plan 33.
    populate_verify_status(&mut tree, module);
    tree
}

/// Plan 45 Ф.21.7 — workspace-режим: построить unified DocTree из
/// нескольких модулей. Cross-module intra-doc-links резолвятся
/// корректно — links pass видит все items.
pub fn build_workspace(modules: &[Module]) -> DocTree {
    let mut tree = collector::collect_workspace(modules);
    strip_hidden_doc(&mut tree);
    links::resolve_intra_doc_links(&mut tree);
    doctests::collect_doc_tests(&mut tree);
    stability::propagate_stability(&mut tree);
    // In workspace mode — no single Module available; verify_status stays NotAttempted.
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
    // Plan 45 Ф.22.1: модули с `#hide_doc` отбрасываются целиком
    // (всех items включая children).
    tree.modules.retain(|m| !m.hide_doc);
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

/// Plan 45 Ф.23.12: run style-guide lints, return violations.
pub fn run_lints(tree: &DocTree) -> Vec<lints::DocLintViolation> {
    lints::run_lints(tree)
}

/// Plan 45 Ф.23.2 / D106: populate verify_status для fn items с contracts.
/// Запускает Plan 33 verify pipeline и обновляет Signature.verify_status.
fn populate_verify_status(tree: &mut DocTree, module: &Module) {
    use crate::ast::Item;
    use crate::verify::pipeline::{VerifyResult, VerificationPipeline};
    use crate::doc::doctree::VerifyStatus;

    // Collect per-fn verify results: fn_name → VerifyStatus.
    let pipeline = VerificationPipeline::new();
    let inferred_pure = crate::verify::pipeline::infer_pure_fns_scc(module);
    let mut fn_status: std::collections::HashMap<String, VerifyStatus> =
        std::collections::HashMap::new();

    for item in &module.items {
        if let Item::Fn(fd) = item {
            if fd.contracts.is_empty() {
                continue;
            }
            let results = pipeline.verify_fn(module, fd, &inferred_pure);
            let mut proven_all = true;
            let mut counterexample: Option<String> = None;
            for (_span, vr) in &results {
                match vr {
                    VerifyResult::Proven => {}
                    VerifyResult::Disproved(_model, msg) => {
                        proven_all = false;
                        counterexample = Some(msg.clone());
                        break;
                    }
                    VerifyResult::Unknown(_) | VerifyResult::EncodingFailed(_) => {
                        proven_all = false;
                    }
                }
            }
            let status = if results.is_empty() {
                VerifyStatus::NotAttempted
            } else if let Some(ce) = counterexample {
                VerifyStatus::HasCounterexample(ce)
            } else if proven_all {
                VerifyStatus::Proven
            } else {
                VerifyStatus::Timeout
            };
            fn_status.insert(fd.name.clone(), status);
        }
    }

    // Populate tree.
    for m in &mut tree.modules {
        for it in &mut m.items {
            if let doctree::ItemKind::Fn(sig) = &mut it.kind {
                if !sig.contracts.is_empty() {
                    // Match by fn name (last segment of item ID after '::').
                    let fn_name = it.id.rsplit("::").next().unwrap_or(&it.id);
                    // Method IDs have form "module::Type.method" — strip type prefix.
                    let fn_name = fn_name.rsplit('.').next().unwrap_or(fn_name);
                    if let Some(status) = fn_status.remove(fn_name) {
                        sig.verify_status = status;
                    }
                }
            }
        }
    }
}
