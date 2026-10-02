//! Registry 221.1 #1448 + #1096: a record literal is checked against the schema
//! of the record it builds.
//!
//! Every other position that holds a value of a known type -- argument, `let`
//! annotation, return, array element -- goes through `assignable`, with all of
//! its coercions (D44 literal adaptation, D55 sum/newtype auto-wrap, D227 range,
//! D54 narrowing, D429 `#coerce`). The field of a record literal did not: its
//! value was walked for its own sake and never compared with the declared field
//! type, so `Hold { tok: Other { .. } }` at `tok Tok` built a `Hold` whose `tok`
//! pointed at an `Other` (#1448). Completeness had one door out of three: the
//! named literal of a plain record (`E_MISSING_FIELD_IN_LITERAL`, #1142); the
//! record VARIANT of a sum, `Self { .. }` and the ANONYMOUS literal whose type
//! comes from the position (`ro u User = { id: 2 }`) left the missing field
//! uninitialised (#1096). A field the record does not declare reached the C
//! compiler.
//!
//! Two entry points, one check (`record_lit_faults`):
//!
//! * `check_named_record_lit` -- `T { .. }`, `Sum.Variant { .. }`,
//!   `Variant { .. }`, `Self { .. }`; called from `f1_expr_inner`, where every
//!   named literal passes whatever its position;
//! * `anon_record_lit_compat` -- `{ .. }` against an expected record type; called
//!   from `assignable_direct`, so every position that already has a door gets
//!   this one for free, nested anonymous literals included (the field value is
//!   itself checked through `assignable`).
//!
//! What is NOT judged, on purpose: a field whose declared type mentions the
//! record's own type parameters with no concrete argument known (a bare generic
//! literal -- its arguments are inferred FROM the fields); `#from_fields` targets
//! (`{ k: v }` there is a map, D55 map-coercion); a literal with a spread (D60)
//! is not required to be complete -- the rest comes from the donor.

use super::*;

/// The declared shape a record literal is checked against.
pub(super) struct LitSchema<'t> {
    /// How the diagnostics name the type (`Hold`, `Shape.Sq`).
    pub(super) display: String,
    pub(super) fields: &'t [RecordField],
    /// The record's own type parameters.
    pub(super) own_generics: HashSet<String>,
    /// Concrete arguments for `own_generics`, when the position knows them.
    pub(super) subst: HashMap<String, TypeRef>,
    /// Associated constants: `E_CONST_FIELD_IN_LITERAL` owns those names.
    pub(super) assoc_consts: Vec<String>,
    /// What a spread source must be (D60 rule 5: "строго тот же тип");
    /// `None` where that type is not a plain name (a variant, a generic).
    pub(super) spread_target: Option<TypeRef>,
}

/// `E_MISSING_FIELD_IN_LITERAL` text -- shared by #1142's plain-record site and
/// the doors this module adds, so one reason reads one way.
pub(super) fn missing_field_message(type_display: &str, missing: &[&str]) -> String {
    format!(
        "[E_MISSING_FIELD_IN_LITERAL] record literal `{}{{ … }}` does not initialise {}: {}. \
         Construction requires every declared field (D02 §Construction); add it, or copy the \
         rest from another value with `...other`.",
        type_display,
        if missing.len() == 1 { "field" } else { "fields" },
        missing.join(", "),
    )
}

impl<'a> TypeCheckCtx<'a> {
    /// The schema of a declared type, as a record-literal target: a plain
    /// record (through aliases), never a `#from_fields` map type.
    fn record_schema_of_type(&self, name: &str, args: &[TypeRef], depth: u8) -> Option<LitSchema<'a>> {
        let td = self.types_get_here(name)?;
        match &td.kind {
            TypeDeclKind::Record(fields) => {
                if td.attrs.iter().any(|a| matches!(a, TypeAttr::FromFields)) {
                    return None;
                }
                let own_generics: HashSet<String> = td.generics.iter().map(|g| g.name.clone()).collect();
                let subst = if args.len() == td.generics.len() {
                    td.generics.iter().map(|g| g.name.clone()).zip(args.iter().cloned()).collect()
                } else {
                    HashMap::new()
                };
                let spread_target = td.generics.is_empty().then(|| TypeRef::Named {
                    path: vec![name.to_string()],
                    generics: Vec::new(),
                    span: Span::default(),
                });
                Some(LitSchema {
                    display: name.to_string(),
                    fields: fields.as_slice(),
                    own_generics,
                    subst,
                    assoc_consts: td.assoc_consts.iter().map(|c| c.name.clone()).collect(),
                    spread_target,
                })
            }
            TypeDeclKind::Alias(TypeRef::Named { path, generics, .. })
                if depth < 8 && td.generics.is_empty() =>
            {
                self.record_schema_of_type(path.last()?, generics, depth + 1)
            }
            _ => None,
        }
    }

    /// The record variant `variant` of the sum `sum_name`.
    fn variant_schema(&self, sum_name: &str, variant: &str) -> Option<LitSchema<'a>> {
        let td = self.types_get_here(sum_name)?;
        let TypeDeclKind::Sum(vs) = &td.kind else { return None };
        let v = vs.iter().find(|v| v.name == variant)?;
        let SumVariantKind::Record(fields) = &v.kind else { return None };
        Some(LitSchema {
            display: format!("{sum_name}.{variant}"),
            fields: fields.as_slice(),
            own_generics: td.generics.iter().map(|g| g.name.clone()).collect(),
            subst: HashMap::new(),
            assoc_consts: Vec::new(),
            spread_target: None,
        })
    }

    /// `T { .. }`, `Sum.V { .. }`, `V { .. }`, `Self { .. }`. The second value
    /// says whether this door owns completeness: the plain named record has
    /// its own site (#1142, `walk_expr`), everything else is new here.
    fn named_lit_schema(&self, path: &[String]) -> Option<(LitSchema<'a>, bool)> {
        let last = path.last()?;
        if last == "Self" {
            let recv = self.current_recv_type.borrow().clone()?;
            return self.record_schema_of_type(&recv, &[], 0).map(|s| (s, true));
        }
        if path.len() >= 2 {
            let owner = &path[path.len() - 2];
            if let Some(s) = self.variant_schema(owner, last) {
                return Some((s, true));
            }
        }
        if self.types_get_here_contains(last) {
            return self.record_schema_of_type(last, &[], 0).map(|s| (s, false));
        }
        // A bare record variant: the ONE sum declaring it (the owner search
        // `f1_expr_inner` and `infer_expr_type` already do for this shape).
        let mut owner: Option<&String> = None;
        for (tn, td) in self.types.iter() {
            if let TypeDeclKind::Sum(vs) = &td.kind {
                if vs.iter().any(|v| &v.name == last && matches!(v.kind, SumVariantKind::Record(_))) {
                    if owner.is_some() {
                        return None;
                    }
                    owner = Some(tn);
                }
            }
        }
        self.variant_schema(owner?, last).map(|s| (s, true))
    }

    /// Every fault of `fields` against `schema`, as ready diagnostics.
    fn record_lit_faults(
        &self,
        fields: &[RecordLitField],
        schema: &LitSchema<'_>,
        lit_span: Span,
        check_missing: bool,
        expr_gs: &GenericScope,
        exp_gs: &GenericScope,
        scope: &HashMap<String, TypeRef>,
    ) -> Vec<(String, Span)> {
        let mut faults = Vec::new();
        let ty = &schema.display;
        // D60: the donor of `...src` is of the literal's own type.
        if let Some(target) = &schema.spread_target {
            for src in fields.iter().filter(|f| f.is_spread).filter_map(|f| f.value.as_ref()) {
                if let Compat::Bad { found } = self.assignable(src, target, expr_gs, exp_gs, scope) {
                    faults.push((
                        format!(
                            "[E7301] cannot spread a value of type `{found}` into a `{ty}` \
                             literal -- the source of `...` is of the literal's own type (D60)"
                        ),
                        src.span,
                    ));
                }
            }
        }
        for f in fields.iter().filter(|f| !f.is_spread) {
            let Some(decl) = schema.fields.iter().find(|d| d.name == f.name) else {
                if !schema.assoc_consts.iter().any(|c| c == &f.name) {
                    faults.push((
                        format!("[E_RECORD_UNKNOWN_FIELD] record `{ty}` has no field `{}`", f.name),
                        f.span,
                    ));
                }
                continue;
            };
            let declared = subst_typeref(&decl.ty, &schema.subst);
            if typeref_mentions_any(&declared, &schema.own_generics) {
                continue;
            }
            // `{ name }` punning (D52): the value is the binding of that name.
            let punned;
            let value = match &f.value {
                Some(v) => v,
                None => {
                    punned = Expr::new(ExprKind::Ident(f.name.clone()), f.span);
                    &punned
                }
            };
            let field = &f.name;
            let shown = typeref_display(&declared);
            match self.assignable(value, &declared, expr_gs, exp_gs, scope) {
                Compat::Ok | Compat::Unknown => {}
                Compat::Bad { found } => faults.push((
                    format!(
                        "[E7301] cannot initialise field `{field}` of `{ty}` declared as \
                         `{shown}` with a value of type `{found}`"
                    ),
                    value.span,
                )),
                Compat::OutOfRange { msg } => {
                    faults.push((super::literal_exact::literal_diag(&msg), value.span))
                }
                Compat::Narrowing { from, to } => faults.push((
                    format!(
                        "[E_IMPLICIT_NARROWING] cannot initialise field `{field}` of `{ty}` \
                         with a value of type `{from}` -- `{to}` is narrower; implicit int \
                         narrowing loses range; use an explicit `... as {to}` cast (D54)"
                    ),
                    value.span,
                )),
                Compat::CoerceConflict { msg } => faults.push((msg, value.span)),
                Compat::RecordLit { faults: inner } => faults.extend(inner),
            }
        }
        if check_missing && !fields.iter().any(|f| f.is_spread) {
            let missing: Vec<&str> = schema
                .fields
                .iter()
                .filter(|d| !d.is_embed)
                .filter(|d| !fields.iter().any(|f| f.name == d.name))
                .map(|d| d.name.as_str())
                .collect();
            if !missing.is_empty() {
                faults.push((missing_field_message(ty, &missing), lit_span));
            }
        }
        faults
    }

    /// Named literal door, from `f1_expr_inner`.
    pub(super) fn check_named_record_lit(
        &self,
        e: &Expr,
        path: &[String],
        fields: &[RecordLitField],
        gs: &GenericScope,
        scope: &HashMap<String, TypeRef>,
        errors: &mut Vec<Diagnostic>,
    ) {
        let Some((schema, owns_missing)) = self.named_lit_schema(path) else { return };
        for (msg, span) in self.record_lit_faults(fields, &schema, e.span, owns_missing, gs, gs, scope) {
            errors.push(Diagnostic::new(msg, span));
        }
    }

    /// Anonymous literal door, from `assignable_direct`: `None` when `expected`
    /// is not a record type (the caller's own rules apply), otherwise the
    /// verdict of the whole literal.
    pub(super) fn anon_record_lit_compat(
        &self,
        expr: &Expr,
        fields: &[RecordLitField],
        expected: &TypeRef,
        expr_gs: &GenericScope,
        exp_gs: &GenericScope,
        scope: &HashMap<String, TypeRef>,
    ) -> Option<Compat> {
        let TypeRef::Named { path, generics, .. } = expected.strip_modifiers() else { return None };
        let schema = self.record_schema_of_type(path.last()?, generics, 0)?;
        let faults = self.record_lit_faults(fields, &schema, expr.span, true, expr_gs, exp_gs, scope);
        Some(if faults.is_empty() { Compat::Ok } else { Compat::RecordLit { faults } })
    }
}
