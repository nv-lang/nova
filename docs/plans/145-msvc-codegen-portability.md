<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 145: MSVC codegen portability — bounds-check stmt-expr → portable

> **Создан:** 2026-06-11 (обнаружено при попытке снять MSVC-baseline в Plan 83-study-go-c-mn).
> **Статус:** 🟢 CODEGEN PORTABILITY ЗАКРЫТА + MSVC ВОССТАНОВЛЕН (majority) — 2026-06-14,
> ветка `plan-145` (НЕ merged). Подробности — §8–§11. Остаток (узкий struct-elem-write +
> 3 редких stmt-expr) → followup `[M-145-msvc-remaining-stmt-expr]` / Plan 145.1.
> **Приоритет:** P1 — MSVC сломан широко (регрессия после Plan 82); MSVC — primary платформа.
> **Оценка:** ~1-2 dev-day (codegen-рефактор core-паттерна + полная регрессия на ДВУХ toolchain).
> **Маркер:** `[M-msvc-bounds-check-stmt-expr]` ✅ ЗАКРЫТ (Ф.1); новый `[M-145-msvc-remaining-stmt-expr]`.

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

---

## 8. План реализации (execution) — ФАКТИЧЕСКИ ВЫПОЛНЕНО

> **Где:** worktree `d:/Sources/nv-lang/nova-p145`, ветка `plan-145` (от чистого `main` @ c0f269dd).
> **Подход (РЕВИЗИЯ vs §4):** Вариант A в плане предполагал per-element-type синтез
> `nova_idx_<T>`. При реализации найден **лучший** дизайн: layout NovaArray и Vec **идентичен**
> (`{ptr data; i64 len; i64 cap}`, data первым), поэтому достаточно **ОДНОГО generic
> рантайм-хелпера** через `void*` (тип-лаундеринг снимает strict-aliasing) + общий
> `NovaArrHdr`-каст. Сайт даёт элемент-тип через `(ELEM*)`-каст + `sizeof(ELEM)`. Это
> сохраняет все 3 инварианта (lvalue/single-eval/portable) БЕЗ per-type synth-инфры.

> **РАСШИРЕНИЕ SCOPE (открыто при MSVC-валидации):** план называл одну причину
> (indexing stmt-expr), но MSVC был сломан **четырьмя** независимыми проблемами каскадом
> (C2059 → C2143 → LNK2019 → C2440). Закрыты первые три; четвёртая (узкая) → followup.

**Фазы:**

- **Ф.1 — codegen stmt-expr → portable хелперы (array.h).** Мигрированы ВСЕ index-сайты:
  `emit_bchk_array_access` (1D, +параметр storage_elem_ty), `emit_bchk_double_array_access`
  (2D), Vec inline `@index` путь (chk/nochk по элизии Plan 140.2), 5 call-sites. Хелперы:
  `nova_idx_chk`/`nova_idx_nochk`. + Vec-slice (`nova_vec_slice_chk/nochk`), str-slice
  (`nova_str_slice_chk/nochk` + `*_to_end_*` для open-ended single-eval, с UTF-8 boundary
  guard), heap-box Err-payload (`nova_box_value`, memcpy), int→f64 bitcast (`nova_bits_i2f`).
  В generated C на этих путях НЕТ `({`/`__typeof__`. Коммиты `1407344c`, `d54d36bc`.
- **Ф.2 — рантайм MSVC-разблокировки (nova_msvc_compat.h, force-included /FI).**
  (a) шим C11 `_Static_assert` → negative-array-size трюк (разблокировал fiber_arena.h /
  Plan 149, C2143); (b) дошимлен полный набор `__atomic_*` (fetch_and/or/xor/nand,
  add_fetch/sub_fetch, fetch_max, non-_n load/store), которых не было в Plan-82 compat-слое
  (LNK2019 в net.obj/eventloop.obj ломал ЛЮБОЙ MSVC-тест). Коммиты `ece06025`, `89c2c051`.
- **Ф.3 — Fixtures.** `nova_tests/plan145/` (6): t1 read (Vec literal/from/str), t2 write
  `v[i]=val`, t3 nested grid + lvalue-receiver `grid[i].push`, t4 `as_bytes()[i]`
  (NovaArray-путь), neg_vec_index_oob, neg_bytes_index_oob (OOB-panic).
- **Ф.4 — clang-регрессия.** 22 затронутые директории — **0 net-new FAIL** (даже +2 fixed:
  plan138_2 t2_vec_mut_index, t7_vec_as_ptr — lvalue write/as_ptr теперь корректен). Метод
  baseline: ПРЕД-фикс бинарь main-репо (set-diff падений, НЕ git stash).
- **Ф.5 — MSVC-валидация.** `--toolchain msvc`: компиляция codegen-кода проходит (C2059/C2143
  устранены), линковка проходит (LNK2019 устранён). Зелёные на MSVC: plan90 9/0, plan90_1 21/0,
  plan96 23/0, plan131 28/0, plan152_1 6/0, plan152_2 3/0, generics 5/0, basics 8/0. Остаток —
  см. §11.
- **Ф.6 — Спека/доки/логи + коммиты.** D-блок про portable index/slice/box codegen + запрет
  GNU stmt-expr; backlog (`[M-msvc-bounds-check-stmt-expr]` закрыт, новый followup открыт);
  project-creation.txt / simplifications.md / nova-private discussion-log.

## 9. Критерии приёмки

- **AC1 (ОБЯЗАТЕЛЬНЫЙ):** реализовано production-grade, без упрощений/заглушек/TODO в
  доставленном scope. ✅ (отложенный struct-elem-write — явный followup, НЕ заглушка).
- **AC2:** index/slice/box/bitcast stmt-expr мигрированы; в generated C НЕТ `({`/`__typeof__`
  на этих путях (проверено инспекцией `.c`: `__typeof__`=0, stmt-expr=0). ✅
- **AC3:** lvalue-семантика цела (write/nested/address-of/method-receiver); подтверждено +2
  починенными lvalue-тестами на clang и MSVC-прогоном write/nested. ✅
- **AC4:** single-eval — arr/idx вычисляются ровно раз (аргументы fn); проверено C-инспекцией;
  open-ended str-slice — `*_to_end_*` сохраняет single-eval. ✅
- **AC5:** clang — 0 net-new FAIL vs pre-fix baseline (22 директории). ✅ (+2 fixed)
- **AC6:** MSVC — C2059/C2143/LNK2019 устранены; index/slice/string/Vec/basics PASS на MSVC.
  🟢 ЧАСТИЧНО: подавляющее большинство зелёное; остаток — узкий struct-elem-write (§11).
- **AC7:** pos+neg фикстуры plan145 проходят через РЕЛИЗНЫЕ nova-codegen (clang 6/6; MSVC 5/6,
  1 = struct-elem-write followup). ✅
- **AC8:** spec/доки/логи обновлены; `[M-msvc-bounds-check-stmt-expr]` закрыт; followup открыт. ✅

## 10. Статус

🟢 **CODEGEN PORTABILITY ЗАКРЫТА + MSVC ВОССТАНОВЛЕН (majority)** — 2026-06-14, ветка
`plan-145` (worktree `nova-p145`, НЕ merged). 6 коммитов: `1407344c` (index), `ece06025`
(_Static_assert), `d54d36bc` (slice/box/bitcast), `89c2c051` (atomics), + docs/logs.

Домен плана («MSVC **codegen** portability») достигнут: генерируемый C компилируется и
линкуется под cl.exe; clang 0 net-new (+2 fixed). MSVC из «сломан широко» → зелёный на
plan90/90_1/96/131/152_1/152_2/generics/basics. Полная цель §5 (≥1049) достижима после
закрытия §11.

## 11. Остаток (followup `[M-145-msvc-remaining-stmt-expr]`, → Plan 145.1)

Узкие codegen-конструкции, всё ещё несовместимые с cl.exe (раскрылись после устранения
основных блокеров; в исходном scope §4 не значились):

1. **struct-element write по индексу — C2440** (`vec_of_struct[i] = val`, напр. `Vec[str]`):
   cl.exe отвергает присваивание struct-значения через `*(struct*)void_ptr`-lvalue
   (init из того же lvalue — ОК; Vec[int]-write — ОК; только struct-элементы). 3 теста
   (plan145 t2, plan138 t_vec_write_index, plan138_1). Нужен memcpy-set helper для
   write-index struct-элементов (+ addressability RHS / const-literal аспект).
2. **heap-box value-rvalue** (`throw <non-ptr value>`): stmt-expr оставлен (src — rvalue,
   не адресуем; нужен compound-literal или per-type helper).
3. **Option-get repack composite** (`composite_arr.get(i)`): stmt-expr (нужен temp +
   generated NovaOpt-тип, недоступный в array.h).
4. **record-invariant wrap** (типы с `invariant`): stmt-expr (редкий путь).
