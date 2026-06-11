<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 145: MSVC codegen portability — bounds-check stmt-expr → portable

> **Создан:** 2026-06-11 (обнаружено при попытке снять MSVC-baseline в Plan 83-study-go-c-mn).
> **Статус:** 📋 PROPOSED — отдельная задача (не блокирует Plan 83-go-cmn, который валидируется на clang).
> **Приоритет:** P1 — MSVC сломан широко (регрессия после Plan 82); MSVC — primary платформа.
> **Оценка:** ~1-2 dev-day (codegen-рефактор core-паттерна + полная регрессия на ДВУХ toolchain).
> **Маркер:** `[M-msvc-bounds-check-stmt-expr]`.

---

## 1. Симптом

`nova test --toolchain msvc` падает `CC-FAIL` на **большинстве** тестов с индексацией
массива: `cl.exe ... error C2059: синтаксическая ошибка: {`.

История: Plan 82 давал MSVC **1049/16**. Сейчас MSVC сломан широко → **регрессия после
Plan 82** (bounds-check codegen добавлен позже — Plan 90 / 131 / 138).

## 2. Root cause

Codegen эмитит **GNU statement-expression** + `__typeof__` для bounds-checked индексации
(`emit_c.rs` ~9700, ~9720, ~15783, ~18571):

```c
(*({ __typeof__(arr) _a = (arr); nova_int _i = (i);
     if (__builtin_expect(_i < 0 || _i >= _a->len, 0)) nv_panic_index_oob(_i, _a->len);
     &_a->data[_i]; }))
```

- `({ ... })` (statement-expression) — расширение GCC/Clang. **cl.exe НЕ поддерживает** → C2059.
- `__typeof__` — тоже GNU (нужен для temp `_a`, чтобы не вычислять `arr` дважды).
- `__builtin_expect` — уже шиммится в `nova_msvc_compat.h` (не проблема).

clang это ест → clang-suite зелёный. cl.exe — нет.

## 3. Почему stmt-expr вообще использован

Он возвращает **lvalue** (`&_a->data[_i]` → `*`) — нужно для `a[i] = v` (write). И
работает в **любом** expression-контексте (индексация бывает глубоко внутри выражения,
куда нельзя вставить preceding-statement). Temp `_a` через `__typeof__` избегает
двойного вычисления `arr` (side-effects).

## 4. Варианты фикса (выбрать в design-фазе)

**Вариант A — per-element-type inline helper (предпочтительно).** Codegen знает тип
элемента → эмитить `nova_idx_<T>(arr, i)` возвращающий `T*`:
```c
static inline T* nova_idx_<T>(NovaArr_<T>* a, nova_int i) {
    if (i < 0 || i >= a->len) nv_panic_index_oob(i, a->len);
    return &a->data[i];
}
// сайт: (*nova_idx_<T>((arr), (i)))   // lvalue сохранён, arr вычислен один раз, portable C
```
Плюс: portable C89/C11, lvalue-семантика цела, без GNU. Минус: генерить helper на тип массива
(уже есть инфра для per-type кода).

**Вариант B — comma-operator (без temp, если arr side-effect-free).** Не годится в общем
случае (двойное вычисление arr).

**Вариант C — hoist проверки в отдельный statement.** Не годится: индексация бывает в
expression-позиции без statement-контекста.

→ **Вариант A** — единственный, что сохраняет все три инварианта (lvalue, single-eval,
portable).

## 5. Acceptance

- `nova build`/`nova test` на **обоих** toolchain (clang + MSVC) — 0 net new FAIL vs clang baseline.
- **MSVC nova test возвращается к ~Plan-82-уровню** (≥1049 PASS на полном прогоне) — C2059 ушёл.
- clang НЕ регрессирует (тот же output-паттерн lvalue/single-eval — проверить indexing-fixtures).
- Все 4 stmt-expr сайта в `emit_c.rs` мигрированы; нет `({`/`__typeof__` в generated C.
- Позитивные/негативные fixtures: read-index, write-index `a[i]=v`, nested-index `a[i][j]`,
  index с side-effecting arr-expr (single-eval проверка), OOB-panic (negative).

## 6. Файлы
- `compiler-codegen/src/codegen/emit_c.rs` (~9700/9720/15783/18571 — index emission + helper synth)
- `compiler-codegen/nova_rt/array.h` (helper, если общий) / per-type helper synth
- fixtures `nova_tests/plan145/`

## 7. Связь
- Обнаружено в [Plan 83-study-go-c-mn](83-study-go-c-mn.md) §9.2 (MSVC baseline blocked).
- Plan 83-go-cmn валидируется на **clang** пока этот план не закрыт.
- Регрессия после [Plan 82](82-windows-fiber-arena.md) (MSVC был 1049/16).
