//! Registry 221.1 #1517: a sum's variant constructor is checked against the
//! instance it builds.
//!
//! A record literal got its door with #1448 (`record_lit_schema.rs`); the
//! constructor of a TUPLE variant -- `Some(x)`, `Ok(v)`, `Err(e)`,
//! `Shape.Sq(1, 2)`, `Two(a, b)` -- had none. Its payload was walked for its own
//! sake and never compared with the variant's declared field type of the
//! instance the position expects, so `fn f() -> Option[str] => Some(1)` passed
//! `check` and failed in the C compiler (`NovaOpt_nova_int` returned as
//! `NovaOpt_nova_str`); `Some(1, 2)` passed `check` and failed in C ("too many
//! arguments"); and a bare `ro x = None` -- nothing says which `Option` it is --
//! was typed by whatever codegen defaulted to. One class (child of #262, the
//! "permissive by design / checker already guaranteed" ring): the checker
//! decided nothing about a variant constructor and codegen took what it found.
//!
//! Three doors, one per question:
//!
//! * `variant_ctor_compat` -- payload against the INSTANCE's field types; called
//!   from `assignable_direct`, so every position that already has a door (`let`
//!   annotation, argument, return, `if`/`match` tail, record field, collection
//!   element) gets this one too, nested constructors included (the payload is
//!   itself checked through the full `assignable`, D55 auto-wrap and literal
//!   adaptation kept). A mismatch is the position's own `E7301`.
//! * `check_variant_ctor_arity` -- the number of payload values, in every
//!   position (called from `f1_expr_inner`'s call arm): `E_VARIANT_CTOR_ARITY`.
//! * `check_untyped_variant_ctor` -- a binding with no declared type whose
//!   value is a generic sum's constructor that fixes not every parameter
//!   (`None`, `Err(e)`): `E_VARIANT_CTOR_UNTYPED`. D55 (`let x = value` without
//!   an annotation: "выводится тип значения") and D88 (a parameter neither the
//!   arguments nor a default fix is not inferred -- `first[]([])` is an error)
//!   give no third source, in particular no inference from a later use.
//!
//! What is NOT judged, on purpose: a constructor whose name is shadowed by a
//! local, a free fn or a type, or that two sums share (the bare name then names
//! no one sum, #962); a sum whose NAME two files of the unit declare (the lookup
//! may pick the wrong one -- measured on `std/src/net/error.nv`); a record variant (`record_lit_schema.rs` owns it); an
//! expected type that is not the constructor's own sum (D55 auto-wrap and
//! `#coerce` decide those, as before).

use super::*;

/// A variant constructor as written: the sum it belongs to, its variant, and
/// the call arguments (`None` for a unit variant written without a call).
pub(super) struct CtorUse<'e> {
    pub(super) sum: String,
    pub(super) variant: String,
    pub(super) args: Option<&'e [CallArg]>,
}

/// `E_VARIANT_CTOR_ARITY` text.
fn arity_message(sum: &str, variant: &str, want: usize, got: usize) -> String {
    format!(
        "[E_VARIANT_CTOR_ARITY] variant `{sum}.{variant}` takes {want} payload value{}, \
         but {got} {} given",
        if want == 1 { "" } else { "s" },
        if got == 1 { "was" } else { "were" },
    )
}

impl<'a> TypeCheckCtx<'a> {
    /// The constructor `expr` names, if any: `V`, `V(..)`, `Sum.V`, `Sum.V(..)`.
    /// A bare name counts only when it is not a local, a free fn or a type, and
    /// exactly one sum declares it; a qualified one when `Sum` is a declared sum
    /// with that variant and no static method of the same name.
    pub(super) fn variant_ctor_use<'e>(
        &self,
        expr: &'e Expr,
        scope: &HashMap<String, TypeRef>,
    ) -> Option<CtorUse<'e>> {
        let (callee, args) = match &expr.kind {
            ExprKind::Call { func, args, .. } => (&**func, Some(args.as_slice())),
            _ => (expr, None),
        };
        let (sum, variant) = match &callee.kind {
            ExprKind::Ident(name) => {
                if scope.contains_key(name)
                    || self.sig.fn_decls.contains_key(name)
                    || self.types_get_here(name).is_some()
                {
                    return None;
                }
                let owners = self.variant_owners.get(name)?;
                let [owner] = owners.as_slice() else { return None };
                (owner.clone(), name.clone())
            }
            ExprKind::Path(parts) if parts.len() == 2 => (parts[0].clone(), parts[1].clone()),
            ExprKind::Member { obj, name } => match &obj.kind {
                ExprKind::Ident(q) if !scope.contains_key(q) => (q.clone(), name.clone()),
                _ => return None,
            },
            _ => return None,
        };
        // A sum name declared in more than one file of the unit may resolve to
        // the wrong declaration here (#705's import-aware lookup does not match
        // every import spelling: `import std.io.{ErrorKind}` against
        // `std/src/io/error.nv`, while `nova-http` declares its own `ErrorKind`)
        // -- a verdict against the wrong schema would be a false refusal.
        if self.colliding_type_names.contains(&sum) {
            return None;
        }
        let td = self.types_get_here(&sum)?;
        let TypeDeclKind::Sum(variants) = &td.kind else { return None };
        if !variants.iter().any(|v| v.name == variant) || self.method_overloads(&sum, &variant).is_some() {
            return None;
        }
        Some(CtorUse { sum, variant, args })
    }

    /// The sum an expected type names, through plain aliases, with its type
    /// arguments (defaults filled in, D88). `None` for anything else.
    fn expected_sum_instance(&self, expected: &TypeRef, depth: u8) -> Option<(String, Vec<TypeRef>)> {
        let TypeRef::Named { path, generics, .. } = expected else { return None };
        let name = path.last()?;
        let td = self.types_get_here(name)?;
        match &td.kind {
            TypeDeclKind::Sum(_) => {
                let mut args = generics.clone();
                for g in td.generics.iter().skip(args.len()) {
                    args.push(g.default.clone()?);
                }
                (args.len() == td.generics.len()).then(|| (name.clone(), args))
            }
            TypeDeclKind::Alias(target) if depth < 8 && td.generics.is_empty() => {
                self.expected_sum_instance(target, depth + 1)
            }
            _ => None,
        }
    }

    /// Is `expr` a call of a constructor of `expected`'s own sum? Then a `Bad`
    /// from `assignable` is this door's payload verdict, not an inference gap --
    /// what lets the return door (`check_return_compat`, which reports `Bad`
    /// only for primitive returns, #959) report it too.
    pub(super) fn is_ctor_of_expected_sum(
        &self,
        expr: &Expr,
        expected: &TypeRef,
        scope: &HashMap<String, TypeRef>,
    ) -> bool {
        let Some(ctor) = self.variant_ctor_use(expr, scope) else { return false };
        ctor.args.is_some()
            && self.expected_sum_instance(expected, 0).is_some_and(|(sum, _)| sum == ctor.sum)
    }

    /// #1517 door 1: a constructor of `expected`'s own sum, its payload against
    /// the instance's field types. `None` = not this door's shape (the caller
    /// goes on as before).
    pub(super) fn variant_ctor_compat(
        &self,
        expr: &Expr,
        expected: &TypeRef,
        expr_gs: &GenericScope,
        exp_gs: &GenericScope,
        scope: &HashMap<String, TypeRef>,
    ) -> Option<Compat> {
        let ctor = self.variant_ctor_use(expr, scope)?;
        let args = ctor.args?;
        let (sum, targs) = self.expected_sum_instance(expected, 0)?;
        if sum != ctor.sum {
            return None;
        }
        let td = self.types_get_here(&sum)?;
        let TypeDeclKind::Sum(variants) = &td.kind else { return None };
        let v = variants.iter().find(|v| v.name == ctor.variant)?;
        let SumVariantKind::Tuple(fields) = &v.kind else { return None };
        if fields.len() != args.len() || args.iter().any(|a| !matches!(a, CallArg::Item(_))) {
            // The count is `check_variant_ctor_arity`'s; one reason, one report.
            return Some(Compat::Unknown);
        }
        let subst: HashMap<String, TypeRef> =
            td.generics.iter().map(|g| g.name.clone()).zip(targs.iter().cloned()).collect();
        for (i, (field, arg)) in fields.iter().zip(args).enumerate() {
            let want = subst_typeref(field, &subst);
            match self.assignable(arg.expr(), &want, expr_gs, exp_gs, scope) {
                Compat::Ok | Compat::Unknown => {}
                Compat::Bad { found } => {
                    return Some(Compat::Bad {
                        found: self.ctor_found_display(td, &ctor.variant, fields, &targs, i, &found),
                    });
                }
                other => return Some(other),
            }
        }
        None
    }

    /// How a refused constructor is named in the position's `E7301`: the
    /// instance its payload would build (`Option[int]`) when the refused field is
    /// one bare type parameter, else the constructor with the payload's type
    /// (`Shape.Sq(int, str)` style, the refused slot spelled `found`).
    fn ctor_found_display(
        &self,
        td: &TypeDecl,
        variant: &str,
        fields: &[TypeRef],
        targs: &[TypeRef],
        slot: usize,
        found: &str,
    ) -> String {
        if let TypeRef::Named { path, generics, .. } = &fields[slot] {
            if path.len() == 1 && generics.is_empty() {
                if let Some(pos) = td.generics.iter().position(|g| g.name == path[0]) {
                    let shown: Vec<String> = targs
                        .iter()
                        .enumerate()
                        .map(|(i, t)| if i == pos { found.to_string() } else { typeref_display(t) })
                        .collect();
                    return format!("{}[{}]", td.name, shown.join(", "));
                }
            }
        }
        let shown: Vec<String> = fields
            .iter()
            .enumerate()
            .map(|(i, f)| if i == slot { found.to_string() } else { typeref_display(f) })
            .collect();
        format!("{}.{}({})", td.name, variant, shown.join(", "))
    }

    /// #1517 door 2: the payload count of a tuple variant's constructor, and a
    /// unit variant called with values.
    pub(super) fn check_variant_ctor_arity(
        &self,
        expr: &Expr,
        scope: &HashMap<String, TypeRef>,
        errors: &mut Vec<Diagnostic>,
    ) {
        let Some(ctor) = self.variant_ctor_use(expr, scope) else { return };
        let Some(args) = ctor.args else { return };
        let Some(td) = self.types_get_here(&ctor.sum) else { return };
        let TypeDeclKind::Sum(variants) = &td.kind else { return };
        let Some(v) = variants.iter().find(|v| v.name == ctor.variant) else { return };
        let want = match &v.kind {
            SumVariantKind::Tuple(fields) => fields.len(),
            SumVariantKind::Unit if !args.is_empty() => 0,
            _ => return,
        };
        if args.iter().any(|a| matches!(a, CallArg::Spread(_))) || args.len() == want {
            return;
        }
        errors.push(Diagnostic::new(arity_message(&ctor.sum, &ctor.variant, want, args.len()), expr.span));
    }

    /// #1517 door 3: `ro x = None` -- a binding with no declared type whose
    /// value is a generic sum's constructor leaving a parameter unfixed (its
    /// payload does not mention it and it has no default).
    pub(super) fn check_untyped_variant_ctor(
        &self,
        value: &Expr,
        binding: &str,
        scope: &HashMap<String, TypeRef>,
        errors: &mut Vec<Diagnostic>,
    ) {
        let Some(ctor) = self.variant_ctor_use(value, scope) else { return };
        let Some(td) = self.types_get_here(&ctor.sum) else { return };
        if td.generics.is_empty() {
            return;
        }
        let TypeDeclKind::Sum(variants) = &td.kind else { return };
        let Some(v) = variants.iter().find(|v| v.name == ctor.variant) else { return };
        let fields: &[TypeRef] = match &v.kind {
            SumVariantKind::Tuple(fields) => fields,
            SumVariantKind::Unit => &[],
            SumVariantKind::Record(_) => return,
        };
        // A wrong count is door 2's.
        if ctor.args.map_or(0, |a| a.len()) != fields.len() {
            return;
        }
        let open: Vec<&str> = td
            .generics
            .iter()
            .filter(|g| g.default.is_none())
            .filter(|g| {
                let one: HashSet<String> = std::iter::once(g.name.clone()).collect();
                !fields.iter().any(|f| typeref_mentions_any(f, &one))
            })
            .map(|g| g.name.as_str())
            .collect();
        if open.is_empty() {
            return;
        }
        let params: Vec<&str> = td.generics.iter().map(|g| g.name.as_str()).collect();
        errors.push(Diagnostic::new(
            format!(
                "[E_VARIANT_CTOR_UNTYPED] `{}` has no type to take here: nothing fixes {} of `{}[{}]` -- \
                 `{}` has no declared type and the constructor's payload does not mention {}. \
                 Declare the type: `ro {} {}[...] = ...` (a binding's type comes from its value, \
                 never from a later use: D55, D88)",
                ctor.variant,
                open.iter().map(|n| format!("`{n}`")).collect::<Vec<_>>().join(", "),
                td.name,
                params.join(", "),
                binding,
                if open.len() == 1 { "it" } else { "them" },
                binding,
                td.name,
            ),
            value.span,
        ));
    }
}
