//! Plan 52 Ф.4/Ф.5 (D108): AST-десугаринг map-литералов `[k: v]`.
//!
//! `MapLit` десугарится в block-expression с `with_capacity` + `@insert`
//! **до** codegen и treewalk-интерпретатора — единый проход закрывает
//! оба пути, без дублирования логики. После этого прохода `ExprKind::MapLit`
//! в AST больше не встречается (codegen/interp заглушки остаются как
//! safety-net на случай нового непокрытого вызова).
//!
//! Десугаринг (D108):
//! ```text
//! [k1: v1, k2: v2]
//! // →
//! {
//!     let mut _m0 = HashMap.with_capacity(2)
//!     let _ = _m0.insert(k1, v1)
//!     let _ = _m0.insert(k2, v2)
//!     _m0
//! }
//! ```
//!
//! - Порядок вычисления нормативный: `k1, v1, k2, v2, ...` — пары слева
//!   направо, ключ перед значением (D108). Block-statements эмитятся в
//!   этом порядке естественно.
//! - `with_capacity(n)` несёт контракт «n вставок без rehash» (Ф.6).
//! - `@insert` возвращает `Option[V]`; возврат отбрасывается через
//!   `let _ = ...` (защита от будущего lint «discarded non-unit»).
//! - Temp-переменная `_m0`, `_m1`, ... — per-module счётчик, valid ISO
//!   C11 (без `$`); вложенные литералы не конфликтуют именами.
//! - Пустой `[]` НЕ доходит сюда как `MapLit` — он остаётся
//!   `ArrayLit(vec![])` и резолвится по ожидаемому типу отдельно.
//! - `HashMap.with_capacity` / `.insert` вызываются **без turbofish** —
//!   мономорфизация codegen/interp выводит `K`/`V` из аргументов.
//!
//! Bootstrap: десугаринг захардкожен на `HashMap`. Точка расширения —
//! протокол `FromPairs[K, V]` (`BTreeMap`, `OrderedMap`) — позже.

use crate::ast::*;
use crate::diag::Span;

/// Прогоняет десугаринг map-литералов по всему модулю. После вызова
/// `ExprKind::MapLit` в AST больше не встречается.
pub fn desugar_module(module: &mut Module) {
    let mut ctx = DesugarCtx { counter: 0 };
    for item in &mut module.items {
        ctx.desugar_item(item);
    }
    // peer_files несут собственные копии items для per-peer name
    // resolution (Plan 42.4) — десугарим и их, чтобы codegen/interp,
    // читающие flat module.items, и любые consumer'ы peer_files видели
    // согласованный AST.
    for pf in &mut module.peer_files {
        for item in &mut pf.items_here {
            ctx.desugar_item(item);
        }
    }
}

struct DesugarCtx {
    /// Монотонный счётчик для temp-имён `_m0`, `_m1`, ... (per-module).
    counter: usize,
}

impl DesugarCtx {
    fn fresh_map_tmp(&mut self) -> String {
        let name = format!("_m{}", self.counter);
        self.counter += 1;
        name
    }

    fn desugar_item(&mut self, item: &mut Item) {
        match item {
            Item::Fn(f) => match &mut f.body {
                FnBody::Expr(e) => self.desugar_expr(e),
                FnBody::Block(b) => self.desugar_block(b),
                FnBody::External => {}
            },
            Item::Const(c) => self.desugar_expr(&mut c.value),
            Item::Let(l) => self.desugar_expr(&mut l.value),
            Item::Test(t) => self.desugar_block(&mut t.body),
            // Plan 57: bench setup/measure_body/teardown — обходим всё
            // (map-литералы внутри setup и measure тоже должны desugar'иться).
            Item::Bench(b) => {
                for s in &mut b.setup {
                    self.desugar_stmt(s);
                }
                self.desugar_block(&mut b.measure_body);
                for s in &mut b.teardown {
                    self.desugar_stmt(s);
                }
            }
            Item::Type(_) => {}
            // Plan 33.3 Ф.13: lemma — spec-only declaration, body имеет
            // proof-statements (Apply/Calc); карты литералов внутри
            // proof-выражений не имеют смысла (lemma эрейзится в codegen),
            // но для consistency обходим тело.
            Item::Lemma(_) => {}
        }
    }

    fn desugar_block(&mut self, b: &mut Block) {
        for s in &mut b.stmts {
            self.desugar_stmt(s);
        }
        if let Some(t) = &mut b.trailing {
            self.desugar_expr(t);
        }
    }

    fn desugar_stmt(&mut self, s: &mut Stmt) {
        match s {
            Stmt::Let(d) => self.desugar_expr(&mut d.value),
            Stmt::Expr(e) => self.desugar_expr(e),
            Stmt::Assign { target, value, .. } => {
                self.desugar_expr(target);
                self.desugar_expr(value);
            }
            Stmt::Return { value, .. } => {
                if let Some(v) = value { self.desugar_expr(v); }
            }
            Stmt::Throw { value, .. } => self.desugar_expr(value),
            Stmt::Break(_) | Stmt::Continue(_) => {}
            Stmt::Defer { body, .. } | Stmt::ErrDefer { body, .. }
            | Stmt::OkDefer { body, .. } | Stmt::DeferWithResult { body, .. } => self.desugar_expr(body),
            Stmt::AssertStatic { expr, .. } | Stmt::Assume { expr, .. } => self.desugar_expr(expr),
            // Plan 33.3 Ф.13: Apply/Calc — proof-statements внутри lemma-body.
            // Spec-only, не emit'ятся в codegen. Map-литералы внутри proof —
            // edge case, не покрываем (lemma body — spec, не runtime).
            Stmt::Apply { .. } | Stmt::Calc { .. } | Stmt::Reveal { .. } => {}
        }
    }

    /// Рекурсивно десугарит выражение. Сначала спускается в под-выражения
    /// (чтобы вложенные `[1: [10: "x"]]` десугарились bottom-up — внешний
    /// блок получит уже десугаренный внутренний), затем — если само
    /// выражение это `MapLit` — заменяет его на block-expression.
    fn desugar_expr(&mut self, e: &mut Expr) {
        // 1. Спуск в дети.
        self.desugar_children(e);
        // 2. Замена самого узла, если это MapLit.
        if matches!(&e.kind, ExprKind::MapLit { .. }) {
            // take elems + inferred K/V из узла, заменяя его на UnitLit-
            // заглушку, затем строим Block и кладём обратно.
            let span = e.span;
            let (elems, inferred_key, inferred_value, inferred_target_type) =
                match std::mem::replace(&mut e.kind, ExprKind::UnitLit) {
                    ExprKind::MapLit {
                        elems,
                        inferred_key,
                        inferred_value,
                        inferred_target_type,
                    } => (elems, inferred_key, inferred_value, inferred_target_type),
                    _ => unreachable!(),
                };
            e.kind = self.build_map_block(
                elems, inferred_key, inferred_value, inferred_target_type, span);
        }
        // Plan 52 Ф.10: D55 map-coercion для `{field: v}`. Когда
        // annotate_map_literals установил `inferred_map_v: Some(V)` —
        // это анонимный record-литерал в позиции `HashMap[str, V]`.
        // Превращаем в pairs = [("field", v), ...] и десугарим как
        // обычный MapLit с K=str, V=inferred_map_v.
        else if let ExprKind::RecordLit { type_name: None, inferred_map_v: Some(_), .. } = &e.kind {
            let span = e.span;
            let (fields, v_ty) =
                match std::mem::replace(&mut e.kind, ExprKind::UnitLit) {
                    ExprKind::RecordLit { fields, inferred_map_v: Some(v_ty), .. } => {
                        (fields, v_ty)
                    }
                    _ => unreachable!(),
                };
            e.kind = self.build_record_map_block(fields, v_ty, span);
        }
    }

    /// Plan 52 Ф.10: десугаринг D55 map-coercion `{field: v}` →
    /// `HashMap[str, V].with_capacity(n) + n × insert("field", v)`
    /// block-expression. Mirror MapLit-десугаринга для consistency.
    ///
    /// Spread (`...src`) уже отвергнут type-checker'ом (Plan 52 Ф.3);
    /// здесь молча пропускаем (panic если встретится — bug type-check'а).
    /// Field-punning `{ name }` — значение это переменная `name` в scope.
    fn build_record_map_block(
        &mut self,
        fields: Vec<RecordLitField>,
        v_ty: TypeRef,
        span: Span,
    ) -> ExprKind {
        let tmp = self.fresh_map_tmp();
        let n = fields.len();
        let mut stmts: Vec<Stmt> = Vec::with_capacity(n + 1);

        // Callee: HashMap[str, V].with_capacity (mirror MapLit с
        // turbofish — codegen mono требует Ident-based callee, не Path).
        let str_ty = TypeRef::Named {
            path: vec!["str".to_string()],
            generics: Vec::new(),
            span,
        };
        let hashmap_ident = Expr::new(ExprKind::Ident("HashMap".to_string()), span);
        let turbofish = Expr::new(
            ExprKind::TurboFish {
                base: Box::new(hashmap_ident),
                type_args: vec![str_ty, v_ty],
            },
            span,
        );
        let with_capacity_callee = Expr::new(
            ExprKind::Member {
                obj: Box::new(turbofish),
                name: "with_capacity".to_string(),
            },
            span,
        );
        let with_capacity_call = Expr::new(
            ExprKind::Call {
                func: Box::new(with_capacity_callee),
                args: vec![CallArg::Item(Expr::new(ExprKind::IntLit(n as i64), span))],
                trailing: None,
            },
            span,
        );
        stmts.push(Stmt::Let(LetDecl {
            mutable: true,
            pattern: Pattern::Ident { name: tmp.clone(), span, is_mut: false },
            ty: None,
            value: with_capacity_call,
            span,
            is_ghost: false,
            consume: false,
        }));

        // Для каждого поля: `let _ = _mN.insert("name", value_expr)`
        for f in fields {
            if f.is_spread {
                // type-checker должен был отвергнуть; пропускаем silently.
                continue;
            }
            let key_expr = Expr::new(ExprKind::StrLit(f.name.clone()), span);
            // Field-punning: { name } → значение это переменная `name`.
            let value_expr = match f.value {
                Some(v) => v,
                None => Expr::new(ExprKind::Ident(f.name.clone()), span),
            };
            // Plan 52 Ф.21: используем `insert_new` (нет возврата Option) —
            // мапа только что создана через with_capacity, дубликатов
            // быть не может. См. std/collections/hashmap.nv::@insert_new.
            let insert_call = Expr::new(
                ExprKind::Call {
                    func: Box::new(Expr::new(
                        ExprKind::Member {
                            obj: Box::new(Expr::new(ExprKind::Ident(tmp.clone()), span)),
                            name: "insert_new".to_string(),
                        },
                        span,
                    )),
                    args: vec![CallArg::Item(key_expr), CallArg::Item(value_expr)],
                    trailing: None,
                },
                span,
            );
            stmts.push(Stmt::Expr(insert_call));
        }

        let trailing = Expr::new(ExprKind::Ident(tmp), span);
        ExprKind::Block(Block {
            stmts,
            trailing: Some(Box::new(trailing)),
            span,
        })
    }

    /// Строит block-expression `{ let mut _mN = HashMap[K,V].with_capacity(n);
    /// let _ = _mN.insert(k, v); ...; _mN }` из пар map-литерала.
    ///
    /// Plan 52 Ф.7 production-fix: если `inferred_key`/`inferred_value`
    /// заполнены type-checker'ом (MapLitCtx::annotate_module) — используем
    /// turbofish `HashMap[K, V].with_capacity(n)` для корректной
    /// мономорфизации. Без turbofish codegen инстанциирует
    /// `HashMap[void*, void*]` → segfault на runtime при `K.hash()`/
    /// `K.eq()` через generic-bound dispatch (Plan 48 Ф.7.7 partial).
    ///
    /// Fallback (K/V неизвестны): bare `HashMap.with_capacity(n)` — может
    /// упасть в codegen без аннотации; type-checker эмитит «cannot infer»
    /// до десугаринга если контекст не даёт K/V.
    fn build_map_block(
        &mut self,
        elems: Vec<MapElem>,
        inferred_key: Option<TypeRef>,
        inferred_value: Option<TypeRef>,
        inferred_target_type: Option<Vec<String>>,
        span: Span,
    ) -> ExprKind {
        // Plan 52 Ф.23: target_type определяется expected'ом (если
        // помечен #from_pairs) или fallback на HashMap для legacy.
        // Last-segment для Ident-конструкции: codegen мономорфизирует
        // только Ident-based callee, не Path([...]).
        let target_type_name: String = inferred_target_type
            .as_ref()
            .and_then(|path| path.last().cloned())
            .unwrap_or_else(|| "HashMap".to_string());
        // Clone для Ф.16 typed-rebinding (см. ниже).
        let kv_for_hint = match (&inferred_key, &inferred_value) {
            (Some(k), Some(v)) => Some((k.clone(), v.clone())),
            _ => None,
        };
        let target_for_hint = target_type_name.clone();
        let tmp = self.fresh_map_tmp();
        // Plan 55 followup: estimate capacity = pairs count + spread sizes.
        // Spread sizes неизвестны statically — используем pairs count как
        // baseline, HashMap.@maybe_grow расширит при overflow. Можно
        // улучшить через runtime estimate (sum of spread.len()), но это
        // micro-opt; correctness не affected.
        let pairs_count = elems.iter().filter(|e| matches!(e, MapElem::Pair(..))).count();
        let spread_count = elems.iter().filter(|e| matches!(e, MapElem::Spread(_))).count();
        // Initial cap = pairs (известно) + 8 на spread (heuristic).
        let n = pairs_count + spread_count * 8;
        let mut stmts: Vec<Stmt> = Vec::with_capacity(elems.len() + 1);

        // Callee: `<TargetType>.with_capacity` (Path) или
        // `<TargetType>[K, V].with_capacity` (TurboFish + Member).
        let with_capacity_callee: Expr = match (inferred_key, inferred_value) {
            (Some(k_ty), Some(v_ty)) => {
                // TurboFish: `<TargetType>[K, V]` затем `.with_capacity`.
                // ВАЖНО: base должен быть `Ident`, не `Path([_])` —
                // парсер для одиночного имени строит Ident; codegen
                // мономорфизирует только Ident-based callee.
                let target_ident = Expr::new(
                    ExprKind::Ident(target_type_name.clone()),
                    span,
                );
                let turbofish = Expr::new(
                    ExprKind::TurboFish {
                        base: Box::new(target_ident),
                        type_args: vec![k_ty, v_ty],
                    },
                    span,
                );
                Expr::new(
                    ExprKind::Member {
                        obj: Box::new(turbofish),
                        name: "with_capacity".to_string(),
                    },
                    span,
                )
            }
            _ => {
                // Fallback: bare `<TargetType>.with_capacity` — мономорфизация
                // через контекст (может не сработать для generic-methods,
                // см. Plan 48 Ф.7.7 baseline-баг).
                Expr::new(
                    ExprKind::Path(vec![
                        target_type_name.clone(),
                        "with_capacity".to_string(),
                    ]),
                    span,
                )
            }
        };
        let with_capacity_call = Expr::new(
            ExprKind::Call {
                func: Box::new(with_capacity_callee),
                args: vec![CallArg::Item(Expr::new(
                    ExprKind::IntLit(n as i64),
                    span,
                ))],
                trailing: None,
            },
            span,
        );
        stmts.push(Stmt::Let(LetDecl {
            mutable: true,
            pattern: Pattern::Ident { name: tmp.clone(), span, is_mut: false },
            ty: None,
            value: with_capacity_call,
            span,
            is_ghost: false,
            consume: false,
        }));

        // Plan 52 Ф.13 production-fix: explicit temp-bindings для
        // гарантированного нормативного eval-order (D108 §4748:
        // k1, v1, k2, v2 слева-направо). Без temp'ов C function-call
        // evaluates args в неопределённом порядке (clang делает
        // right-to-left) → реальный observable порядок был v1, k1, v2, k2.
        // Каждая пара десугарится в:
        //   let _kN = <key-expr>;     ← evaluated first
        //   let _vN = <value-expr>;   ← evaluated second
        //   let _ = _mN.insert(_kN, _vN);
        //
        // Plan 55 followup (spread): MapElem::Spread(e) → for-loop
        // через elements входной map'ы:
        //   for _entry in (e).entries() { _mN.insert(_entry.0, _entry.1); }
        // (Bootstrap: используем _mN.insert т.к. spread может добавить
        // duplicate keys — insert_new недопустимо.)
        let mut has_spreads_any = false;
        for (idx, me) in elems.into_iter().enumerate() {
            match me {
                MapElem::Pair(k, v) => {
                    let k_tmp = format!("{}_k{}", tmp, idx);
                    let v_tmp = format!("{}_v{}", tmp, idx);
                    stmts.push(Stmt::Let(LetDecl {
                        mutable: false,
                        pattern: Pattern::Ident { name: k_tmp.clone(), span, is_mut: false },
                        ty: None,
                        value: k,
                        span,
                        is_ghost: false,
                        consume: false,
                    }));
                    stmts.push(Stmt::Let(LetDecl {
                        mutable: false,
                        pattern: Pattern::Ident { name: v_tmp.clone(), span, is_mut: false },
                        ty: None,
                        value: v,
                        span,
                        is_ghost: false,
                        consume: false,
                    }));
                    // Plan 52 Ф.21: insert_new — мапа только что создана.
                    // Plan 55 followup: если уже был spread выше, ключ мог
                    // быть уже добавлен через spread → используем insert
                    // (override semantics), не insert_new.
                    let method = if has_spreads_any { "insert" } else { "insert_new" };
                    let insert_call = Expr::new(
                        ExprKind::Call {
                            func: Box::new(Expr::new(
                                ExprKind::Member {
                                    obj: Box::new(Expr::new(ExprKind::Ident(tmp.clone()), span)),
                                    name: method.to_string(),
                                },
                                span,
                            )),
                            args: vec![
                                CallArg::Item(Expr::new(ExprKind::Ident(k_tmp), span)),
                                CallArg::Item(Expr::new(ExprKind::Ident(v_tmp), span)),
                            ],
                            trailing: None,
                        },
                        span,
                    );
                    stmts.push(Stmt::Expr(insert_call));
                }
                MapElem::Spread(src_map) => {
                    has_spreads_any = true;
                    // Desugar: spread `[...src]` → итерация через keys() + get()
                    // вместо iter() + tuple destructure. Tuple element type
                    // inference для mono'd HashMapIter — orthogonal issue
                    // (выйдет за scope spread feature). Через keys() мы получаем
                    // K напрямую, get(k) даёт Option[V] — оба известных типа.
                    //
                    //   let _src = <spread-expr>;
                    //   for _k in _src.keys() {
                    //       let _v = _src.get(_k).unwrap()
                    //       _mN.insert(_k, _v)
                    //   }
                    let src_tmp = format!("{}_spr{}", tmp, idx);
                    stmts.push(Stmt::Let(LetDecl {
                        mutable: false,
                        pattern: Pattern::Ident { name: src_tmp.clone(), span, is_mut: false },
                        ty: None,
                        value: src_map,
                        span,
                        is_ghost: false,
                        consume: false,
                    }));
                    let k_name = format!("{}_sk{}", tmp, idx);
                    let v_name = format!("{}_sv{}", tmp, idx);
                    let keys_call = Expr::new(
                        ExprKind::Call {
                            func: Box::new(Expr::new(
                                ExprKind::Member {
                                    obj: Box::new(Expr::new(ExprKind::Ident(src_tmp.clone()), span)),
                                    name: "keys".to_string(),
                                },
                                span,
                            )),
                            args: vec![],
                            trailing: None,
                        },
                        span,
                    );
                    // let _v = _src.get(_k).unwrap()
                    let get_call = Expr::new(
                        ExprKind::Call {
                            func: Box::new(Expr::new(
                                ExprKind::Member {
                                    obj: Box::new(Expr::new(ExprKind::Ident(src_tmp.clone()), span)),
                                    name: "get".to_string(),
                                },
                                span,
                            )),
                            args: vec![CallArg::Item(Expr::new(ExprKind::Ident(k_name.clone()), span))],
                            trailing: None,
                        },
                        span,
                    );
                    let unwrap_call = Expr::new(
                        ExprKind::Call {
                            func: Box::new(Expr::new(
                                ExprKind::Member {
                                    obj: Box::new(get_call),
                                    name: "unwrap".to_string(),
                                },
                                span,
                            )),
                            args: vec![],
                            trailing: None,
                        },
                        span,
                    );
                    let v_let = Stmt::Let(LetDecl {
                        mutable: false,
                        pattern: Pattern::Ident { name: v_name.clone(), span, is_mut: false },
                        ty: None,
                        value: unwrap_call,
                        span,
                        is_ghost: false,
                        consume: false,
                    });
                    let insert_in_loop = Expr::new(
                        ExprKind::Call {
                            func: Box::new(Expr::new(
                                ExprKind::Member {
                                    obj: Box::new(Expr::new(ExprKind::Ident(tmp.clone()), span)),
                                    name: "insert".to_string(),
                                },
                                span,
                            )),
                            args: vec![
                                CallArg::Item(Expr::new(ExprKind::Ident(k_name.clone()), span)),
                                CallArg::Item(Expr::new(ExprKind::Ident(v_name), span)),
                            ],
                            trailing: None,
                        },
                        span,
                    );
                    let for_body = Block {
                        stmts: vec![v_let, Stmt::Expr(insert_in_loop)],
                        trailing: None,
                        span,
                    };
                    let for_expr = Expr::new(
                        ExprKind::For {
                            pattern: Pattern::Ident { name: k_name, span, is_mut: false },
                            iter: Box::new(keys_call),
                            body: for_body,
                            elem_type: None,
                            invariants: vec![],
                            decreases: None,
                            iter_consume: false,
                        },
                        span,
                    );
                    stmts.push(Stmt::Expr(for_expr));
                }
            }
        }
        let _ = pairs_count; let _ = spread_count;

        // Plan 52 Ф.16: typed-rebinding для Block-trailing inference.
        // Без этого `let m = [k:v]` (без аннотации) — codegen infer_expr_c_type
        // на Block смотрит на trailing (Ident `_mN`), но var_types['_mN']
        // ещё не записан в момент infer (вызывается ДО emit). Fallback
        // даёт `nova_int` → `let m = _nv_tmp` пишется как nova_int → method
        // calls идут через generic version → runtime fail. Workaround:
        // если K/V известны (annotate_map_literals выдал turbofish-форму),
        // добавляем явно-аннотированный rebind `let _mN_typed HashMap[K,V] = _mN`
        // — это даёт let-stmt подсказку типа для outer-let.
        let trailing_name = if let Some((k_ty, v_ty)) = kv_for_hint {
            let typed = format!("{}_typed", tmp);
            stmts.push(Stmt::Let(LetDecl {
                mutable: false,
                pattern: Pattern::Ident { name: typed.clone(), span, is_mut: false },
                ty: Some(TypeRef::Named {
                    path: vec![target_for_hint],
                    generics: vec![k_ty, v_ty],
                    span,
                }),
                value: Expr::new(ExprKind::Ident(tmp), span),
                span,
                is_ghost: false,
                consume: false,
            }));
            typed
        } else {
            tmp
        };

        let trailing = Expr::new(ExprKind::Ident(trailing_name), span);

        ExprKind::Block(Block {
            stmts,
            trailing: Some(Box::new(trailing)),
            span,
        })
    }

    /// Рекурсивный спуск во все под-выражения `e` (без обработки самого
    /// `e` — это делает `desugar_expr`).
    fn desugar_children(&mut self, e: &mut Expr) {
        match &mut e.kind {
            ExprKind::MapLit { elems, .. } => {
                for me in elems.iter_mut() {
                    match me {
                        MapElem::Pair(k, v) => {
                            self.desugar_expr(k);
                            self.desugar_expr(v);
                        }
                        MapElem::Spread(e) => self.desugar_expr(e),
                    }
                }
            }
            ExprKind::ArrayLit(elems) => {
                for el in elems.iter_mut() {
                    match el {
                        ArrayElem::Item(x) | ArrayElem::Spread(x) => self.desugar_expr(x),
                    }
                }
            }
            ExprKind::TupleLit(elems) => {
                for x in elems.iter_mut() { self.desugar_expr(x); }
            }
            ExprKind::RecordLit { fields, .. } => {
                for f in fields.iter_mut() {
                    if let Some(v) = &mut f.value { self.desugar_expr(v); }
                }
            }
            ExprKind::Call { func, args, trailing } => {
                self.desugar_expr(func);
                for a in args.iter_mut() {
                    match a {
                        CallArg::Item(x) | CallArg::Spread(x) => self.desugar_expr(x),
                        CallArg::Named { value, .. } => self.desugar_expr(value),
                    }
                }
                if let Some(t) = trailing {
                    self.desugar_trailing(t);
                }
            }
            ExprKind::TurboFish { base, .. } => self.desugar_expr(base),
            ExprKind::Try(x) | ExprKind::Bang(x) => self.desugar_expr(x),
            ExprKind::Coalesce(a, b) => { self.desugar_expr(a); self.desugar_expr(b); }
            ExprKind::As(x, _) | ExprKind::Is(x, _) => self.desugar_expr(x),
            ExprKind::Binary { left, right, .. } => {
                self.desugar_expr(left);
                self.desugar_expr(right);
            }
            ExprKind::Unary { operand, .. } => self.desugar_expr(operand),
            ExprKind::Member { obj, .. } => self.desugar_expr(obj),
            ExprKind::Index { obj, index } => {
                self.desugar_expr(obj);
                self.desugar_expr(index);
            }
            ExprKind::If { cond, then, else_ } => {
                self.desugar_expr(cond);
                self.desugar_block(then);
                if let Some(eb) = else_ {
                    match eb {
                        ElseBranch::Block(b) => self.desugar_block(b),
                        ElseBranch::If(x) => self.desugar_expr(x),
                    }
                }
            }
            ExprKind::IfLet { scrutinee, then, else_, .. } => {
                self.desugar_expr(scrutinee);
                self.desugar_block(then);
                if let Some(eb) = else_ {
                    match eb {
                        ElseBranch::Block(b) => self.desugar_block(b),
                        ElseBranch::If(x) => self.desugar_expr(x),
                    }
                }
            }
            ExprKind::Match { scrutinee, arms } => {
                self.desugar_expr(scrutinee);
                for arm in arms.iter_mut() {
                    if let Some(g) = &mut arm.guard { self.desugar_expr(g); }
                    match &mut arm.body {
                        MatchArmBody::Expr(x) => self.desugar_expr(x),
                        MatchArmBody::Block(b) => self.desugar_block(b),
                    }
                }
            }
            ExprKind::For { iter, body, .. } | ExprKind::ParallelFor { iter, body, .. } => {
                self.desugar_expr(iter);
                self.desugar_block(body);
            }
            ExprKind::While { cond, body, .. } => {
                self.desugar_expr(cond);
                self.desugar_block(body);
            }
            ExprKind::WhileLet { scrutinee, body, .. } => {
                self.desugar_expr(scrutinee);
                self.desugar_block(body);
            }
            ExprKind::Loop { body, .. } => self.desugar_block(body),
            ExprKind::Block(b) => self.desugar_block(b),
            ExprKind::Spawn(x) => self.desugar_expr(x),
            ExprKind::Detach(b) | ExprKind::Blocking(b) => self.desugar_block(b),
            ExprKind::Supervised { body, cancel } => {
                self.desugar_block(body);
                if let Some(c) = cancel { self.desugar_expr(c); }
            }
            ExprKind::Forbid { body, .. } | ExprKind::Realtime { body, .. } => {
                self.desugar_block(body);
            }
            ExprKind::Throw(x) => self.desugar_expr(x),
            ExprKind::Interrupt(opt) => {
                if let Some(x) = opt { self.desugar_expr(x); }
            }
            ExprKind::Range { start, end, .. } => {
                if let Some(s) = start { self.desugar_expr(s); }
                if let Some(e) = end { self.desugar_expr(e); }
            }
            ExprKind::InterpolatedStr { parts } => {
                for p in parts.iter_mut() {
                    if let InterpStrPart::Expr(x) = p { self.desugar_expr(x); }
                }
            }
            ExprKind::TaggedTemplate { args, .. } => {
                for x in args.iter_mut() { self.desugar_expr(x); }
            }
            ExprKind::Lambda { body, .. } => self.desugar_expr(body),
            ExprKind::ClosureLight { body, .. } => match body {
                ClosureBody::Expr(x) => self.desugar_expr(x),
                ClosureBody::Block(b) => self.desugar_block(b),
            },
            ExprKind::ClosureFull(sb) => match &mut sb.body {
                FnBody::Expr(x) => self.desugar_expr(x),
                FnBody::Block(b) => self.desugar_block(b),
                FnBody::External => {}
            },
            ExprKind::With { bindings, body } => {
                for b in bindings.iter_mut() { self.desugar_expr(&mut b.handler); }
                self.desugar_block(body);
            }
            // Plan 97 Ф.4 (D142): protocol-литерал — desugar тел методов
            // идентично handler-литералу.
            ExprKind::HandlerLit { methods, .. } | ExprKind::ProtocolLit { methods, .. } => {
                for m in methods.iter_mut() {
                    match &mut m.body {
                        HandlerMethodBody::Expr(x) => self.desugar_expr(x),
                        HandlerMethodBody::Block(b) => self.desugar_block(b),
                    }
                }
            }
            ExprKind::Select { arms } => {
                for arm in arms.iter_mut() {
                    match &mut arm.op {
                        SelectOp::Recv { chan, .. } => self.desugar_expr(chan),
                        SelectOp::Send { chan, value } => {
                            self.desugar_expr(chan);
                            self.desugar_expr(value);
                        }
                        SelectOp::Default => {}
                    }
                    if let Some(g) = &mut arm.guard { self.desugar_expr(g); }
                    self.desugar_block(&mut arm.body);
                }
            }
            // Plan 33.3 Ф.13: Forall/Exists — quantifiers в spec-выражениях.
            // Body — proposition; map-литералы внутри не имеют runtime-смысла
            // (spec эрейзится в codegen), но обходим тело для consistency.
            ExprKind::Forall { body, .. } | ExprKind::Exists { body, .. } => {
                self.desugar_expr(body);
            }
            // Листовые — нет под-выражений.
            ExprKind::Ident(_) | ExprKind::Path(_) | ExprKind::SelfAccess
            | ExprKind::IntLit(_) | ExprKind::FloatLit(_) | ExprKind::BoolLit(_)
            | ExprKind::StrLit(_) | ExprKind::CharLit(_) | ExprKind::UnitLit => {}
        }
    }

    fn desugar_trailing(&mut self, t: &mut Trailing) {
        match t {
            Trailing::Block(b) => self.desugar_block(b),
            Trailing::LegacyBlockWithParams(tb) => self.desugar_block(&mut tb.body),
            Trailing::Fn(sb) => match &mut sb.body {
                FnBody::Expr(x) => self.desugar_expr(x),
                FnBody::Block(b) => self.desugar_block(b),
                FnBody::External => {}
            },
        }
    }
}
