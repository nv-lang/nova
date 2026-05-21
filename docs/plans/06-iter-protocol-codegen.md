# План 06: `Iter[T]` protocol в codegen — общий `for-in`

**Статус:** ✅ ВЫПОЛНЕНО. Ф.1-Ф.3 + Ф.5 закрыты (`emit_for` Case 3 —
Iter[T] protocol fallback, tuple-destructuring, implicit `.iter()`;
тесты `nova_tests/syntax/for_iter*.nv`). Ф.4 (sweep ad-hoc
`chars()`/`bytes()`/`split()`-обходок) сознательно отложен — не входил
в acceptance, eager-конверсия в массивы продолжает работать.
Шапка-метка обновлена 2026-05-21 (аудит план-статусов: была
устаревшая «не начат», хотя фича реализована давно).
**Дата создания:** 2026-05-08.
**Зависимости:** [D58](../../spec/decisions/03-syntax.md#d58) — спека уже
описывает `Iter[T]` и implicit `.iter()` для `for-in`.

---

## Проблема

В bootstrap-codegen (`compiler-codegen/src/codegen/emit_c.rs:4692`)
`emit_for` поддерживает **только два частных случая** итератора:

1. `for i in 0..n` (Range литерал) — спец-обработка через C `for(...)`.
2. `for x in arr` (`[]T`) — спец-обработка через NovaArray `data[i]` access.

Для **всего остального** последняя строка функции (4782):

```rust
Err(format!("for-in: unsupported iterator type '{}' — only Range and Array are supported", arr_ty))
```

То есть **универсальный `Iter[T]` protocol из D58 не реализован**.

### Что не работает

#### 1. Range-методы возвращающие итераторы

```nova
for i in (0..n).step_by(2)        // ❌ StepRangeIter
for i in (0..n).reverse()         // ❌ ReverseRangeIter
```

Декларации в `std/collections/range.nv` есть (после D82), но `for-in`
их не понимает — эмиттер не знает что у `StepRangeIter` есть
`mut @next() -> Option[int]`.

#### 2. Iterator-методы коллекций

```nova
for (k, v) in map.entries()       // ❌ HashMapIter[K, V]
for k in map.keys()               // ❌ KeysIter[K, V]
for v in map.values()             // ❌ ValuesIter[K, V]
for x in deque.iter()             // ❌ DequeIter[T]
for x in lru.iter()               // ❌ LRUIter[K, V]
```

Все эти типы определены в `std/collections/*.nv` структурно как
`Iter[T]` (имеют `mut @next() -> Option[T]`), но bootstrap не делает
структурную проверку — он смотрит **только** на `NovaArray_*` и
`ExprKind::Range`.

#### 3. String iterators

```nova
for c in s.chars()                // ⚠️ работает специально через codegen-spec случай
for b in s.bytes()                // ⚠️ работает специально
for part in s.split(",")          // ⚠️ работает специально
```

`chars()`/`bytes()`/`split()` обрабатываются **специально** в codegen
(раунд 4, см. simplifications.md) — eager-конверсия в `[]int`/`[]byte`/
`[]str` массивы. Это **не общий путь** через Iter[T], а ad-hoc обходка.

#### 4. Iter composition

```nova
for x in xs.filter(non_empty)     // ❌
for x in xs.map(f).filter(g)      // ❌
for x in xs.take(10)              // ❌
for x in chain(xs, ys)            // ❌
```

Любой композиционный итератор — не работает.

#### 5. Pattern-destructuring в binding'е

```nova
for (i, x) in xs.enumerate()      // ❌ binding — не identifier
for (k, v) in map.entries()       // ❌
for (idx, c) in s.chars().enumerate() { ... }
```

`emit_for` использует `pattern_binding(pattern)?` который требует
single identifier; tuple-destructuring не поддержан.

### Масштаб проблемы

В `std/` сейчас:
- **HashMap** (`std/collections/hashmap.nv`) — содержит `for k in m.keys()`,
  `for (k, v) in counts.entries()` — **не компилируется**.
- **Deque, LRU, BloomFilter** — все имеют `iter()` методы, но for-loop
  через них не работает.
- **Property-based testing** (`std/testing/property.nv`) — `Iter[T]` в
  `Generator[T].@shrink(value) -> Iter[T]` — не работает.
- **Любая будущая stdlib-либа** упирается в этот gap.

Это **главный архитектурный gap** между D58 (spec) и реальностью.

### Что говорит spec

[D58](../../spec/decisions/03-syntax.md#d58) явно описывает:

> **`Iter[T]`** — структурный protocol для итераторов. Любой
> тип с методом `mut next() -> Option[T]` автоматически удовлетворяет.
> `for x in collection`-синтаксис вызывает `collection.iter().next()` в
> цикле; коллекции реализуют `iter()` возвращая собственный
> iterator-тип.

То есть `for-in` **должен** работать через структурный protocol-check,
а не через hardcoded type-list.

---

## Цель

`emit_for` поддерживает третий случай — **fallback на `Iter[T]` protocol**:
если тип итератора имеет метод `mut @next() -> Option[T]`, цикл
работает.

Поведение:
- Получить итератор: если `iter` это вызов `.iter()` или результат —
  использовать как есть; иначе вызвать `.iter()` (implicit).
- В loop'е: `let v_opt = it.next(); if v_opt is None { break }; let x = v_opt.unwrap()`.
- Pattern-destructuring `(i, x)` в binding'е через tuple-pattern.

---

## Не цель

- **Compile-time протокол-check** ("этот тип реализует Iter[T]?") как в Rust.
  Bootstrap делает structural duck-typing — есть метод `next` с правильной
  сигнатурой → работает.
- **Lazy evaluation** для Iter composition (`map`, `filter`). Это требует
  closure-state'а. Откладывается — пока eager `.collect()` обходка.
- **Async iterators** (`AsyncIter[T]`). Не в scope.

---

## Что делаем

### Ф.1 — Case 3 в `emit_for`: Iter[T] protocol fallback

После Case 2 (Array), перед `Err`:

```rust
// Case 3: Iter[T] protocol — type has `mut @next() -> Option[T]`
if let Some(elem_ty) = self.try_resolve_iter_protocol(&arr_ty) {
    let binding_decl = self.pattern_destructure_decl(pattern, &elem_ty)?;
    let it_tmp = self.fresh_tmp();
    let opt_tmp = self.fresh_tmp();
    let result_tmp = self.fresh_tmp();

    // let mut _it = <iter>; (берём по value, мутируем .cur и т.д.)
    let it_expr = self.emit_expr(iter)?;
    self.line(&format!("{} {} = {};", arr_ty, it_tmp, it_expr));
    self.var_types.insert(it_tmp.clone(), arr_ty.clone());

    self.line(&format!("nova_unit {};", result_tmp));
    self.line("for (;;) {");
    self.indent += 1;

    // _opt = _it.@next()
    let opt_ty = format!("NovaOpt_{}", elem_ty);
    self.line(&format!("{} {} = Nova_{}_next(&{});",
        opt_ty, opt_tmp, type_for_method_name(&arr_ty), it_tmp));

    // if (_opt.tag == None) break;
    self.line(&format!("if ({}.tag == NOVA_TAG_{}_None) break;",
        opt_tmp, opt_ty));

    // let <pattern> = _opt.payload.Some_value;
    self.emit_pattern_destructure(pattern, &elem_ty,
        &format!("{}.payload.Some_value", opt_tmp))?;

    for stmt in &body.stmts { self.emit_stmt(stmt)?; }
    if let Some(trailing) = &body.trailing {
        let v = self.emit_expr(trailing)?;
        self.line(&format!("(void)({});", v));
    }

    self.indent -= 1;
    self.line("}");
    self.line(&format!("{} = NOVA_UNIT;", result_tmp));
    return Ok(result_tmp);
}
```

`try_resolve_iter_protocol(arr_ty)` ищет метод `Nova_<Type>_next` в
текущем scope. Если есть — возвращает element type из его сигнатуры
(`Option[T]` → `T`).

### Ф.2 — Tuple-destructuring в `pattern_binding`

```nova
for (k, v) in map.entries() { ... }
```

Сейчас `pattern_binding` падает на tuple-pattern. Расширить:

```rust
fn pattern_destructure_decl(&self, pattern: &Pattern, elem_ty: &str)
    -> Result<String, String>
{
    match pattern {
        Pattern::Ident(name) => Ok(format!("{} {}", elem_ty, name)),
        Pattern::Tuple(parts) => {
            // Нужно разбить tuple-payload на части
            // Эмитим: _NovaTuple2 _t = <expr>; T0 a = _t.f0; T1 b = _t.f1;
            // ...
        }
        _ => Err("for-in pattern: only ident or tuple supported".into())
    }
}
```

### Ф.3 — Implicit `.iter()` для коллекций

D58 говорит: «`for x in collection` вызывает `collection.iter()`».
Bootstrap уже делает это для `[]T` (Case 2). Расширить на остальные:

```nova
for x in deque        // должно вызвать deque.iter()
for k in map.keys()   // .keys() уже возвращает iterator — не нужен .iter()
```

Эвристика:
- Если выражение возвращает тип с `next()` — используем как есть.
- Иначе если возвращает тип с `iter()` — вставляем `.iter()`.
- Иначе — error.

### Ф.4 — Удалить ad-hoc обходку для `chars()`/`bytes()`/`split()`

После Ф.1 эти методы могут вернуться к нормальному пути:
`s.chars()` возвращает `CharsIter` с `next() -> Option[char]` — общий
Iter[T] case подхватит.

Но **не в этом плане** — это отдельный sweep после стабилизации Ф.1.
Текущая ad-hoc обходка работает; не ломаем.

### Ф.5 — Тесты в `nova_tests/`

Новый файл `nova_tests/syntax/for_iter.nv`:

```nova
test "for-in over Range.step_by" {
    let mut sum = 0
    for i in (0..10).step_by(2) {
        sum += i
    }
    assert(sum == 0 + 2 + 4 + 6 + 8)
}

test "for-in over Range.reverse" {
    let mut collected []int = []
    for i in (1..=3).reverse() {
        collected.push(i)
    }
    assert(collected == [3, 2, 1])
}

test "for-in over HashMap.values" {
    let mut m = HashMap[str, int].new()
    m.insert("a", 1)
    m.insert("b", 2)
    let mut sum = 0
    for v in m.values() {
        sum += v
    }
    assert(sum == 3)
}

test "for-in tuple destructure" {
    let mut m = HashMap[str, int].new()
    m.insert("x", 10)
    let mut found_x_10 = false
    for (k, v) in m.entries() {
        if k == "x" && v == 10 { found_x_10 = true }
    }
    assert(found_x_10)
}

test "for-in implicit iter() on Deque" {
    let mut d = Deque[int].new()
    d.push_back(1)
    d.push_back(2)
    d.push_back(3)
    let mut sum = 0
    for x in d {                        // implicit .iter()
        sum += x
    }
    assert(sum == 6)
}
```

### Ф.6 — Smoke std/

После Ф.1-Ф.5 проверить:
- `std/collections/hashmap.nv` — тесты с `for k in m.keys()` проходят.
- `std/collections/deque.nv` — `for x in d.iter()` работает.
- `std/testing/property.nv` — Iter[T] generators работают для shrinking.

Ничего не правим — просто запускаем `run_tests.ps1 -IncludeStdlib` и
смотрим что новые либы перешли в PASS.

---

## Acceptance criteria

- ✅ `for i in (0..10).step_by(2)` — компилируется и работает.
- ✅ `for i in (0..10).reverse()` — то же.
- ✅ `for v in m.values()`, `for k in m.keys()` — работает.
- ✅ `for (k, v) in m.entries()` — tuple destructuring работает.
- ✅ `for x in deque` — implicit `.iter()` работает.
- ✅ `nova_tests/syntax/for_iter.nv` — все тесты PASS.
- ✅ Существующие тесты (`tests-nova/39_for_in_array.nv` и др.) — нет
  регрессий.
- ✅ `std/collections/hashmap.nv` тесты переходят в PASS (как минимум
  те где for-iter блокировал).

---

## Trade-offs / упрощения

### Iter composition (`map`/`filter`/...) — отдельный план

Lazy iterators (`xs.map(f)`, `xs.filter(g)`, `xs.take(n)`) требуют
closure-захвата состояния и нетривиальной C-репрезентации. Откладываем.

В bootstrap'е пока: `xs.map(f).collect()` через eager `[]T`-аллокацию
и `for x in arr.collect()`. Не идеально, но работает для большинства
use-case'ов.

### Сохраняем Range/Array fast-path

Не трогаем Case 1 и Case 2 — они эффективнее (нет вызова `.next()`,
нет Option-распаковки). Iter[T] — только fallback.

### Структурное вместо явного `impl Iter for T`

В Rust требуется `impl Iterator for X { type Item = T; fn next(&mut self) -> Option<T> }`.
В Nova по D58 — структурное соответствие. Bootstrap делает
**duck-typing**: есть `Nova_X_next` с сигнатурой `(X*) -> NovaOpt_T` —
работает. Никаких explicit-decl'ов.

---

## План работ

1. **Ф.1** — Case 3 в `emit_for` (~50 строк Rust).
2. **Ф.2** — Tuple-destructuring в `pattern_destructure_decl` (~30 строк).
3. **Ф.3** — Implicit `.iter()` для коллекций (heuristic, ~20 строк).
4. **Ф.5** — `nova_tests/syntax/for_iter.nv` (5+ тестов).
5. **Ф.6** — Smoke check stdlib.
6. **Ф.4** — sweep ad-hoc обходок (отдельным коммитом, не блокер).

---

## Оценка

День работы для компилятор-агента. ~150 строк Rust + тесты.

Главный вопрос реализации: **где хранить mapping `Type → next-method
signature`** для structural Iter[T] check. Возможно через существующий
`method_table` или новое поле в `EmitContext`.

---

## Связь с другими планами

- [Plan 02 — Codegen C backend](02-codegen-c-backend.md) — общий roadmap codegen.
- [Plan 04 — Buffer split + external](04-buffer-split-and-external.md) — `external`
  keyword позволит эффективнее эмитить iterator-state через FFI.
- [Plan 05 — as-cast codegen](05-as-cast-codegen.md) — параллельная задача,
  не зависит.

---

## Ссылки

- [spec/decisions/03-syntax.md → D58](../../spec/decisions/03-syntax.md#d58)
  — `Iter[T]` protocol, implicit `.iter()`.
- [spec/decisions/03-syntax.md → D82](../../spec/decisions/03-syntax.md#d82)
  — C-style for отвергнут, range-iterator + step_by покрывает все случаи.
  Реализация D82 требует Iter[T] для `step_by`/`reverse`.
- `compiler-codegen/src/codegen/emit_c.rs:4692-4783` — текущий `emit_for`.
- `std/collections/hashmap.nv:218-265` — пример Iter[T] реализаций.
- `std/collections/range.nv:80-180` — пример Iter[T] для Range вариаций.
- `std/testing/property.nv` — Generator[T].@shrink использует Iter[T].
- `docs/simplifications.md:51-54` — закрытие Array fast-path (раунд 1).
