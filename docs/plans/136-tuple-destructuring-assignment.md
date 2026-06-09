<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 136 — Tuple destructuring assignment

> **Создан:** 2026-06-09.  **Статус:** 📋 PLANNED.
> **Эстимат:** ~1 dev-day.  **Model:** Sonnet 4.6.
> **Зависит от:** Plan 59 ✅ (tuple mono), Plan 120 ✅ (named tuples).

---

## Что

Разрешить tuple-паттерн в левой части присваивания:

```nova
// swap — одна строка вместо трёх
(a, b) = (b, a)

// rotate
(a, b, c) = (b, c, a)

// независимые rhs
(x, y) = (f(), g())

// вложенные lvalue (поля, индексы)
(@data[lo], @data[hi]) = (@data[hi], @data[lo])
```

Сейчас `(a, b) = (b, a)` → parse error: левая сторона присваивания
должна быть единственным lvalue-expression.

---

## Почему

Встречается в языках: Python, Go, Lua, Swift, Kotlin, JavaScript.  
В Rust — нет (только `std::mem::swap`).

Ложится на парадигму Nova органично: tuple-literal уже есть,
destructuring в `let` уже есть (Plan 53). Это расширение того же
механизма на `=`-присваивание.

Мотивирующий пример — `Vec[T].reverse()` в stdlib:

```nova
// до
ro tmp = @data[lo]
@data[lo] = @data[hi]
@data[hi] = tmp

// после
(@data[lo], @data[hi]) = (@data[hi], @data[lo])
```

---

## Семантика

### Вычисление в два этапа

Правая сторона вычисляется **полностью** (все элементы) до того,
как происходит любое присваивание слева. Это стандартная семантика
(Python, Go).

```
(lhs_0, lhs_1, ..., lhs_N) = (rhs_0, rhs_1, ..., rhs_N)
```

1. Вычислить все `rhs_i` → временные `t_i`.
2. Присвоить `lhs_i = t_i` в порядке слева направо.

### Допустимые lhs-элементы

Каждый `lhs_i` должен быть **mutable lvalue**:
- локальная `mut`-переменная: `a`
- `mut`-поле через цепочку: `@field`, `obj.field`
- `mut`-индексный доступ: `arr[i]`, `@data[i]`
- вложенная комбинация: `@data[lo]`, `v.items[j]`

`ro`-binding или immutable field → `E_TUPLE_ASSIGN_LHS_NOT_MUT`.

### Ограничения V1

- Глубина вложенности tuple слева: 1 уровень (не `((a,b),c) = ...`).
- Количество элементов: оба кортежа должны совпадать → `E_TUPLE_ASSIGN_ARITY_MISMATCH`.
- `consume`-типы в tuple-assign: запрещены (нет consume-destructuring
  assignment) → `E_TUPLE_ASSIGN_CONSUME_TYPE`.

---

## Codegen: cycle-decomposition

Наивный подход — N временных переменных — корректен, но избыточен.
Оптимальный: **разложение на циклы перестановки**.

### Определения

Пусть lhs = `[L_0, ..., L_{n-1}]`, rhs = `[R_0, ..., R_{n-1}]`.

**Зависимость**: `R_i` зависит от `L_j` если `L_j` встречается как
свободный lvalue в выражении `R_i` (т.е. чтение `L_j` входит в
вычисление `R_i`). Обозначим: `dep(i) = j` если `R_i` читает `L_j`
и эта связь формирует перестановочный цикл.

### Случай: pure permutation (все rhs — ровно один lhs)

`(a, b, c) = (b, c, a)` — каждый `R_i` — это ровно один `L_j`,
и `{j}` — перестановка `{0..n-1}`. Стандартный алгоритм:

```
1. Построить граф перестановки: perm[i] = j означает
   "на позицию i нужно значение из позиции j".
2. Разложить на независимые циклы.
3. Для каждого цикла (c_0 → c_1 → ... → c_k → c_0):
   tmp = L[c_0]
   L[c_0] = L[c_1]
   L[c_1] = L[c_2]
   ...
   L[c_{k-1}] = L[c_k]
   L[c_k] = tmp
   → ровно 1 tmp на цикл, k+2 присваивания.
```

**Примеры**:

```nova
// swap: (a, b) = (b, a)
// цикл: [0 → 1 → 0]
tmp = a; a = b; b = tmp          // 1 tmp, 3 операции

// rotate-3: (a, b, c) = (b, c, a)
// цикл: [0 → 1 → 2 → 0]
tmp = a; a = b; b = c; c = tmp   // 1 tmp, 4 операции

// double-swap: (a, b, c, d) = (b, a, d, c)
// цикл 1: [0 → 1 → 0], цикл 2: [2 → 3 → 2]
tmp = a; a = b; b = tmp
tmp = c; c = d; d = tmp          // 2 tmp, 6 операций

// identity: (a, b) = (a, b)
// каждый элемент — фиксированная точка → 0 tmp, 0 операций
```

### Случай: mixed rhs (некоторые rhs — не lhs)

`(a, b) = (x, y)` где `x, y` не являются элементами `{a, b}`.
Нет циклов — все `R_i` независимы → 0 tmp, прямые присваивания:

```c
a = x;
b = y;
```

### Случай: частичная зависимость

`(a, b) = (b, 1)` — `R_0 = b` читает `L_1`, `R_1 = 1` независим.

Алгоритм:
1. Вычислить множество зависимых `R_i` (читают хотя бы один lhs).
2. Для них выделить tmp:
   ```c
   tmp_0 = b;   // R_0 зависит от L_1
   a = tmp_0;
   b = 1;
   ```

### Общий алгоритм (V1 консервативный)

Для простоты реализации V1 можно использовать **консервативный** подход:
выделить tmp для каждого `R_i`, который содержит хотя бы одно lvalue
из множества `{L_0, ..., L_{n-1}}` (статический анализ свободных
переменных). Cycle-decomposition — V2 оптимизация.

```
for each i:
    if free_lvalues(R_i) ∩ {L_0..L_{n-1}} ≠ ∅:
        t_i = R_i          // нужна tmp
    else:
        t_i = R_i_inline   // можно inline (отложить до assign-фазы)
for each i:
    L_i = t_i / R_i_inline
```

C-codegen (statement-expression):
```c
// (a, b) = (b, a)  →  conservative: 2 tmp
{
    nova_int _t0 = b;
    nova_int _t1 = a;
    a = _t0;
    b = _t1;
}

// (a, b) = (b, a)  →  cycle-decomposition: 1 tmp
{
    nova_int _t = a;
    a = b;
    b = _t;
}
```

**V1 реализует консервативный подход. V2 (followup) — cycle-decomposition.**

---

## Фазы

### Ф.0 — Spec (D-block) (~30 min)

Добавить новый D-block в `spec/decisions/03-syntax.md`:

```
**D236 — Tuple destructuring assignment**

(lhs_0, ..., lhs_N) = (rhs_0, ..., rhs_N)

Семантика: rhs вычисляется полностью до любого присваивания.
lhs-элементы: mutable lvalues (bindings, fields, indices).
Codegen: conservative tmp-per-dependent-rhs (V1);
         cycle-decomposition (V2, [M-136-cycle-decomp]).
Ограничения V1: no nested tuple lhs; no consume types.
```

Примеры в spec.

**Commit:** `spec(D236): tuple destructuring assignment`

### Ф.1 — Parser (~1h)

**Файл:** `compiler-codegen/src/parser/mod.rs`

`parse_assign_stmt` / `parse_stmt`:

Сейчас парсер видит `(a, b)` в lhs как tuple-expression и выдаёт
ошибку «not an lvalue». Нужно:

1. В `parse_stmt`, когда текущий токен `(`, попробовать распарсить
   как tuple-lhs:
   ```rust
   // Lookahead: ( ident_or_lvalue , ... ) =
   if self.peek() == LParen && self.is_tuple_assign_lhs() {
       return self.parse_tuple_assign_stmt();
   }
   ```

2. `parse_tuple_assign_stmt`:
   - Парсит `(lhs_0, ..., lhs_N)` — список lvalue-expressions.
   - Ожидает `=`.
   - Парсит `(rhs_0, ..., rhs_N)` — tuple-literal (уже поддерживается).
   - Возвращает `Stmt::TupleAssign { lhs: Vec<Expr>, rhs: Vec<Expr>, span }`.

3. Lookahead `is_tuple_assign_lhs()`:
   - Не потребляет токены (peek-only).
   - Проверяет: `(` → series of `ident|member|index` separated by `,`
     → `)` → `=`.
   - Используется чтобы не конфликтовать с tuple-expression в
     statement-position (которое уже обрабатывается).

**AST:** добавить `Stmt::TupleAssign { lhs: Vec<Expr>, rhs: Vec<Expr>, span: Span }`.

**Commit:** `feat(plan136 Ф.1): parser — tuple destructuring assignment`

### Ф.2 — Type-checker (~1h)

**Файл:** `compiler-codegen/src/types/mod.rs`

В `check_stmt` / `walk_stmt` обработать `Stmt::TupleAssign`:

1. **Arity check**: `lhs.len() == rhs.len()` → иначе `E_TUPLE_ASSIGN_ARITY_MISMATCH`.

2. **lhs mutability**: для каждого `lhs_i` через `lvalue_root_ident(lhs_i)`
   (уже есть из Plan 128.2) проверить что binding мутабелен →
   иначе `E_TUPLE_ASSIGN_LHS_NOT_MUT`.

3. **consume-type ban**: для каждого `lhs_i` проверить что тип не consume →
   иначе `E_TUPLE_ASSIGN_CONSUME_TYPE`.

4. **Recurse** в каждый `lhs_i` и `rhs_i`.

**Новые error codes:**
- `E_TUPLE_ASSIGN_ARITY_MISMATCH` — количество lhs ≠ rhs.
- `E_TUPLE_ASSIGN_LHS_NOT_MUT` — элемент lhs не mutable lvalue.
- `E_TUPLE_ASSIGN_CONSUME_TYPE` — consume-type в tuple-assign (V1 ban).

**Commit:** `feat(plan136 Ф.2): type-checker — tuple assign validation`

### Ф.3 — Codegen (~2h)

**Файл:** `compiler-codegen/src/codegen/emit_c.rs`

В `emit_stmt` добавить arm `Stmt::TupleAssign { lhs, rhs }`:

```rust
Stmt::TupleAssign { lhs, rhs, .. } => {
    // Шаг 1: определить зависимые rhs-i
    // (читают хотя бы один lvalue из lhs-множества).
    let lhs_names: HashSet<String> = lhs.iter()
        .flat_map(|e| lvalue_ident_set(e))
        .collect();
    let deps: Vec<bool> = rhs.iter()
        .map(|r| expr_reads_any(r, &lhs_names))
        .collect();

    // Шаг 2: вычислить зависимые rhs → tmp-переменные.
    let mut tmps: Vec<Option<String>> = Vec::with_capacity(rhs.len());
    for (i, r) in rhs.iter().enumerate() {
        if deps[i] {
            let tmp = self.fresh_tmp_var();
            let val = self.emit_expr(r)?;
            let ty  = self.infer_expr_c_type(r);
            self.emit_line(format!("{ty} {tmp} = {val};"));
            tmps.push(Some(tmp));
        } else {
            tmps.push(None);
        }
    }

    // Шаг 3: присвоить lhs.
    for (i, l) in lhs.iter().enumerate() {
        let val = match &tmps[i] {
            Some(t) => t.clone(),
            None    => self.emit_expr(&rhs[i])?,
        };
        let target = self.emit_lvalue(l)?;
        self.emit_line(format!("{target} = {val};"));
    }
    Ok(())
}
```

Вспомогательные функции:

- `lvalue_ident_set(e: &Expr) -> Vec<String>` — собирает все lvalue-имена
  из lhs-expression (ident + поля + индексы корня).
- `expr_reads_any(e: &Expr, names: &HashSet<String>) -> bool` — проверяет
  содержит ли выражение чтение хотя бы одного из names.
- `emit_lvalue(e: &Expr) -> Result<String, _>` — эмитирует lvalue-expression
  как C lvalue (уже частично есть через `emit_assign_target`).

**Commit:** `feat(plan136 Ф.3): codegen — tuple assign with conservative tmp`

### Ф.4 — Fixtures (~1h)

**`nova_tests/plan136/`**:

Позитивные:
- `t1_swap_basic.nv` — `(a, b) = (b, a)`, проверить результат
- `t2_rotate3.nv` — `(a, b, c) = (b, c, a)`
- `t3_independent_rhs.nv` — `(x, y) = (1, 2)` (нет зависимостей)
- `t4_partial_dep.nv` — `(a, b) = (b, 1)` (частичная зависимость)
- `t5_field_swap.nv` — `(@x, @y) = (@y, @x)` для полей
- `t6_index_swap.nv` — `(arr[0], arr[1]) = (arr[1], arr[0])`
- `t7_double_swap.nv` — `(a, b, c, d) = (b, a, d, c)` (2 независимых цикла)
- `t8_stdlib_reverse.nv` — Vec-like reverse через tuple-swap в цикле
- `t9_identity.nv` — `(a, b) = (a, b)` (0 tmp, no-op)

Негативные (`neg/`):
- `t_neg_arity.nv` — `(a, b) = (1, 2, 3)` → `E_TUPLE_ASSIGN_ARITY_MISMATCH`
- `t_neg_ro_lhs.nv` — `(ro a, b) = (b, a)` → `E_TUPLE_ASSIGN_LHS_NOT_MUT`
- `t_neg_nested_tuple_lhs.nv` — `((a, b), c) = ...` → parse error (V1)

**Commit:** `test(plan136 Ф.4): fixtures`

### Ф.5 — stdlib migration (~30 min)

Обновить `std/collections/vec_owned.nv`:

```nova
// reverse() — было:
ro tmp = @data[lo]
@data[lo] = @data[hi]
@data[hi] = tmp

// стало:
(@data[lo], @data[hi]) = (@data[hi], @data[lo])
```

Аналогично любые другие swap-паттерны в stdlib.

**Commit:** `refactor(plan136 Ф.5): stdlib swap → tuple destructuring assign`

### Ф.6 — Docs + close (~30 min)

- `docs/simplifications.md` — запись о tuple-assign.
- `nova-private/discussion-log.md` + `project-creation.txt`.
- Обновить README.md.

**Commit:** `docs(plan136 Ф.6): close`

---

## Acceptance criteria

- **A1** — `(a, b) = (b, a)` компилируется и выполняется корректно
- **A2** — `(a, b, c) = (b, c, a)` — rotate, 1 tmp в C
- **A3** — `(@data[lo], @data[hi]) = (@data[hi], @data[lo])` — внутри `unsafe {}`
- **A4** — независимые rhs → 0 tmp в C
- **A5** — `E_TUPLE_ASSIGN_ARITY_MISMATCH` при несовпадении длин
- **A6** — `E_TUPLE_ASSIGN_LHS_NOT_MUT` при `ro`-binding слева
- **A7** — 0 regressions в full `nova test`

---

## Followups

- `[M-136-cycle-decomp]` — V2 codegen: cycle-decomposition вместо
  conservative N-tmp. Для `(a, b) = (b, a)` → 1 tmp (не 2).
  Алгоритм: построить граф перестановки, найти циклы, эмитировать
  rotate-через-tmp на каждый цикл.
- `[M-136-nested-tuple-lhs]` — `((a, b), c) = ((c, a), b)` (V1 ban).
- `[M-136-consume-tuple-assign]` — consume-types в tuple-assign
  (требует precise consume-flow через несколько lvalue одновременно).

---

## Связанные планы / D-блоки

| Связь | Что |
|---|---|
| Plan 53 ✅ | `let (a, b) = expr` — destructuring в let |
| Plan 59 ✅ | Tuple monomorphization (basis для tuple C repr) |
| Plan 120 ✅ | Named tuples (stack-allocated) |
| Plan 128.2 ✅ | `lvalue_root_ident` helper — reused в Ф.2 |
| D236 NEW | Spec for tuple destructuring assignment |
| `[M-118-ptr-index-unsafe]` | `@data[i]` в tuple-swap требует unsafe |
