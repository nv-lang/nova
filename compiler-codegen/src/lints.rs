//! Lint-проходы по AST.
//!
//! Lint — это **warning**, не error: компилятор возвращает Diagnostic'и,
//! но компиляция продолжается. CLI решает выводить ли их (по умолчанию
//! да; `--no-lint` отключает). В отличие от parser/typecheck-error'ов,
//! lints программист может игнорировать.
//!
//! Текущие правила:
//!  - `export-fail-untyped`: `export fn ... Fail -> ...` без `[E]` —
//!    warning. Public API должен иметь typed Fail (D65 convention).

use crate::ast::{FnDecl, Item, Module, TypeDeclKind, TypeRef};
use crate::diag::Diagnostic;
use std::collections::HashSet;

/// Один lint-warning.
#[derive(Debug, Clone)]
pub struct LintWarning {
    pub rule: &'static str,
    pub diag: Diagnostic,
}

/// Прогон всех lint-проверок на модуле. Возвращает список warning'ов.
pub fn lint_module(m: &Module) -> Vec<LintWarning> {
    let mut warnings = Vec::new();
    let effect_names = collect_effect_names(m);
    let protocol_names = collect_protocol_names(m);
    for item in &m.items {
        if let Item::Fn(f) = item {
            check_fn(f, &mut warnings);
            check_protocol_in_effect_position(f, &protocol_names, &effect_names, &mut warnings);
        }
    }
    warnings
}

/// Собирает имена user-defined эффектов: `type X effect { ... }`.
/// Также включает встроенные stdlib effects из prelude (D26 + D62).
fn collect_effect_names(m: &Module) -> HashSet<String> {
    let mut names: HashSet<String> = [
        "Fail", "Io", "Net", "Db", "Fs", "Time", "Random",
        "Log", "Trace", "Ask", "Alloc", "Detach", "Blocking", "Mem",
    ].iter().map(|s| s.to_string()).collect();
    for item in &m.items {
        if let Item::Type(td) = item {
            if matches!(td.kind, TypeDeclKind::Effect(_)) {
                names.insert(td.name.clone());
            }
        }
    }
    names
}

/// Собирает имена user-defined protocols: `type X protocol { ... }`.
/// Также включает встроенные prelude protocols.
fn collect_protocol_names(m: &Module) -> HashSet<String> {
    // Bootstrap-парсер представляет `type X protocol { ... }` через
    // отдельный TypeDeclKind? Проверка в коде ниже — пока через
    // собирание известных имён.
    let mut names: HashSet<String> = [
        "Hashable", "Ord", "Eq", "Iter", "From", "Into",
        "TryFrom", "TryInto", "ToStr",
    ].iter().map(|s| s.to_string()).collect();
    // User-defined: parser хранит protocol через TypeDeclKind::Effect
    // (D53 unification), но различает по флагу `is_effect` который
    // в bootstrap'е отсутствует. Прокси: скан items не покрывает
    // user protocols. Достаточно встроенных.
    for item in &m.items {
        if let Item::Type(td) = item {
            // В bootstrap-AST `protocol` пока эффективно не отличается
            // от `effect`. Если будущая ревизия добавит TypeDeclKind::Protocol,
            // здесь проверка обновится.
            let _ = td;
        }
    }
    names
}

/// Rule: `protocol-in-effect-position` — `fn f() Hashable -> ()` где
/// `Hashable` это protocol. Should be `fn f(x T Hashable) -> ()` (как
/// generic-bound на параметре, D72) или `fn f[T Hashable](x T) -> ()`.
fn check_protocol_in_effect_position(
    f: &FnDecl,
    protocols: &HashSet<String>,
    effects: &HashSet<String>,
    out: &mut Vec<LintWarning>,
) {
    for eff in &f.effects {
        if let TypeRef::Named { path, .. } = eff {
            if path.len() == 1 {
                let name = &path[0];
                if protocols.contains(name) && !effects.contains(name) {
                    out.push(LintWarning {
                        rule: "protocol-in-effect-position",
                        diag: Diagnostic::new(
                            format!(
                                "warning: `{}` is a protocol, not an effect, but appears in \
                                 effect-position (between `)` and `->`) of fn `{}` \
                                 (D62: protocols are structural type-bounds, not handler-substitutable; \
                                 use `fn {} (x T {}) -> ...` or generic-bound `[T {}]` instead)",
                                name, f.name, f.name, name, name
                            ),
                            eff.span(),
                        ),
                    });
                }
            }
        }
    }
}

fn check_fn(f: &FnDecl, out: &mut Vec<LintWarning>) {
    if !f.is_export {
        return;
    }
    // Rule: export-fail-untyped — `Fail` без [E] в public API.
    for eff in &f.effects {
        if is_fail_untyped(eff) {
            let span = eff.span();
            out.push(LintWarning {
                rule: "export-fail-untyped",
                diag: Diagnostic::new(
                    format!(
                        "warning: export fn `{}` uses `Fail` without type parameter \
                         (D65 convention: public API should specify `Fail[E]` with concrete error type; \
                         use `Fail[any]` to opt into explicit erasure)",
                        f.name
                    ),
                    span,
                ),
            });
        }
    }
}

/// `Fail` без generic-параметра. Не путаем с `Fail[E]` (typed) или
/// `Fail[any]` (явная erasure — программист сознательно opt-in).
fn is_fail_untyped(ty: &TypeRef) -> bool {
    if let TypeRef::Named { path, generics, .. } = ty {
        if path.len() == 1 && path[0] == "Fail" && generics.is_empty() {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::lex;
    use crate::parser::Parser;

    fn parse(src: &str) -> Module {
        let toks = lex(src).unwrap();
        let mut p = Parser::new(toks);
        p.parse_module().unwrap()
    }

    #[test]
    fn warns_on_export_fail_untyped() {
        let m = parse("module foo\nexport fn parse(s str) Fail -> int => 0\n");
        let ws = lint_module(&m);
        assert_eq!(ws.len(), 1);
        assert_eq!(ws[0].rule, "export-fail-untyped");
    }

    #[test]
    fn no_warning_on_export_fail_typed() {
        let m = parse("module foo\nexport fn parse(s str) Fail[ParseError] -> int => 0\n");
        let ws = lint_module(&m);
        assert_eq!(ws.len(), 0);
    }

    #[test]
    fn no_warning_on_export_fail_any() {
        // Fail[any] — explicit erasure, программист opt-in
        let m = parse("module foo\nexport fn dump() Fail[any] -> () => ()\n");
        let ws = lint_module(&m);
        assert_eq!(ws.len(), 0);
    }

    #[test]
    fn no_warning_on_private_fail() {
        // Private fn — Fail без E это inference placeholder, OK
        let m = parse("module foo\nfn parse(s str) Fail -> int => 0\n");
        let ws = lint_module(&m);
        assert_eq!(ws.len(), 0);
    }

    #[test]
    fn warns_on_protocol_in_effect_position() {
        // Hashable — встроенный protocol; в effect-position warning.
        let m = parse("module foo\nfn process(x int) Hashable -> int => x\n");
        let ws = lint_module(&m);
        assert_eq!(ws.len(), 1);
        assert_eq!(ws[0].rule, "protocol-in-effect-position");
    }

    #[test]
    fn no_warning_on_effect_in_effect_position() {
        // Db — effect, OK в effect-position.
        let m = parse("module foo\nfn lookup(id int) Db -> int => id\n");
        let ws = lint_module(&m);
        assert_eq!(ws.len(), 0);
    }
}
