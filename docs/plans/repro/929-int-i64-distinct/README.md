# Проба к №TBD — чекер считает `int ≡ i64` и `uint ≡ u64`, спека (D129) говорит обратное

Заведено исследовательским окном (owner-research) 2026-09-04. D129 (AMEND Plan 133): `int` =
`intptr_t`, `i64` = `int64_t` — разные типы, совпадающие по ширине только на 64-битном bootstrap;
Go `int` ≠ `int64`, Rust `isize` ≠ `i64`. Тем же днём D130 получил ту же поправку для `uint`/`u64`.

## Как запускать

```sh
cp docs/plans/repro/929-int-i64-distinct/mixed.nv.txt docs/plans/repro/929-int-i64-distinct/mixed.nv
nova-cli/target/release/nova.exe check docs/plans/repro/929-int-i64-distinct/mixed.nv
```

## Замер 2026-09-04

| выражение | ожидание по D129/D405 | `nova check` |
|---|---|---|
| `a + b`, `a int`, `b i64` | `E_MIXED_WIDTH_ARITH` / ошибка типа | **`ok`** |
| `a == b` | ошибка | **`ok`** |
| `take64(a)` при `fn take64(x i64)` | `E7301` | **`ok`** |
| `c + d`, `c uint`, `d u64` | ошибка | **`ok`** |
| `ro e u64 = c` | `E7301` | **`ok`** |

Файл собирается и печатает `12 false 5 12 5`. Чекер сверяет пару (ширина, знак), кодоген
различает типы по имени (`nova_int` vs `int64_t`, разные mangle) — две двери, два ответа.
