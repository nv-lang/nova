---
source_rev: 3c7a1adda
source_date: 2026-08-06
---

[English](typed-pointers.md) | **Русский**

<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Типизированные указатели (семейство `*T`) + модель `unsafe`

> **Планы 118 / 118.5** (D216 V1 + V2 + V3 амендменты, **План 138.5 FINAL
> pointer model**, D2 амендмент, D214 амендмент, D32 амендмент, D184 амендмент,
> **амендмент Плана 174.5 «всё через методы»**). **Статус:** ✅ FINAL pointer
> model АКТИВЕН с 2026-06-11; операторный доступ к содержимому указателей
> ВЫВЕДЕН 2026-07-09 в пользу intrinsic-методов (`E_POINTER_OP_USE_METHOD`);
> с 2026-08-05 снятые формы записи отвергает и `nova check`, а голый
> `*uninit T` строго только для чтения.

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
  коробкой*: `*mut T` (в коробку можно писать), голый `*T` (коробка только
  для чтения — по умолчанию; явное `*ro T` избыточно и отвергается,
  `E_REDUNDANT_POINTER_RO`), `*uninit T` (коробка может быть
  неинициализированной — и всё равно только для чтения; запись требует
  составной формы `*mut uninit T`).
- **Сама стрелка — биндинг (`ro` / `mut`, D36)** — говорит, *можно ли
  перенацелить стрелку на другую коробку*: `ro p` = стрелка зафиксирована,
  `mut p` = стрелку можно перенацеливать.

Это две независимые оси. Они никогда не пересекаются, потому что одна живёт в
**типе** (постфиксно на `*`), а другая — на **биндинге** (перед именем):

```nova
mut p *mut T        // arrow re-pointable (mut binding) + box writable (*mut pointee)
ro q *T             // arrow fixed (ro binding)         + box read-only (*T pointee)
mut p *T            // arrow re-pointable               + box read-only
ro p *mut T         // arrow fixed                      + box writable
```

> **НЕ существует префикса `mut *` / `ro *` / `uninit *`.** Модификатор перед
> `*` — жёсткая ошибка `E_POINTER_PREFIX_MODIFIER` (прецедент: в Rust
> `*mut T` / `*const T` = мутабельность pointee; `let mut p` —
> перенацеливаемость).

### Канонические формы (постфиксный модификатор pointee)

```nova
*T                  // pointer to read-only T (the ONLY read-only form —
                    //   `*ro T` is redundant, E_REDUNDANT_POINTER_RO)
*mut T              // pointer to mutable T (p.write(v) allowed)
*uninit T           // pointer to possibly-uninit T — READ-ONLY pointee
*mut uninit T       // writable + possibly-uninit pointee (the write opt-in)
Option[*T]          // NULLABLE pointer (NPO: None = null, 8 bytes)
Option[*uninit T]   // FFI nullable-uninit ptr (None = null, Some = non-null
                    //   ptr to a possibly-uninit pointee)
```

Модификатор **всегда постфиксный** — он крепится к pointee того `*`, за
которым следует, а «только для чтения» — это дефолт pointee (для него
модификатор не пишется). Ось `uninit` ортогональна оси `mut`: голый
`*uninit T` помечает pointee как possibly-uninitialized, но записи **не**
даёт — запись требует явной составной формы `*mut uninit T` (порядок
фиксирован `E_MODIFIER_ORDER`: мутабельность снаружи / безопасность внутри).
Само значение указателя **всегда non-null**; для nullable используйте
`Option[*T]` (zero-cost через NPO).

### Перенацеливаемость — это биндинг (D36), а не тип

```nova
mut p *T = &acc     // mut binding → p may be reassigned later (p = &other)
ro q *T = &acc      // ro binding → q is fixed (q = &other ⇒ E_REBIND)
```

Переменная-указатель подчиняется **тем же** правилам `ro` / `mut`, что и
любая другая переменная (D36). Тип никогда не кодирует перенацеливаемость.

### Цепочки указателей (несколько уровней) — постфиксно на каждом `*`

```nova
*mut *Node          // writable-target pointer  →  (read-only-target pointer → Node)
                    //   p.write(other_ptr)   OK   (outer pointee mut)
                    //   p.read().write(v)    ERR  (inner pointee ro)

**mut Node          // read-only-target pointer →  (writable-target pointer → Node)
                    //   p.write(other_ptr)   ERR  (outer pointee ro)
                    //   p.read().write(v)    OK   (inner pointee mut)
```

Каждый модификатор стоит постфиксно, сразу после своего `*`, и описывает цель
этого уровня `*`. Читается слева направо. Доступ на каждом уровне идёт через
intrinsic-методы (`.read()` / `.write(v)`) — оператора `*p` нет (см.
[доступ — только методы](#доступ--только-методы-план-1745)).

### Возврат указателей — pointee-mut по умолчанию (D184 амендмент)

D184 (мутабельность возвращаемого типа по умолчанию) применяется к **pointee**
для возвращаемых указателей:

```nova
fn alloc_cell() -> *T       // returns a ptr to read-only T (the pointee L3 default)
fn alloc_mut()  -> *mut T   // returns a ptr to WRITABLE T
```

Перенацеливаемость **результата** решается в точке биндинга, а не в типе
возврата:

```nova
ro p = alloc_mut()          // p fixed (ro binding); p.write(v) still OK (pointee mut)
mut q = alloc_mut()         // q re-pointable + q.write(v) OK
```

Это устраняет старую неоднозначность «двух mut в позиции возврата» (внешнего
pointer-mut больше не из чего выбирать).

### FFI out-param / неинициализированный pointee

```nova
extern "C" fn os_read(fd int, buf *mut uninit u8, n int) -> int
//                              ^^^^^^^^^^^^^^^
//                       pointee writable (*mut) + possibly-uninit (uninit);
//                       arrow re-pointability is the binding's concern
```

Голый `*uninit T` (без `mut`) — pointee только для чтения: запись с
Nova-стороны (`p.write(v)` / `p.write_at(i, v)` и остальное write-семейство)
отвергается с `E_POINTER_RO_ASSIGN`. Опт-ин на запись — всегда явная
составная форма `*mut uninit T`; для FFI out-параметра, чей буфер заполняет
вызываемая сторона, объявлять нужно именно её.

## Доступ — только методы (План 174.5)

> **Амендмент D216 «всё через методы» (План 174.5, 2026-07-09):** доступ к
> значению и адресная арифметика на сырых указателях — **только через
> intrinsic-методы**. Операторные формы ВЫВЕДЕНЫ с жёсткой ошибкой
> `E_POINTER_OP_USE_METHOD` — включая формы чтения: `*p`, `*p = v`, `p[i]`,
> `p[i] = v`, `p ± i`, `p - q`, `p < q` (все сравнения порядка).
> `nova check` отвергает и формы записи (с 2026-08-05), и формы чтения
> `x = *p` / `y = p[i]` (с 2026-08-06) — раньше они падали только на
> стадии сборки.

| Метод | Заменяет | Семантика |
|---|---|---|
| `p.read() -> T` | `*p` | чтение по указателю |
| `p.write(v T)` | `*p = v` | запись по указателю — требует `*mut`-pointee |
| `p.read_at(i) -> T` | `p[i]` | чтение `*(p+i)`, element units, без проверки границ |
| `p.write_at(i, v)` | `p[i] = v` | запись `*(p+i)` — требует `*mut`-pointee |
| `p.offset(n) -> *T` | `p ± i` | адресная арифметика, element units; тип **НЕ** деградирует |
| `p.dist(q) -> int` | `p - q` | знаковое число элементов; порядок = знак (`p < q` выведен) |
| `p.read_unaligned()` / `p.write_unaligned(v)` | — | memcpy-семантика (невыровненный доступ) |
| `p.read_volatile()` / `p.write_volatile(v)` | — | volatile-доступ |
| `p.write(v *T) -> *mut T` | — | копия из указателя-источника (без value-копии) |
| `p.copy_from(src, n)` / `p.copy_to(dst, n)` | — | memmove; варианты `_nonoverlapping` — memcpy |

```nova
mut buf []int = [10, 20, 30, 40]
unsafe {
    ro p = buf.ptr()
    assert(p.read_at(2) == 30)          // was p[2] — retired
    ro p2 = p.offset(2)                 // was p + 2 — retired; type stays *int
    assert(p2.read() == 30)             // was *p2 — retired
    assert(p2.dist(p) == 2)             // was p2 - p — retired
}
unsafe {
    mut q = buf.ptr()                   // mut receiver overload → *mut int
    q.write_at(1, 99)                   // was q[1] = 99 — retired
}
assert(buf[1] == 99)
```

**Что осталось операторами:** `p == q` / `p != q` (идентичность), `p as *U`
(каст; unsafe при `U ≠ T`) и одноуровневый авто-deref **доступ к полям**
(следующий раздел). `[]`-индексация — только у безопасных контейнеров
(D138), у указателей её нет.

**Право записи** проверяется в одном месте для всего write-семейства
(`.write` / `.write_at` / `.write_unaligned` / `.write_volatile` /
`.copy_from[_nonoverlapping]`): pointee обязан быть `*mut …`, иначе
`E_POINTER_RO_ASSIGN`.

## Авто-deref доступ к полям (D216 §5)

Одноуровневый доступ к полю через указатель работает без явного deref:

```nova
type Counter { mut v int }

mut a = Counter { v: 1 }
ro p = &a                   // a is mut → p is *mut Counter
p.v = 5                     // ✓ field store via auto-deref (requires *mut pointee)
ro r = p.v                  // ✓ field read via auto-deref (any *T)
assert(a.v() == 5)
```

| Оп | `*T` | `*mut T` |
|---|---|---|
| `p.field` (чтение) | ✓ | ✓ |
| `p.field = v` (присваивание) | ❌ E_POINTER_RO_ASSIGN | ✓ |

**Только один уровень.** Для более глубоких цепочек сначала прочитайте
значение указателя (`p.read()`) и продолжайте на значении.

> **Вызовы методов через указатель** (`p.method()`) работают для обычных
> методов. Осталось одно узкое исключение: вызов *метода-свойства поля*
> через указатель (`p.v()` для поля `v`) сейчас не компилируется — дефект
> передан и отслеживается. Читайте поле напрямую (`p.v`) или сначала
> значение (`p.read().v()`).

## Краткий справочник

| Потребность | Каноническая FINAL-форма | Spec |
|---|---|---|
| Типизированный указатель (цель ro по умолчанию) | `*T` (`*ro T` избыточен — `E_REDUNDANT_POINTER_RO`) | [D216 §1](../../spec/decisions/02-types.md#d216-typed-pointer-family--unsafe-model--null-safety-через-npo) |
| Указатель на записываемую цель | `*mut T` | D216 §1 |
| Указатель на possibly-uninit цель (только чтение) | `*uninit T` | D216 §1 + V2 §V2.3 |
| Записываемая possibly-uninit цель | `*mut uninit T` | D216 §V2.2 (№358) |
| Перенацеливаемая переменная-указатель | `mut p *T` (биндинг) | D216 §2 + D36 |
| Зафиксированная переменная-указатель | `ro p *T` (биндинг) | D216 §2 + D36 |
| Nullable типизированный указатель | `Option[*T]` (NPO) | D216 §7 + V2 §V2.4 |
| FFI nullable-uninit указатель | `Option[*uninit T]` | D216 §1 + V2 §V2.4 |
| Возврат указателя (записываемая цель) | `-> *mut T` | D184 амендмент (План 138.5) |
| Создание указателя (safe, auto-promote) | `&value` | D216 §4 + 118.6 |
| Сырой stack-адрес (без промоута) | `unsafe { raw &value }` | D216 §4 амендмент 2 (118.7) |
| Чтение / запись по указателю | `p.read()` / `p.write(v)` | D216 амендмент (174.5) |
| Чтение / запись по индексу | `p.read_at(i)` / `p.write_at(i, v)` | D216 амендмент (174.5) |
| Арифметика указателей | `p.offset(n)` / `p.dist(q)` | D216 амендмент (174.5) |
| Авто-deref поле | `p.field` / `p.field = v` | D216 §5 |
| Граница unsafe | `unsafe { ... }` блок / `unsafe fn` | D216 §8-9 |
| Указатель на функцию для FFI | `*fn(Args) -> Ret` | D216 §10 |
| Opaque-нетипизированный (legacy) | `ptr` (D214 амендмент → newtype `Option[*uninit ()]`) | D214 амендмент |

## Семейство типов `*T`

**ABI:** все варианты — шириной в один указатель (8 байт на 64-bit; bootstrap
цель — только 64-bit). C-type emission: `*T` → `const T*` (помогает
оптимизатору clang/MSVC), `*mut T` / `*uninit T` → `T*`.

**Действительность (validity):** каждое значение указателя (`*T` /
`*mut T` / `*uninit T`) **всегда non-null** (инвариант на этапе компиляции).
Nullable-вариант — `Option[*T]` через NPO (один указатель, NULL = None;
см. §V2.4 в spec). `*uninit T` описывает possibly-**неинициализированный**
pointee — сам *указатель* всё ещё non-null; null — это `Option[*uninit T]`
(`None`).

### Выведенные формы (План 138.5)

> ⚠️ **ВЫВЕДЕНЫ (жёсткие ошибки — без льготного периода):** три слоя
> ретракций. **План 138.5** снял префиксные формы модификаторов `ro * T` /
> `mut * T` / `unsafe * T`, стоппер распространения `safe` и интерпретацию
> `unsafe * T` как `Unsafe(Pointer)` (они противоречили модели
> «стрелка → коробка»). **Планы 118.6/118.7** сняли `addr_of()` /
> `addr_of_mut()` (`E_ADDR_OF_REMOVED`) в пользу safe `&x` + unsafe
> `raw &x`, а План 118.1.7 заменил атрибут `#unsafe` ключевым словом
> `unsafe fn` (`E_UNSAFE_ATTR_DEPRECATED`). **План 174.5** вывел всё
> операторное семейство указателей (`E_POINTER_OP_USE_METHOD` — см.
> [доступ — только методы](#доступ--только-методы-план-1745)).

```nova
// RETIRED form:           FINAL canonical equivalent:
ro * T                  // *T               (postfix pointee modifier;
                        //   bare = ro, `*ro T` itself is E_REDUNDANT_POINTER_RO)
mut * T                 // *mut T
unsafe * T              // *uninit T  — for a UNINIT pointee (§10a rename,
                        //   was `*unsafe T`); for a NULLABLE pointer use Option[*T]
mut * ro * Acc          // *mut *Acc        (postfix chain)
unsafe * safe T         // *T              (`safe` stopper removed)
```

- Модификатор **перед** `*` ⇒ `E_POINTER_PREFIX_MODIFIER`.
- Типовой модификатор `safe` ⇒ `E_SAFE_RETIRED` (останавливать нечего —
  префиксного распространения модификаторов больше нет).
- Перенацеливаемость выражается биндингом (`ro` / `mut`), никогда `mut *`.

## Правило биндинга (D216 §2)

Ведущий `mut` / `ro` перед именем — это **связывание** (перенацеливаемость, D36).
Оно НЕЗАВИСИМО от постфиксного модификатора: `mut`-связывание НЕ делает
содержимое за указателем изменяемым:

```nova
ro p *Acc                   // ro-binding: arrow fixed, pointee read-only
mut p *Acc                  // mut-binding: arrow re-pointable, pointee STILL read-only
mut p *mut Acc              // writable pointee — the ONLY way: explicit *mut
ro p *mut Acc               // valid edge: arrow fixed, pointee writable

p = other_ptr               // allowed only with a mut binding (L1)
p.field = 1                 // allowed only with a *mut pointee (L3)
```

Связывание ничего не говорит о содержимом: `mut p *Acc` позволяет перенацелить
стрелку, но запись за ней (`p.field = …` / `p.write(v)`) по-прежнему требует
явного `*mut Acc` (D246: `*T ≡ *ro T` универсально, оси L1/L2/L3 независимы).
Перенацеливаемость приходит только от связывания — префикса `mut *` в типе нет.

## `&value` + escape-анализ (D216 §4, 118.6/118.7)

```nova
ro acc = Account { name: "Piter" }    // acc — heap reference
ro p = &acc                            // safe; type *Account; GC tracks acc

ro x = 42                              // x — stack primitive
ro q = &x                              // safe; x auto-promoted to heap; type *int
```

`&x` — **safe** (План 118.6): обёртка `unsafe { }` не нужна — escape-анализ
авто-промоутит stack-значения в кучу. Мутабельность pointee результата
следует за **биндингом источника** (амендмент D216 §4, решение владельца
2026-08-06): `&a` от `mut`-переменной — `*mut T`, от `ro`-переменной — `*T`.
Канонический способ получить пишущий указатель — просто:

```nova
mut x int = 1
ro p = &x                   // x is mut → p is *mut int, no cast needed
unsafe { p.write(42) }
assert(x == 42)
```

Явная аннотация — равносильная форма (`ro p *mut int = &x`); в аннотациях
голый `*T` по-прежнему всегда означает read-only pointee. Старый опт-ин
кастом `(&x) as *mut int` **ретрактирован**: переутверждение мутабельности
кастом над тем же типом pointee отвергается (семья
`E_POINTER_OP_USE_METHOD`). И гарантия за всем этим: от `ro`-переменной
пишущий указатель **не получить никаким путём**.

Для **сырого stack-адреса** без escape-анализа и авто-промоута есть отдельный
оператор (План 118.7) — он может стать висячим после выхода из скоупа,
поэтому требует unsafe-контекста:

```nova
unsafe {
    ro rp = raw &x          // raw stack address; E_UNSAFE_REQUIRED outside unsafe
}
```

`addr_of()` / `addr_of_mut()` выведены (`E_ADDR_OF_REMOVED`).

**Критично:** `&value` — это **НЕ borrow из Rust** (D32 амендмент). Нет
lifetime-чекера, нет параметров `'a`, нет XOR-алиасинга. Безопасность
обеспечивается:
1. Escape-анализ + auto-promote (в стиле Go) для stack-значений
2. Unsafe-gating сырого доступа — intrinsic-методы и `raw &x`
3. GC honor-system — пользователь обещает не триггерить GC в unsafe (D216 §16)

## Арифметика указателей (D216 §6, форма методов)

```nova
unsafe {
    ro p2 = p.offset(1)             // element-units step; type is preserved (*T)
    ro diff = p2.dist(p)            // int (signed element count) — here 1
    ro v = p2.read()                // deref read
}
```

- `.offset(n)` / `.dist(q)` — единственная адресная арифметика; операторные
  формы `p ± i`, `p - q` и сравнения порядка `p < q` выведены
  (`E_POINTER_OP_USE_METHOD`). Порядок, когда нужен, — знак `.dist()`.
- Единицы: в масштабе sizeof(T) (конвенция C/Rust).
- `.offset()` **не** деградирует тип — результат тот же `*T` (старое правило
  «арифметика деградирует в `*uninit T`» снято).

## Null safety: `Option[*T]` + NPO (D216 §7)

```nova
extern "C" fn malloc(sz int) -> Option[*u8]
// C codegen: uint8_t* malloc(size_t sz);   // single pointer, NULL = None

unsafe {
    match malloc(1024) {
        Some(buf) => use(buf)                // buf: *u8 non-null guaranteed
        None      => Fail.throw(OutOfMemory)
    }
}
```

**NPO применяется к:** `Option[*T]`, `Option[*fn(...)]`, `Option[ptr]`,
`Option[Newtype-over-pointer]`.

**Исключены:** `Option[Option[*T]]` — tagged-fallback + `W_OPTION_DOUBLE_NESTED`.

## Блок `unsafe { }` (D216 §8/§21, D2 амендмент)

Что **требует** обёртки `unsafe { }` (checker-enforced, карта D216 §21):

| Оп | Пример | Диагностика |
|---|---|---|
| Сырой stack-адрес | `raw &x` | `E_UNSAFE_REQUIRED` |
| Вызов `unsafe fn` / `external unsafe fn` | `ffi_write(...)` | `E_UNSAFE_CALL_REQUIRES_WRAP` |
| Чтение value-биндинга `uninit T` | `ro v = u` | `E_UNSAFE_T_READ_REQUIRES_WRAP` |
| Передача аргумента `uninit T` | `f(u)` | `E_UNSAFE_ARG_REQUIRES_WRAP` |
| Сужающий каст `uninit T → T` | `u as T` | `E_UNSAFE_T_NARROW_REQUIRES_UNSAFE` |

Intrinsic-методы сырых указателей (`.read()` / `.write()` / `.read_at()` /
`.write_at()` / `.offset()` / `.dist()` / volatile / unaligned / copy) —
**контрактно unsafe**: оборачивайте их в `unsafe { }` — чекер засчитывает их
как законное использование блока, хотя отсутствие обёртки вокруг них пока
механически не отвергается (известный пробел enforcement, карта D216 §21,
п. 8).

Блок `unsafe { }`, не содержащий **ни одной** операции из карты, — сам по
себе ошибка `E_UNSAFE_UNUSED` (мёртвые unsafe-блоки не переживают
рефакторинг):

```nova
mut buf []int = [1, 2, 3]
unsafe {
    ro p = buf.ptr()
    ro v = p.read_at(2)      // ✓ intrinsic method — the block is "used"
    assert(v == 3)
}
```

Под капотом `unsafe { }` — сахар над встроенным обработчиком эффекта (дух
D2: всё — эффекты); эффект наверх не распространяется — граница
инкапсулируется на fn (канонический паттерн Rust).

## `unsafe fn` (D216 §9, План 118.1.7)

Канон — форма с **ключевым словом**; старый атрибут `#unsafe` удалён
(`E_UNSAFE_ATTR_DEPRECATED`):

```nova
unsafe fn peek(p *u8) -> u8 { p.read() }    // body is implicitly an unsafe context

fn safe_caller(p *u8) -> u8 {
    // peek(p)                  ← ERROR E_UNSAFE_CALL_REQUIRES_WRAP
    unsafe { peek(p) }          // ✓
}
```

- Тело `unsafe fn` имплицитно — unsafe-контекст.
- Вызывающий должен обернуть вызов в `unsafe { }` (даже из другой
  `unsafe fn` — визуальный маркер).
- Распространения эффектов наверх НЕТ.
- FFI-декларации компонуются так же: `external unsafe fn ...`.

## Указатели на функции `*fn(...)` (D216 §10)

```nova
extern "C" fn libuv_set_timer_cb(cb *fn(i64) -> ()) -> i64

fn my_callback(timeout i64) -> () { ... }       // no Fail

unsafe {
    libuv_set_timer_cb(my_callback as *fn(i64) -> ())
}
```

- Каст `fn → *fn` — требуется captureless (`E_CLOSURE_HAS_ENV`)
- Каст `*fn → fn` — unsafe (оборачивает в captureless closure)
- **Callback no-throw:** каст Fn-с-Fail → `*fn` — `E_CALLBACK_THROWS_OVER_C_ABI`
- **Extern fn без Fail:** extern-функция, декларирующая эффект `Fail`, —
  `E_EXTERNAL_FN_FAIL_EFFECT`
- Композиция unsafe-**указателя на функцию** сохраняет написание `unsafe`:
  `*unsafe fn(...)` (отдельное понятие от модификатора possibly-uninit-данных
  `uninit`).

C ABI текущей платформы (System V на Unix, MS x64 на Windows).

## Контракт аллокации FFI-хендлов (D216 §18)

**Канон для opaque-хендлов — tuple-newtype** (zero-overhead):

```nova
type Sqlite3Handle(*sqlite3)               // stack, single pointer ABI
extern "C" fn open(path str) -> (Option[Sqlite3Handle], i64)
```

vs форма записи (лишняя косвенность — ABI указатель-на-структуру):

```nova
type DbSession {
    ro handle Sqlite3Handle
    ro path str
    ro opened_at Time
}                                           // record — for handles with extra state
```

Миграция примеров из cookbook Плана 115 V1 (форма record) → tuple newtype
(zero-overhead) отслеживается в `[M-118-handle-migration]`.

## GC honor-system (D216 §16)

Внутри `unsafe { ... }` пользователь **обещает** не триггерить GC между
созданием указателя и его использованием. GC-триггер = аллокация в куче,
yield-point (await/spawn/supervised), строковое форматирование, которое
аллоцирует, вызовы `#parks`/`#wakes`-функций.

Это контракт спецификации, а не механическая проверка — компилятор пока не
эмитит предупреждение о нарушениях (диагностика `W_UNSAFE_GC_TRIGGER`
описана в D216 §16, но в bootstrap-компиляторе не реализована).

V1 GC = Boehm conservative → не двигает объекты → honor-system безопасна для
V1. Будущий moving GC потребует формального pin API (`[M-118-pin-api]`
followup).

## Debug-форматирование указателей (D216 §17, План 91.14 D229)

Каноническая форма — format-spec `${expr:?}` (План 91.14, D229):

```nova
unsafe {
    ro p *Account = &acc
    ro s = "ptr=${&value:?}"                  // V3 canonical (Plan 91.14)
    println("pointer: ${p:?}")                // → "pointer: 0x7f... -> Account"
}
```

- `${p:?}` debug-format интерполяция — канонический рендер указателя внутри
  `unsafe { ... }` (План 91.14 D229).
- `(*T).to_debug_str() -> str` — легаси built-in алиас, оставлен для обратной
  совместимости; та же семантика, что `${p:?}`, разрешён только в unsafe.
- Прямая интерполяция `"${p}"` (Display) → `E_PTR_NO_DISPLAY_USE_DEBUG_STR`;
  хинт диагностики указывает на `${p:?}` (см. [D229](../../spec/decisions/02-types.md#d229-Debug-protocol--format-spec-expr)).
- Адреса указателей недетерминированы, утекают ASLR-информацию — явное
  решение заставило так сделать.

## Запрещённые операции (D216 §15)

```nova
unsafe {
    ro arr = [1, 2, 3]
    ro p = &arr[1]               // ❌ E_ARRAY_INDEX_PTR_BANNED
                                  //   (array may realloc / GC compaction)
}

ro q = &42                       // ❌ E_AMP_LITERAL (no address of a literal)
```

`null` / `undefined` в языке нет: отсутствующий указатель — это
`Option[*T] = None` (легаси-написание `null ptr` отвергается с
`E_NULL_PTR_RETRACTED_USE_OPTION`).

## Коды диагностик компилятора

### Ошибки

- `E_POINTER_OP_USE_METHOD` — выведенный операторный доступ (`*p`, `*p = v`,
  `p[i]`, `p[i] = v`, `p ± i`, `p - q`, сравнения порядка) или снятый каст
  переутверждения мутабельности `p as *mut T` над тем же pointee;
  используйте intrinsic-методы (`.read()` / `.write()` / `.read_at()` /
  `.write_at()` / `.offset()` / `.dist()`) и вывод по биндингу источника
  для `&`
- `E_POINTER_RO_ASSIGN` — `p.field = v` или вызов метода write-семейства
  (`.write()` / `.write_at()` / …) через read-only pointee; записываемый
  pointee требует опт-ина `*mut T`
- `E_UNSAFE_REQUIRED` — `raw &x` вне unsafe-контекста
- `E_UNSAFE_CALL_REQUIRES_WRAP` — вызов `unsafe fn` без unsafe-обёртки
- `E_UNSAFE_T_READ_REQUIRES_WRAP` — чтение значения `uninit T` без блока `unsafe { }` (V2 §V2.3; имя кода сохранило `UNSAFE` даже после переименования type-модификатора `unsafe T` → `uninit T`, §10a)
- `E_UNSAFE_ARG_REQUIRES_WRAP` — передача аргумента `uninit T` без unsafe-обёртки (V2 §V2.3b)
- `E_UNSAFE_T_NARROW_REQUIRES_UNSAFE` — сужающий каст `uninit T → T` без unsafe (V2 §V2.3b)
- `E_UNSAFE_UNUSED` — блок `unsafe { }` без единой операции из карты D216 §21
- `E_UNSAFE_ATTR_DEPRECATED` — удалённый атрибут `#unsafe`; используйте
  ключевое слово `unsafe fn` (План 118.1.7)
- `E_ADDR_OF_REMOVED` — `addr_of()` / `addr_of_mut()`; используйте `&x` / `raw &x`
- `E_ARRAY_INDEX_PTR_BANNED` — `&arr[i]`
- `E_AMP_LITERAL` — `&42` (адрес литерала)
- `E_NULL_PTR_RETRACTED_USE_OPTION` — легаси `null ptr`; используйте `Option[ptr] = None`
- `E_CLOSURE_HAS_ENV` — каст fn → *fn с closure-env
- `E_CALLBACK_THROWS_OVER_C_ABI` — каст Fn-с-Fail → *fn
- `E_EXTERNAL_FN_FAIL_EFFECT` — extern-функция с эффектом Fail
- `E_PTR_CAST_INVALID_TARGET` — `p as bool / f64 / ...`
- `E_PTR_ORDER_COMPARE_REQUIRES_UNSAFE` — checker-гейт на сравнение порядка
  указателей; сама операторная форма выведена (`E_POINTER_OP_USE_METHOD`),
  используйте знак `.dist()`
- `E_INVALID_POINTER_MODIFIER` — `*const T` и др.
- `E_POINTER_PREFIX_MODIFIER` — модификатор **перед** `*` (`mut * T` / `ro * T` /
  `uninit * T`); используйте постфиксный pointee `*mut T` / `*T` / `*uninit T`
  или биндинг `mut x *T` (План 138.5, расширяет `E_INVALID_POINTER_MODIFIER`)
- `E_REDUNDANT_POINTER_RO` — явно написан `*ro T`; голый `*T` уже readonly
  (дефолт pointee на уровне L3, D246 / План 147: `*T ≡ *ro T` универсально) —
  fix-it убирает `ro` (`*T`)
- `E_UNSAFE_TYPE_MODIFIER_RENAMED` — `unsafe` использован как **типовой**
  модификатор на не-`Func` payload (старое написание для data-uninit);
  переименован в `uninit` (§10a, План 174.5) — используйте `uninit T` /
  `*uninit T`. Только `*unsafe fn(...)` (композиция unsafe-**указателя на
  функцию**, D216 §10) сохраняет написание `unsafe` — это отдельное понятие,
  не «данные могут быть не инициализированы».
- `E_SAFE_RETIRED` — использован типовой модификатор `safe`; стоппер
  распространения `safe` выведен (останавливать префиксное распространение
  нечего) (План 138.5)
- `E_REALTIME_POINTER_OP` — операция с указателем в теле `#realtime fn`
- `E_PTR_NO_DISPLAY_USE_DEBUG_STR` — интерполяция `"${p}"`; хинт предлагает
  канонический `${p:?}` (План 91.14 D229) или легаси `p.to_debug_str()`

#### Ошибки V3-композиции модификаторов (D216 V3 амендмент, 2026-06-04)

- `E_MUTABILITY_CONFLICT_VALUE_TYPE` — в позиции типа `ro mut T` / `mut ro T`
  на **value-типе T** (примитивы / value-записи / именованные кортежи /
  анонимные кортежи / Unit). Биндинг-форма `ro x mut T` остаётся разрешённой
  (ортогональные биндинг-модификаторы). Spec §V3.1.
- `E_MODIFIER_ORDER` — модификатор безопасности (`uninit`) оборачивает
  модификатор мутабельности (`ro` / `mut`); требуется обратный порядок —
  **safety-inner / mutability-outer** (`ro uninit T` ✅ / `uninit ro T` ❌),
  согласовано с `external unsafe fn`. Применяется к value-T и к постфиксному
  содержимому **pointee** (`*mut uninit T` ✅ / `*uninit mut T` ❌ — pointee
  `*ro …` больше вообще не токен, см. `E_REDUNDANT_POINTER_RO`). Spec §V3.2
  (ПЕРЕВЁРНУТ в Плане 138.5).
- `E_REDUNDANT_TYPE_MODIFIER` — повторение модификатора одного класса.
  **Биндинг-уровень** (`ro x ro T`) и **постфиксная цепочка pointee**
  (`*mut mut T`) сохраняются; старые V3-случаи префиксных цепочек на уровне
  типа (`ro * ro T`, `unsafe * unsafe T`) — неактуальны: префикс перед `*` —
  уже `E_POINTER_PREFIX_MODIFIER` (План 138.5), а повторённый pointee `ro`
  теперь перехватывается раньше через `E_REDUNDANT_POINTER_RO` (ошибка уже
  на первом `*ro`, до повторения не доходит). Запасной выход `safe` выведен.
  Spec §V3.4.

> **Примечание:** стоппер распространения `safe` и форма `Unsafe(Pointer)`
> (`unsafe * T` = nullable-raw) ВЫВЕДЕНЫ (План 138.5). `safe` в позиции типа
> ⇒ `E_SAFE_RETIRED`; nullable-указатели используют `Option[*T]`.

### Предупреждения

- `W_OPTION_DOUBLE_NESTED` — fallback NPO для `Option[Option[*T]]`

## Сравнение с мейнстримом

| Язык | Typed ptr | Модель unsafe | Null safety | Доступ к pointee | Арифметика указателей |
|---|---|---|---|---|---|
| Rust | `*const T`/`*mut T`/`&T`/`&mut T` | `unsafe {}` + `unsafe fn` | `Option<&T>` + NPO | оператор `*p` | только unsafe |
| Zig | `*T`/`*const T`/`[*]T` | (intrinsics кастов) | `?*T` + NPO | `.*` постфикс + `.` | `+` для `[*]T` |
| C# | `T*` / `ref T` / `in T` / `out T` | модификатор `unsafe` | `T?` | стрелка `p->field` | только unsafe |
| Swift | `UnsafePointer<T>` / `UnsafeMutablePointer<T>` | префикс на основе типа | Optional + NPO | `.pointee` | только `.advanced(by:)` |
| D | `T*` / `ref T` / `scope T*` | `@safe`/`@trusted`/`@system` | `Nullable!T` | `p.field` авто | только `@system` |
| Go | `*T` (managed) / `unsafe.Pointer` | пакет `unsafe` | Nil в рантайме | `p.field` авто | только `unsafe.Pointer` |
| **Nova V1** (План 115) | только `ptr` | (нет) | `null ptr` | (нет) | запрещено |
| **Nova V2** (План 118) | **семейство `*T`** + `unsafe` | `unsafe { }` + `unsafe fn` (D2 амендмент) | `Option[*T]` + NPO | `p.field` один уровень + операторы | gated unsafe |
| **Nova FINAL** (Планы 138.5 + 174.5) | **постфиксный pointee** `*T` / `*mut T` / `*uninit T` / `*mut uninit T`; перенацеливаемость = биндинг (`ro`/`mut`) | (как V2) + правила композиции value-T (§V3.1-V3.2) | `Option[*T]` (только) + NPO | **методы** `.read()`/`.write()` + `p.field` один уровень | **методы** `.offset()`/`.dist()` |

## См. также

- [`docs/plans/118-typed-pointers-and-unsafe.md`](../plans/118-typed-pointers-and-unsafe.md) — дорожная карта ядра Плана 118
- [`docs/plans/118.1-ffi-intrinsics-and-cstring.md`](../plans/118.1-ffi-intrinsics-and-cstring.md) — суб-план 118.1 (FFI intrinsics)
- [`docs/plans/118.2-slice-fat-pointer-and-uninit.md`](../plans/118.2-slice-fat-pointer-and-uninit.md) — суб-план 118.2 (slice + uninit)
- [`docs/plans/118.3-pointer-concurrency-safety.md`](../plans/118.3-pointer-concurrency-safety.md) — суб-план 118.3 (concurrency)
- [`docs/guide/ffi-cookbook.md`](ffi-cookbook.md) — паттерны FFI с ptr + tuple FFI (План 115 V1)
- [D216 V1](../../spec/decisions/02-types.md#d216-typed-pointer-family--unsafe-model--null-safety-через-npo) — фундамент spec (семейство типизированных указателей + модель unsafe + NPO)
- [D216 FINAL pointer model (План 138.5)](../../spec/decisions/02-types.md#d216-typed-pointer-family--unsafe-model--null-safety-через-npo) — тип указателя = только постфиксный pointee-mut; перенацеливаемость = биндинг (D36); префиксные модификаторы ⇒ `E_POINTER_PREFIX_MODIFIER`; nullable = только `Option[*T]`; `safe` и `Unsafe(Pointer)` выведены
- [Амендмент D216 «всё через методы» (План 174.5)](../../spec/decisions/02-types.md#d216-typed-pointer-family--unsafe-model--null-safety-через-npo) — операторы указателей выведены в пользу intrinsic-методов; `E_POINTER_OP_USE_METHOD`
- [D216 V2 амендмент](../../spec/decisions/02-types.md#d216-v2-amend-2026-06-04--universal-right-binding-rule-для-type-level-modifiers--unsafe-t-first-class) — историческое правило right-binding (§V2.1, ОТОЗВАНО) + first-class value-обёртка `uninit T` (§V2.3, СОХРАНЕНО; переименована из `unsafe T` §10a, План 174.5) + пересчёт NPO (§V2.4)
- [D216 V3 амендмент](../../spec/decisions/02-types.md#d216-v3-amend-plan-1185-v3-2026-06-04--4-modifier-composition-rules) — правила композиции модификаторов value-T (V3.3/V3.4 заменены Планом 138.5):
  - §V3.1 — запрет смежности `ro+mut` с учётом storage-class (`E_MUTABILITY_CONFLICT_VALUE_TYPE`) — СОХРАНЕНО
  - §V3.2 — порядок модификаторов safety-inner / mutability-outer (`ro uninit T`; `E_MODIFIER_ORDER`) — ПЕРЕВЁРНУТ, СОХРАНЁН
  - §V3.3 — right-binding распространение — ЗАМЕНЁН (префиксного распространения нет)
  - §V3.4 — стоппер `safe` — ВЫВЕДЕН; `E_REDUNDANT_TYPE_MODIFIER` сохраняется на уровне биндинга/постфиксного pointee
- [D216 §10a rename](../../spec/decisions/02-types.md#d216-typed-pointer-family--unsafe-model--null-safety-через-npo) — переименование type-модификатора `unsafe` → `uninit` (План 174.5, 2026-07-11): `*unsafe T` → `*uninit T`, голая value-обёртка `unsafe T` → `uninit T`; блок `unsafe { }`, ключевое слово `unsafe fn` и композиция указателя-на-функцию `*unsafe fn(...)` сохраняют написание `unsafe` (другое понятие)
- [D2 амендмент](../../spec/decisions/04-effects.md#d2) — восстановление ключевого слова unsafe (сахар обработчика эффекта)
- [D214 амендмент](../../spec/decisions/02-types.md#d214-ptr-opaque-pointer-type--tuple-ffi-returns--opaque-handle-pattern) — переопределение `ptr`
- [D32 амендмент](../../spec/decisions/02-types.md#d32-семантика-передачи-параметров) — `&value` — не borrow из Rust
- [`examples/typed_pointers/`](../../examples/typed_pointers/) — минимальные рабочие примеры
