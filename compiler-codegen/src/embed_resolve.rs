//! Plan 186 / D412 — компайл-тайм интринсик `embed("path")`.
//!
//! AST-пасс: каждый вызов `embed("relative/path")` заменяется на
//! `ExprKind::HexBlobLit(<байты файла>)` — дальше по конвейеру (чекер,
//! codegen) блоб неотличим от hex-литерала `x"…"` (единая точка
//! материализации, см. emit_c). Запускается СРАЗУ после
//! resolve_imports_inline (пути peer-файлов уже известны), ДО type-check —
//! чекер никогда не видит вызов неизвестной fn `embed`.
//!
//! Правила (D412):
//! - аргумент — ТОЛЬКО строковый литерал (`E_EMBED_ARG_NOT_STR_LITERAL`);
//! - путь разрешается относительно .nv-файла, где стоит вызов (модель
//!   Rust `include_bytes!`); файл вызова определяется по `span.file_id`
//!   через `module.peer_files`, fallback — entry-файл;
//! - выход из дерева проекта наружу — ошибка (`E_EMBED_OUTSIDE_PROJECT`);
//! - файл не найден / не читается — `E_EMBED_NOT_FOUND`;
//! - список встроенных файлов возвращается caller'у: nova-cli включает
//!   их содержимое в fingerprint кэша сборки (build_cache::compute_c_key) —
//!   правка встроенного файла инвалидирует кэш.

use crate::ast::*;
use crate::diag::{Diagnostic, FileId};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Прогоняет embed-резолюцию по модулю. `Ok(files)` — список успешно
/// встроенных файлов (canonical paths, для fingerprint пересборки);
/// `Err(diags)` — хотя бы один вызов не разрешился.
pub fn resolve_embeds(
    module: &mut Module,
    entry_path: &Path,
    project_root: &Path,
) -> Result<Vec<PathBuf>, Vec<Diagnostic>> {
    // Карта file_id → каталог файла (для относительного резолва).
    let mut dirs: HashMap<FileId, PathBuf> = HashMap::new();
    for pf in &module.peer_files {
        if let Some(parent) = pf.path.parent() {
            dirs.insert(pf.file_id, parent.to_path_buf());
        }
    }
    let entry_dir = entry_path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    // Canonical project root — выходы за его пределы запрещены. Если
    // canonicalize не удался (виртуальный корень) — проверка мягко
    // пропускается (файл всё равно должен существовать).
    let canon_root = project_root.canonicalize().ok();

    let mut ctx = EmbedCtx {
        dirs,
        entry_dir,
        canon_root,
        files: Vec::new(),
        diags: Vec::new(),
    };
    for item in &mut module.items {
        ctx.walk_item(item);
    }
    for pf in &mut module.peer_files {
        for item in &mut pf.items_here {
            ctx.walk_item(item);
        }
    }
    if ctx.diags.is_empty() {
        // Дедуп: один файл может быть встроен многократно.
        ctx.files.sort();
        ctx.files.dedup();
        Ok(ctx.files)
    } else {
        // Дедуп диагностик: module.items и peer_files[].items_here — КОПИИ
        // одного AST (Plan 42.4), один битый вызов виден дважды.
        let mut seen: std::collections::HashSet<(crate::diag::Span, String)> =
            std::collections::HashSet::new();
        ctx.diags
            .retain(|d| seen.insert((d.span, d.message.clone())));
        Err(ctx.diags)
    }
}

struct EmbedCtx {
    dirs: HashMap<FileId, PathBuf>,
    entry_dir: PathBuf,
    canon_root: Option<PathBuf>,
    files: Vec<PathBuf>,
    diags: Vec<Diagnostic>,
}

impl EmbedCtx {
    fn base_dir(&self, file_id: FileId) -> &Path {
        self.dirs
            .get(&file_id)
            .map(|p| p.as_path())
            .unwrap_or(self.entry_dir.as_path())
    }

    /// Если `e` — вызов `embed(...)`, заменить его на HexBlobLit (или
    /// накопить диагностику). Возвращает true, если узел был embed-вызовом
    /// (детей у него больше нет — рекурсия не нужна).
    fn try_replace_embed(&mut self, e: &mut Expr) -> bool {
        let ExprKind::Call { func, args, trailing } = &e.kind else {
            return false;
        };
        let ExprKind::Ident(name) = &func.kind else {
            return false;
        };
        if name != "embed" {
            return false;
        }
        if trailing.is_some() || args.len() != 1 {
            self.diags.push(Diagnostic::new(
                "[E_EMBED_ARG_NOT_STR_LITERAL] `embed` takes exactly one string-literal \
                 argument: embed(\"relative/path\") (D412)",
                e.span,
            ));
            return true;
        }
        let rel: String = match &args[0] {
            CallArg::Item(a) => match &a.kind {
                ExprKind::StrLit(s) => s.clone(),
                _ => {
                    self.diags.push(Diagnostic::new(
                        "[E_EMBED_ARG_NOT_STR_LITERAL] the argument of `embed` must be a \
                         string LITERAL — the path is resolved at compile time (D412)",
                        a.span,
                    ));
                    return true;
                }
            },
            _ => {
                self.diags.push(Diagnostic::new(
                    "[E_EMBED_ARG_NOT_STR_LITERAL] the argument of `embed` must be a \
                     plain string literal (no spread/named args) (D412)",
                    e.span,
                ));
                return true;
            }
        };
        let base = self.base_dir(e.span.file_id).to_path_buf();
        let candidate = base.join(&rel);
        let canon = match candidate.canonicalize() {
            Ok(c) => c,
            Err(_) => {
                self.diags.push(Diagnostic::new(
                    format!(
                        "[E_EMBED_NOT_FOUND] embedded file not found: `{}` (resolved \
                         relative to `{}`)",
                        rel,
                        base.display()
                    ),
                    e.span,
                ));
                return true;
            }
        };
        if let Some(root) = &self.canon_root {
            if !canon.starts_with(root) {
                self.diags.push(Diagnostic::new(
                    format!(
                        "[E_EMBED_OUTSIDE_PROJECT] embedded file `{}` escapes the project \
                         root `{}` — paths above the project tree are forbidden (D412)",
                        canon.display(),
                        root.display()
                    ),
                    e.span,
                ));
                return true;
            }
        }
        match std::fs::read(&canon) {
            Ok(bytes) => {
                e.kind = ExprKind::HexBlobLit(bytes);
                self.files.push(canon);
            }
            Err(err) => {
                self.diags.push(Diagnostic::new(
                    format!(
                        "[E_EMBED_NOT_FOUND] cannot read embedded file `{}`: {}",
                        canon.display(),
                        err
                    ),
                    e.span,
                ));
            }
        }
        true
    }

    fn walk_item(&mut self, item: &mut Item) {
        match item {
            Item::Fn(f) => match &mut f.body {
                FnBody::Expr(e) => self.walk_expr(e),
                FnBody::Block(b) => self.walk_block(b),
                FnBody::External => {}
            },
            Item::Const(c) => self.walk_expr(&mut c.value),
            Item::Let(l) => self.walk_expr(&mut l.value),
            Item::Test(t) => self.walk_block(&mut t.body),
            Item::Bench(b) => {
                for s in &mut b.setup {
                    self.walk_stmt(s);
                }
                self.walk_block(&mut b.measure_body);
                for s in &mut b.teardown {
                    self.walk_stmt(s);
                }
            }
            Item::Type(_) | Item::Lemma(_) => {}
        }
    }

    fn walk_block(&mut self, b: &mut Block) {
        for s in &mut b.stmts {
            self.walk_stmt(s);
        }
        if let Some(t) = &mut b.trailing {
            self.walk_expr(t);
        }
    }

    fn walk_stmt(&mut self, s: &mut Stmt) {
        match s {
            Stmt::Let(d) => self.walk_expr(&mut d.value),
            Stmt::Const(d) => self.walk_expr(&mut d.value),
            Stmt::Expr(e) => self.walk_expr(e),
            Stmt::Assign { target, value, .. } => {
                self.walk_expr(target);
                self.walk_expr(value);
            }
            Stmt::Return { value, .. } => {
                if let Some(v) = value {
                    self.walk_expr(v);
                }
            }
            Stmt::Throw { value, .. } => self.walk_expr(value),
            Stmt::Break(_) | Stmt::Continue(_) => {}
            Stmt::Defer { body, .. } => self.walk_expr(body),
            Stmt::ConsumeScope { init, body, .. } => {
                self.walk_expr(init);
                self.walk_block(body);
            }
            Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => self.walk_expr(expr),
            Stmt::Apply { .. } | Stmt::Calc { .. } | Stmt::Reveal { .. } => {}
            Stmt::TupleAssign { lhs, rhs, .. } => {
                for e in lhs {
                    self.walk_expr(e);
                }
                for e in rhs {
                    self.walk_expr(e);
                }
            }
        }
    }

    fn walk_expr(&mut self, e: &mut Expr) {
        if self.try_replace_embed(e) {
            return;
        }
        match &mut e.kind {
            ExprKind::MapLit { elems, .. } => {
                for me in elems.iter_mut() {
                    match me {
                        MapElem::Pair(k, v) => {
                            self.walk_expr(k);
                            self.walk_expr(v);
                        }
                        MapElem::Spread(x) => self.walk_expr(x),
                    }
                }
            }
            ExprKind::ArrayLit(elems) => {
                for el in elems.iter_mut() {
                    match el {
                        ArrayElem::Item(x) | ArrayElem::Spread(x) => self.walk_expr(x),
                    }
                }
            }
            ExprKind::TupleLit(elems) => {
                for x in elems.iter_mut() {
                    self.walk_expr(x);
                }
            }
            ExprKind::RecordLit { fields, .. } => {
                for f in fields.iter_mut() {
                    if let Some(v) = &mut f.value {
                        self.walk_expr(v);
                    }
                }
            }
            ExprKind::Call { func, args, trailing } => {
                self.walk_expr(func);
                for a in args.iter_mut() {
                    match a {
                        CallArg::Item(x) | CallArg::Spread(x) => self.walk_expr(x),
                        CallArg::Named { value, .. } => self.walk_expr(value),
                    }
                }
                if let Some(t) = trailing {
                    match t {
                        Trailing::Block(b) => self.walk_block(b),
                        Trailing::LegacyBlockWithParams(tb) => self.walk_block(&mut tb.body),
                        Trailing::Fn(sb) => match &mut sb.body {
                            FnBody::Expr(x) => self.walk_expr(x),
                            FnBody::Block(b) => self.walk_block(b),
                            FnBody::External => {}
                        },
                    }
                }
            }
            ExprKind::TurboFish { base, .. } => self.walk_expr(base),
            ExprKind::Try(x) | ExprKind::Bang(x) | ExprKind::RefArg(x) => self.walk_expr(x),
            ExprKind::Coalesce(a, b) => {
                self.walk_expr(a);
                self.walk_expr(b);
            }
            ExprKind::As(x, _) | ExprKind::Is(x, _) => self.walk_expr(x),
            ExprKind::Binary { left, right, .. } => {
                self.walk_expr(left);
                self.walk_expr(right);
            }
            ExprKind::Unary { operand, .. } => self.walk_expr(operand),
            ExprKind::Member { obj, .. } => self.walk_expr(obj),
            ExprKind::Index { obj, index } => {
                self.walk_expr(obj);
                self.walk_expr(index);
            }
            ExprKind::If { cond, then, else_ } => {
                self.walk_expr(cond);
                self.walk_block(then);
                if let Some(eb) = else_ {
                    match eb {
                        ElseBranch::Block(b) => self.walk_block(b),
                        ElseBranch::If(x) => self.walk_expr(x),
                    }
                }
            }
            ExprKind::IfLet { scrutinee, guard, then, else_, .. } => {
                self.walk_expr(scrutinee);
                if let Some(g) = guard {
                    self.walk_expr(g);
                }
                self.walk_block(then);
                if let Some(eb) = else_ {
                    match eb {
                        ElseBranch::Block(b) => self.walk_block(b),
                        ElseBranch::If(x) => self.walk_expr(x),
                    }
                }
            }
            ExprKind::Match { scrutinee, arms } => {
                self.walk_expr(scrutinee);
                for arm in arms.iter_mut() {
                    if let Some(g) = &mut arm.guard {
                        self.walk_expr(g);
                    }
                    match &mut arm.body {
                        MatchArmBody::Expr(x) => self.walk_expr(x),
                        MatchArmBody::Block(b) => self.walk_block(b),
                    }
                }
            }
            ExprKind::For { iter, body, invariants, decreases, .. } => {
                self.walk_expr(iter);
                self.walk_block(body);
                for inv in invariants {
                    self.walk_expr(inv);
                }
                if let Some(d) = decreases {
                    self.walk_expr(d);
                }
            }
            ExprKind::ParallelFor { iter, body, .. } => {
                self.walk_expr(iter);
                self.walk_block(body);
            }
            ExprKind::While { cond, body, invariants, decreases } => {
                self.walk_expr(cond);
                self.walk_block(body);
                for inv in invariants {
                    self.walk_expr(inv);
                }
                if let Some(d) = decreases {
                    self.walk_expr(d);
                }
            }
            ExprKind::WhileLet { scrutinee, guard, body, .. } => {
                self.walk_expr(scrutinee);
                if let Some(g) = guard {
                    self.walk_expr(g);
                }
                self.walk_block(body);
            }
            ExprKind::Loop { body, .. } => self.walk_block(body),
            ExprKind::Block(b) => self.walk_block(b),
            ExprKind::Spawn(x) => self.walk_expr(x),
            ExprKind::Detach(b) | ExprKind::Blocking(b) => self.walk_block(b),
            ExprKind::Supervised { body, cancel, deadline } => {
                self.walk_block(body);
                if let Some(c) = cancel {
                    self.walk_expr(c);
                }
                if let Some(dl) = deadline {
                    self.walk_expr(&mut dl.expr);
                }
            }
            ExprKind::Forbid { body, .. } | ExprKind::Realtime { body, .. } => {
                self.walk_block(body);
            }
            ExprKind::Throw(x) => self.walk_expr(x),
            ExprKind::Interrupt(opt) => {
                if let Some(x) = opt {
                    self.walk_expr(x);
                }
            }
            ExprKind::Range { start, end, .. } => {
                if let Some(s) = start {
                    self.walk_expr(s);
                }
                if let Some(x) = end {
                    self.walk_expr(x);
                }
            }
            ExprKind::InterpolatedStr { parts } => {
                for p in parts.iter_mut() {
                    if let InterpStrPart::Expr { expr: x, .. } = p {
                        self.walk_expr(x);
                    }
                }
            }
            ExprKind::TaggedTemplate { args, .. } => {
                for x in args.iter_mut() {
                    self.walk_expr(x);
                }
            }
            ExprKind::Lambda { body, .. } => self.walk_expr(body),
            ExprKind::ClosureLight { body, .. } => match body {
                ClosureBody::Expr(x) => self.walk_expr(x),
                ClosureBody::Block(b) => self.walk_block(b),
            },
            ExprKind::ClosureFull(sb) => match &mut sb.body {
                FnBody::Expr(x) => self.walk_expr(x),
                FnBody::Block(b) => self.walk_block(b),
                FnBody::External => {}
            },
            ExprKind::With { bindings, body } => {
                for b in bindings.iter_mut() {
                    self.walk_expr(&mut b.handler);
                }
                self.walk_block(body);
            }
            ExprKind::HandlerLit { methods, .. } | ExprKind::ProtocolLit { methods, .. } => {
                for m in methods.iter_mut() {
                    match &mut m.body {
                        HandlerMethodBody::Expr(x) => self.walk_expr(x),
                        HandlerMethodBody::Block(b) => self.walk_block(b),
                    }
                }
            }
            ExprKind::Select { arms } => {
                for arm in arms.iter_mut() {
                    match &mut arm.op {
                        SelectOp::Recv { chan, .. } => self.walk_expr(chan),
                        SelectOp::Send { chan, value } => {
                            self.walk_expr(chan);
                            self.walk_expr(value);
                        }
                        SelectOp::Default => {}
                    }
                    if let Some(g) = &mut arm.guard {
                        self.walk_expr(g);
                    }
                    self.walk_block(&mut arm.body);
                }
            }
            ExprKind::Forall { body, .. } | ExprKind::Exists { body, .. } => {
                self.walk_expr(body);
            }
            // Листовые — нет под-выражений.
            ExprKind::Ident(_)
            | ExprKind::Path(_)
            | ExprKind::SelfAccess
            | ExprKind::IntLit(_)
            | ExprKind::FloatLit(_)
            | ExprKind::BoolLit(_)
            | ExprKind::StrLit(_)
            | ExprKind::HexBlobLit(_)
            | ExprKind::CharLit(_)
            | ExprKind::UnitLit
            | ExprKind::NullPtrLit => {}
        }
    }
}
