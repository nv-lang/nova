# Plan 70.1 — module alias resolution fixtures

## Scope

[Plan 70.1](../../docs/plans/70.1-module-alias-resolution.md) bug:
`import X.Y as alias` + `alias.func(args)` не раскрывается в C codegen —
emit'ит alias-Ident напрямую → `use of undeclared identifier` в C → CC-FAIL.

## Fix (compiler-codegen/src/codegen/emit_c.rs)

- new field `imported_modules: HashSet<String>` populated в `emit_module`
  pre-pass из `module.imports` + `peer_files[].imports`
- emit_call `ExprKind::Member { obj: Ident(prefix), name: method }` arm
  rewrites в bare `method(args)` если `prefix ∈ imported_modules`
- Imported fns доступны через bare name в text scope — префикс это
  namespace hint, не actual C-name component

## Fixtures

| File | Coverage |
|---|---|
| `helper_mod.nv` | Export'ы `add_one`/`double`/`greet` + self-check test |
| `f1_alias_call_pos.nv` | `import X as h` + `h.add_one(int)` / chaining / str args |
| `f2_whole_module_pos.nv` | `import X.Y.Z` (без alias) + `Z.func(args)` last-segment rewrite |

## Negative tests — deferred

Originally planned: `import X as h` + `h.nonexistent_function(args)` →
EXPECT_COMPILE_ERROR. **Не реализовано** в этом scope потому что:

1. Type-checker не валидирует Member-call args против alias-resolved fn
   signature (`h.add_one("not_int")` silently accepted — coercion
   non-обнаружена). Wrong-type case → CC-FAIL, не codegen-FAIL — runner
   не categorizes как EXPECT_COMPILE_ERROR.
2. Unknown-fn case (`h.nonexistent_function`) — Plan 70.1 fix rewrites в
   codegen post-typecheck; undefined symbol manifests как **link error**
   (lld-link), не codegen error — runner aware of CC-FAIL category but
   `EXPECT_COMPILE_ERROR` only matches codegen error messages.

Proper type-checker integration (validate `<alias>.<method>(args)` против
imported module exports + arg-type checking) — **отдельный plan** (70.1
follow-up). Текущий fix закрывает основной CC-FAIL (alias-undeclared)
который блокировал реальный код (snowflake test, std/testing/handlers
usage).

## Reproducer (pre-fix bug)

```nova
module repro
import std.testing.handlers as th

test "alias" {
    with Time = th.fixed_ms(100 as u64) {
        let ms = Time.now_ms()
        assert(ms == 100)
    }
}
```

Pre-fix: `error: use of undeclared identifier 'th'`.
Post-fix: alias resolution works (other errors — unrelated Duration type).
