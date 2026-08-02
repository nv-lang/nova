---
source_rev: 07df7d2c9
source_date: 2026-08-02
---

[English](typed-pointers.md) | **Русский**

<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Типизированные указатели (семейство `*T`) + модель `unsafe`

> **Планы 118 / 118.5** (D216 V1 + V2 + V3 амендменты, **План 138.5 FINAL
> pointer model**, D2 амендмент, D214 амендмент, D32 амендмент, D184 амендмент).
> **Статус:** ✅ FINAL pointer model АКТИВЕН с 2026-06-11; поддержка
> парсером/чекером заземлена (`E_POINTER_PREFIX_MODIFIER`). Полный NPO-codegen +
> escape-анализ — последующие фазы.

Production-grade FFI и низкоуровневая работа с памятью требуют типизированных
указателей. План 118 вводит семейство типов `*T` + модель `unsafe` + Null
Pointer Optimization (NPO) для zero-cost null-safety через `Option[*T]`.

## Модель мутабельности указателей: «стрелка → коробка» (План 138.5 FINAL)

> **План 138.5 (2026-06-11) FINAL model — заменяет V2 (right-binding) и
> V3 (propagation/safe-stopper):** ТИП указателя несёт мутабельность pointee
> **ТОЛЬКО**, записываемую **ПОСТФИКСНО** (модификатор стоит *после* `*`).
> Старые префиксные формы `ro * T` / `mut * T` / `unsafe * T`, стоппер `safe`
> и форма `Unsafe(Pointer)` (`unsafe * T` = nullable-raw) **ВЫВЕДЕНЫ** — см.
> [выведенные формы](#выведенные-формы-план-1385).

Думайте об указателе как о **стрелке**, указывающей на **коробку** (pointee):

- **Цель стрелки — в ТИПЕ, постфиксно на `*`** — говорит, *что можно делать с
  коробкой*: `*mut T` (в коробку можно писать), `*ro T` ≡ `*T` (коробка
  только для чтения), `*unsafe T` (коробка может быть неинициализированной).
- **Сама стрелка — биндинг (`let` / `mut`, D36)** — говорит, *можно ли
  перенацелить стрелку на другую коробку*: `let p` = стрелка зафиксирована,
  `mut p` = стрелку можно перенацеливать.

Это две независимые оси. Они никогда не пересекаются, потому что одна живёт в
**типе** (постфиксно на `*`), а другая — на **биндинге** (перед именем):

```nova
mut p *mut T        // arrow re-pointable (mut binding) + box writable (*mut pointee)
let q *ro T         // arrow fixed (let binding)        + box read-only (*ro pointee)
mut p *ro T         // arrow re-pointable               + box read-only
let p *mut T        // arrow fixed                      + box writable
```

> **НЕ существует префикса `mut *` / `ro *` / `unsafe *`.** Модификатор перед
> `*` — жёсткая ошибка `E_POINTER_PREFIX_MODIFIER` (прецедент: в Rust
> `*mut T` / `*const T` = мутабельность pointee; `let mut p` —
> перенацеливаемость).

### Канонические формы (постфиксный модификатор pointee)

```nova
*T                  // pointer to read-only T (default canonical; ≡ *ro T)
*ro T               // pointer to read-only T (explicit; identical to *T)
*mut T              // pointer to mutable T (deref-store `*p = v` allowed)
*unsafe T           // pointer to possibly-uninit T (MaybeUninit pointee)
Option[*T]          // NULLABLE pointer (NPO: None = null, 8 bytes)
Option[*unsafe T]   // FFI nullable-uninit ptr (None = null, Some = non-null
                    //   ptr к possibly-uninit pointee)
```

Модификатор **всегда постфиксный** — он крепится к pointee того `*`, за
которым следует. Само значение указателя **всегда non-null**; для nullable
используйте `Option[*T]` (zero-cost через NPO).

### Перенацеливаемость — это биндинг (D36), а не тип

```nova
mut p *T = &acc     // mut binding → p may be reassigned later (p = &other)
let q *T = &acc     // let binding → q is fixed (q = &other ⇒ E_REBIND)
```

Переменная-указатель подчиняется **тем же** правилам `let` / `mut`, что и
любая другая переменная (D36). Тип никогда не кодирует перенацеливаемость.

### Цепочки указателей (несколько уровней) — постфиксно на каждом `*`

```nova
*mut *ro Node       // writable-target pointer  →  (read-only-target pointer → Node)
                    //   *p   = other_ptr   OK   (outer pointee mut)
                    //   **p  = new_value   ERR  (inner pointee ro)

*ro *mut Node       // read-only-target pointer →  (writable-target pointer → Node)
                    //   *p   = other_ptr   ERR  (outer pointee ro)
                    //   **p  = new_value   OK   (inner pointee mut)
```

Каждый модификатор стоит постфиксно, сразу после своего `*`, и описывает цель
этого уровня `*`. Читается слева направо.

### Возврат указателей — pointee-mut по умолчанию (D184 амендмент)

D184 (мутабельность возвращаемого типа по умолчанию) применяется к **pointee**
для возвращаемых указателей:

```nova
fn alloc_cell() -> *T       // ≡ -> *ro T : returns a ptr to read-only T
fn alloc_mut()  -> *mut T   // returns a ptr to WRITABLE T
```

Перенацеливаемость **результата** решается в точке биндинга, а не в типе
возврата:

```nova
ro p = alloc_mut()          // p fixed (ro binding); *p = v still OK (pointee mut)
mut q = alloc_mut()         // q re-pointable + *q = v OK
```

Это устраняет старую неоднозначность «двух mut в позиции возврата» (внешнего
pointer-mut больше не из чего выбирать).

### FFI out-param / неинициализированный pointee

```nova
external fn os_read(fd int, buf *mut unsafe u8, n usize) -> int
//                              ^^^^^^^^^^^^^^^
//                       pointee writable (*mut) + possibly-uninit (unsafe);
//                       arrow re-pointability is the binding's concern
```

Оси pointee (`mut` / `ro` и `unsafe`) коммутируют на value-поintee и обе
записываются постфиксно.

## Краткий справочник

| Потребность | Каноническая FINAL-форма | Spec |
|---|---|---|
| Типизированный указатель (цель ro по умолчанию) | `*T` ≡ `*ro T` | [D216 §1](../../spec/decisions/02-types.md#d216-typed-pointer-family--unsafe-model--null-safety-через-npo) |
| Указатель на записываемую цель | `*mut T` | D216 §1 |
| Указатель на possibly-uninit цель | `*unsafe T` | D216 §1 + V2 §V2.3 |
| Перенацеливаемая переменная-указатель | `mut p *T` (биндинг) | D216 §2 + D36 |
| Зафиксированная переменная-указатель | `let p *T` / `ro p *T` (биндинг) | D216 §2 + D36 |
| Nullable типизированный указатель | `Option[*T]` (NPO) | D216 §7 + V2 §V2.4 |
| FFI nullable-uninit указатель | `Option[*unsafe T]` | D216 §1 + V2 §V2.4 |
| Возврат указателя (записываемая цель) | `-> *mut T` | D184 амендмент (План 138.5) |
| Создание указателя | `&value` | D216 §4 |
| Явный deref | `*p` | D216 §5 |
| Авто-deref поле/метод | `p.field` / `p.method()` | D216 §5 |
| Арифметика указателей | `unsafe { p + n }` → `*unsafe T` | D216 §6 |
| Граница unsafe | `unsafe { ... }` блок / `#unsafe fn` | D216 §8-9 |
| Указатель на функцию для FFI | `*fn(Args) -> Ret` | D216 §10 |
| Opaque-нетипизированный (legacy) | `ptr` (D214 амендмент → newtype `Option[*unsafe ()]`) | D214 амендмент |

## Семейство типов `*T`

**ABI:** все варианты — шириной в один указатель (8 байт на 64-bit; bootstrap
цель — только 64-bit). C-type emission: `*ro T` → `const T*` (помогает
оптимизатору clang/MSVC), `*mut T` / `*unsafe T` → `T*`.

**Действительность (validity):** каждое значение указателя (`*T` / `*ro T` /
`*mut T` / `*unsafe T`) **всегда non-null** (инвариант на этапе компиляции).
Nullable-вариант — `Option[*T]` через NPO (один указатель, NULL = None;
см. §V2.4 в spec). `*unsafe T` описывает possibly-**неинициализированный**
pointee — сам *указатель* всё ещё non-null; null — это `Option[*unsafe T]`
(`None`).

### Выведенные формы (План 138.5)

> ⚠️ **ВЫВЕДЕНЫ (План 138.5, жёсткая ошибка — без льготного периода):**
> префиксные формы модификаторов `ro * T` / `mut * T` / `unsafe * T`,
> стоппер распространения `safe` и интерпретация `unsafe * T` как
> `Unsafe(Pointer)` (nullable-raw указатель) больше не существуют. Они
> противоречили модели «стрелка → коробка» (мутабельность pointee живёт в
> типе постфиксно; перенацеливаемость принадлежит биндингу).

```nova
// RETIRED form:           FINAL canonical equivalent:
ro * T                  // *ro T            (postfix pointee modifier)
mut * T                 // *mut T
unsafe * T              // *unsafe T  — for a UNINIT pointee;
                        //   for a NULLABLE pointer use Option[*T]
mut * ro * Acc          // *mut *ro Acc     (postfix chain)
unsafe * safe T         // *T              (`safe` stopper removed)
```

- Модификатор **перед** `*` ⇒ `E_POINTER_PREFIX_MODIFIER`.
- Типовой модификатор `safe` ⇒ `E_SAFE_RETIRED` (останавливать нечего —
  префиксного распространения модификаторов больше нет).
- Перенацеливаемость выражается биндингом (`let` / `mut`), никогда `mut *`.

## Правило биндинга (D216 §2)

Ведущий `mut` / `ro` перед именем — это **биндинг** (перенацеливаемость, D36).
Он ортогонален постфиксному модификатору pointee:

```nova
ro p *Acc                   // ro binding (fixed arrow); pointee ro
mut p *Acc                  // mut binding (re-pointable); pointee mut by default
mut p *Acc  ≡  mut p *mut Acc   // mut binding defaults pointee to mut
ro p *mut Acc               // valid edge: arrow fixed, pointee writable

mut q = &acc                // mut binding; pointee mut auto (no &mut acc needed)
ro p = &acc                 // ro binding; pointee ro auto
```

`mut`-биндинг по умолчанию делает pointee `mut` (`mut p *Acc` ≡
`mut p *mut Acc`); это снижает шум в hot-path FFI-коде. Перенацеливаемость
по-прежнему приходит только от биндинга — префикса `mut *` в типе нет.

## Порядок в цепочке (D216 §3)

Модификатор pointee записывается **постфиксно**, сразу после каждого `*`, и
применяется к **цели** этого уровня `*`; читается слева направо:

```nova
*mut *ro Acc        // writable-target pointer → (read-only-target pointer → Acc)
                    // *p  = другой_pointer OK   (outer pointee mut)
                    // **p = новое_значение ERR  (inner pointee ro)

*ro *mut Acc        // read-only-target pointer → (writable-target pointer → Acc)
                    // *p  = ...            ERR  (outer pointee ro)
                    // **p = ...            OK   (inner pointee mut)
```

Перенацеливаемость переменной, держащей цепочку, — как всегда, дело биндинга
(`let` / `mut`).

## `&value` + escape-анализ (D216 §4)

```nova
ro acc = Account { name: "Piter" }    // acc — heap reference
ro p = &acc                            // ro binding, type *ro Account; GC tracks acc

ro x = 42                              // x — stack primitive
ro p = &x                              // x auto-promoted to heap; type *ro i64
```

**Критично:** `&value` — это **НЕ borrow из Rust** (D32 амендмент). Нет
lifetime-чекера, нет параметров `'a`, нет XOR-алиасинга. Безопасность
обеспечивается:
1. Escape-анализ + auto-promote (в стиле Go) для stack-значений
2. Unsafe-gating — `&` и deref указателя только в unsafe-контексте
3. GC honor-system — пользователь обещает не триггерить GC в unsafe (D216 §16)

## Авто-deref (D216 §5)

```nova
unsafe {
    p.field                 // ✓ auto-deref one level (read)
    p.method()              // ✓ auto-deref method call
    p.field = v             // ✓ auto-deref assignment (requires *mut T)
    *p                      // ✓ explicit deref
    (*p).field              // ✓ multi-level chain через explicit *
}
```

| Оп | `*ro T` | `*mut T` |
|---|---|---|
| `p.field` (чтение) | ✓ | ✓ |
| `p.field = v` (присваивание) | ❌ E_POINTER_RO_ASSIGN | ✓ |
| `p.method()` (ro recv) | ✓ | ✓ |
| `p.method()` (mut recv) | ❌ E_POINTER_RO_MUT_METHOD | ✓ |

**Только один уровень.** Многоуровневые требуют явной цепочки `(*p).field`
(паттерн Go/D; авто-deref-рекурсия, зависящая от пути, = запутывает).

## Арифметика указателей (D216 §6)

```nova
unsafe {
    ro p1 = some_ptr + 1            // *unsafe T (degrades — alignment/bounds gone)
    ro diff = p2 - p1               // isize (element count)
    *p1                              // deref of a degraded *unsafe T pointee
}
```

- `+`/`-` только внутри `unsafe { }`, результат `*unsafe T` для `ptr ± int`,
  `isize` для `ptr - ptr`
- Единицы: в масштабе sizeof(T) (конвенция C/Rust)
- `*`/`/`/`%` — `E_PTR_ARITHMETIC_INVALID`

## Null safety: `Option[*T]` + NPO (D216 §7)

```nova
external fn malloc(sz usize) -> Option[*u8]
// C codegen: uint8_t* malloc(size_t sz);   // single pointer, NULL = None

unsafe {
    match malloc(1024) {
        Some(buf) => use(buf),               // buf: *u8 non-null guaranteed
        None      => Fail.throw(OutOfMemory),
    }
}
```

**NPO применяется к:** `Option[*T]`, `Option[*fn(...)]`, `Option[ptr]`,
`Option[Newtype-over-pointer]`.

**Исключены:** `Option[Option[*T]]` — tagged-fallback + `W_OPTION_DOUBLE_NESTED`.

## Блок `unsafe { }` (D216 §8, D2 амендмент)

```nova
fn safe_user_code() {
    // ro x = *p                    ← ERROR E_UNSAFE_REQUIRED
    // ro v = buf[2]                ← ERROR E_UNSAFE_REQUIRED (ptr[i] ≡ *(ptr+i))

    unsafe {
        ro x = *p                    // ✓ pointer deref
        ro v = buf[2]                // ✓ pointer index (ptr[i] syntax, [M-118-ptr-index-unsafe])
        ro y = malloc(1024)          // ✓ external fn returning pointer
    }
}
```

**Операции, требуемые внутри `unsafe { }`:**

| Оп | Пример | Примечания |
|---|---|---|
| Deref указателя | `*p` | читает/пишет pointee |
| Индекс указателя | `p[i]` | `≡ *(p + i)` — без проверки границ |
| Address-of | `&value` | создаёт типизированный указатель |
| Вызов unsafe fn | `ffi_write(...)` | тело `unsafe fn` |
| Сравнение порядка | `p < q` | упорядочивание адресов |

**Индекс указателя `ptr[i]`** (D216 §8, закрыт `[M-118-ptr-index-unsafe]`
2026-06-09): `ptr[i]` — синтаксический сахар для `*(ptr + i)` — сырая
арифметика указателя со смещением, без проверки границ, указатель должен быть
валиден. Требует `unsafe { }` или тела `unsafe fn`.

```nova
unsafe fn read_at(p *u8, i int) -> u8 { p[i] }   // ✓ inside unsafe fn

// Outside unsafe — compile error:
// ro v = buf[0]                 ← E_UNSAFE_REQUIRED
```

**Реализация:** сахар над встроенным обработчиком эффекта `unsafe_handler`.

```nova
unsafe { expr }
// ≡
with unsafe_handler { perform UnsafeOps.<op>(expr) }
```

Дух D2 (всё — эффекты) сохранён через встроенный `unsafe_handler`
(не переопределяется пользователем). Распространения эффектов наверх нет —
инкапсулируется на fn (канонический паттерн Rust).

## Атрибут функции `#unsafe` (D216 §9)

```nova
#unsafe
fn ffi_wrapper(p *T) -> T {
    *p                              // ✓ body implicitly unsafe context
}

fn safe_caller() {
    // ffi_wrapper(p)               ← ERROR E_UNSAFE_CALL_REQUIRES_WRAP
    unsafe {
        ro x = ffi_wrapper(p)       // ✓
    }
}
```

- Тело `#unsafe fn` имплицитно — unsafe-контекст
- Вызывающий должен обернуть вызов в `unsafe { }` (даже другая `#unsafe fn` —
  визуальный маркер)
- Распространения эффектов наверх НЕТ

## Указатели на функции `*fn(...)` (D216 §10)

```nova
external fn libuv_set_timer_cb(cb *fn(i64) -> ()) -> i64

fn my_callback(timeout i64) -> () { ... }       // no Fail

unsafe {
    libuv_set_timer_cb(my_callback as *fn(i64) -> ())
}
```

- Каст `fn → *fn` — требуется captureless (`E_CLOSURE_HAS_ENV`)
- Каст `*fn → fn` — unsafe (оборачивает в captureless closure)
- **Callback no-throw:** каст Fn-с-Fail → `*fn` — `E_CALLBACK_THROWS_OVER_C_ABI`
- **external fn без Fail:** `external fn ... Fail -> ...` — `E_EXTERNAL_FN_FAIL_EFFECT`

C ABI текущей платформы (System V на Unix, MS x64 на Windows). Явных
ключевых слов `extern "C"` нет — единый ABI V1.

## Контракт аллокации FFI-хендлов (D216 §18)

**Канон для opaque-хендлов — tuple-newtype** (zero-overhead):

```nova
type Sqlite3Handle(*sqlite3)               // stack, single pointer ABI
external fn open(path str) -> (Option[Sqlite3Handle], i64)
```

vs форма записи (лишняя косвенность — ABI указатель-на-структуру):

```nova
type DbSession {
    ro handle Sqlite3Handle
    ro path str
    ro opened_at Time
}                                           // record — для handles с extra state
```

Миграция примеров из cookbook Плана 115 V1 (форма record) → tuple newtype
(zero-overhead) отслеживается в `[M-118-handle-migration]`.

## GC honor-system (D216 §16)

Внутри `unsafe { ... }` пользователь **обещает** не триггерить GC между
созданием указателя и его использованием. GC-триггер = аллокация в куче,
yield-point (await/spawn/supervised), строковое форматирование, которое
аллоцирует, вызовы `#parks`/`#wakes`-функций.

Компилятор эмитит предупреждение `W_UNSAFE_GC_TRIGGER` на каждом месте
нарушения. Подавление: маркер строки `// noqa: W_UNSAFE_GC_TRIGGER`.

V1 GC = Boehm conservative → не двигает объекты → в V1 безопасно через
предупреждение. Будущий moving GC потребует формального pin API
(`[M-118-pin-api]` followup).

## Debug-форматирование указателей (D216 §17, План 91.14 D229)

Каноническая форма — format-spec `${expr:?}` (План 91.14, D229):

```nova
unsafe {
    ro p *ro Account = &acc
    ro s = "ptr=${&value:?}"                  // V3 canonical (Plan 91.14)
    println("pointer: ${p:?}")                // → "pointer: 0x7f... -> Account"
}
```

- `${p:?}` debug-format интерполяция — канонический рендер указателя внутри
  `unsafe { ... }` (План 91.14 D229).
- `(*T).to_debug_str() -> str` — легаси built-in алиас, оставлен для обратной
  совместимости; та же семантика, что `${p:?}`, разрешён только в unsafe.
- Прямая интерполяция `"${p}"` (Display) → `E_PTR_NO_DISPLAY_USE_DEBUG_STR`;
  хинт диагностики указывает на `${p:?}` (обновлено в Ф.5.3).
- Адреса указателей недетерминированы, утекают ASLR-информацию — явное
  решение заставило так сделать.

## Запрещённые операции (D216 §15)

```nova
unsafe {
    ro arr = [1, 2, 3]
    ro p = &arr[1]               // ❌ E_ARRAY_INDEX_PTR_BANNED
                                  //   (array may realloc / GC compaction)
}

ro p Option[*u8] = null          // ❌ E_NULL_LITERAL_USE_NONE; use None
mut p *mut u8 = undefined        // ❌ E_UNDEFINED_USE_NONE_INIT_PATTERN
```

## Коды диагностик компилятора

### Ошибки

- `E_UNSAFE_REQUIRED` — операция с указателем вне unsafe-контекста (`*p`, `p[i]`, `&v`, order-compare)
- `E_UNSAFE_CALL_REQUIRES_WRAP` — вызов `#unsafe` fn без unsafe-обёртки
- `E_UNSAFE_T_READ_REQUIRES_WRAP` — чтение значения `unsafe T` без блока `unsafe { }` (V2 §V2.3)
- `E_UNSAFE_ARG_REQUIRES_WRAP` — передача аргумента `unsafe T` без unsafe-обёртки (V2 §V2.3b)
- `E_UNSAFE_T_NARROW_REQUIRES_UNSAFE` — сужающий каст `unsafe T → T` без unsafe (V2 §V2.3b)
- `E_ARRAY_INDEX_PTR_BANNED` — `&arr[i]`
- `E_NULL_LITERAL_USE_NONE` — использован литерал `null` (общий); используйте `None`
- `E_NULL_PTR_RETRACTED_USE_OPTION` — `null ptr` отозван; используйте `Option[ptr] = None`
- `E_UNDEFINED_USE_NONE_INIT_PATTERN` — использован `undefined`
- `E_CLOSURE_HAS_ENV` — каст fn → *fn с closure-env
- `E_CALLBACK_THROWS_OVER_C_ABI` — каст Fn-с-Fail → *fn
- `E_EXTERNAL_FN_FAIL_EFFECT` — external fn с эффектом Fail
- `E_PTR_ARITHMETIC_INVALID` — `p * 2`, `p / 4`, и т.д.
- `E_POINTER_RO_ASSIGN` — `*p = v` / `p.field = v` где p — ro
- `E_POINTER_RO_MUT_METHOD` — `p.mut_method()` где p — ro
- `E_PTR_CAST_INVALID_TARGET` — `p as bool / f64 / ...`
- `E_INVALID_POINTER_MODIFIER` — `*const T` и др.
- `E_POINTER_PREFIX_MODIFIER` — модификатор **перед** `*` (`mut * T` / `ro * T` /
  `unsafe * T`); используйте постфиксный pointee `*mut T` / `*ro T` / `*unsafe T`
  или биндинг `mut x *T` (План 138.5, расширяет `E_INVALID_POINTER_MODIFIER`)
- `E_SAFE_RETIRED` — использован типовой модификатор `safe`; стоппер
  распространения `safe` выведен (останавливать префиксное распространение
  нечего) (План 138.5)
- `E_PARSE_POINTER_TYPE_INCOMPLETE` — `*` без типа
- `E_REALTIME_POINTER_OP` — операция с указателем в теле `#realtime fn`
- `E_UNSAFE_HANDLER_BUILTIN_ONLY` — попытка определить пользовательский unsafe_handler
- `E_AMP_CONST_BINDING` — `&const_value`
- `E_AMP_LITERAL` — `&42`
- `E_PTR_NO_DISPLAY_USE_DEBUG_STR` — интерполяция `"${p}"`; хинт предлагает
  канонический `${p:?}` (План 91.14 D229) или легаси `p.to_debug_str()`
- `E_VARARG_NOT_SUPPORTED` — vararg FFI-вызов
- `E_CAST_RAW_FN_TO_CLOSURE` — каст `*fn → fn` вне unsafe

#### Ошибки V3-композиции модификаторов (D216 V3 амендмент, 2026-06-04)

- `E_MUTABILITY_CONFLICT_VALUE_TYPE` — в позиции типа `ro mut T` / `mut ro T`
  на **value-типе T** (примитивы / value-записи / именованные кортежи /
  анонимные кортежи / Unit). Биндинг-форма `ro x mut T` остаётся разрешённой
  (ортогональные биндинг-модификаторы). Spec §V3.1.
- `E_MODIFIER_ORDER` — модификатор безопасности (`unsafe`) оборачивает
  модификатор мутабельности (`ro` / `mut`); требуется обратный порядок —
  **safety-inner / mutability-outer** (`ro unsafe T` ✅ / `unsafe ro T` ❌),
  согласовано с `external unsafe fn`. Применяется к value-T и к постфиксному
  содержимому **pointee** (`*ro unsafe T` ✅ / `*unsafe ro T` ❌). Spec §V3.2
  (ПЕРЕВЁРНУТ в Плане 138.5).
- `E_REDUNDANT_TYPE_MODIFIER` — повторение модификатора одного класса.
  **Биндинг-уровень** (`ro x ro T`) и **постфиксная цепочка pointee**
  (`*ro ro T`) сохраняются; старые V3-случаи префиксных цепочек на уровне
  типа (`ro * ro T`, `unsafe * unsafe T`) — неактуальны: префикс перед `*` —
  уже `E_POINTER_PREFIX_MODIFIER` (План 138.5). Запасной выход `safe` выведен.
  Spec §V3.4.

> **Примечание:** стоппер распространения `safe` и форма `Unsafe(Pointer)`
> (`unsafe * T` = nullable-raw) ВЫВЕДЕНЫ (План 138.5). `safe` в позиции типа
> ⇒ `E_SAFE_RETIRED`; nullable-указатели используют `Option[*T]`.

### Предупреждения

- `W_UNSAFE_GC_TRIGGER` — GC-триггер внутри unsafe с активным указателем в области видимости
- `W_PTR_AS_USIZE_GC_HASH_HAZARD` — `p as usize` как ключ HashMap
- `W_OPTION_DOUBLE_NESTED` — fallback NPO для `Option[Option[*T]]`

## Сравнение с мейнстримом

| Язык | Typed ptr | Модель unsafe | Null safety | Авто-deref | Арифметика указателей |
|---|---|---|---|---|---|
| Rust | `*const T`/`*mut T`/`&T`/`&mut T` | `unsafe {}` + `unsafe fn` | `Option<&T>` + NPO | через ref | только unsafe |
| Zig | `*T`/`*const T`/`[*]T` | (intrinsics кастов) | `?*T` + NPO | `.*` постфикс + `.` | `+` для `[*]T` |
| C# | `T*` / `ref T` / `in T` / `out T` | модификатор `unsafe` | `T?` | стрелка `p->field` | только unsafe |
| Swift | `UnsafePointer<T>` / `UnsafeMutablePointer<T>` | префикс на основе типа | Optional + NPO | `.pointee` | только `.advanced(by:)` |
| D | `T*` / `ref T` / `scope T*` | `@safe`/`@trusted`/`@system` | `Nullable!T` | `p.field` авто | только `@system` |
| Go | `*T` (managed) / `unsafe.Pointer` | пакет `unsafe` | Nil в рантайме | `p.field` авто | только `unsafe.Pointer` |
| **Nova V1** (План 115) | только `ptr` | (нет) | `null ptr` | (нет) | запрещено |
| **Nova V2** (План 118) | **семейство `*T`** + `unsafe` | `unsafe { }` + `#unsafe` (D2 амендмент) | `Option[*T]` + NPO | `p.field`/`p.method()` один уровень | gated unsafe → `*unsafe T` |
| **Nova FINAL** (План 138.5) | **постфиксный pointee** `*ro T` / `*mut T` / `*unsafe T`; перенацеливаемость = биндинг (`let`/`mut`) | (как V2) + правила композиции value-T (§V3.1-V3.2) | `Option[*T]` (только) + NPO | (как V2) | (как V2) → `*unsafe T` |

## См. также

- [`docs/plans/118-typed-pointers-and-unsafe.md`](../plans/118-typed-pointers-and-unsafe.md) — дорожная карта ядра Плана 118
- [`docs/plans/118.1-ffi-intrinsics-and-cstring.md`](../plans/118.1-ffi-intrinsics-and-cstring.md) — суб-план 118.1 (FFI intrinsics)
- [`docs/plans/118.2-slice-fat-pointer-and-uninit.md`](../plans/118.2-slice-fat-pointer-and-uninit.md) — суб-план 118.2 (slice + uninit)
- [`docs/plans/118.3-pointer-concurrency-safety.md`](../plans/118.3-pointer-concurrency-safety.md) — суб-план 118.3 (concurrency)
- [`docs/guide/ffi-cookbook.md`](ffi-cookbook.md) — паттерны FFI с ptr + tuple FFI (План 115 V1)
- [D216 V1](../../spec/decisions/02-types.md#d216-typed-pointer-family--unsafe-model--null-safety-через-npo) — фундамент spec (семейство типизированных указателей + модель unsafe + NPO)
- [D216 FINAL pointer model (План 138.5)](../../spec/decisions/02-types.md#d216-typed-pointer-family--unsafe-model--null-safety-через-npo) — тип указателя = только постфиксный pointee-mut; перенацеливаемость = биндинг (D36); префиксные модификаторы ⇒ `E_POINTER_PREFIX_MODIFIER`; nullable = только `Option[*T]`; `safe` и `Unsafe(Pointer)` выведены
- [D216 V2 амендмент](../../spec/decisions/02-types.md#d216-v2-amend-2026-06-04--universal-right-binding-rule-для-type-level-modifiers--unsafe-t-first-class) — историческое правило right-binding (§V2.1, ОТОЗВАНО) + first-class value-обёртка `unsafe T` (§V2.3, СОХРАНЕНО) + пересчёт NPO (§V2.4)
- [D216 V3 амендмент](../../spec/decisions/02-types.md#d216-v3-amend-plan-1185-v3-2026-06-04--4-modifier-composition-rules) — правила композиции модификаторов value-T (V3.3/V3.4 заменены Планом 138.5):
  - §V3.1 — запрет смежности `ro+mut` с учётом storage-class (`E_MUTABILITY_CONFLICT_VALUE_TYPE`) — СОХРАНЕНО
  - §V3.2 — порядок модификаторов safety-inner / mutability-outer (`ro unsafe T`; `E_MODIFIER_ORDER`) — ПЕРЕВЁРНУТ, СОХРАНЁН
  - §V3.3 — right-binding распространение — ЗАМЕНЁН (префиксного распространения нет)
  - §V3.4 — стоппер `safe` — ВЫВЕДЕН; `E_REDUNDANT_TYPE_MODIFIER` сохраняется на уровне биндинга/постфиксного pointee
- [D2 амендмент](../../spec/decisions/04-effects.md#d2) — восстановление ключевого слова unsafe (сахар обработчика эффекта)
- [D214 амендмент](../../spec/decisions/02-types.md#d214-ptr-opaque-pointer-type--tuple-ffi-returns--opaque-handle-pattern) — переопределение `ptr`
- [D32 амендмент](../../spec/decisions/02-types.md#d32-семантика-передачи-параметров) — `&value` — не borrow из Rust
- [`examples/typed_pointers/`](../../examples/typed_pointers/) — минимальные рабочие примеры
