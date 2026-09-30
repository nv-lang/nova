//! Registry 221.1 #1097 (D134): a free function's C symbol is decided by its
//! DECLARING module, never by its bare name.
//!
//! The D84 free-fn registry (`method_overloads[("", name)]`) used to build every
//! overload's base symbol with `free_fn_c_name(name)`, i.e. from `fn_module_map`,
//! which remembers only the FIRST module that declared the name. When one name
//! lives in several modules with at least two distinct signatures (so it is not
//! routed through `colliding_fn_names`), every declaration got the first module's
//! base: `beta.tag(int)` was dropped as a "duplicate" of `alpha.tag(int)` and its
//! body emitted under `alpha`'s symbol, `gamma.tag(int, int)` became
//! `nova_fn_<alpha>tag__nova_int_nova_int`, and the definition side then matched
//! by parameter C-types — first match wins. Two functions, one C symbol.
//!
//! Three rules restore D134's invariant, all keyed by the declaration itself:
//! the base comes from the declaring file's module (`decl_base_c_name`), a
//! declaration is a duplicate only of an identical one in the SAME module
//! (`same_decl_module`), and the definition takes the registry entry that
//! carries its own span before any parameter-type guess (`own_registry_sig`).
//! Call sites already read the checker's `resolved_callees` span against the
//! same `fn_span`, so a resolved call now reaches the declaration it names.
//! A fn VALUE (`ro f = tag`) reads the same channel under the Ident's own id
//! (written by `types/fn_visibility.rs`) for its thunk, target and signature.

use super::{CEmitter, MethodSig};
use crate::ast::FnDecl;
use crate::ast::ExprId;
use crate::diag::Span;

impl CEmitter {
    /// Base C symbol for registering free fn `f` in the D84 registry.
    ///
    /// Byte-identical to `free_fn_c_name` whenever the declaring module is the
    /// one `fn_module_map` already names (every name declared in one module),
    /// and whenever the name is routed elsewhere (literal `extern "C"`,
    /// runtime registry, file-private map). Only a namesake from ANOTHER module
    /// gets its own module's base instead of the first declarer's.
    pub(super) fn decl_base_c_name(&self, f: &FnDecl) -> String {
        let by_name = self.free_fn_c_name(&f.name);
        if f.is_external {
            return by_name;
        }
        let (Some(own), Some(first)) = (
            self.emit_file_module.get(&f.span.file_id),
            self.fn_module_map.get(&f.name),
        ) else {
            return by_name;
        };
        if own.is_empty() || own == first || by_name != Self::mangle_free_fn(first, &f.name) {
            return by_name;
        }
        Self::mangle_free_fn(own, &f.name)
    }

    /// Whether registry entry `sig` (declared at `sig_span`) and declaration
    /// `f` come from the same module. Unknown module on either side answers
    /// `true` — the pre-#1097 behaviour, where every entry shared one base.
    pub(super) fn same_decl_module(&self, sig_span: Option<Span>, f: &FnDecl) -> bool {
        let Some(sp) = sig_span else { return true };
        match (
            self.emit_file_module.get(&sp.file_id),
            self.emit_file_module.get(&f.span.file_id),
        ) {
            (Some(a), Some(b)) => a == b,
            _ => true,
        }
    }

    /// The registry entry registered for THIS declaration (by its span), if any.
    pub(super) fn own_registry_sig<'a>(overloads: &'a [MethodSig], f: &FnDecl) -> Option<&'a MethodSig> {
        overloads.iter().find(|s| s.fn_span == Some(f.span))
    }

    /// A free fn named as a VALUE by the Ident `ident`: the registry entry of the
    /// declaration the checker resolved that Ident to (`resolved_callees`, written
    /// by `types/fn_visibility.rs` only when the name has several declarations).
    /// `None` keeps the by-name path, byte-identical for every other name.
    pub(super) fn fn_value_sig(&self, name: &str, ident: ExprId) -> Option<MethodSig> {
        let span = self.resolved_callees.get(&ident)?;
        self.method_overloads
            .get(&(String::new(), name.to_string()))?
            .iter()
            .find(|s| s.fn_span == Some(*span))
            .cloned()
    }

    /// A fn VALUE whose name has registry entries from two or more modules and
    /// no checker answer (two imports of the name, none of them the caller's
    /// own module): refused. The by-name fallback would take the first declaring
    /// module's symbol — a guess that links, and calls another module's function.
    pub(super) fn refuse_unresolved_fn_value(&self, name: &str, ident: ExprId) -> Result<(), String> {
        if self.resolved_callees.contains_key(&ident) {
            return Ok(());
        }
        let Some(sigs) = self.method_overloads.get(&(String::new(), name.to_string())) else { return Ok(()) };
        let module_of = |s: &MethodSig| s.fn_span.and_then(|sp| self.emit_file_module.get(&sp.file_id));
        let first = sigs.first().and_then(|s| module_of(s));
        if sigs.iter().all(|s| module_of(s) == first) {
            return Ok(());
        }
        Err(format!(
            "internal compiler error (registry 221.1 #1097): `{name}` is used as a function \
             value, several modules declare a free fn `{name}`, and the checker did not resolve \
             which one this reference means. Taking one by name would silently bind another \
             module's function. Import only the intended one into this file, or wrap the call \
             in a lambda."
        ))
    }

    /// `(param C types, return C type)` of the free fn value `ident` names —
    /// what `ro f = tag` gives `f`, so `f(x)` picks the right closure-call macro.
    pub(super) fn fn_value_param_sig(&self, name: &str, ident: ExprId) -> Option<(Vec<String>, String)> {
        match self.fn_value_sig(name, ident) {
            Some(sig) => Some((sig.param_c_types, sig.return_c_type)),
            None => self.user_fn_sigs.get(name).cloned(),
        }
    }

    /// C symbol of free fn `name` referenced as a value by `ident`: the registry
    /// entry the checker named; else, for a name routed around the registry as a
    /// cross-module collision, the per-module symbol of the declaring file the
    /// checker named (the #1090 door for calls); else the by-name symbol.
    pub(super) fn fn_value_c_name(&self, name: &str, ident: ExprId) -> String {
        if let Some(sig) = self.fn_value_sig(name, ident) {
            return sig.c_name;
        }
        self.resolved_callees
            .get(&ident)
            .and_then(|sp| self.file_priv_fn_c_names.get(&(sp.file_id, name.to_string())))
            .cloned()
            .unwrap_or_else(|| self.free_fn_c_name(name))
    }

    /// Plan 14 Ф.3: a free fn named as a first-class value (`ro f = inc`,
    /// `xs.map(inc)`) becomes a closure value `(void*)NovaClos_X*` over an envless
    /// thunk `<symbol>_thunk(void* env, args...)` emitted once per SYMBOL.
    /// `None` when the fn has no known signature (generic, method, ...) — the
    /// caller falls back to the raw `nova_fn_<name>` pointer.
    ///
    /// #1097: signature and target come from the declaration the value names
    /// (`fn_value_sig`), and the thunk is deduplicated by the target
    /// symbol, not the bare name — by name, `beta`'s `ro f = tag` reused
    /// `alpha`'s thunk and took another module's signature from `user_fn_sigs`.
    pub(super) fn emit_free_fn_value(&mut self, fn_name: &str, ident: ExprId) -> Option<String> {
        let (param_c_tys, ret_c_ty, target) = match self.fn_value_sig(fn_name, ident) {
            Some(sig) => (sig.param_c_types, sig.return_c_type, sig.c_name),
            None => {
                let (p, r) = self.user_fn_sigs.get(fn_name).cloned()?;
                (p, r, self.fn_value_c_name(fn_name, ident))
            }
        };
        let thunk_name = format!("{}_thunk", target);
        if self.emitted_fn_thunks.insert(thunk_name.clone()) {
            let mut params = vec!["void* _env".to_string()];
            for (i, ty) in param_c_tys.iter().enumerate() {
                params.push(format!("{} p{}", ty, i));
            }
            let params_str = params.join(", ");
            let storage = self.top_level_storage();
            self.lambda_forward_decls
                .push_str(&format!("{}{} {}({});\n", storage, ret_c_ty, thunk_name, params_str));
            let call_args: Vec<String> = (0..param_c_tys.len()).map(|i| format!("p{}", i)).collect();
            let mut impl_buf = format!("{}{} {}({}) {{\n    (void)_env;\n", storage, ret_c_ty, thunk_name, params_str);
            if ret_c_ty == "nova_unit" {
                impl_buf.push_str(&format!("    {}({});\n    return NOVA_UNIT;\n", target, call_args.join(", ")));
            } else {
                impl_buf.push_str(&format!("    return {}({});\n", target, call_args.join(", ")));
            }
            impl_buf.push_str("}\n\n");
            self.lambda_impls.push_str(&impl_buf);
        }
        let clos_struct = Self::clos_struct_name(&param_c_tys, &ret_c_ty);
        let clos_fn_ty = Self::clos_fn_ty(&param_c_tys, &ret_c_ty);
        let clos_tmp = self.fresh_tmp();
        self.line(&format!("{}* {} = ({}*)nova_alloc(sizeof({}));", clos_struct, clos_tmp, clos_struct, clos_struct));
        self.line(&format!("{}->fn = ({})({});", clos_tmp, clos_fn_ty, thunk_name));
        self.line(&format!("{}->env = (void*)0;", clos_tmp));
        Some(format!("(void*)({})", clos_tmp))
    }
}
