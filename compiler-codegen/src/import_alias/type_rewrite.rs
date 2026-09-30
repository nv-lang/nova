//! Registry 221.1 #1444: `import m.{T as U}` names the TYPE `m.T` as `U` in
//! the IMPORTING file only -- the type half of #1419.
//!
//! The import resolver used to rename the type declaration itself (`T` became
//! `U` in the merged unit), so `m`'s own `T` and every other importer's `T`
//! named a type nobody declared: measured as `use of undeclared identifier
//! 'Nova_Crate'` (a peer importing `T` merged first) and `unknown type 'Box'
//! in record literal` (the alias merged first).
//!
//! Now the declaration keeps its name, and this pass -- run once, at the end
//! of import resolution, before anything reads a type name -- rewrites every
//! TYPE position of the importing file that names `U` to `T`: annotations,
//! parameters and returns, generic arguments, record literals `U { .. }`,
//! `U.Variant` / `U.method()` heads, `match` patterns, method receivers
//! `fn U @m()`, `impl` lists. Value positions are `alpha_rename`'s (#1419).
//!
//! A re-export alias (`export import m.{T as U}`, D29 "Re-export") is the
//! facade's PUBLIC name: a file importing `U` from the facade gets the same
//! rewrite, so `facade.U` is `m.T` and `m` itself keeps calling it `T`.
//!
//! A generic parameter of an item that happens to be called `U` shadows the
//! alias inside that item.

use crate::ast::*;
use std::collections::{HashMap, HashSet};

type Aliases = HashMap<String, ImportAliasRef>;

/// Rewrite type aliases of every file of `module` (both `items` and the
/// `peer_files` copies), recording each rewritten reference in
/// `Module::import_alias_refs` (the unused-import lint counts it as a use).
pub fn rewrite_type_aliases(module: &mut Module) {
    let types = type_names(module);
    let per_file = super::aliases_where(module, &|name| types.contains(name));
    if per_file.is_empty() {
        return;
    }
    let mut refs: HashMap<crate::diag::Span, ImportAliasRef> = HashMap::new();
    for item in &mut module.items {
        if let Some(a) = per_file.get(&super::item_file(item)) {
            Rw { aliases: a, refs: &mut refs }.item(item);
        }
    }
    for pf in &mut module.peer_files {
        for item in &mut pf.items_here {
            if let Some(a) = per_file.get(&super::item_file(item)) {
                Rw { aliases: a, refs: &mut refs }.item(item);
            }
        }
    }
    module.import_alias_refs.extend(refs);
}

/// Every declared type name of the unit (after the merge).
pub(crate) fn type_names(module: &Module) -> HashSet<&str> {
    module
        .items
        .iter()
        .filter_map(|it| match it {
            Item::Type(t) => Some(t.name.as_str()),
            _ => None,
        })
        .collect()
}

struct Rw<'a> {
    aliases: &'a Aliases,
    refs: &'a mut HashMap<crate::diag::Span, ImportAliasRef>,
}

impl<'a> Rw<'a> {
    /// `name` rewritten in place when it is an alias; `at` keys the record.
    fn name(&mut self, name: &mut String, at: crate::diag::Span) {
        if let Some(r) = self.aliases.get(name.as_str()) {
            *name = r.name.clone();
            self.refs.insert(at, r.clone());
        }
    }

    /// A type path: `U` or `facade.U` (the last segment names the type).
    fn type_path(&mut self, path: &mut [String], at: crate::diag::Span) {
        if let Some(last) = path.last_mut() {
            self.name(last, at);
        }
    }

    /// A value path head: `U.Variant`, `U.method` -- the first segment.
    fn head(&mut self, path: &mut [String], at: crate::diag::Span) {
        if path.len() >= 2 {
            self.name(&mut path[0], at);
        }
    }

    /// Run `f` with the aliases a generic parameter list shadows removed.
    fn scoped(&mut self, generics: &[GenericParam], f: impl FnOnce(&mut Rw<'_>)) {
        if generics.iter().any(|g| self.aliases.contains_key(&g.name)) {
            let mut narrowed = self.aliases.clone();
            for g in generics {
                narrowed.remove(&g.name);
            }
            f(&mut Rw { aliases: &narrowed, refs: &mut *self.refs });
        } else {
            f(self);
        }
    }

    fn item(&mut self, item: &mut Item) {
        match item {
            Item::Fn(f) => {
                let generics = f.generics.clone();
                self.scoped(&generics, |rw| rw.fn_decl(f));
            }
            Item::Type(t) => {
                let generics = t.generics.clone();
                self.scoped(&generics, |rw| rw.type_decl(t));
            }
            Item::Let(l) => self.let_decl(l),
            Item::Const(c) => self.const_decl(c),
            Item::Test(t) => self.block(&mut t.body),
            Item::Bench(b) => {
                self.stmts(&mut b.setup);
                self.block(&mut b.measure_body);
                self.stmts(&mut b.teardown);
                for g in &mut b.groups {
                    for c in &mut g.cases {
                        self.stmts(&mut c.setup);
                        self.block(&mut c.measure_body);
                        self.stmts(&mut c.teardown);
                    }
                }
            }
            Item::Lemma(l) => {
                let generics = l.generics.clone();
                self.scoped(&generics, |rw| {
                    rw.generics(&mut l.generics);
                    rw.params(&mut l.params);
                    rw.contracts(&mut l.contracts);
                    rw.fn_body(&mut l.body);
                });
            }
        }
    }

    fn fn_decl(&mut self, f: &mut FnDecl) {
        if let Some(r) = &mut f.receiver {
            let at = r.span;
            self.name(&mut r.type_name, at);
            self.tys(&mut r.generics);
            self.generics(&mut r.carrier_bounds);
            if let Some(t) = &mut r.receiver_ty {
                self.ty(t);
            }
        }
        self.generics(&mut f.generics);
        self.params(&mut f.params);
        self.tys(&mut f.effects);
        if let Some(t) = &mut f.return_type {
            self.ty(t);
        }
        let at = f.span;
        for p in &mut f.impl_protocols {
            self.name(p, at);
        }
        self.contracts(&mut f.contracts);
        for ft in f.reads.iter_mut().chain(f.modifies.iter_mut()) {
            match ft {
                FrameTarget::Whole(e) => self.expr(e),
                FrameTarget::Field { receiver, .. } => self.expr(receiver),
                FrameTarget::ArrayElem { array, index, .. } => {
                    self.expr(array);
                    self.expr(index);
                }
                FrameTarget::ArrayAll { array, .. } => self.expr(array),
            }
        }
        if let Some(d) = &mut f.decreases {
            self.expr(d);
        }
        self.fn_body(&mut f.body);
    }

    fn type_decl(&mut self, t: &mut TypeDecl) {
        self.generics(&mut t.generics);
        match &mut t.kind {
            TypeDeclKind::Record(fields) => self.record_fields(fields),
            TypeDeclKind::Sum(variants) => {
                for v in variants {
                    match &mut v.kind {
                        SumVariantKind::Unit => {}
                        SumVariantKind::Tuple(ts) => self.tys(ts),
                        SumVariantKind::Record(fields) => self.record_fields(fields),
                    }
                }
            }
            TypeDeclKind::Effect(methods) => self.effect_methods(methods),
            TypeDeclKind::Protocol { methods, embeds } => {
                self.effect_methods(methods);
                self.tys(embeds);
            }
            TypeDeclKind::Newtype(ty) | TypeDeclKind::Alias(ty) => self.ty(ty),
            TypeDeclKind::TypeSet(ts) => self.tys(ts),
            TypeDeclKind::NamedTuple(fields) => {
                for f in fields {
                    self.ty(&mut f.ty);
                    if let Some(d) = &mut f.default {
                        self.expr(d);
                    }
                }
            }
            TypeDeclKind::Opaque => {}
        }
        for c in &mut t.assoc_consts {
            if let Some(ty) = &mut c.ty {
                self.ty(ty);
            }
            self.expr(&mut c.value);
        }
        self.contracts(&mut t.invariants);
        for ax in &mut t.axioms {
            let generics = ax.generics.clone();
            self.scoped(&generics, |rw| {
                rw.generics(&mut ax.generics);
                for b in &mut ax.binders {
                    if let BinderType::Typed(ty) = &mut b.kind {
                        rw.ty(ty);
                    }
                }
                rw.expr(&mut ax.formula);
            });
        }
        let at = t.span;
        for p in &mut t.impl_protocols {
            self.name(p, at);
        }
    }

    fn record_fields(&mut self, fields: &mut [RecordField]) {
        for f in fields {
            self.ty(&mut f.ty);
        }
    }

    fn effect_methods(&mut self, methods: &mut [EffectMethod]) {
        for m in methods {
            let generics = m.generics.clone();
            self.scoped(&generics, |rw| {
                rw.generics(&mut m.generics);
                rw.params(&mut m.params);
                rw.tys(&mut m.effects);
                if let Some(t) = &mut m.return_type {
                    rw.ty(t);
                }
                rw.contracts(&mut m.contracts);
                if let Some(b) = &mut m.default_body {
                    rw.block(b);
                }
            });
        }
    }

    fn generics(&mut self, gs: &mut [GenericParam]) {
        for g in gs {
            self.tys(&mut g.bounds);
            if let Some(d) = &mut g.default {
                self.ty(d);
            }
        }
    }

    fn params(&mut self, ps: &mut [Param]) {
        for p in ps {
            self.ty(&mut p.ty);
            if let Some(d) = &mut p.default {
                self.expr(d);
            }
        }
    }

    fn contracts(&mut self, cs: &mut [Contract]) {
        for c in cs {
            self.expr(&mut c.expr);
            if let Some(m) = &mut c.message_expr {
                self.expr(m);
            }
        }
    }

    fn let_decl(&mut self, l: &mut LetDecl) {
        self.pattern(&mut l.pattern);
        if let Some(t) = &mut l.ty {
            self.ty(t);
        }
        self.expr(&mut l.value);
    }

    fn const_decl(&mut self, c: &mut ConstDecl) {
        if let Some(t) = &mut c.ty {
            self.ty(t);
        }
        self.expr(&mut c.value);
    }

    fn fn_body(&mut self, b: &mut FnBody) {
        match b {
            FnBody::Expr(e) => self.expr(e),
            FnBody::Block(b) => self.block(b),
            FnBody::External => {}
        }
    }

    fn fn_sig_body(&mut self, s: &mut FnSigBody) {
        self.params(&mut s.params);
        self.tys(&mut s.effects);
        if let Some(t) = &mut s.return_type {
            self.ty(t);
        }
        self.fn_body(&mut s.body);
    }

    fn tys(&mut self, ts: &mut [TypeRef]) {
        for t in ts {
            self.ty(t);
        }
    }

    fn ty(&mut self, t: &mut TypeRef) {
        match t {
            TypeRef::Named { path, generics, span } => {
                let at = *span;
                self.type_path(path, at);
                self.tys(generics);
            }
            TypeRef::Array(inner, _)
            | TypeRef::FixedArray(_, inner, _)
            | TypeRef::Readonly(inner, _)
            | TypeRef::Mut(inner, _)
            | TypeRef::Uninit(inner, _)
            | TypeRef::Pointer(inner, _)
            | TypeRef::Ref(inner, _) => self.ty(inner),
            TypeRef::Tuple(ts, _) => self.tys(ts),
            TypeRef::Func { params, effects, return_type, .. } => {
                self.tys(params);
                self.tys(effects);
                if let Some(r) = return_type {
                    self.ty(r);
                }
            }
            TypeRef::Protocol { methods, .. } => self.effect_methods(methods),
            TypeRef::Unit(_) => {}
        }
    }

    fn block(&mut self, b: &mut Block) {
        self.stmts(&mut b.stmts);
        if let Some(t) = &mut b.trailing {
            self.expr(t);
        }
    }

    fn stmts(&mut self, ss: &mut [Stmt]) {
        for s in ss {
            self.stmt(s);
        }
    }

    fn stmt(&mut self, s: &mut Stmt) {
        match s {
            Stmt::Let(l) => self.let_decl(l),
            Stmt::Const(c) => self.const_decl(c),
            Stmt::Expr(e) => self.expr(e),
            Stmt::Assign { target, value, .. } => {
                self.expr(target);
                self.expr(value);
            }
            Stmt::TupleAssign { lhs, rhs, .. } => {
                self.exprs(lhs);
                self.exprs(rhs);
            }
            Stmt::Return { value, .. } => {
                if let Some(v) = value {
                    self.expr(v);
                }
            }
            Stmt::Break(_) | Stmt::Continue(_) | Stmt::Reveal { .. } => {}
            Stmt::Throw { value, .. } => self.expr(value),
            Stmt::Defer { body, .. } => self.expr(body),
            Stmt::ConsumeScope { type_annot, init, body, .. } => {
                if let Some(t) = type_annot {
                    self.ty(t);
                }
                self.expr(init);
                self.block(body);
            }
            Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => self.expr(expr),
            Stmt::Apply { args, .. } => self.exprs(args),
            Stmt::Calc { steps, .. } => {
                for st in steps {
                    self.expr(&mut st.expr);
                }
            }
        }
    }

    fn exprs(&mut self, es: &mut [Expr]) {
        for e in es {
            self.expr(e);
        }
    }

    fn opt(&mut self, e: &mut Option<Box<Expr>>) {
        if let Some(e) = e {
            self.expr(e);
        }
    }

    fn else_branch(&mut self, eb: &mut Option<ElseBranch>) {
        match eb {
            Some(ElseBranch::Block(b)) => self.block(b),
            Some(ElseBranch::If(e)) => self.expr(e),
            None => {}
        }
    }

    fn loop_spec(&mut self, invariants: &mut [Expr], decreases: &mut Option<Box<Expr>>) {
        self.exprs(invariants);
        self.opt(decreases);
    }

    fn handler_methods(&mut self, ms: &mut [HandlerMethod]) {
        for m in ms {
            for p in &mut m.params {
                if let Some(t) = &mut p.ty {
                    self.ty(t);
                }
            }
            if let Some(t) = &mut m.ret_ty {
                self.ty(t);
            }
            match &mut m.body {
                HandlerMethodBody::Expr(e) => self.expr(e),
                HandlerMethodBody::Block(b) => self.block(b),
            }
        }
    }

    fn expr(&mut self, e: &mut Expr) {
        let at = e.span;
        match &mut e.kind {
            ExprKind::IntLit(_)
            | ExprKind::FloatLit(_)
            | ExprKind::StrLit(_)
            | ExprKind::HexBlobLit(_)
            | ExprKind::BoolLit(_)
            | ExprKind::UnitLit
            | ExprKind::CharLit(_)
            | ExprKind::NullPtrLit
            | ExprKind::SelfAccess
            // A bare Ident is a value: `alpha_rename`'s (#1419), which knows locals.
            | ExprKind::Ident(_) => {}
            ExprKind::Path(path) => self.head(path, at),
            ExprKind::InterpolatedStr { parts } => {
                for p in parts {
                    if let InterpStrPart::Expr { expr, .. } = p {
                        self.expr(expr);
                    }
                }
            }
            ExprKind::ArrayLit(elems) => {
                for el in elems {
                    match el {
                        ArrayElem::Item(x) | ArrayElem::Spread(x) => self.expr(x),
                    }
                }
            }
            ExprKind::MapLit { elems, inferred_key, inferred_value, inferred_target_type } => {
                for me in elems {
                    match me {
                        MapElem::Pair(k, v) => {
                            self.expr(k);
                            self.expr(v);
                        }
                        MapElem::Spread(x) => self.expr(x),
                    }
                }
                for t in [inferred_key, inferred_value].into_iter().flatten() {
                    self.ty(t);
                }
                if let Some(p) = inferred_target_type {
                    self.type_path(p, at);
                }
            }
            ExprKind::RecordLit { type_name, fields, inferred_map_v, inferred_target_type } => {
                // `U { .. }` names the type by its first segment (`U.Variant { .. }`
                // for a record variant), like a value path head.
                if let Some(p) = type_name {
                    if p.len() == 1 {
                        self.type_path(p, at);
                    } else {
                        self.head(p, at);
                    }
                }
                if let Some(p) = inferred_target_type {
                    self.type_path(p, at);
                }
                if let Some(t) = inferred_map_v {
                    self.ty(t);
                }
                for f in fields {
                    if let Some(v) = &mut f.value {
                        self.expr(v);
                    }
                }
            }
            ExprKind::TupleLit(xs) => self.exprs(xs),
            ExprKind::Member { obj, .. } => self.expr(obj),
            ExprKind::Index { obj, index } => {
                self.expr(obj);
                self.expr(index);
            }
            ExprKind::TurboFish { base, type_args } => {
                self.expr(base);
                self.tys(type_args);
            }
            ExprKind::Call { func, args, trailing } => {
                self.expr(func);
                for a in args {
                    match a {
                        CallArg::Item(x) | CallArg::Spread(x) => self.expr(x),
                        CallArg::Named { value, .. } => self.expr(value),
                    }
                }
                match trailing {
                    Some(Trailing::Block(b)) => self.block(b),
                    Some(Trailing::Fn(sb)) => self.fn_sig_body(sb),
                    Some(Trailing::LegacyBlockWithParams(tb)) => {
                        for p in &mut tb.params {
                            if let Some(t) = &mut p.ty {
                                self.ty(t);
                            }
                        }
                        self.block(&mut tb.body);
                    }
                    None => {}
                }
            }
            ExprKind::RefArg(x)
            | ExprKind::Try(x)
            | ExprKind::Bang(x)
            | ExprKind::Spawn(x)
            | ExprKind::Throw(x) => self.expr(x),
            ExprKind::Coalesce(a, b) => {
                self.expr(a);
                self.expr(b);
            }
            ExprKind::As(x, t) | ExprKind::Is(x, t) => {
                self.expr(x);
                self.ty(t);
            }
            ExprKind::Binary { left, right, .. } => {
                self.expr(left);
                self.expr(right);
            }
            ExprKind::Unary { operand, .. } => self.expr(operand),
            ExprKind::If { cond, then, else_ } => {
                self.expr(cond);
                self.block(then);
                self.else_branch(else_);
            }
            ExprKind::IfLet { pattern, scrutinee, guard, then, else_ } => {
                self.pattern(pattern);
                self.expr(scrutinee);
                self.opt(guard);
                self.block(then);
                self.else_branch(else_);
            }
            ExprKind::Match { scrutinee, arms } => {
                self.expr(scrutinee);
                for arm in arms {
                    self.pattern(&mut arm.pattern);
                    if let Some(g) = &mut arm.guard {
                        self.expr(g);
                    }
                    match &mut arm.body {
                        MatchArmBody::Expr(x) => self.expr(x),
                        MatchArmBody::Block(b) => self.block(b),
                    }
                }
            }
            ExprKind::For { pattern, iter, body, elem_type, invariants, decreases, .. } => {
                self.pattern(pattern);
                self.expr(iter);
                self.block(body);
                if let Some(t) = elem_type {
                    self.ty(t);
                }
                self.loop_spec(invariants, decreases);
            }
            ExprKind::ParallelFor { pattern, iter, body, elem_type } => {
                self.pattern(pattern);
                self.expr(iter);
                self.block(body);
                if let Some(t) = elem_type {
                    self.ty(t);
                }
            }
            ExprKind::While { cond, body, invariants, decreases } => {
                self.expr(cond);
                self.block(body);
                self.loop_spec(invariants, decreases);
            }
            ExprKind::WhileLet { pattern, scrutinee, guard, body, invariants, decreases } => {
                self.pattern(pattern);
                self.expr(scrutinee);
                self.opt(guard);
                self.block(body);
                self.loop_spec(invariants, decreases);
            }
            ExprKind::Loop { body, invariants, decreases } => {
                self.block(body);
                self.loop_spec(invariants, decreases);
            }
            ExprKind::Select { arms } => {
                for arm in arms {
                    match &mut arm.op {
                        SelectOp::Recv { chan, .. } => self.expr(chan),
                        SelectOp::Send { chan, value } => {
                            self.expr(chan);
                            self.expr(value);
                        }
                        SelectOp::Default => {}
                    }
                    if let Some(g) = &mut arm.guard {
                        self.expr(g);
                    }
                    self.block(&mut arm.body);
                }
            }
            ExprKind::Lambda { params, effects, return_type, body } => {
                for p in params {
                    if let Some(t) = &mut p.ty {
                        self.ty(t);
                    }
                }
                self.tys(effects);
                if let Some(t) = return_type {
                    self.ty(t);
                }
                self.expr(body);
            }
            ExprKind::ClosureLight { body, .. } => match body {
                ClosureBody::Expr(x) => self.expr(x),
                ClosureBody::Block(b) => self.block(b),
            },
            ExprKind::ClosureFull(sb) => self.fn_sig_body(sb),
            ExprKind::With { bindings, body } => {
                for b in bindings {
                    self.ty(&mut b.effect);
                    self.expr(&mut b.handler);
                }
                self.block(body);
            }
            ExprKind::HandlerLit { effect_name, methods } => {
                self.type_path(effect_name, at);
                self.handler_methods(methods);
            }
            ExprKind::ProtocolLit { proto_name, methods } => {
                self.type_path(proto_name, at);
                self.handler_methods(methods);
            }
            ExprKind::Interrupt(x) | ExprKind::CoalesceReturnFallback(x) => self.opt(x),
            ExprKind::Forbid { effects, body } => {
                self.tys(effects);
                self.block(body);
            }
            ExprKind::Realtime { body, .. }
            | ExprKind::Block(body)
            | ExprKind::Detach(body)
            | ExprKind::Blocking(body) => self.block(body),
            ExprKind::Range { start, end, .. } => {
                self.opt(start);
                self.opt(end);
            }
            ExprKind::Forall { range, body, .. } | ExprKind::Exists { range, body, .. } => {
                self.expr(range);
                self.expr(body);
            }
            ExprKind::Supervised { body, cancel, deadline, on_timeout } => {
                self.block(body);
                self.opt(cancel);
                if let Some(d) = deadline {
                    self.expr(&mut d.expr);
                }
                self.opt(on_timeout);
            }
            ExprKind::TaggedTemplate { tag, args, .. } => {
                self.expr(tag);
                self.exprs(args);
            }
        }
    }

    fn pattern(&mut self, p: &mut Pattern) {
        match p {
            Pattern::Wildcard(_) | Pattern::Literal(..) | Pattern::Ident { .. } => {}
            Pattern::Variant { path, kind, span } => {
                let at = *span;
                self.head(path, at);
                if let VariantPatternKind::Tuple { patterns, .. } = kind {
                    for x in patterns {
                        self.pattern(x);
                    }
                }
            }
            Pattern::Record { type_path, fields, span, .. } => {
                let at = *span;
                if let Some(tp) = type_path {
                    if tp.len() == 1 {
                        self.type_path(tp, at);
                    } else {
                        self.head(tp, at);
                    }
                }
                for f in fields {
                    if let Some(x) = &mut f.pattern {
                        self.pattern(x);
                    }
                }
            }
            Pattern::Array { elems, .. } => {
                for el in elems {
                    if let ArrayPatternElem::Item(x) = el {
                        self.pattern(x);
                    }
                }
            }
            Pattern::Tuple(xs, _) => {
                for x in xs {
                    self.pattern(x);
                }
            }
            Pattern::Binding { inner, .. } => self.pattern(inner),
            Pattern::Or { alternatives, .. } => {
                for x in alternatives {
                    self.pattern(x);
                }
            }
        }
    }
}
