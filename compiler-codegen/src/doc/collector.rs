//! Plan 45 Ф.4 — collector: AST → DocTree.
//!
//! MVP: один pass, без passes-pipeline (Plan 45 §3). Production-grade
//! passes (`strip_private`, `propagate_stability`, `resolve_intra_doc_links`,
//! `collect_doc_tests`, `lint_docs`) добавляются инкрементально как
//! отдельные модули в `doc/passes/`.

use crate::ast::{
    ConstDecl, FnDecl, Item, Module, ModuleAttrKind, TypeDecl, TypeDeclKind,
};
use crate::doc::doctree::*;

/// Plan 45 Ф.4 — построить `DocTree` из парсенного, type-checked `Module`.
///
/// Поведение MVP:
/// - Один module → один `DocModule`.
/// - Items: `Fn` / `Type` / `Const` собираются. `Effect` / `Protocol`
///   через `TypeDeclKind::Effect` / `TypeDeclKind::Protocol`.
/// - Visibility: `is_export = true` → `Export`, иначе `Private`.
///   По дефолту filter — Export-only; flag `--include-private` (Plan 45
///   Ф.12) переключает (на collector-уровне всё собирается; filter — в
///   renderer'е).
/// - Module summary: из `module.doc` (`//!` inner) + любых `#doc "..."`
///   module-attr (D101).
pub fn collect(module: &Module) -> DocTree {
    let mut tree = DocTree::new();
    let module_path = module.name.clone();

    // Module-level documentation: концат `//!` (inner doc) + все
    // `#doc "..."` module-attr строки (alphabetical filename order
    // уже обеспечивается parser'ом).
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

    let mut items: Vec<DocItem> = Vec::new();
    for item in &module.items {
        match item {
            Item::Fn(f) => {
                items.push(collect_fn(&module_path, f));
            }
            Item::Type(t) => {
                items.push(collect_type(&module_path, t));
            }
            Item::Const(c) => {
                items.push(collect_const(&module_path, c));
            }
            // `Item::Let` / `Item::Test` / `Item::Lemma` не документируются.
            Item::Let(_) | Item::Test(_) | Item::Lemma(_) => {}
        }
    }

    // Deterministic order: по `id`.
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

    tree.modules.push(DocModule {
        path: module_path,
        name: module_name,
        kind,
        peers,
        summary: module_summary,
        description: module_description,
        items,
        source_span: module.span,
    });

    tree
}

fn collect_fn(module_path: &[String], f: &FnDecl) -> DocItem {
    let module_str = module_path.join(".");
    let id = match &f.receiver {
        Some(r) => format!("{}::{}.{}", module_str, r.type_name, f.name),
        None => format!("{}::{}", module_str, f.name),
    };
    let (summary, description, sections) = crate::doc::doctree::split_doc(&f.doc);
    let visibility = if f.is_export {
        Visibility::Export
    } else {
        Visibility::Private
    };
    let signature = build_signature(f);
    DocItem {
        id,
        module_path: module_path.to_vec(),
        name: f.name.clone(),
        visibility,
        summary,
        description,
        sections,
        deprecation: None,
        stability: None,
        kind: ItemKind::Fn(signature),
        source_span: f.span,
    }
}

fn collect_type(module_path: &[String], t: &TypeDecl) -> DocItem {
    let module_str = module_path.join(".");
    let id = format!("{}::{}", module_str, t.name);
    let (summary, description, sections) = crate::doc::doctree::split_doc(&t.doc);
    let visibility = if t.is_export {
        Visibility::Export
    } else {
        Visibility::Private
    };
    let kind = match &t.kind {
        TypeDeclKind::Record(fields) => {
            let record_fields = fields
                .iter()
                .map(|f| RecordField {
                    name: f.name.clone(),
                    ty: render_type(&f.ty),
                    mutable: f.mutable,
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
        TypeDeclKind::Newtype(ty) => ItemKind::Type(TypeDefinition::Alias(render_type(ty))),
        TypeDeclKind::Effect(methods) => {
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
            ItemKind::Effect { methods: sigs }
        }
        TypeDeclKind::Protocol(methods) => {
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
            ItemKind::Protocol { methods: sigs }
        }
    };
    DocItem {
        id,
        module_path: module_path.to_vec(),
        name: t.name.clone(),
        visibility,
        summary,
        description,
        sections,
        deprecation: None,
        stability: None,
        kind,
        source_span: t.span,
    }
}

fn collect_const(module_path: &[String], c: &ConstDecl) -> DocItem {
    let module_str = module_path.join(".");
    let id = format!("{}::{}", module_str, c.name);
    let (summary, description, sections) = crate::doc::doctree::split_doc(&c.doc);
    let visibility = if c.is_export {
        Visibility::Export
    } else {
        Visibility::Private
    };
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
        deprecation: None,
        stability: None,
        kind: ItemKind::Const {
            ty,
            value: render_expr(&c.value),
        },
        source_span: c.span,
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
            bound: g.bound.as_ref().map(render_type),
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
    // Effect-row: rendered as alphabetical-sorted set для детерминизма.
    let mut effects: Vec<String> =
        f.effects.iter().map(render_effect).collect();
    effects.sort();
    effects.dedup();
    // Raises: вытащить из `Fail[X]` в effect-row.
    let mut raises: Vec<String> = Vec::new();
    for eff in &f.effects {
        if let Some(inner) = extract_fail_inner(eff) {
            raises.push(inner);
        }
    }
    raises.sort();
    raises.dedup();
    Signature {
        receiver,
        generics,
        params,
        return_type,
        effects,
        raises,
    }
}

/// Минимальный pretty-print TypeRef в Nova source. **MVP-простой** —
/// для популярных форм; сложные случаи могут округляться (best-effort
/// строковое представление).
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
        TypeRef::Unit(_) => "()".to_string(),
    }
}

/// Effect — это `TypeRef` (обычно `Named`). Render через `render_type`.
fn render_effect(eff: &crate::ast::TypeRef) -> String {
    render_type(eff)
}

/// Извлечь имя `X` из effect-row элемента `Fail[X]` (для `raises`-списка).
/// Возвращает `None`, если элемент не `Fail[...]`.
fn extract_fail_inner(eff: &crate::ast::TypeRef) -> Option<String> {
    use crate::ast::TypeRef;
    if let TypeRef::Named { path, generics, .. } = eff {
        if path.len() == 1 && path[0] == "Fail" && !generics.is_empty() {
            return Some(render_type(&generics[0]));
        }
    }
    None
}

/// Минимальный pretty-print выражения в Nova source. **MVP**: для
/// литералов и Ident — точный; для остального — placeholder.
/// Полный pretty-printer — Plan 45.A или separate util.
fn render_expr(e: &crate::ast::Expr) -> String {
    use crate::ast::{ArrayElem, BinOp, CallArg, ExprKind, UnOp};
    match &e.kind {
        ExprKind::IntLit(n) => n.to_string(),
        ExprKind::FloatLit(f) => f.to_string(),
        ExprKind::BoolLit(b) => b.to_string(),
        ExprKind::StrLit(s) => format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"")),
        ExprKind::CharLit(c) => format!("'{}'", char::from_u32(*c).unwrap_or('?')),
        ExprKind::UnitLit => "()".to_string(),
        ExprKind::Ident(name) => name.clone(),
        ExprKind::Path(parts) => parts.join("."),
        ExprKind::ArrayLit(elems) => {
            let parts: Vec<String> = elems
                .iter()
                .map(|el| match el {
                    ArrayElem::Item(x) => render_expr(x),
                    ArrayElem::Spread(x) => format!("...{}", render_expr(x)),
                })
                .collect();
            format!("[{}]", parts.join(", "))
        }
        ExprKind::TupleLit(xs) => {
            let parts: Vec<String> = xs.iter().map(render_expr).collect();
            format!("({})", parts.join(", "))
        }
        ExprKind::RecordLit { type_name, fields } => {
            let head = type_name
                .as_ref()
                .map(|p| format!("{} ", p.join(".")))
                .unwrap_or_default();
            let parts: Vec<String> = fields
                .iter()
                .map(|f| {
                    if f.is_spread {
                        format!("...{}", f.value.as_ref().map(render_expr).unwrap_or_default())
                    } else {
                        match &f.value {
                            Some(v) => format!("{}: {}", f.name, render_expr(v)),
                            None => f.name.clone(),
                        }
                    }
                })
                .collect();
            format!("{}{{ {} }}", head, parts.join(", "))
        }
        ExprKind::Member { obj, name } => format!("{}.{}", render_expr(obj), name),
        ExprKind::Unary { op, operand } => {
            let sym = match op {
                UnOp::Neg => "-",
                UnOp::Not => "!",
            };
            format!("{}{}", sym, render_expr(operand))
        }
        ExprKind::Binary { op, left, right } => {
            let sym = match op {
                BinOp::Add => "+", BinOp::Sub => "-", BinOp::Mul => "*",
                BinOp::Div => "/", BinOp::Mod => "%",
                BinOp::Eq => "==", BinOp::Neq => "!=",
                BinOp::Lt => "<", BinOp::Le => "<=",
                BinOp::Gt => ">", BinOp::Ge => ">=",
                BinOp::And => "&&", BinOp::Or => "||",
                BinOp::Implies => "==>", BinOp::Iff => "<==>",
                BinOp::BitAnd => "&", BinOp::BitOr => "|", BinOp::BitXor => "^",
                BinOp::Shl => "<<", BinOp::Shr => ">>",
            };
            format!("{} {} {}", render_expr(left), sym, render_expr(right))
        }
        ExprKind::Call { func, args, .. } => {
            let parts: Vec<String> = args
                .iter()
                .map(|a| match a {
                    CallArg::Item(x) => render_expr(x),
                    CallArg::Spread(x) => format!("...{}", render_expr(x)),
                    CallArg::Named { name, value } => format!("{}: {}", name, render_expr(value)),
                })
                .collect();
            format!("{}({})", render_expr(func), parts.join(", "))
        }
        _ => "...".to_string(),
    }
}
