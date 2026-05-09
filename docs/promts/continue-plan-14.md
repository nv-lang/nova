# Продолжение Plan 14: оставшиеся фазы (Ф.1 / Ф.5 / Ф.6 / Ф.7)

> **Цель этого промпта** — передать контекст следующему агенту чтобы
> он мог продолжить план 14 без повторного исследования. Использовать
> как первый prompt в новой сессии.

---

## Состояние на момент передачи (2026-05-09)

### Что закрыто

- **Plan 17** (все Q-resolutions) ✅ — `docs/plans/17-q-resolutions.md`.
- **Plan 17 Ф.4** (string interpolation полная реализация: lexer/parser/
  AST/codegen/interp). Codegen — StringBuilder backend.
- **Plan 14 Ф.3** (free-fn-as-value) ✅ — `let f = inc; xs.map(inc)`.
- **Plan 14 Ф.2** (const non-trivial — lazy-init) ✅.
- **Plan 14 Ф.4** (fn-в-record closure-call) ✅.

### Tests

- **88/88 nova_tests PASS** через `nova-codegen compile → cl.exe → exe`.
- **91/137 std overall PASS** (87 nova_tests + 4 std).

### Что **остаётся** в Plan 14

См. `docs/plans/14-stdlib-codegen-gaps.md`. По убыванию ROI:

1. **Ф.1 — Iter[T] element-type generalization** (hardcoded на nova_int).
   Блокирует ~10 std-файлов. См. секцию Ф.1 плана.
2. **Ф.7 — `int as char` для compile-time-known литералов**. ~10 строк
   кода, блокирует 4 std-файла (uuid/ulid/base64/hex/property).
3. **Ф.5 — Cross-file resolution для compile-mode**. Самая большая
   задача (~500 строк), но открывает 15+ std-файлов.
4. **Ф.6 — D69 variadic + spread**. Не блокирует много, но spec'нуто.

---

## Что точно НЕ работает в std/ (категориями)

Запустить `run_tests.ps1 -IncludeStdlib` и увидеть 47 FAIL'ов:

| Категория | Файлов | Корень | Фаза |
|---|---|---|---|
| `for-in unsupported iterator type` | 8 | `Iter[T]` element type hardcoded на `nova_int` | Ф.1 |
| `int as char` запрещён | 4 | spec D54 строгий, нужно relax для compile-time literals | Ф.7 |
| `if condition must be bool` | 3 | infer-fallback для unknown obj_ty (generic protocol method, cross-file Duration, tuple destructure return) | Ф.5 + tuple-tracking |
| `anonymous record literal without spread` | 2 | D55 coercion-context не достигает sql.nv / cron.nv (требует tuple/match flow) | мелкий gap |
| `nova_str API gap` (strip_suffix, ...) | ~5 | методов нет в registry | std/runtime/string.nv extension |
| Cross-module type не объявлен (HashMap, Duration, Version) | ~10 | без Ф.5 cross-file файлы не видят чужие decls | Ф.5 |
| LINK fail std/runtime/* | 6 | library-only файлы не должны идти через cl как exe | run_tests.ps1 issue |

---

## Архитектурные точки кода

Все изменения — в `compiler-codegen/src/codegen/emit_c.rs` если не
указано иное.

### Реестры в CEmitter (полезно для Ф.1/Ф.5)

- `record_schemas: HashMap<String, HashMap<String, String>>` — record name → field → C type.
- `sum_schemas: HashMap<String, HashMap<String, Vec<String>>>` — sum-тип → variant → field types.
- `effect_schemas: HashMap<String, HashMap<String, (Vec<String>, String)>>`.
- `method_overloads: HashMap<(String, String), Vec<MethodSig>>` — user-method registry.
- `external_registry: ExternalRegistry` — built-in opaque типы (StringBuilder/WriteBuffer/ReadBuffer/char) + str.
- `from_targets`, `into_targets`, `try_from_targets`, `try_into_targets` — D73.
- `iter_returns: HashMap<String, String>` — `coll_type → IterT_name` (Plan 06).
- `tuple_element_types: HashMap<String, Vec<String>>` — для `t.0` / `t.1` access.
- `array_element_types: HashMap<String, String>` — для `xs[i]`.
- `lazy_consts: HashSet<String>` — Plan 14 Ф.2.
- `record_field_fn_sigs: HashMap<(String, String), (Vec<String>, String)>` — Plan 14 Ф.4.
- `user_fn_sigs: HashMap<String, (Vec<String>, String)>` — Plan 14 Ф.3.
- `emitted_fn_thunks: HashSet<String>` — Plan 14 Ф.3 dedup.
- `fn_param_sigs: HashMap<String, (Vec<String>, String)>` — closure-call signature для `f(x)` через NOVA_CLOS_CALL.

### Ключевые методы

- `emit_expr(&mut Expr) -> Result<String, String>` — главный emit; возвращает C-выражение (может эмитить side-statements через `self.line(...)`).
- `infer_expr_c_type(&Expr) -> String` — type infer; **fallback `nova_int`**, что ломает strict bool-check.
- `emit_call(func, args)` — главный диспатч вызовов.
- `emit_call_with_trailing(func, args, trailing)` — wrapper для trailing-block.
- `check_bool_condition_at(cond_ty, ctx, span)` — strict bool-check с line:col.
- `emit_lazy_const(name, ty_c, value)` — Plan 14 Ф.2.
- `emit_free_fn_value(name)` — Plan 14 Ф.3.
- `emit_method_value(obj, method)` / `emit_method_value_typed(...)` — Plan 11 Ф.4 method values.
- `emit_interpolated_str(parts)` — Plan 17 Ф.4.

### Closure runtime (`compiler-codegen/nova_rt/nova_rt.h:23-39`)

```c
typedef struct { void* fn; void* env; } NovaClosBase;
typedef struct { nova_fn_vi fn; void* env; } NovaClos_vi;   // void env, int -> int
typedef struct { nova_fn_ii fn; void* env; } NovaClos_ii;   // env, int -> int
typedef struct { nova_fn_ib fn; void* env; } NovaClos_ib;   // env, int -> bool
typedef struct { nova_fn_iii fn; void* env; } NovaClos_iii;
typedef struct { nova_fn_vii fn; void* env; } NovaClos_vii;
#define NOVA_CLOS_CALL_xx(f, args) ...
```

Helpers: `Self::clos_struct_name(param_tys, ret_ty)`, `Self::clos_fn_ty(...)`, `Self::clos_call_macro(...)`.

### Bootstrap-particularity: flat var_types (нет scope!)

`var_types: HashMap<String, String>` — глобальная для всего файла. Local
let-bindings перетирают global names и не восстанавливаются. Тесты Ф.3
обошли это через `top_inc`/`top_dbl` (а не `inc`/`dbl`).

---

## Как продолжить Ф.1 (Iter[T] element-type)

**Корень:** `emit_c.rs` — for-loop с `Iter[T]`-collection захардкожен на
`NovaOpt_nova_int = X.next(it)`. Нужно:

1. Перед эмитом for-body — вычислить `T` элемента итератора:
   - Если `iter_expr_type` это `Nova_<X>*` — найти `Nova_<X>_method_next` в method_overloads, взять return-type, извлечь `T` из `NovaOpt_<T>`.
   - Для `s.chars()` (`Iter[char]`), `s.bytes()` (`Iter[byte]`) — спецтаблица.
2. Заменить хардкод `NovaOpt_nova_int = ... .next(it)` на `NovaOpt_<T> opt_<n> = Nova_<X>_method_next(it);`.
3. Loop-body binding должен иметь тип `T`, не `nova_int`.

**Файлы:**
- `compiler-codegen/src/codegen/emit_c.rs` — найти `for-in: unsupported iterator type` и переписать ~80 строк.

**Тесты:**
- `nova_tests/syntax/for_iter_typed.nv` (новый) — for x in `Iter[char]`,
  `Iter[(int,str)]`, `Iter[bool]`.
- Smoke: после Ф.1 должны компилироваться `std/collections/set.nv`,
  `std/collections/vec.nv`, `std/collections/range.nv` (step_by).

См. секцию **Ф.1** в `docs/plans/14-stdlib-codegen-gaps.md`.

---

## Как продолжить Ф.7 (int as char)

**Корень:** spec D54 строгий — `0x41 as char` запрещён, нужно
`char.try_from(0x41)?` (которое требует `Fail` в сигнатуре).
Для compile-time-known литералов это шум.

**Релакс:** в type-checker (`compiler-codegen/src/types/checker.rs`)
для `as`-cast: если LHS — `IntLit n` где `n ∈ 0..0x10FFFF` исключая
`0xD800..0xDFFF` — разрешить без runtime-check.

**Файлы:**
- `spec/decisions/03-syntax.md` D54 — добавить exception для
  compile-time literals.
- `compiler-codegen/src/types/checker.rs` или место где обрабатывается
  AsCast — ~10 строк.

**Тесты:**
- `nova_tests/syntax/as_cast_char_literal.nv` (новый, ~5 тестов).

См. секцию **Ф.7**.

---

## Как продолжить Ф.5 (cross-file resolution)

**Самая большая задача.** Корень — single-file compile mode, std/*.nv
не подгружается автоматически. `import std.collections.HashMap` парсится,
но codegen не находит type/method.

**Архитектура:**
- При parse'е main file — собрать список `import std.X.Y` декларацией.
- Module resolver: загрузить .nv по path (relative к workspace),
  парсить, добавить в текущую compilation-unit, unique-by-module-path.
- Топологический order для глубоких зависимостей (`hmac → sha256 → write_buffer`).

**Альтернатива (cheap):** оставить как known-limitation, документировать
что multi-file тестируется через interp (`nova-codegen run`).

См. секцию **Ф.5** — это **последняя по порядку** в рекомендованном
плане Ф.1 → Ф.7 → Ф.6 → Ф.5.

---

## Workflow для агента

1. **Читать** `docs/plans/14-stdlib-codegen-gaps.md` — главный источник.
2. **Проверять текущее состояние** через `run_tests.ps1` (без флагов = nova_tests, с `-IncludeStdlib` = + std).
3. **Не ломать nova_tests** — после любой правки прогнать nova_tests, ожидать `88/88 PASS`.
4. **Каждая фаза** = отдельный коммит (см. правило в `docs/project-creation.txt`).
5. **После каждой фазы** обновить:
   - `docs/plans/14-stdlib-codegen-gaps.md` — статус ✅/частично, retro.
   - `docs/plans/README.md` — статус plan 14.
   - `docs/project-creation.txt` — entry с описанием.
   - `docs/simplifications.md` — что упростилось в use-site.
   - `d:/Sources/nova-lang-private/discussion-log.md` — retrospective.
6. **Коммит**: 3 коммита (codegen + tests + docs sync) + 1 в private.

## Memory entries для агента

В `~/.claude/projects/d--Sources-nova-lang/memory/` есть:
- `feedback_discussion_log.md` — добавлять в private discussion-log после содержательных сессий.
- `feedback_project_docs.md` — обновлять project-creation.txt + simplifications.md после крупных задач, коммит per task.
- `project_codegen_status.md` — текущий pass rate.
- `feedback_concurrency_tests.md` — assert(sum) недостаточно для concurrent кода.
- `feedback_revolutionary_changes.md` — выбирать правильное решение, не минимальное.
- `feedback_third_party_libs.md` — minicoro/GC не трогать.
- `project_promts_dir.md` — этот каталог.

## Polezные команды

```powershell
# Прогон всех nova_tests
d:\Sources\nova-lang\run_tests.ps1

# Прогон + std
d:\Sources\nova-lang\run_tests.ps1 -IncludeStdlib

# Один файл
d:\Sources\nova-lang\run_tests.ps1 -Filter "const_complex"

# Сборка codegen
cd d:\Sources\nova-lang\compiler-codegen ; cargo build

# Прямой compile одного файла (для дебага codegen-ошибок)
d:\Sources\nova-lang\compiler-codegen\target\debug\nova-codegen.exe compile <path.nv>
```

## Spec для контекста

- `spec/syntax.md` — общий синтаксис.
- `spec/decisions/03-syntax.md` — D44 (числовые литералы + interp), D38
  (массивы), D54 (as/is), D58 (Iter[T]), D69 (variadic), D83 (keywords).
- `spec/decisions/02-types.md` — D52 (newtype/alias/sum), D53 (protocol
  как kind-token), D55 (coercion).
- `spec/decisions/08-runtime.md` — D26 prelude, D73 From/Into, D81
  (assert), D82 (external).
- `spec/open-questions.md` — Q-вопросы (большинство закрыто Plan 17).

---

## Баги/гитчи в codegen, которые я обнаружил

1. **Flat var_types** (нет scope) — local `let dbl = ...` затмевает
   global `fn dbl`. Тесты обходят через имена-suffix (`top_inc`,
   `top_dbl`). Real fix — отдельная задача.

2. **Tuple destructure не передаёт типы элементов через fn-return.**
   `let (a, b) = parse(s)` где `parse → (str, str)` — `a` и `b`
   получают default `nova_int` тип. Корень — `parse` имеет return
   `Tuple([str, str])`, но `var_types[a]` = ... не записывается с
   нужной точностью.

3. **str-методы dispatch.** `runtime_registry.rs` хардкоден; emit
   использует `str_method_to_rt(method) -> Option<&'static str>`
   (для C-name); infer использует `str_method_ret_type(method)`
   (для return-C-type). Я добавил вторую map в Plan 14 std-fix.

4. **Generic K=void* не разрешает protocol-методы.** В `hashmap.nv:299`
   `k.eq(key)` где K — generic — codegen не знает sig (требует Ф.5).

---

## Вопросы для агента (если нужны решения)

1. **Ф.5 cheap или full?** Полный (parse/resolve/topo) — ~500 строк,
   неделя. Cheap (документировать как known limit) — 1 час. Решение —
   считать Ф.5 cheap и заняться Ф.1/Ф.7/Ф.6 которые имеют чёткий
   action.

2. **Ф.7 spec change.** Релакс D54 — нужен ли D-блок «Numeric literal
   exception in `as`-cast»? Или просто edge-case в существующем D54?

3. **Tuple-destructure type tracking** — отдельная задача? Не входит
   в Plan 14, но блокирует url.nv (и потенциально другие).

---

## Финальный prompt для агента

> Продолжить Plan 14 (`docs/plans/14-stdlib-codegen-gaps.md`).
> Закрыты Ф.2, Ф.3, Ф.4. Остаются Ф.1 (Iter[T] element-type, ~80 строк,
> блокирует 8 std-файлов), Ф.7 (int as char literal relax, ~10 строк,
> блокирует 4 файла), Ф.6 (D69 variadic, ~150 строк), Ф.5 (cross-file,
> ~500 строк или cheap).
>
> Рекомендуемый порядок: Ф.7 → Ф.1 → Ф.6 → (Ф.5 опционально cheap).
>
> Прежде чем начать, прочитать:
> - `docs/promts/continue-plan-14.md` (этот файл).
> - `docs/plans/14-stdlib-codegen-gaps.md` (план).
> - `docs/project-creation.txt` (последний entry — Plan 14 Ф.2 + Ф.4).
>
> Не ломать nova_tests (88/88 baseline). После каждой фазы — отдельный
> коммит (codegen + tests + docs sync) + private discussion-log.
