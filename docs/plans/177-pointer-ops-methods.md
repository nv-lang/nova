# Plan 177 — Указатели: операции через методы (retire `*p`/`p+i`/`p[i]`) + полный метод-набор + write-cap fix

> **Top-level план.** Создан 2026-06-21 (по аудиту pointer-модели + cross-lang Rust). **Статус:** 📋 PROPOSED.
> **Маркер:** `[M-177-pointer-ops-methods]`. **Запуск:** «**выполни план 177**».
> **Координация:** pointer-модель = D216 / Plan 147 (D246) / Plan 138.5; type-engine = Plan 172. **НЕ править
> `spec/decisions/02-types.md` в одиночку** — файл в зоне 172-переработки; spec-амендменты этого плана применять
> согласованно с 172/138.5. **Поглощает** `[M-138.5-unsafe-ptr-write-cap]`.
> **Сквозной критерий:** «без упрощений, как для прода».

## 1. Зачем (вердикт аудита 2026-06-21)

Pointer-фундамент Nova сильный (двухуровневость safe/unsafe; `&x` safe+auto-promote; `*T` ro-default; `*unsafe T`
degradation; Option-null; realtime/fiber-bans). **Три недочёта:**

1. **Операторы `*p` / `p+i` / `p[i]` маскируются.** `p[i]` (raw, без проверки границ) выглядит **идентично** `arr[i]`
   (safe, bounds-checked, D138 Index) — один синтаксис, разная семантика → footgun даже внутри `unsafe`. Rust
   намеренно не даёт `+`/`[]` на сырых указателях (только методы `.add(n)`), чтобы опасное было видно.
2. **Write-cap дыра** (`[M-138.5-unsafe-ptr-write-cap]`): `.write()`-таблица (`02-types.md:8278`) принимает **голый
   `*unsafe T`** как writable — конфликтует с `*mut unsafe T` (канон §V3.2 flip) и позволяет писать сквозь non-mut
   указатель (нарушение capability-модели).
3. **Нет методов**, которые есть в Rust: arith-методы, struct read/write, unaligned, cast, bulk-copy, wrapping.

## 2. Текущая схема (как есть)

| Операция | Форма | Safe? |
|---|---|---|
| `&x` | оператор | ✅ safe (auto-promote) |
| `raw &x` | контекст. kw | unsafe (стек-адрес) |
| deref read `*p` | **оператор** | unsafe |
| deref write `*p = v` | **оператор** | unsafe, нужен `*mut` (иначе `E_POINTER_RO_ASSIGN`) |
| index read `p[i]` | **оператор** | unsafe (сахар `*(p+i)`) |
| index write `p[i] = v` | **оператор** | unsafe, нужен `*mut` |
| arith `p+i` / `p-i` | **оператор** | unsafe → `*unsafe T` (degraded) |
| dist `p - q` | **оператор** | unsafe → `int` |
| order `p < q` | **оператор** | unsafe |
| eq `p == q` | оператор | ✅ safe (identity) |
| cast `p as *T` | **оператор** | unsafe |
| `.read()` / `.write(v)` | метод | unsafe — **только примитивы**; `.write` ошибочно берёт `*unsafe T` |
| `.read_volatile()` / `.write_volatile(v)` | метод | unsafe |
| auto-deref `p.field` / `p.method()` | сахар на `.` | unsafe (one-level) |
| null | — | `Option[*T]` (NPO) |

Методов **нет**: `.offset`, `.wrapping_offset`, `.dist` (offset_from), struct `.read/.write`, `.read_unaligned/.write_unaligned`, `.copy_to/.copy_from`.

## 3. Новая схема (всё через методы; `[]`/`+`/`*p` retired)

**Принцип:** value-доступ и адресная арифметика — **только методы** (видно + `unsafe`-gated); `[]` — **только
безопасные контейнеры** (D138 Index, bounds-checked); указателям `@index` **не давать**; `==`/`!=` и auto-deref `.`
остаются. Все методы — `unsafe` (требуют `unsafe {}` / `#unsafe fn`), кроме отмеченного.

| Операция | Новая форма | Заметка |
|---|---|---|
| создать (safe) | `&x` | без изменений (auto-promote) |
| создать (raw стек) | `raw &x` | без изменений |
| read (любой `T`, incl. struct) | `p.read() -> T` | **заменяет `*p`** (read); закрывает struct-gap |
| write value | `p.write(v T) -> *mut T` | заменяет `*p=v`; нужен mut-cap; на `*mut uninit T` → **апгрейд `*mut T` (инициализировано)** (= Rust `MaybeUninit::write`); возврат `*mut T` (не unit — несёт init-апгрейд) |
| write from-ptr | `p.write(v *T) -> *mut T` | копия из источника-указателя (большой struct без value-копии); тот же init-апгрейд; overload по типу арг (`T` vs `*T`) |
| offset (арифметика) | `*T`→`p.offset(n) -> *T`; `*mut T`→`*mut T` | заменяет `p+i`/`p-i`; **сохраняет cap, тип НЕ деградирует** (Model A); bounds/align = unsafe-контракт; element-units |
| offset без UB | `p.wrapping_offset(n)` | UB-free вычисление out-of-bounds адреса (**ниша; низкий приоритет**) |
| distance | `p.dist(q) -> int` | заменяет `p-q`; signed element count (= Rust `offset_from`) |
| indexed read | `p.at(i) -> T` | **Nova-сахар** (не Rust) `= p.offset(i).read()`; заменяет `p[i]` |
| indexed write | `p.set(i, v)` | **Nova-сахар** `= p.offset(i).write(v)`, mut; заменяет `p[i]=v` |
| unaligned | `p.read_unaligned()` / `p.write_unaligned(v)` | в C это UB — явные ops (close gap) |
| volatile | `p.read_volatile()` / `p.write_volatile(v)` | как есть |
| copy bulk (N эл.) | `p.copy_from(src *T, n)` / `p.copy_to(dst, n)` | **N элементов** memcpy/memmove (single-эл. = `.write(v *T)`); Rust `copy` |
| cast | `p as *U` (**оператор остаётся**) | общий cast-оператор, не deref-маскарад; `.cast`-метод не нужен |
| order (в буфере) | знак `p.dist(q)` | отдельный `<`/`.addr_lt` **не нужен** — выводится из `.dist` |
| eq | `p == q` / `p != q` | ✅ **остаётся** оператором (safe identity) |
| member access | `p.field` / `p.method()` | ✅ **остаётся** auto-deref на `.` (one-level) |
| null | `Option[*T]` + match | как есть (нет `.is_null()` — Option лучше) |

**Retired (становятся ошибкой):** `*p`, `p+i`/`p-i`, `p-q` (→ `.dist`), `p[i]`/`p[i]=v`, `p<q`/`>` (→ знак `.dist`). **`p as *U` ОСТАЁТСЯ** (общий cast-оператор, не маскарад).
`[]`/`@index` указателям **не вводить** — `p[i]` просто не компилируется (нет `@index` на `*T`) → форсит `.at(i)`/`.offset(i).read()`.

Имена методов: **`.offset`** (полное слово, как Rust + стиль Nova full-word; не `.offs`), `.at`/`.set`/`.dist`. Прочие — финал при реализации.

## 4. Write-cap fix (закрытие дыры; absorb `[M-138.5-unsafe-ptr-write-cap]`)

> **RENAME (sign-off 2026-06-22): `unsafe T` → `uninit T`** (и `*unsafe T`→`*uninit T`, `*mut unsafe T`→`*mut uninit T`,
> `Unsafe(T)`→`Uninit(T)`). Имя самодокументирует «возможно-неинициализированный» (= Rust `MaybeUninit`) и развязывает
> перегрузку слова «unsafe». **НЕ переименовываются:** `unsafe {}`-блок, `#unsafe fn`, `*unsafe fn` (указатель на
> unsafe-**вызываемую функцию** — другое значение).

- `*uninit T` = **ro** + possibly-uninit (ro — дефолт pointee).
- writable-uninit = **`*mut uninit T`** (`Pointer(Mut(Uninit(T)))`).
- `.write()` / `.set()` / (legacy `*p=v` до retire) требуют **mut-capability** (`*mut T` / `*mut uninit T`); голый
  `*uninit T` → `E_POINTER_RO_ASSIGN`.
- **Spec-амендмент:** `02-types.md:8278` write-таблица — убрать голый uninit-указатель из write-allowed (оставить `*mut T` / `*mut uninit T`).
- **Init-upgrade:** `.write(v T)` / `.write(v *T)` на `*mut uninit T` возвращает **`*mut T`** (pointee инициализирован) —
  канонический способ инициализировать uninit (= Rust `MaybeUninit::write() -> &mut T`). Возврат `*mut T` несёт uninit→init в типе.
- **✅ Конфляция uninit/degraded РЕШЕНА (Model A, sign-off 2026-06-22):** арифметика (`.offset`) **НЕ деградирует тип**
  (`*T`→`*T`, `*mut T`→`*mut T`); bounds/align — **unsafe-контракт** (ты в `unsafe`), не состояние типа (тулинг: проверки/тесты).
  `uninit`-тип несёт **только** ось «инициализирован ли» — как Rust (`MaybeUninit` = тип; bounds/align = контракт). Поэтому
  init-апгрейд `.write` чист (uninit→init), а «восстанавливать bounds/align после write» — несуществующий вопрос.
- **Stale-тест:** `nova_tests/plan118/plan118_5_v3_t9_safety_outer_ok.nv:23-24` — старый порядок `*unsafe mut/ro Acc4`
  (pre-138.5-flip → `E_MODIFIER_ORDER`) → мигрировать на `*mut uninit` / `*uninit` (ro implicit).

## 5. Фазы

- **Ф.1 — write-cap fix** (мелкая, изолированная): `.write()`/`*p=v` требует mut-cap; spec-таблица 8278; stale-тест. Закрывает дыру.
- **Ф.2 — недостающие методы:** `.offset`/`.wrapping_offset`/`.dist`/`.read`(any-T)/`.write`(any-T)/`.at`/`.set`/`.read_unaligned`/`.write_unaligned`/(`.copy_to`/`.copy_from`). Struct read/write — закрыть gap. (`.cast`-метод НЕ вводим — `as` остаётся.)
- **Ф.3 — retire операторов:** `*p`/`p+i`/`p-i`/`p-q`/`p[i]`/`p[i]=v`/`p<q` → диагностика `[E_POINTER_OP_USE_METHOD]` с fix-it на метод. **`p as *U` НЕ ретайрить** (cast-оператор остаётся). `[]`/`@index` указателям не давать. `==`/`!=` + auto-deref `.` оставить.
- **Ф.4 — миграция:** detect-режим → blast-radius по std/nova_tests (§7); переписать все `*p`/`p[i]`/`p+i` на методы.

## 6. Spec / D / Q / docs

- amend **D216** (§ ops): «операции с указателями — методы; операторы `*p`/`+`/`[]`/`<` на `*T` ретайрятся (`as`-cast остаётся);
  `[]`/`@index` — только safe-контейнеры; `==`/`!=` и auto-deref `.` остаются»; write-cap (`*uninit T`=ro,
  write нужен `*mut`); полный метод-набор + сигнатуры. error-index: `E_POINTER_OP_USE_METHOD`.
- `docs/typed-pointers.md` — переписать раздел операций на метод-набор + таблица «было→стало».

## 7. Миграция (§7 compiler-conventions)

nv не в релизе → меняем напрямую, но **измерить blast-radius** (сколько `*p`/`p[i]`/`p+i` в std/nova_tests) в
detect-режиме, переписать на методы в том же изменении. Верификация против чистого бинаря.

## 8. Тесты (pos + neg)

- **pos** `nova_tests/ptr177/`: `.read()/.write()` на примитиве И **struct/record/tuple/sum** (close gap);
  `.offset(n)` (`*mut T`→`*mut T`, cap сохраняется)/`.at(i)`/`.set(i,v)`/`.dist(q)`/`p as *U`/`.read_unaligned()`/`.wrapping_offset(n)`; `*mut uninit T` writable + init-upgrade → `*mut T`; `==`/`!=`; auto-deref `.`.
- **neg:** `*p`/`p+i`/`p[i]`/`p<q` → `[E_POINTER_OP_USE_METHOD]` (fix-it на метод); `.write()` на голом `*uninit T`/`*T` → `E_POINTER_RO_ASSIGN`; `p[i]` (нет `@index` на `*T`) → понятная ошибка.

## 9. Критерии приёмки

1. Все pointer-операции — методы (кроме `&x`/`raw &x`/`==`/`as`-cast/auto-deref-`.`); операторы `*p`/`+`/`[]`/`<` ретайрнуты.
2. `[]`/`@index` — только safe-контейнеры; `p[i]` не компилируется.
3. Write-cap дыра закрыта: `.write()` требует mut-cap; `*uninit T` = ro; writable-uninit = `*mut uninit T`.
4. Метод-набор покрывает Rust (read/write any-T, offs/dist, unaligned, volatile, cast, wrapping, bulk).
5. std/nova_tests мигрированы; полный регресс зелёный; **без упрощений**.

## 10. Конвенции + координация

§1 (проверки в чекере), §3 (методы резолвятся из реестра/`.nv`, не хардкод), §5 spec-first (D216 amend до кода),
§6 (коды ошибок + error-index), §7 (blast-radius + чистый бинарь), §8 (pos+neg, C-codegen). **Координировать с 172**
(type-engine; pointer write-cap живёт в чекере единого движка) и **138.5/147** (pointer-модель). `02-types.md` —
не править в одиночку (hot, 172).

## 11. Followup

`[M-177-pointer-ops-methods]`. Поглощает `[M-138.5-unsafe-ptr-write-cap]` (Ф.1). Имена методов — финал при реализации.
