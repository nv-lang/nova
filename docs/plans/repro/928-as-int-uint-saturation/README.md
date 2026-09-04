# Проба к №TBD — `int as uint` насыщает, все остальные `iN as uM` — bit-pattern: одна операция, два правила

Заведено исследовательским окном (owner-research) 2026-09-04 при разборе `as` между числовыми
типами (`docs/dev/research/2026-09-04-numeric-widening.md` §6). Таблица D54 задаёт для всех
`iN → uM` bit-pattern (`-1i32 as u16 == 65535`); D130 Q2 отдельно постановил «`int as uint`
saturates (negative → 0); `int as u64` — direct bit-cast». Компилятор исполняет обе буквы, и
получается три разных ответа на одну операцию.

## Как запускать

```sh
cp docs/plans/repro/928-as-int-uint-saturation/casts.nv.txt docs/plans/repro/928-as-int-uint-saturation/casts.nv
nova-cli/target/release/nova.exe build docs/plans/repro/928-as-int-uint-saturation/casts.nv -o <куда-нибудь>.exe
```

## Замер 2026-09-04 (owner-research, `nova-cli/target/release/nova.exe`)

| выражение | результат | правило |
|---|---|---|
| `(-1 as i8) as u8` | `255` | D54 bit-pattern |
| `(-1 as i32) as u32` | `4294967295` | D54 bit-pattern |
| `(-1 as i64) as u64` | `18446744073709551615` | D54 bit-pattern |
| `(-1 as int) as u64` | `18446744073709551615` | D54 bit-pattern (D130: «`int as u64` — bit-cast») |
| **`(-1 as int) as uint`** | **`0`** | **D130 Q2 — насыщение** |
| **`(-5 as i32) as uint`** | **`18446744073709551611`** | bit-pattern — та же цель `uint`, другой источник, другое правило |
| `(2^64-1 as u64) as int` | `-1` | D54 bit-pattern |
| `(2^64-1 as u64) as i32` | `-1` | D54 wraparound |

Все прочие строки таблицы D54 подтверждены той же пробой: `300i32 as u8 == 44`,
`0x1FFFF as i16 == -1`, `70000.5 as i16 == 32767`, `-1.0 as u16 == 0`, `1e20 as int == INT64_MAX`,
`NaN as int == 0`.
