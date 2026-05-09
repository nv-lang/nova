// SPDX-License-Identifier: MIT OR Apache-2.0
# План 14: stdlib-codegen gaps — to compile std/* natively

**Статус:** ⏸️ **PAUSED после 6 фаз** (2026-05-09). Возобновится позже.
Ф.1 ✅, Ф.2 ✅, Ф.3 ✅, Ф.4 ✅, Ф.6 ✅, Ф.7 ✅. Остаётся **Ф.5**
(cross-file resolve) + хвост накопленных gap'ов (см. ниже).
**Дата создания:** 2026-05-08.
**Цель:** убрать ровно те ограничения codegen'а, без которых std/*.nv
не собирается через `nova-codegen compile → cl.exe → exe`.

---

## Контекст

Изначальный (post-Plan 13):

- `nova_tests/`: 85/85 PASS через codegen→C→exe.
- `std/*.nv check`: 50/50 PASS (type-check чист).
- `std/*.nv compile→exe`: 3/50 PASS.

47 файлов падали на 4-5 codegen-gap'ах. Type-checker мягче codegen'а —
это и есть зазор. План закрывает зазор.

**Текущий статус (2026-05-09, после Ф.1/2/3/4/6/7):**
- nova_tests: **91/91 PASS** (+ const_complex, fn_first_class, as_cast_char_literal,
  for_iter_typed, variadic).
- Std/-эффект: ограничен — Ф.1 (Option[T] refactor) сделана прод-grade,
  но открыла **другие** gap'ы за рамками Plan 14 (см. секцию «Накопленные
  блокеры std/» ниже).

Реализовали по убыванию ROI.

---

## Ф.1 — Iter[T] element-type generalization (Option[T] full refactor) ✅ ЗАКРЫТ (2026-05-09)

**Что:** Plan 06 закрыл общий `for-in` через `Iter[T]` protocol-fallback,
но `Option[T]` всегда эмитился как `NovaOpt_nova_int` (legacy int-stomp,
см. бывший `emit_c.rs:6027-6088`). Element-binding терял реальный тип T;
strict bool-check ругался для `Iter[bool]`, byte-arithmetic для
`Iter[byte]` использовал нелогичные типы и т.д.

**Решение — полный refactor `Option[T]` в codegen** (не локальный
work-around). После Ф.1 `Option[T]` правильно типизирован для **любого
T** (primitive, str, tuple, record-pointer, nested Option).

**Реализация (~250 строк в [`emit_c.rs`](../../compiler-codegen/src/codegen/emit_c.rs)):**

1. **`type_ref_to_c(Option[T])`** — теперь читает inner T через generic,
   возвращает `NovaOpt_<sanitized T>`. Для T = generic-erased (type-param
   из generic fn/type) или void* — fallback на `NovaOpt_nova_int`.
2. **Lazy NovaOpt_<T> typedef через marker+splice**:
   - В preamble эмитится sentinel `/*__NOVAOPT_TYPEDEFS__*/`.
   - `register_novaopt_decl(&self, sanitized, c_ty)` (interior-mut через
     `RefCell<String>`) — append'ит typedef в registration order
     (innermost-first → topological correct).
   - После полного `emit_module` — `out.replace(marker, buf)`.
   - Pre-decl'нутые в `nova_rt/array.h` (`nova_int / nova_byte /
     nova_bool / nova_str / nova_f64`) пропускаются.
3. **Some(v) / None constructors** — заменены compound literal'ом
   `((NovaOpt_<T>){.tag = NOVA_TAG_Option_Some, .value = (v)})` /
   `((NovaOpt_<T>){.tag = NOVA_TAG_Option_None})`. T извлекается:
   - для `Some(v)` — из `infer_expr_c_type(arg)`;
   - для `None` — из `current_fn_return_ty` (если NovaOpt_<X>),
     иначе legacy NovaOpt_nova_int.
4. **`?`-оператор** (`Try`) — typed early-return None compound literal.
5. **Pattern-match (`pattern_cond` + `pattern_bind_typed`)** —
   nested-Option match (`Some(Some(Some(n)))`) теперь работает через
   direct value access (`scr.value.value.value`) с правильным
   `var_types`-tracking'ом через temporary registrations.
6. **`emit_for` (Iter[T])** — использует `MethodSig.return_c_type`
   (теперь корректно `NovaOpt_<T>`) для container'а и binding'а.
   Tuple-pattern destructure поддерживает direct `_NovaTupleN value`,
   `_NovaTupleN*` pointer, и legacy nova_int box.
7. **`infer_expr_c_type` Some/None** — теперь возвращает `NovaOpt_<T>`,
   не legacy `NovaOpt_nova_int`.

**Файлы:**
- `compiler-codegen/src/codegen/emit_c.rs` — ~250 строк изменений
  по 7 функциям/методам:
  + новые поля `novaopt_typedefs_buf`, `novaopt_decls_seen`;
  + хелперы `sanitize_for_novaopt`, `register_novaopt_decl`;
  + переписаны 7 codegen-paths.

**Тесты:** `nova_tests/syntax/for_iter_typed.nv` — 5 тестов для
`Iter[byte/bool/i32/str/(int,int)]`. Прогон **90/90 PASS** (89
baseline + 1 new). Бонус: `match_advanced` triple-nested
`Some(Some(Some(42)))` теперь корректно matches через typed payload.

**Std-эффект (smoke-check):**

Ф.1 закрыла ровно `Option[T]` generalization. Std/ файлы по-прежнему
блокированы на **других** codegen-gap'ах (не относящихся к Option[T]):

- `std/collections/set.nv` → abstract `Nova_Iter*` без concrete next
  (требует generic specialization при monomorphization).
- `std/collections/vec.nv` → broken type-name `Nova_[]T*` (требует
  правильного mangling'а array-types).
- `std/collections/range.nv` → `(0..).step_by(3)` infer'ится как
  `nova_int` (Fail-method return type не propagated).
- `std/collections/hashmap.nv` → strict bool-check на generic-erased
  `K.eq(key)` (требует Ф.5 cross-file resolve).
- `std/text/diff.nv`, `std/crypto/bcrypt.nv` → infer fallback
  `nova_int` для нестандартных iter-выражений.

Эти gap'ы требуют **отдельных задач** (generic specialization, mangling
fixes, type-inference improvements). Ф.1 свою часть закрыла полностью —
дальнейший std/-юнблок зависит от других фаз.

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

## Ф.6 — D69 variadic + spread ✅ ЗАКРЫТ (2026-05-09)

**Что:** [D69](../../spec/decisions/03-syntax.md#d69) variadic-параметры
+ call-site spread.

```nova
fn sum_all(...items []int) -> int { ... }    // declaration
sum_all(1, 2, 3)                               // regular args → []int
sum_all(...arr)                                // spread существующего []int
sum_all(1, ...middle, 5)                       // mixed (D60-style)
```

**Реализация:**

1. **AST** (`compiler-codegen/src/ast/mod.rs`):
   - `Param { ..., is_variadic: bool }` — флаг для последнего param'а.
   - Новый enum `CallArg { Item(Expr), Spread(Expr) }` — symmetric с
     существующим `ArrayElem::Item|Spread`.
   - `ExprKind::Call { args: Vec<CallArg>, ... }` — раньше `Vec<Expr>`.

2. **Parser** (`compiler-codegen/src/parser/mod.rs`):
   - `parse_param`: ловит `DotDotDot` префикс перед именем; валидирует
     тип = `[]T` (TypeRef::Array).
   - `parse_fn`: проверяет position constraint — variadic только последний.
   - Call-args parsing (line 1485+): mirroring `parse_array_lit` — ловит
     `DotDotDot` ДО `parse_expr`, формирует `CallArg::Spread`.

3. **Codegen** (`compiler-codegen/src/codegen/emit_c.rs`):
   - `MethodSig.variadic_last: bool` + `user_fn_variadic: HashSet<String>`
     регистрируются при emit_fn_decl_forward / emit_method_overload.
   - `emit_call` — первой строкой вызывает `lookup_variadic_arity(func)`.
     Если variadic, args[regular_arity..] упаковываются в синтезированный
     `Expr { kind: ArrayLit(elems), ... }` где elems из CallArg
     mapped в ArrayElem (Item→Item, Spread→Spread). Затем recurse в
     `emit_call` с переписанными args (через `suppress_variadic_routing`
     guard).
   - **Reuse:** `emit_array_lit` (D60 spread support) — отвечает за
     unrolling `[a, ...arr, c]` → последовательность `nova_array_push`.
   - Spread в non-variadic call → compile error.

4. **Mechanical refactor** (~30 sites): все consumers `Call.args`
   через `arg.expr()` вместо raw `&Expr`.

**Файлы:**
- `compiler-codegen/src/ast/mod.rs` — `Param.is_variadic`, новый
  `CallArg` enum.
- `compiler-codegen/src/parser/mod.rs` — DotDotDot handling в
  `parse_param` + call-args, validation в `parse_fn`.
- `compiler-codegen/src/codegen/emit_c.rs` — registries, routing в
  `emit_call`, `lookup_variadic_arity`, mechanical refactor 30+ sites.
- `compiler-codegen/src/interp/mod.rs`, `types/mod.rs` — extract
  Vec<Expr> from Vec<CallArg> (interp пока без variadic).

**Тесты:** `nova_tests/syntax/variadic.nv` — 7 тестов:
declaration + body, regular args, empty (variadic-position пустая),
spread, mixed, regular+variadic комбинация, instance-method с variadic.

Прогон: nova_tests **91/91 PASS** (90 baseline + 1 new файл).

**Std-эффект:** `std/path/path.nv` `Path.join(parts []str)` → `Path.join(...parts []str)`.
Caller-side оба варианта работают (regular + spread).

**Известные ограничения:**
- Interpreter (`nova-codegen run`) пока не поддерживает spread
  (compile-error при попытке) — отдельная задача.
- print/println остаются special-case (миграция на variadic — отдельная задача).
- Multiple variadic-overloads ambiguous и не поддержаны (только single overload variadic).

---

## Ф.7 — `int as char` для compile-time-known литералов ✅ ЗАКРЫТ (2026-05-09, spec-only)

**Что:** [D54](../../spec/decisions/03-syntax.md#d54) запрещал
`int as char`, требовал `char.try_from(n)?`. Раздражало в местах, где
литерал заведомо валиден.

**Реализация:**

1. **`check_as_cast_allowed`** в [`emit_c.rs`](../../compiler-codegen/src/codegen/emit_c.rs):
   после CharLit-исключения добавлена ветка `IntLit(n) → char`:
   - `n ∈ [0, 0x10FFFF]` (валидный Unicode-диапазон),
   - `n ∉ [0xD800, 0xDFFF]` (surrogate range — invalid scalar),
   - off-range → compile error с **конкретным codepoint** в сообщении
     (не generic suggestion).
2. **Spec D54** — добавлен абзац-исключение «для int-литералов → char»
   в существующий D54, без нового D-номера (edge-case).
3. **Codegen output** — без изменений: `((nova_int)(n))` (no-op cast,
   nova_char и nova_int одинаковы в C).

**Файлы:**
- `compiler-codegen/src/codegen/emit_c.rs` — ~25 строк в
  `check_as_cast_allowed`.
- `spec/decisions/03-syntax.md` D54 — абзац исключения.

**Тесты:** `nova_tests/syntax/as_cast_char_literal.nv` — 8 тестов:
decimal/hex/binary/underscore literals, ASCII, NUL/DEL, кириллица,
U+10FFFF (граница), U+D7FF/U+E000 (вокруг surrogate). Negative-cases
(U+110000, U+D800) — ручная проверка `nova-codegen compile`.

Прогон: **89/89 PASS** (88 baseline + 1 new).

**⚠️ Std-эффект — ноль.** Изначальный план обещал юнблок 4 файлов
(uuid/ulid/base64/hex/property), но ВСЕ они используют паттерн
`('0' as int + n as int) as char` — не чистый IntLit. Ф.7 строго
literal-only, поэтому эти файлы остаются заблокированными.

**Что нужно для std/-юнблока (отдельные задачи):**
- **Ф.7-bis** (extend): распознавать паттерн `(CharLit + IntExpr) as
  char` (~30-50 строк, binary-pattern recognition).
- **Refactor std/**: `char.try_from(n)?` с `?`-propagation в
  `identifiers/{uuid,ulid}.nv`, `encoding/{base64,hex}.nv`.
- **Revolutionary D54**: снять `int as char` запрет полностью (как
  C/Kotlin).

**Решение пользователя (2026-05-09):** оставить Ф.7 в strict
literal-only варианте. Ф.7-bis или refactor — отдельной задачей если
нужны эти файлы. Spec-correctness > unblock-std в этом раунде.

---

## Накопленные блокеры std/ (открыты smoke-check'ом Ф.1, не входят в Plan 14)

После прод-grade Ф.1 (Option[T] full refactor) и Ф.6 (variadic) попытка
скомпилировать std/-файлы натолкнулась на **другие** ограничения, не
относящиеся к Option[T] и не покрытые Plan 14:

| Std-файл | Корень ошибки | Природа gap'а |
|---|---|---|
| `std/collections/set.nv` | `for-in: Nova_Iter*` | Abstract `Iter[T]` erasure — `keys()` возвращает type-erased Iter, без concrete `next` в method_overloads. Нужна generic specialization при monomorphization. |
| `std/collections/vec.nv` | `for-in: Nova_[]T*` | Broken type-name mangling — `Vec.iter()` возвращает inner array, codegen синтезирует malformed `Nova_[]T*` вместо `NovaArray_<T>*`. |
| `std/collections/range.nv` | `for-in: nova_int` для `(0..).step_by(3)` | Fail-throwing method's return type (`-> StepRangeIter Fail[OverflowError]`) не propagated через type-inference — fallback на `nova_int`. |
| `std/collections/hashmap.nv` | `if condition must be bool` (line 299) | Generic-erased `K.eq(key)` без method dispatch — type-checker не resolves protocol-bound через D72. **Это блокер для Plan 15 enforcement.** |
| `std/text/diff.nv`, `std/crypto/bcrypt.nv` | `for-in: nova_int` | infer fallback для нестандартных iter-выражений. |
| `std/identifiers/{uuid,ulid}.nv`, `std/encoding/{base64,hex}.nv` | `('0' as int + n) as char` | Ф.7 не покрыта (literal-only). Требует Ф.7-bis (binary-pattern recognition) или refactor через `try_from`. |
| Tuple типизация (`(int, str)`) | `_NovaTupleN` все поля nova_int | Hardcoded в preamble. Mixed-type tuples broken. Открывает HashMap[K, V], Iter[(K, V)] proper. |

Каждый — **отдельная задача**, не входит в Plan 14. После выбора
приоритета формулируется как самостоятельный план или Plan 14
extension (Ф.7-bis, Ф.8 tuple-types, etc.).

---

## Приоритизация и оценка

| Фаза | ROI | Объём | Зависимости |
|---|---|---|---|
| **Ф.1** Option[T] full refactor | ✅ ЗАКРЫТ | ~250 строк, 5 тестов | — |
| **Ф.2** const non-trivial | ✅ ЗАКРЫТ | ~140 строк, 6 тестов | — |
| **Ф.3** free-fn-as-value | ✅ ЗАКРЫТ | ~95 строк, 5 тестов | — |
| **Ф.4** fn-в-record | ✅ ЗАКРЫТ | ~50 строк, 4 тестов | — |
| **Ф.7** `int as char` literal | ✅ ЗАКРЫТ (spec-only) | ~25 строк, 8 тестов | — |
| **Ф.6** D69 variadic | ✅ ЗАКРЫТ | ~500 строк (с CallArg refactor), 7 тестов | — |
| **Ф.5** cross-file resolve | низкий ROI / высокая стоимость | ~500 строк, 1 неделя | Ф.1-Ф.4 |

**Реализованный порядок:** Ф.7 → Ф.1 → Ф.6 (выполнено).

**Retrospective:** ожидание «+25-30 std-файлов в PASS» оказалось
завышенным. Ф.1/2/3/4/6/7 закрыли свои частные codegen-gap'ы, но
std/-файлы упёрлись в **другие** блокеры (см. секцию выше). Plan 14
свою declared-цель закрыл архитектурно правильно; std/-юнблок
требует отдельных планов на конкретные паттерны.

**Pause + roadmap для возобновления:**
- Ф.5 (cross-file resolve) — возможно cheap-вариант (документировать
  как known-limit) или full при появлении свободного времени.
- «Накопленные блокеры» (выше) — каждый формулируется в отдельный план
  при необходимости.

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
