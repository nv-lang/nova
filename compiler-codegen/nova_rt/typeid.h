#ifndef NOVA_RT_TYPEID_H
#define NOVA_RT_TYPEID_H

/*
 * Plan 61 Ф.1: TypeId runtime infrastructure.
 *
 * Каждый Nova-тип (user-defined sum, record, primitive) получает unique
 * compile-time константу NovaTypeId. Используется:
 *
 *   1. Plan 61 Ф.2 — erased Fail[any] path: throw упаковывает
 *      `(void* err, NovaTypeId tid)`. Handler arm для `Fail` (без [E])
 *      получает `e: any` + runtime tag.
 *
 *   2. Nova-side `any.is[T]() -> bool` / `any.as[T]() -> Option[T]`
 *      (D54 anonymous-protocol `any`) — runtime сравнение tag'а.
 *
 *   3. Diagnostic / debug: `nova_typeid_to_name(tid)` для panic messages,
 *      bug reports, gdb pretty-print.
 *
 * Allocation strategy:
 *   - NOVA_TID_NONE = 0 — sentinel, "not a real type" (используется
 *     в default-init vtables).
 *   - Per-type IDs >= 1 эмитятся compile-time как `#define NOVA_TID_<mangled> N`
 *     в auto-gen'd секции preamble (emit_c.rs:emit_typeid_defines).
 *   - Monotonic counter (1, 2, 3, ...) — порядок не стабилен между
 *     compile-сессиями, но это OK: TID используется только within
 *     compilation unit (single C source per Nova module/program).
 *
 * Primitive типы (nova_int, nova_str, nova_bool, и т.д.) получают
 * reserved IDs 1..16 для cheap pattern-match в any.is_int() / etc.
 */

#include <stddef.h>
#include <stdint.h>

typedef uint32_t NovaTypeId;

#define NOVA_TID_NONE         ((NovaTypeId)0)

/* Reserved primitive IDs (1..16). Plan 61: эти константы стабильны
 * между compile-сессиями и используются в hard-coded runtime helpers
 * (nova_any_is_*, nova_print_any). User-defined types получают IDs >=17. */
#define NOVA_TID_nova_int     ((NovaTypeId)1)
#define NOVA_TID_nova_str     ((NovaTypeId)2)
#define NOVA_TID_nova_bool    ((NovaTypeId)3)
#define NOVA_TID_nova_f64     ((NovaTypeId)4)
#define NOVA_TID_nova_f32     ((NovaTypeId)5)
#define NOVA_TID_nova_byte    ((NovaTypeId)6)
#define NOVA_TID_nova_unit    ((NovaTypeId)7)
/* 8..16 reserved для future primitives. User IDs start at 17. */
#define NOVA_TID_USER_BASE    ((NovaTypeId)17)

/* Auto-gen'd defines будут splice'нуты ниже codegen'ом в emit_preamble
 * (после emit_c.rs::emit_typeid_defines). Здесь — runtime helpers. */

/* Сравнение типов. Тривиально inline. */
static inline int nova_typeid_eq(NovaTypeId a, NovaTypeId b) {
    return a == b;
}

/* Diagnostic name lookup. Implementation — в auto-gen'd C-file
 * (compiler-codegen генерирует switch-case на основе всех registered
 * types). Здесь — forward decl. Если codegen не emit'ит implementation
 * (например, в minimal test), linker найдёт weak fallback в typeid.c. */
const char* nova_typeid_to_name(NovaTypeId tid);

/* Plan 61 Ф.2 (forward): `any.is[T]()` / `any.as[T]()` builtin support.
 * Используется codegen'ом для `e is ParseError` runtime check внутри
 * `with Fail = |e: any| ...` handler-arm. tid — actual type tag из
 * boxed any-value; expected — compile-time константа NOVA_TID_<T>. */
static inline int nova_any_is_typeid(NovaTypeId actual, NovaTypeId expected) {
    return actual == expected;
}

/* ─────────────────────────────────────────────────────────────────────────
 * Plan 174.3 (D53/D54 v1): `any` top-type — type-erased boxed value.
 *
 * Представление `any` в C = `void*`, указывающий на heap-boxed `NovaAny`
 * (fat-pointer, но boxed чтобы `any`-ABI оставался void* — нулевой blast-radius
 * на существующий erased-void*-код: print/println, R::Any generic-параметр).
 * `data` — отдельная GC-allocation с payload; и box, и payload сканируются
 * консервативным GC (Boehm). `info` — per-type статический `NovaTypeInfo`
 * (сидит на type_id-реестре Plan 61 — НЕ хардкод per-тип).
 *
 * Операции (D54 v1):
 *   upcast T→any : nova_any_box(&NOVA_TYPEINFO_<T>, &value, sizeof(T))
 *   x is T       : nova_any_is(x, NOVA_TID_<T>)
 *   x.try_as[T]()/narrowing: *(T*)nova_any_data(x)  (после успешного is)
 * ───────────────────────────────────────────────────────────────────────── */
typedef struct NovaTypeInfo {
    NovaTypeId  type_id;   /* реестр Plan 61 (NOVA_TID_<T>) */
    const char* name;      /* human-readable — Display / diagnostics */
} NovaTypeInfo;

typedef struct NovaAny {
    const NovaTypeInfo* info;   /* per-type static (type_id + name) */
    void*               data;   /* heap-boxed payload (GC-scanned) */
} NovaAny;

/* upcast: box a value of arbitrary type into `any`. `payload` — адрес значения
 * (compound-literal или temp), `sz` = sizeof(тип). Копирует payload в свежую
 * GC-allocation, чтобы box владел собственной копией (не dangling на stack). */
static inline void* nova_any_box(const NovaTypeInfo* info, const void* payload, size_t sz) {
    NovaAny* a = (NovaAny*)nova_alloc(sizeof(NovaAny));
    void* d = nova_alloc(sz ? sz : 1);
    if (sz) memcpy(d, payload, sz);
    a->info = info;
    a->data = d;
    return (void*)a;
}

/* `x is T` — runtime type_id-сравнение. NULL-safe. */
static inline int nova_any_is(const void* a, NovaTypeId expected) {
    return a != NULL
        && ((const NovaAny*)a)->info != NULL
        && ((const NovaAny*)a)->info->type_id == expected;
}

/* Указатель на boxed payload (для downcast `*(T*)nova_any_data(x)`). */
static inline void* nova_any_data(const void* a) {
    return ((const NovaAny*)a)->data;
}

/* type_id боксированного значения (NOVA_TID_NONE если NULL). */
static inline NovaTypeId nova_any_tid(const void* a) {
    return (a != NULL && ((const NovaAny*)a)->info != NULL)
        ? ((const NovaAny*)a)->info->type_id : NOVA_TID_NONE;
}

/* Имя динамического типа (для Display / диагностик). */
static inline const char* nova_any_name(const void* a) {
    return (a != NULL && ((const NovaAny*)a)->info != NULL)
        ? ((const NovaAny*)a)->info->name : "<null>";
}

/* ─────────────────────────────────────────────────────────────────────────
 * Plan 173 Ф.4 (#5, D188/D190): box an ALREADY-heap-boxed typed payload into
 * `any` when only its RUNTIME `NovaTypeId` is known (not a compile-time static
 * `NovaTypeInfo`). Used by `assign_scope_outcome_from_frame` to lift a
 * `NovaFailFrame`'s `error_user_payload` (a typed-throw payload, heap-boxed at
 * the throw site — survives the unwind) + `error_user_type_id` into the
 * `ScopeOutcome.Failure(any)` variant, so `@cleanup`/`defer(o)` bodies can do
 * `if err is T` narrowing on the original thrown value.
 *
 * `payload` is a typed-throw payload — the `Nova_T*` pointer a `throw MyErr{..}`
 * heap-allocates at the throw site (user error types are records → pointer
 * representation). The `any` ABI for such a pointer type keeps `data` pointing
 * at a slot that HOLDS the `Nova_T*` (narrowing lowers to `*(Nova_T**)data`), so
 * we box `payload` one indirection deep — mirroring `nova_any_box(&info,&ptr,
 * sizeof(ptr))`, but with a `NovaTypeInfo` synthesised from the runtime tid
 * (name via the codegen `nova_typeid_to_name` switch; `nova_any_is`/`_data`/
 * `_name` read only `type_id`/`data`/`name`, so a heap info == a static one). */
static inline void* nova_any_from_boxed(void* payload, NovaTypeId tid) {
    NovaAny* a = (NovaAny*)nova_alloc(sizeof(NovaAny));
    NovaTypeInfo* ti = (NovaTypeInfo*)nova_alloc(sizeof(NovaTypeInfo));
    ti->type_id = tid;
    ti->name = nova_typeid_to_name(tid);
    a->info = ti;
    /* Plan 173 хвост (D414 §1, вскрыто scope-агрегацией, 2026-07-13):
     * ABI-класс определяет форму `data`. Narrowing/`try_as[T]` читает
     * `*(<value-repr>*)data`, т.е. `data` обязан указывать НА value-repr:
     *   - value-ABI примитивы (tid 1..7: int/str/bool/f64/f32/byte/unit) —
     *     `payload` (heap-box throw-сайта) УЖЕ указывает на значение →
     *     data = payload напрямую; прежняя слот-обёртка давала лишнюю
     *     индирекцию (`try_as[int]` возвращал адрес бокса как int);
     *   - pointer-repr типы (user-записи ≥ NOVA_TID_USER_BASE — универсум
     *     typed-errors, Ф.4) — value-repr САМ указатель → нужен слот,
     *     держащий его (`*(Nova_T**)data`), как nova_any_box(&ptr,…).
     *     (Известный pre-existing остаток: user VALUE-типы ≥ USER_BASE
     *      здесь неотличимы от записей — как и было, предполагаем
     *      pointer-repr; см. Ф.4 #5 «box-repr предполагает pointer».) */
    if (tid >= (NovaTypeId)1 && tid < NOVA_TID_USER_BASE) {
        a->data = payload;
    } else {
        void** slot = (void**)nova_alloc(sizeof(void*));
        *slot = payload;
        a->data = (void*)slot;
    }
    return (void*)a;
}

#endif /* NOVA_RT_TYPEID_H */
