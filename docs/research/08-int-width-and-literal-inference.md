# Int-ширина, signed-индексация и literal inference

> **Дата:** 2026-06-03
> **Статус:** completed; foundation для [D226](../../spec/decisions/02-types.md#d226) + [D227](../../spec/decisions/03-syntax.md#d227)
> **Открытое:** pointer-aware amendments для D226/D227 (см. §3 ниже)

Материал собран в три раунда обсуждения. Цель — зафиксировать
обоснование за решениями, чтобы будущий architect мог проверить
рассуждения, а не реконструировать их по diff'ам.

## §1. Signed vs unsigned для `len` / `capacity` / index

### Контекст

В Nova `int` ([D129](../../spec/decisions/02-types.md#d129)) — alias
для `i64` на bootstrap. Stdlib уже использует `int` для `@len()`,
`@capacity()`, `with_capacity(n)`, индексов, slice-границ. Вопрос —
подтверждать ли это решение явно (был ли выбор сознательным?) и
закрывать ли его контрактной защитой от негатива.

### Industry baseline

| Язык | Index/len тип | Знак | Hindsight |
|---|---|---|---|
| Go | `int` (platform-word) | **signed** | Сознательный выбор после C |
| Swift | `Int` (platform-word) | **signed** | Apple: «harder to make off-by-one errors» |
| Java | `int` (i32) | **signed** | Историческое; принято |
| Kotlin | `Int` (i32) | **signed** | Mirror Java |
| C# | `int` (i32) | **signed** | `LongLength` для >2B |
| Python | `int` (arbitrary) | **signed** | Negative-index slicing |
| TypeScript | `number` (f64) | signed (de facto) | Один тип |
| Rust | `usize` (platform) | unsigned | Community regrets vocal |
| C++ STL | `size_t` (platform) | unsigned | **Stroustrup: «I regret using unsigned for size in STL»** |
| Zig | `usize` (platform) | unsigned | Embedded-first рационал |

**Счёт 7:3 в пользу signed.** Двое из трёх unsigned-языков (C++ и
Rust) имеют публичные authorial regrets.

### Аргументы за signed (что теряет Rust на `usize`)

1. **Underflow trap.** `vec.len() - 1` на пустом vec в Rust паникует
   (`usize` underflow → `0_usize - 1` → overflow). В Go/Java/Nova
   даёт `-1`, что естественно ловится `if i < 0` или валит loop guard
   (`for j in 0..-1` — пустой range). Самая частая Rust newbie-trap.
2. **Sentinel `-1`.** `indexOf`/`find`/`strings.Index` в Go/Java
   возвращают `int` с `-1 = not found` — удобно без `Option`
   аллокации.
3. **Разности.** `a.len() - b.len()` естественно signed; sorting
   comparators, diff'ы, position deltas, scroll offsets — все хотят
   signed.
4. **Mixed arithmetic.** В Rust `i32 + usize` requires explicit cast;
   в Nova/Go/Swift всё `int` — никакой ceremony.
5. **Reverse loops.** `for i in 0..n-1` на `n=0` в Rust паникует; в
   Nova даёт пустой range.
6. **Bit-width аргумент мёртв на 64-bit.** Signed-`int` (= `i64`) даёт
   2⁶³−1 ≈ 9.2 × 10¹⁸ элементов — никакая коллекция не достигнет.

### Аргументы за unsigned (что теряет Nova)

1. **Type-encoded invariant.** Длина не может быть отрицательной —
   гарантия из типа. С `int` это runtime invariant.
2. **Лучшая оптимизация bounds-check.** Компилятор знает `len ≥ 0`,
   не нужен `i >= 0` check.
3. **`with_capacity(-5)` compile error.** В Rust типом; в Nova —
   нужен контракт.

В Nova первые два **частично** закрываются:
- (1) → `requires len >= 0` через [D24](../../spec/decisions/09-tooling.md#d24)
  + Z3 ([Plan 33.x](../../docs/plans/33-contracts-implementation.md)) даёт
  compile-time гарантию.
- (2) → escape analysis + range-VC в верификаторе теоретически
  выводят то же.
- (3) → `requires n >= 0` на capacity-API — одна строка на API.

### Nova-специфичные факторы

1. **D129 invariant.** `int = i64`, codegen — `int64_t`. Нет
   platform-pointer story в bootstrap. Не нужно ломать.
2. **Future-arch путь.** Если Nova станет multi-arch, `int` мигрирует
   к platform-pointer-width **signed** (= Rust `isize`). Это **не**
   аргумент за unsigned, это аргумент за «оставить signed».
3. **Overflow семантика ([Plan 33.8](../../docs/plans/33.8-verifier-soundness.md) Ф.1).**
   `int` overflow → `nv_panic`. Если ввести `uint` для len, заменим
   один trap (overflow) на другой (underflow on `0 - 1`) — без
   выигрыша.
4. **AI-first ([D10](../../spec/decisions/01-philosophy.md#d10)).**
   LLM пишет signed-индексацию правильно чаще, чем балансирует
   `usize`/`i64` касты. Compile-time errors как обучающий сигнал
   работают, только если ошибка указывает на дизайн пользователя.

### Рекомендация → реализовано как D226

- **Подтвердить signed `int` для len/capacity/index.**
- **Вынести в отдельный D-блок** (D130 Q3 решил это в 2026-05-19, но
  спрятано внутри `uint`-плана и не findable).
- **Добавить контрактную защиту от негатива:** `requires n >= 0` на
  capacity-API.
- **Future-path записать явно:** при переходе на 32-bit `int`
  становится platform-pointer-width signed, index API не меняется.

Реализовано [D226](../../spec/decisions/02-types.md#d226) (2026-06-03):
spec block + 4 stdlib edits (`hashmap.nv`/`set.nv`/`string_builder.nv`/
`write_buffer.nv` `with_capacity` clauses) + README index.

---

## §2. `int` ширина и literal inference

### Контекст

Если D226 фиксирует `int` для всего — а `int = i64` — нужно ли:
(a) сужать `int` к `i32` (как Rust/Java/Kotlin/C#)?
(b) выводить литерал `42` как `i32` if fits (Rust-style fallback)?

### Default integer width (a)

| Язык | Тип `int` | Width |
|---|---|---|
| Rust | n/a (только sized) | — |
| Java/Kotlin/C# | `int` | i32 fixed |
| C/C++ | `int` | ≥16, обычно 32 |
| Go | `int` | platform-word (i64 на 64-bit) |
| Swift | `Int` | platform-word (i64 на 64-bit) |
| Python | `int` | ∞ (arbitrary precision) |
| TypeScript | `number` | 53-bit safe (f64) |
| Zig | n/a (только sized + `comptime_int`) | — |
| Nim | `int` | platform-word |

**4 за i32 fixed** (всё legacy C-эпохи), **6 за ≥ 64-bit** (всё
modern, post-2009).

#### Аргументы за i32

- Память (2× плотнее).
- Matching Java/C#/Kotlin baseline.
- Микро-perf x86-64 encoding.
- FFI к C `int` без cast.

#### Против i32

1. **Overflow на 2.1B современная реальность:**
   - File sizes — 4 GB на одиночный видеофайл.
   - Byte counters HTTP / Content-Length — >2 GB.
   - Sums и aggregates — 10⁹ × 100 = 10¹¹.
   - Timestamps ms — 2³¹ ms = 24.9 дней. Unix seconds = **Y2038**.
   - Hash collision space недостаточен.
2. **Plan 33.8 panic.** Nova-style overflow паникует, не wrap. i32 =
   panic-runway.
3. **Конфликт с D226.** `[]u8` максимум 2.1B элементов = 2 GB —
   пробивается mmap'ом крупного файла на 64-bit.
4. **Cast-hell Rust pattern.** `arr[x as usize]` — заслуженно
   критикуемая боль. Перейти на i32 → имитировать без выгоды.
5. **AI-first.** LLM забудет `L` суффикс (Java mistake), сгенерирует
   overflow в `sum(.len())`.
6. **Density argument слабее, чем кажется.** Когда программа хранит
   миллионы int — она явно объявляет `[]i32`/`[]i16`. Default int в
   loops — loop variables, не arrays.

**Авторские regrets:**
- Java: `int = 32` — historical artifact, no chance fix из-за
  stability commitment.
- Rust: signed default + usize index — RFC discussions 2015-2020
  обсуждали как design mistake.
- Go (Russ Cox): «one of the things we got right» — wide default.
- Swift (Lattner WWDC 2014): «90% кода нет причин думать о точности».

### Literal inference (b)

Даже если `int = i64`, можно делать `let x = 42` → `i32` если влезает
(Rust-style fallback).

**Аргументы против в Nova:**

1. **Inconsistency.** `let a = 42; let b = 3_000_000_000;` — разные
   типы; `a + b` требует promotion.
2. **Brittle generic instantiation.** `Vec.new()` + `push(42)` →
   `Vec[i32]` вместо общего `Vec[int]`.
3. **Mangling pollution.** Каждый «случайный 42» создаёт
   `Map[i32, V]` инстанциацию.
4. **Refactor hostility.** Изменить `42` на `3_000_000_000` ломает
   downstream типы.
5. **Plan 33.8 паника.** i32 default + overflow-panic = unhappy users.
6. **Zig-style alternative лучше.** Literal как `comptime_int` до
   точки использования; coerce в target. Расширить
   [D55](../../spec/decisions/02-types.md#d55) на numeric literals
   полезнее, чем Rust-style fallback.

### Рекомендация → реализовано как D227

**Часть 1 — `int = i32`?** **Нет, оставить i64.**

**Часть 2 — Narrow literal inference?** **Нет.** Сохранить:
- Default to `int` (i64) когда context отсутствует.
- Literal coercion в позиции с явным типом — Zig-style, расширение D55.
- Hard compile-time range-check (`ro a i32 = 3_000_000_000` → error).
- Negative literal в unsigned position = hard error, не wrap.
- Suffix синтаксис — отвергнут (D44 stands).

Реализовано [D227](../../spec/decisions/03-syntax.md#d227) (2026-06-03):
spec block (4 правила + industry table + 5 rejected alternatives) +
D44 §«default-типы» inline amend (drift-fix: `int = i64` per D129).

---

## §3. Pointer interactions — gap analysis (открыто)

### Контекст

После Plan 115 V1 ([D214](../../spec/decisions/02-types.md#d214)) и
Plan 118 ([D216](../../spec/decisions/02-types.md#d216)) Nova получил:
- `ptr` opaque type
- `*T` / `*ro T` / `*mut T` / `*unsafe T` typed pointer family
- pointer arithmetic в `unsafe { }`
- NPO codegen для `Option[*T]`
- `null ptr` literal retracted → `(0 as ptr)`

D226 и D227 написаны **до** учёта этого контекста (моя ошибка —
research-cycle прошёл без проверки текущего spec состояния).
Получился частичный пробел.

### Что упустил D226

**D216 §6 уже определяет:**
- `ptr + N` → `*unsafe T`, offset — **`int`** (согласуется с D226
  Rule 1, но не отмечено явно).
- `ptr - ptr` → **`isize`** (= signed по духу D226 «разности
  естественно signed»).

**D216 §FFI:**
- `external fn malloc(sz usize) -> Option[*u8]` — `usize` в FFI
  sig. D226 §5 говорит «`uint`/`u64` только для bit-twiddling и FFI»,
  но не упоминает `usize` как ABI-bridge type.

**D216 §casts:**
- `p as usize` / `usize as *T` / `(0x1000 as ptr)` — explicit address
  arithmetic. D226 §5 покрывает через «FFI», конкретики нет.

**D214 §casts:**
- `ptr as u64` / `ptr as i64` — extract integer для opaque handles,
  hash. Формально OK через §5, но не упомянуто.

### Что упустил D227

- **`(0 as ptr)`** — explicit replacement для retracted `null ptr`
  (Plan 118 A23, D214 amend 2026-06-02). Нужно добавить как пример
  «context-coercion НЕ работает для pointer types — требуется явный
  `as ptr`».
- **Литерал в pointer arithmetic:** `ptr + 1` — `1` это `int` per
  D227 Rule 1, scaled by `sizeof(T)` per D216 §6. Явно сказать (LLM
  иначе попробует `1usize` или `1 as usize`).
- **`None` для `Option[*T]`** — это constructor через NPO codegen,
  не литерал. D227 не пересекается.

### Spec-drift

`isize`/`usize` нет explicit D-блока aliasing для `i64`/`u64`
(implicit по аналогии с D129/D130, но не задокументировано).

Followup `[M-D226-isize-usize-alias-D-block]` — отдельный spec
cleanup, **не** в scope D226/D227 amend.

### Предлагаемые amendments

**Amend D226:**
- §5 расширить exemption-список: «pointer arithmetic offsets
  (`ptr + int`), pointer differences (`ptr - ptr → isize`), `usize` в
  external fn signatures (C ABI), `p as usize`/`usize as ptr` casts
  для opaque handles / hash».
- Добавить §7 «Pointer interactions» с матрицей: index-API → `int`;
  offset → `int`; diff → `isize` (= `i64`); FFI ABI → `usize`; cast
  bridge → explicit.
- Cross-refs §«Связь»: [D214](../../spec/decisions/02-types.md#d214),
  [D216](../../spec/decisions/02-types.md#d216).

**Amend D227:**
- Rule 7 «Pointer-typed positions»: литерал не coerce'ится в pointer
  type автоматически; требуется `0 as ptr` / `0x1000 as *T`.
- Cross-ref на D214 amend (null retract → `(0 as ptr)`).
- Пример `ptr + 1` → `1: int` per Rule 1.

**Размер:** ~80 строк в 02-types.md, ~30 строк в 03-syntax.md.

---

## Выводы

| # | Вопрос | Решение | D-блок |
|---|---|---|---|
| 1 | `int` для len/cap/index — signed? | **Да, signed** | [D226](../../spec/decisions/02-types.md#d226) ✅ |
| 2 | `int = i32` или `i64`? | **i64** (per D129) | [D129](../../spec/decisions/02-types.md#d129) + D226 ✅ |
| 3 | Literal `42` → narrow-fallback (Rust)? | **Нет** | [D227](../../spec/decisions/03-syntax.md#d227) ✅ |
| 4 | Compile-time range-check? | **Да, hard error** | [D227](../../spec/decisions/03-syntax.md#d227) ✅ |
| 5 | Negative в unsigned context? | **Hard error**, не wrap | [D227](../../spec/decisions/03-syntax.md#d227) ✅ |
| 6 | Type suffix (`42i64`)? | **Нет** | [D44](../../spec/decisions/03-syntax.md#d44) stands |
| 7 | Pointer interactions? | amend D226 §5 + D227 Rule 7 | ⏳ pending |
| 8 | `isize`/`usize` explicit D-блок? | followup | `[M-D226-isize-usize-alias-D-block]` |

## Связь

- [D129](../../spec/decisions/02-types.md#d129) — `int = i64` bootstrap alias.
- [D130](../../spec/decisions/02-types.md#d130) — `uint = u64` symmetric pair (Q3 indexing decision — historical origin D226).
- [D44](../../spec/decisions/03-syntax.md#d44) — numeric literal grammar (D227 amends).
- [D214](../../spec/decisions/02-types.md#d214) — `ptr` opaque type.
- [D216](../../spec/decisions/02-types.md#d216) — `*T` typed pointer family + arithmetic + NPO.
- [D226](../../spec/decisions/02-types.md#d226) — signed indexing convention.
- [D227](../../spec/decisions/03-syntax.md#d227) — numeric literal inference policy.
- [D24](../../spec/decisions/09-tooling.md#d24) — `requires`/`ensures` contracts.
- [Plan 33.8](../../docs/plans/33.8-verifier-soundness.md) — int overflow → panic.
- [Plan 115](../../docs/plans/115-bootstrap-ffi-v1.md) — bootstrap FFI V1.
- [Plan 118](../../docs/plans/118-typed-pointers.md) — typed pointers + NPO + unsafe model.
