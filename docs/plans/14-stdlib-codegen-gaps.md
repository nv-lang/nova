// SPDX-License-Identifier: MIT OR Apache-2.0
# План 14: stdlib-codegen gaps — to compile std/* natively

**Статус:** активный, не начат.
**Дата создания:** 2026-05-08.
**Цель:** убрать ровно те ограничения codegen'а, без которых std/*.nv
не собирается через `nova-codegen compile → cl.exe → exe`.

---

## Контекст

Сейчас (post-Plan 13):

- `nova_tests/`: **85/85 PASS** через codegen→C→exe.
- `std/*.nv check`: **50/50 PASS** (type-check чист).
- `std/*.nv compile→exe`: **3/50 PASS**.

47 файлов падают на 4-5 codegen-gap'ах, не зависящих друг от друга.
Type-checker мягче codegen'а — это и есть зазор. План закрывает зазор.

Реализуем по убыванию ROI.

---

## Ф.1 — Iter[T] element-type generalization (расширение Plan 06)

**Что:** Plan 06 закрыл общий `for-in` через `Iter[T]` protocol-fallback,
но element type **захардкожен на `nova_int`** (см. `emit_c.rs:6027-6088`).
Реальные итераторы с `T ≠ int` — `HashMapIter[(K,V)]`, `KeysIter[K]`,
`Set @iter`, `[]T-extension iterators`, `Iter[char]` от `s.chars()`,
`Iter[byte]` от `s.bytes()` — падают с
`for-in: unsupported iterator type` или эмитят некорректный C.

**Затронутые std-файлы (10+):**
`collections/{hashmap,set,vec,linkedlist,deque,bloom_filter}.nv`,
`collections/range.nv` (step_by/reverse), `crypto/bcrypt.nv`,
`text/{regex,diff}.nv`.

**Что делать:**
1. Перед эмитом for-loop — определить element-type итератора:
   - Если выражение возвращает `Nova_X*` — найти `Nova_X_method_next`
     в method_table, взять его return-type (`NovaOpt_T`), извлечь `T`.
   - Для primitive iterator-types (`s.chars() → Iter[char]`,
     `s.bytes() → Iter[byte]`) — спецтаблица fallback.
2. Заменить хардкод `NovaOpt_nova_int = ... .next(it)` на
   `NovaOpt_<T> opt_<n> = Nova_<X>_method_next(it);`.
3. Корректно обрабатывать element-type в loop-body — `let x = opt.value`
   с типом `T`, а не `nova_int`.

**Файлы:** `compiler-codegen/src/codegen/emit_c.rs` (~80 строк).

**Тесты:**
- `nova_tests/syntax/for_iter_typed.nv` — for x in `Iter[char]`,
  `Iter[(int,str)]`, `Iter[bool]` (новый файл).
- Smoke-check: после Ф.1 должны компилироваться `std/collections/set.nv`,
  `std/collections/vec.nv`, `std/collections/range.nv` (step_by).

---

## Ф.2 — `non-constant expression in const declaration` ✅ ЗАКРЫТ (2026-05-09)

**Что:** [D33](../../spec/decisions/03-syntax.md#d33) разрешает любые
expression в `const`. Раньше `emit_c.rs:821` принимал только
`IntLit`/`FloatLit`/`BoolLit`/`StrLit`/`CharLit`/`Unary`. Запись:

```nova
const SERVER_DEFAULTS = ServerOpts { port: 8080, host: "0.0.0.0", ... }
const ZERO = Complex { re: 0.0, im: 0.0 }
const ZERO_DURATION = Duration { nanos: 0 }
const NIL_UUID = Uuid.new(0, 0)
```

— все падают.

**Затронутые std-файлы (4):**
`identifiers/{uuid,uuid_namespace}.nv`, `math/complex.nv`,
`time/duration.nv` (косвенно — много мест используют `Duration.ZERO`).

**Что делать:**
- Const с runtime-expression десахарить в **lazy-init function**:
  ```c
  static Nova_ServerOpts _const_SERVER_DEFAULTS_value;
  static int _const_SERVER_DEFAULTS_init = 0;
  static Nova_ServerOpts SERVER_DEFAULTS(void) {
      if (!_const_SERVER_DEFAULTS_init) {
          _const_SERVER_DEFAULTS_value = (Nova_ServerOpts){...};
          _const_SERVER_DEFAULTS_init = 1;
      }
      return _const_SERVER_DEFAULTS_value;
  }
  ```
- На use-site — `SERVER_DEFAULTS()` вместо `SERVER_DEFAULTS`.
  Альтернатива: `__attribute__((constructor))` для cl.exe + gcc.
- Простые литералы остаются как `#define` / static const.

**Реализация:**

1. **`emit_const_decl`** — после неудачи `emit_const_expr` (constexpr-only)
   делегирует в `emit_lazy_const`.
2. **`emit_lazy_const`**:
   - Эмитит storage `static <Ty> _nova_const_<name>_value;` + flag
     `static int _nova_const_<name>_init = 0;` в file-scope.
   - Эмитит геттер `static <Ty> nova_const_<name>(void)` в `deferred_impls`
     (после forward declarations) — в нём lazy-init pattern с проверкой
     init-flag, выполнением expr через обычный `emit_expr` (включая
     side-statements), запись в storage и flag=1.
   - Регистрирует `name` в `lazy_consts: HashSet<String>`.
   - Регистрирует тип в `var_types[name]` для type-inference.
   - Устанавливает `expected_record_type` перед `emit_expr` для D55
     coercion (`const FOO Type = { ... }` без явного имени типа).
3. **Use-site routing**: 
   - `Ident(name)` в `emit_expr` для lazy-const → `nova_const_<name>()`.
   - `Path([name, field, ...])` для lazy-const + record-поле →
     `nova_const_<name>()->field` (важно: парсер строит Path, не Member,
     для `FOO.x` если первый сегмент — Ident-with-uppercase).

**Файлы:** `compiler-codegen/src/codegen/emit_c.rs` (~140 строк):
- `lazy_consts: HashSet<String>` field;
- `emit_lazy_const(name, ty_c, value)` метод;
- Ident-path routing (приоритет lazy_consts перед is_local_var);
- Path-emit routing для `lazy_const.field`.

**Тесты:** `nova_tests/syntax/const_complex.nv` — 6 тестов:
- record-литерал с явным именем типа,
- D55 record-coercion без имени типа,
- function call в const,
- использование lazy const в fn body,
- кеширование (повторное обращение),
- простые const'ы (int/f64/bool) — остаются как `static const`.

Все 6 PASS. std/ effect: 4 файла (complex/duration/uuid/uuid_namespace)
теперь проходят const-emit и упираются в **другие** codegen-gaps
(strip_suffix, int-as-char, cross-file resolve) — не const-emit.

---

## Ф.3 — Free fn по имени как value ✅ ЗАКРЫТ (2026-05-08)

**Что:** [D22](../../spec/decisions/03-syntax.md#d22) +
[D35](../../spec/decisions/03-syntax.md#d35) делают любую функцию
first-class. Было:

```nova
fn inc(x int) -> int => x + 1
let f = inc                 // ❌ codegen эмитил extern fn-pointer
let ys = xs.map(inc)        // ❌ same
```

`emit_c.rs:3580` (Ident → `nova_fn_<name>`) делал только половину
работы — возвращал указатель на функцию, но callers ждут closure-struct
`{void* env; fn_ptr}`. Без env-обёртки `NOVA_CLOS_CALL_*` не работал.

**Реализация:**

1. **Реестр user_fn_sigs** в CEmitter (`HashMap<String, (Vec<String>, String)>`):
   при `emit_fn_decl_forward` для top-level fn без receiver/generics
   сохраняется sig `(param_c_types, ret_c_type)`. Используется для
   построения thunk'а.

2. **`emit_free_fn_value(name)`** (~75 строк в emit_c.rs):
   - Один раз эмитит **envless thunk**:
     ```c
     static <ret> nova_fn_<name>_thunk(void* env, args...) {
         (void)env;
         return nova_fn_<name>(args...);
     }
     ```
     Дедупликация через `emitted_fn_thunks: HashSet<String>` —
     несколько ссылок на одну fn делят один thunk.
   - На use-site эмитит closure-литерал:
     ```c
     NovaClos_X* tmp = nova_alloc(sizeof(NovaClos_X));
     tmp->fn = (nova_fn_X)nova_fn_<name>_thunk;
     tmp->env = NULL;
     // returns (void*)tmp
     ```

3. **Routing в `Ident(name)` path** (emit_c.rs:3580): если
   `is_user_fn && !is_local_var` — пробует `emit_free_fn_value`;
   если sig в user_fn_sigs — возвращает closure-value, иначе fallback
   на raw `nova_fn_<name>`.

4. **Регистрация bound в fn_param_sigs**: при `let f = inc` (RHS — Ident,
   не local var, есть в user_fn_sigs) — копирует sig из user_fn_sigs в
   fn_param_sigs[`f`]. Тогда `f(x)` идёт через `NOVA_CLOS_CALL_*` macro.

**Что НЕ ломается:**
- Direct calls (`inc(5)`) идут через `emit_call` → `infer_func_c_name`
  → `nova_fn_inc(...)` напрямую. emit_expr на `Ident(inc)` тут не вызывается.
- Generic functions (имеющие type params) НЕ регистрируются в
  user_fn_sigs (sig зависит от инстанциации) — fallback на старое поведение.

**Известное ограничение (bootstrap):** flat `var_types` (нет scoping'а)
означает что local-let-binding `dbl` в одном тесте затмит global `fn dbl`
в другом. Тесты Ф.3 используют `top_inc`/`top_dbl` чтобы избежать
конфликта. Real fix — введение scope'а в bootstrap-codegen — отдельная
задача.

**Файлы:**
- `compiler-codegen/src/codegen/emit_c.rs` (~95 строк):
  + `user_fn_sigs` field, `emitted_fn_thunks` field;
  + регистрация в `emit_fn_decl_forward`;
  + `emit_free_fn_value()` метод;
  + Ident-path в `emit_expr` — emit closure-value;
  + let-decl path — register в fn_param_sigs.

**Тесты:** `nova_tests/syntax/fn_first_class.nv` — 5 новых тестов:
1. `free fn name as value — let-binding`
2. `free fn name as value — передача в HOF`
3. `две free fn в HOF — независимые`
4. `free fn в compose с lambda`
5. `две именованные fn в compose`

Все 17/17 в файле PASS, full nova_tests **87/87 PASS** (без регрессий).

**Pre-Ф.3 баланс:** Ф.3 закрыта. Из плана 14 остаются Ф.1, Ф.2, Ф.4, Ф.5,
Ф.6, Ф.7.

---

## Ф.4 — fn-поле в record + вызов через member-access ✅ ЗАКРЫТ (2026-05-09)

**Что:** [D35](../../spec/decisions/03-syntax.md#d35). Раньше:

```nova
type Op { name str, f fn(int) -> int }

let op = Op { name: "inc", f: (x) => x + 1 }
op.f(5)                      // ❌ codegen эмитил direct-call вместо closure-call
```

**Реализация:**

1. **Реестр `record_field_fn_sigs`** в CEmitter
   (`HashMap<(record_name, field_name), (param_c_tys, ret_c_ty)>`):
   при `emit_record_type` для каждого поля с `TypeRef::Func`
   сохраняем sig.

2. **Member-call routing** в `emit_call`:
   - Если `func.kind == Member { obj, name: method }`,
   - И `obj_ty` — `Nova_<record_name>*`,
   - И `(record_name, method)` есть в `record_field_fn_sigs` →
   - Эмитим `NOVA_CLOS_CALL_*` macro с обращением к полю
     `(obj->field_mangled)` как f-value.
   - Известные fn-сигнатуры (vi/ii/ib/iii/vii) — через типизированный
     macro; arbitrary — через `NovaClosBase` cast.

3. **Member-expr (без call)** — продолжает работать как обычное
   field-access (`obj->f` — void*-closure-value, который уже корректно
   передаётся в HOF / let-binding).

**Файлы:** `compiler-codegen/src/codegen/emit_c.rs` (~50 строк).

**Тесты:** `nova_tests/syntax/fn_first_class.nv` +4:
- `fn-поле в record + вызов через obj.f(x)`
- `fn-поле в record — две разные fn`
- `fn-поле arity 2 — несколько параметров`
- `fn-поле + free fn (Plan 14 Ф.3)` — combo с user_fn_sigs.

Все 21/21 в файле PASS.

---

## Ф.5 — Cross-file resolution для compile-mode

**Что:** ограничение #2 из README (`compiler-codegen/README.md:286`):

> std/*.nv не подключается автоматически. Если в твоём .nv есть
> `import std.collections.HashMap` — codegen не найдёт.

**Затронутые std-файлы (15+):** все cross-module зависимости —
`collections/{lru,priority_queue,deque}.nv` использует HashMap;
`crypto/{hmac,jwt,md5,sha1,sha256,bcrypt}.nv` использует WriteBuffer;
`encoding/{csv,toml}.nv` использует HashMap; `path/path.nv` использует
StringBuilder.

**Что делать:**
- При compile-режиме (один-файл-в-exe) разрешать `import std.X.Y` в
  module-resolver: загружать src, добавлять в текущую compilation-unit,
  unique-by-module-path избегая дублей.
- Module-cache по абсолютному пути.
- Глубокие зависимости (`hmac → sha256 → write_buffer`) — топологически.

**Файлы:** `compiler-codegen/src/{codegen/emit_c.rs, types/, parser/}` —
большая правка по архитектуре, ~300-500 строк. **Самая дорогая фаза**
плана. Делать **последней**, после Ф.1-Ф.4.

**Альтернатива (cheap):** оставить cross-file как есть, документировать
что multi-file тестируется через `nova-codegen run` (interp). Тогда
fail в codegen для cross-file — known limitation, не блокер.

**Тесты:**
- `nova_tests/modules/import_std.nv` (новый) — сборка модуля с явным
  `import std.collections.HashMap` через codegen.

---

## Ф.6 — D69 variadic + spread (отдельный мини-план, низкий приоритет)

**Что:** [D69](../../spec/decisions/03-syntax.md#d69).

```nova
fn print(...items []any) Io -> () => ...     // декларация
print(a, b, c)                                  // вызов
print(...arr)                                   // spread на call-site
```

Парсер не поддерживает prefix-`...` ни в декларации, ни в args.
`print`/`println` сейчас — special case в lexer/codegen.

**Что делать:**
1. Lexer: `...` уже есть как `DotDotDot` (D60 spread в литералах).
2. Parser: при разборе параметра функции — если первый токен `...`,
   parameter — variadic. Только последний параметр.
3. Parser: при разборе args — `...expr` в одном из args = spread.
4. Codegen: variadic-параметр компилируется в `NovaArray_<T>* items`
   арг-passing; на call-site — собирать из args в массив.
5. Spread на call-site: если последний arg = spread, передать array
   напрямую без обёртки.

**Затронутые std-файлы:** `path/path.nv` сейчас обходит через явный
`parts []str`. После Ф.6 вернуть variadic.

**Файлы:** `compiler-codegen/src/{lexer/,parser/,codegen/emit_c.rs,
ast/mod.rs}` (~150 строк).

**Тесты:**
- `nova_tests/syntax/variadic.nv` (новый) — variadic-fn declaration,
  call с отдельными args, call со spread, mixed.

---

## Ф.7 — `int as char` для compile-time-known литералов

**Что:** [D54](../../spec/decisions/03-syntax.md#d54) запрещает
`int as char`. Нужно `char.try_from(n)?`. Раздражает в hot loops где
литерал заведомо валиден:

```nova
let c = 0x41 as char         // ❌ запрещено
let c = char.try_from(0x41)?  // ✅ но ? нужен Fail в сигнатуре
```

**Что делать:** ослабить D54 — для `IntLit n` где `n ∈ 0..0x10FFFF`
исключая surrogate range (0xD800..0xDFFF) — разрешить `as char` без
runtime-check'а. Type-checker validity знает на compile-time.

**Затронутые std-файлы:** `identifiers/ulid.nv` обходит через try_from;
`testing/property.nv` (Char generator).

**Файлы:** `compiler-codegen/src/types/checker.rs` (`as`-validation,
~10 строк) + spec-update `spec/decisions/03-syntax.md` D54 (исключение
для compile-time-known литералов).

**Тесты:**
- `nova_tests/syntax/as_cast_char_literal.nv` (новый, ~5 тестов).

---

## Приоритизация и оценка

| Фаза | ROI | Объём | Зависимости |
|---|---|---|---|
| **Ф.1** Iter[T] element-type | 🔥 высший | ~80 строк, 1 день | нет |
| **Ф.2** const non-trivial | ✅ ЗАКРЫТ | ~140 строк, 6 тестов | — |
| **Ф.3** free-fn-as-value | ✅ ЗАКРЫТ | ~95 строк, 5 тестов | — |
| **Ф.4** fn-в-record | ✅ ЗАКРЫТ | ~50 строк, 4 тестов | — |
| **Ф.7** `int as char` literal | средний | ~10 строк, 1 час | нет |
| **Ф.6** D69 variadic | средний | ~150 строк, 2 дня | нет |
| **Ф.5** cross-file resolve | низкий ROI / высокая стоимость | ~500 строк, 1 неделя | Ф.1-Ф.4 |

**Рекомендуемый порядок:** Ф.1 → Ф.3 → Ф.2 → Ф.4 → Ф.7 → Ф.6 → (Ф.5 опционально).

После Ф.1+Ф.2+Ф.3+Ф.4 ожидаем **+25-30 std-файлов** в PASS-колонке
(из 47 fail сейчас), без ломания nova_tests/.

---

## Связь с другими планами

- [Plan 06](06-iter-protocol-codegen.md) — Ф.1 расширяет Plan 06
  (там был только int-iterator path).
- [Plan 11](11-method-values-and-overload.md) — Ф.3 опирается на
  fn_param_sigs (closure-struct routing).
- [Plan 15](15-generic-bounds-enforcement.md) — отдельный план для
  D72 enforcement, **не входит в этот план**.

---

## Ссылки

- [spec/decisions/03-syntax.md → D33](../../spec/decisions/03-syntax.md#d33) — const expressions.
- [spec/decisions/03-syntax.md → D58](../../spec/decisions/03-syntax.md#d58) — Iter[T] protocol.
- [spec/decisions/03-syntax.md → D69](../../spec/decisions/03-syntax.md#d69) — variadic.
- [spec/decisions/03-syntax.md → D54](../../spec/decisions/03-syntax.md#d54) — as / is.
- [compiler-codegen/README.md](../../compiler-codegen/README.md) — известные ограничения.
