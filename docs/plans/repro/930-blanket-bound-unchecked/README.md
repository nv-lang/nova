# Проба к №TBD — set-bound blanket-метода не проверяется при вызове метода: `(2.5).try_to_i16()` собирается и печатает `true`

Заведено исследовательским окном (owner-research) 2026-09-04 при проверке, существует ли
проверяемая конверсия float → целое (`i16.try_from(f)` из D54 — не существует). Blanket
`fn[S Ints] S @try_to_i16()` (`prelude/protocols.nv:1053`) ограничен set'ом `Ints`; `f64` и `str`
в него не входят — вызов метода проходит чекер. Контроль: та же посылка на свободной функции
`fn only_ints[T Ints](x T)` — `E_TYPE_NOT_IN_SET`. Две двери, два ответа.

## Как запускать

```sh
cp docs/plans/repro/930-blanket-bound-unchecked/<файл>.nv.txt docs/plans/repro/930-blanket-bound-unchecked/<файл>.nv
nova-cli/target/release/nova.exe check docs/plans/repro/930-blanket-bound-unchecked/<файл>.nv
nova-cli/target/release/nova.exe build docs/plans/repro/930-blanket-bound-unchecked/<файл>.nv -o <куда-нибудь>.exe
```

## Замер 2026-09-04

| проба | выражение | `nova check` | `nova build` → stdout |
|---|---|---|---|
| `method_f64` | `f.try_to_i16()`, `f f64` | **`ok`** | **собрано, печатает `true`** — молча, `f64` прошёл через целочисленный blanket |
| `method_str` | `"x".try_to_i16()` | **`ok`** | CC-FAIL: `passing 'nova_int' to parameter of incompatible type 'nova_str'` |
| `free_fn` | `only_ints(f)` при `fn only_ints[T Ints](x T)` — КОНТРОЛЬ | `[E_TYPE_NOT_IN_SET] type f64 is not a member of type-set Ints` | — |
