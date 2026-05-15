// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 48: Monomorphization — generic functions без type-erasure

> **Создан 2026-05-14. Переписан 2026-05-15** (с «erasure + adapters» на
> мономорфизацию — это разница между «как в TS без JIT» и «как в Rust»).
>
> **СТАТУС:** Ф.0-Ф.3 DONE — свободные функции, методы, замыкания,
> generic records/sum-types (полная мономорфизация). Ф.4 (within/race) —
> заблокирован spawn closure-capture багом (V2 followup); Ф.5 collections —
> pre-existing fails, не регрессировали Plan 48; Ф.6 — docs обновлены.
>
> **Регрессия:** 393/393 PASS на release-сборке (2026-05-15).
>
> **Followups:**
> - `[M-spawn-closure-capture-mono]` — в mono'д generic fn body,
>   `body fn() -> T` captured into spawn ctx, но spawn body использует
>   `body` без `_c->body` rewrite. Блокирует cancellation.nv (within/race).
> - `[M-mono-spawn-fwd-decls]` — mono эмитит новые spawn-body функции,
>   но их forward-decls не добавляются в spawn pre-scan. race[T=nova_str]
>   падает на `_nova_spawn_3` undeclared.
> - `[M-random-effect-codegen]` — `type Random effect` теперь объявлен
>   в handlers.nv, но `type Time effect` не объявлен — runtime Time
>   pre-registration (sleep/now/after) не совпадает с handlers.nv
>   методами (now_ms/now_ns). retry.nv заблокирован этим (не Plan 48).
>
> **Приоритет:** P1 — generic-codegen сейчас 100% type-erasure (всё
> `void*`); это перформанс-долг И корректностный долг (замыкания/массивы
> замыканий в generic-функциях вообще не кодогенерируются — Plan 47 Ф.5
> упёрлась именно сюда). Мономорфизация закрывает оба.
>
> **Предшественники:** нет жёстких.

---

## Зачем переписан план

Первая версия плана решала «замыкания в generic-функциях» через
**type-erasure + call-site adapters + uniform erased ABI** — то есть всё
остаётся `void*`, замыкание оборачивается в боксящий адаптер. Это
**упрощение**: получается «как в TypeScript, но без JIT, который вернул
бы перформанс». Permanent boxing overhead, indirect calls, никакого
инлайнинга.

Состояние индустрии (то, с чем нас просили сравнивать):

| | Стратегия | Перформанс generic'ов | Замыкания в generic'ах |
|---|---|---|---|
| **Rust** | Полная мономорфизация (`F: Fn`) + `dyn` opt-in | zero-cost, инлайнится | работают тривиально (конкретный тип) |
| **Go 1.18+** | GC-shape stenciling + dictionaries | indirect, но специализирован по shape | работают (uniform closure ABI) |
| **TypeScript** | Полная type-erasure | recovered JIT'ом в рантайме | работают (всё dynamic) |
| **Nova сейчас** | Полная type-erasure, **без JIT** | permanent `void*` overhead | **не кодогенерируются вообще** |

Nova сейчас — **строго хуже всех трёх**: erasure как в TS, но AOT — JIT
не вернёт перформанс; и при этом замыкания в generic'ах не работают
(хуже даже Go/TS, где работают).

Чтобы быть «не хуже Rust» — нужна **мономорфизация**. Это не «большой
мега-план в стороне», как утверждала v1: разведка codegen'а показала,
что erasure локализован (~150-200 строк, 2 функции `emit_generic_*_erased`),
инфраструктура мангления и `type_ref_to_c` готовы, а ленивая
инстанциация для builtin-generic'ов (`NovaOpt_<T>`, `NovaArray_<T>`) уже
**существует** — её паттерн обобщается на user-generic'и. Memory
`feedback_revolutionary_changes`: выбираем правильное решение, не
минимальное.

**Побочный эффект, который и был исходной целью:** при мономорфизации
замыкания в generic-функциях работают **тривиально и zero-cost** —
`within[int]` это настоящая функция с параметром `body fn() -> nova_int`,
прямой вызов, инлайнится. Никаких адаптеров, боксинга, ABI-кастов. Старый
план 48 решал симптом; новый убирает причину.

---

## Проблема (точно)

Generic-функции кодогенерируются через **полную type-erasure**: каждый
type-параметр `T` → `void*`, тело эмитится один раз
(`emit_generic_fn_erased`, emit_c.rs:4039 — *«All type parameters map to
void*»*; `generic_fns` HashSet, emit_c.rs:329 — *«Generic functions are
emitted with void* erasure; call sites must box/unbox»*). Нет worklist'а,
нет кэша инстанциаций, нет per-concrete-type кода. TurboFish парсится и
**сразу выбрасывается** (emit_c.rs:5508 — *«type_args не нужны на этом
этапе»*).

Следствия:
1. **Замыкания в generic-функциях не работают** — вызов closure-параметра
   эмитится как `nova_fn_<name>()` (именованная функция, а не closure-call);
   `[]fn()->T` эрейзится в `void*`, `.len()`/`[i]`/`for-in` не резолвятся.
   (Plan 47 Ф.5 — `within`/`race` — упёрлась сюда; `std/concurrency/retry.nv`
   с тем же паттерном вообще никогда не codegen-проверялся.)
2. **Перформанс-долг** — каждый generic вызов боксит/un-боксит через
   `void*`, indirect calls, ноль инлайнинга. Permanent, JIT'а нет.
3. **Корректностные хаки** — generic record `Box[T]` это `Nova_Box*` с
   `void*`-полями (emit_c.rs:4050); `f64` через generic ломается ABI;
   `emit_generic_fn_erased` и `emit_generic_method_erased` эрейзят
   массивы по-разному (рассинхрон).

Что **уже** мономорфизировано (доказательство, что подход совместим с
кодовой базой): `NovaArray_<T>` (runtime-макрос `NOVA_ARRAY_DECL` +
codegen `register_novaopt_decl`) и `NovaOpt_<T>` (ленивый typedef в
`novaopt_typedefs_buf`, splice в маркер). Это per-concrete-type
инстанциация — но только для builtin'ов. Plan 48 обобщает механизм.

---

## Архитектурное решение: instantiation worklist (Rust-style)

Полная мономорфизация generic-функций, методов и типов — каждая
комбинация `(generic_item, concrete_type_args)`, реально использованная
в программе, эмитится как отдельная конкретная C-сущность.

### Алгоритм

1. **Discovery.** Codegen-pass, встречая ссылку на generic-item с
   конкретными type-args (call-site, type-annotation, turbofish,
   inference из аргументов) — резолвит type-args в конкретные C-типы и
   кладёт `(item_id, [c_type_args])` в **worklist инстанциаций**
   (с дедупом через `HashSet<(item_id, Vec<c_type>)>`).
2. **Type-arg resolution.** На call-site type-args берутся: (a) из
   turbofish `within[int](...)` если есть; (b) inference из типов
   аргументов (`within(1000) fn() -> int {...}` → `T = int` через уже
   существующий `infer_expr_c_type`); (c) из контекста (return-type
   annotation). Turbofish перестаёт «выбрасываться» — он источник истины.
3. **Drain + emit.** После основного pass'а — слить worklist: для каждой
   `(item, type_args)` эмитить мономорфную копию с подстановкой
   `T → concrete_C_type` в сигнатуре И теле. Имя — мангленное:
   `nova_fn_within__nova_int`, `Nova_Box__nova_int`, и т.д.
4. **Fixpoint.** Инстанциация `within[int]` может породить ссылки на
   `Option[int]`, `Box[int]`, другой generic — добавляются в worklist.
   Гонять до пустого worklist'а. Множество типов в программе конечно →
   терминируется (кроме polymorphic recursion — см. Риски R3).
5. **Call-site rewrite.** Вызовы эмитят мангленное имя инстанциации.

### Что это даёт

- **Замыкания в generic'ах — тривиально.** `within[int]` имеет
  `body fn() -> nova_int` — реальная сигнатура, `body()` это обычный
  closure-call через `fn_param_sigs` с конкретными типами (механизм уже
  есть для не-generic функций). `[]fn()->T` → `NovaArray` конкретного
  closure-типа. Никаких адаптеров.
- **Zero-cost** — прямые типизированные вызовы, C-компилятор инлайнит.
  Паритет с Rust.
- **Generic records/sum-types** монолитны: `Box[int]` → реальный
  `struct Nova_Box__nova_int { nova_int value; }`, не `void*`-поля.
- **Уходит целый класс хаков** — `void*`-боксинг на call-site'ах,
  `erased_type_ref_c`, рассинхрон массивов между двумя эмиттерами.

### Почему мономорфизация, а не Go-style (GC-shape + dictionaries)

Go-подход (типы с одинаковым «GC shape» делят инстанциацию + рантайм-
словарь для type-специфичных операций) — production-grade, но **сложнее
в реализации**, чем полная мономорфизация: нужен shape-анализ,
dictionary-passing ABI, рантайм-диспетч по словарю. Полная мономорфизация
концептуально проще И даёт лучший перформанс (Rust-grade). Для bootstrap-
компилятора без JIT это правильный выбор. Go пошёл на GC-shape ради
**времени компиляции и размера бинарника** — для Nova на текущем масштабе
это преждевременная оптимизация (см. Риск R1 — code bloat — и его V2-митигацию).

### Почему не оставить erasure как `dyn`-fallback (Rust имеет оба)

Rust имеет `dyn Trait` — явный erased вариант для рантайм-полиморфизма.
Nova **может** захотеть аналог позже (`dyn`-замыкания в коллекциях
разнородных типов), но это **отдельная фича** (D-decision про
trait-objects), не Plan 48. Plan 48 = убрать *неявную* erasure, которая
сейчас единственный режим. Явный opt-in erased режим — out of scope,
см. ниже.

---

## Фазы

### Ф.0 — Type-arg resolution на call-site

- TurboFish: перестать выбрасывать `type_args`; резолвить в C-типы через
  `type_ref_to_c`.
- Inference: для generic-вызова без turbofish — вывести type-args из
  типов аргументов. Сопоставление `param.ty` (с type-params) против
  `infer_expr_c_type(arg)`. Базовый unification — без полноценного HM,
  достаточно «param это голый `T` → `T = тип arg`», «param это `[]T` →
  `T` = элемент», «param это `fn(..)->T` → `T` = ret замыкания».
- Generic records в type-annotation (`let b Box[int] = ...`) и generic
  return types — тоже источники type-args.
- Результат: на каждой generic-ссылке известен `Vec<c_type>` для type-params.

### Ф.1 — Instantiation worklist + мангление

- `MonoKey = (ItemId, Vec<CType>)`; `worklist: Vec<MonoKey>`;
  `instantiated: HashSet<MonoKey>` (дедуп).
- Мангление: `nova_fn_<name>__<t0>__<t1>`, `Nova_<Record>__<t0>`,
  стабильное и коллизие-безопасное (санитизация C-типов в идентификаторы).
- Discovery встроен в codegen-pass: каждая резолвнутая generic-ссылка
  (Ф.0) → `worklist.push` если не в `instantiated`.

### Ф.2 — Мономорфная эмиссия функций и методов

- `emit_generic_fn_erased` / `emit_generic_method_erased` **заменяются**
  на `emit_monomorphized_fn(item, type_args)`: подстановка
  `type_param → concrete C-type` в `var_types` перед эмиссией тела,
  реальная сигнатура, реальный return.
- Closure-параметры регистрируются в `fn_param_sigs` с **конкретными**
  типами (а не erased) → `body()` идёт через обычный closure-call.
  Это и есть «замыкания в generic'ах» — выпадает бесплатно.
- `[]T` → `NovaArray_<concrete>` через существующий
  `register_novaopt_decl`-механизм; `[]fn()->T` → массив конкретного
  closure-типа.
- **Protocol-bounded generics** (`fn f[T: SomeProtocol](x T)`): при
  мономорфизации `f[Concrete]` методы протокола резолвятся **статически**
  по `Concrete` — это static dispatch вместо vtable, **лучше** текущей
  erasure (паритет с Rust `fn f<T: Trait>`). Protocol-bound — это
  ограничение на discovery (какие `Concrete` допустимы), не на эмиссию.
- Worklist drain'ится в фикспойнт; инстанциации эмитятся в
  `deferred_impls` (после forward-decls).

### Ф.3 — Мономорфные generic records / sum-types

- `type Box[T] {...}` → `Nova_Box__<T>` реальная struct per инстанциация
  (поля с подставленным `T`), forward-decl + typedef.
- Generic sum-types (`type Result[T,E]` уже частично спец-кейснут — свести
  к общему механизму).
- `record_schemas` / `sum_schemas` — расширить ключ инстанциацией либо
  завести `mono_record_schemas`.
- **Staging:** если объём великоват — Ф.3 можно вынести в V2; Ф.0-Ф.2
  (функции/методы + замыкания) самодостаточны для разблокировки
  Plan 47 Ф.5 и `retry.nv`. Решить по факту объёма после Ф.2.

### Ф.4 — Разблокировать Plan 47 Ф.5: `within` / `race`

- Восстановить `std/concurrency/cancellation.nv`: `within[T]`, `race[T]`
  — теперь компилируются (мономорфные, замыкания работают).
- `nova_tests/concurrency/cancellation_stdlib_test.nv`.
- Снять `[M-race-closure-array]` из simplifications.md.
  (`[M-within-error-conflation]` — отдельная ось, Plan 49.)

### Ф.5 — Валидация на боевой stdlib + протоколы

- `std/concurrency/retry.nv` (`RetryPolicy @execute[T,E](body fn()...)`)
  — generic-метод с closure-параметром, никогда не codegen-проверявшийся.
  Написать `retry_test.nv` — покрывает мономорфный method-путь.
- `std/collections/*` — пройтись по generic-коллекциям, убедиться что
  мономорфизация не сломала (там generic records).
- **Protocol-объекты (vtable dispatch) — не сломать.** Если в языке есть
  значения с erased-типом за протоколом (`[]SomeProtocol`, vtable-диспетч)
  — они **ортогональны** generic-мономорфизации (это отдельный механизм,
  не type-параметры). Ф.5 явно прогоняет protocol-using тесты, чтобы
  убедиться: мономорфизация generic'ов не задела protocol-object путь.
  Protocol-**bounded** generics (`f[T: P]`) наоборот — становятся лучше
  (static dispatch, Ф.2).

### Ф.6 — Regression + perf-sanity + docs + spec

- Полный `nova test` (release) — без новых FAIL. **Особое внимание:**
  generic-heavy тесты, рекурсивные generic'и, generic record'ы.
- Perf-sanity: микробенч generic-HOF до/после — подтвердить что прямые
  вызовы вместо `void*`-боксинга (не обязан быть строгий бенч, но
  зафиксировать что не регресс).
- spec `02-types.md`: D-decision — «generics: full monomorphization»
  (стратегия, мангление, отличие от erased `dyn` future-work).
- `project-creation.txt` + `simplifications.md`: закрытие; снять
  `[M-race-closure-array]`; зафиксировать code-bloat trade-off.
- discussion-log.

### Ф.7 — Production hardening (2026-05-15)

> Acceptance criteria из плана не выполнены в V1 — есть hybrid: mono
> работает для свободных fn + instance методов, но всё что не выводится
> тихо fallback'ит в erased path. Plan §R5 требовал «не тихий
> void*-fallback». Ф.7 закрывает разрыв plan vs реальность.

**Ф.7.1 — Method-call inference в `infer_expr_c_type`.**
- `let r = c.method(...)` для generic-метода типизирует `r` как `void*`
  потому что Method-branch в `infer_expr_c_type` (emit_c.rs:~8624) не
  делает Plan 48 mono inference. Workaround в текущих тестах: явная
  `let r int = ...` annotation.
- **Fix:** Method-branch должен вызывать `resolve_mono_type_args` +
  `apply_type_subst_to_ref` к `fn_decl.return_type` (зеркало free-fn
  пути на 12450-12478). Источник fn_decl — `mono_method_decls`.
- Закрывает `[M-mono-method-call-inference]` из simplifications.md.

**Ф.7.2 — Static generic methods через sentinel routing.**
- `Type.method[T]()` (без instance receiver) — sentinel
  `__mono_method__T__m` попадает в C-output как линкуемое имя
  (undefined symbol на линковке). Sentinel detection в `emit_call`
  работает для instance, но static-путь идёт мимо.
- **Fix:** Расширить sentinel-detection branch в `emit_call`
  (около emit_c.rs:8621) на static-method dispatch path — фильтровать
  `__mono_method__` префикс и роутить в mono pipeline.
- Закрывает `[M-mono-static-methods]`.

**Ф.7.3 — Cannot-infer → понятная error, не silent erasure.**
- `resolve_mono_type_args` возвращает `Err("cannot infer T...")`, но
  call site (emit_c.rs:9226) ловит и делает silent `register_erased_instance`.
  Plan §R5 требовал противоположного.
- **Fix:** Заменить `Err(_e) => fallback` на `Err(msg) => return Err(msg)`
  с уточнённым сообщением (имя fn, какой type-param не выведен,
  hint про turbofish `fn_name[T](...)`).
- Закрывает `[M-mono-error-not-fallback]`.
- **ЗАБЛОКИРОВАНО Ф.3.** Сейчас erasure fallback используется для cases
  где T в generic-record param (`fn box_get[T](b Box[T]) -> T`) и
  `infer_type_param_binding` не извлекает T из `Nova_Box*` (потому что
  generic record erased до mono'д specialization). Без Ф.3 убрать
  fallback = ломать `nova_tests/types/generics.nv` (box_get + box использует
  arg-based inference, но Box[T] arg type не несёт T info). Решение
  парой: Ф.3 (generic records mono) + Ф.7.3 (erasure → error).
- Ф.7.5 (param-position в diag) уже улучшил message — пользователи
  видят понятную инструкцию когда turbofish помогает. Это V1 partial fix.

**Ф.7.4 — Удалить erased-эмиттеры (acceptance criteria #10).**
- После Ф.7.3 erasure-fallback больше нет → `emit_generic_fn_erased`
  (4864), `emit_generic_method_erased` (4204), `erased_type_ref_c`
  (4166), `register_erased_instance` (4470), `generic_fns: HashSet`
  (329) становятся dead-code.
- **Fix:** Удалить целиком. Ожидаемое сокращение: −400 LOC.
- Acceptance criteria «Erased-эмиттеры удалены» — закрывается.
- **ЗАБЛОКИРОВАНО Ф.7.3, которое ЗАБЛОКИРОВАНО Ф.3.**

**Ф.7.5 — Param-position в diagnostic.**
- Текущее сообщение «cannot infer type argument T» не указывает в
  каком параметре T используется. LLM-фрустрирующее.
- **Fix:** Найти param с `T` в типе, добавить hint
  «type argument `T` appears in parameter `<n>` of type `<ty>`».

**Ф.7.6 — MONO_INSTANTIATION_DEPTH_LIMIT через CLI/env.**
- Magic-number в коде; spec/CLI не упоминают. R3 говорил «понятная
  compile-error при превышении».
- **Fix:** CLI flag `--mono-depth=N` (default 64) через nova-cli
  arguments → BuildOpts → emit_c.rs. Span к месту первой
  инстанциации в diagnostic.

**Ф.7.7 — Protocol-bounded generic dispatch (CRITICAL для collections).**
- Generic-параметр с bound (`K: Hashable`, `T: Ord`) — методы протокола
  внутри generic body должны резолвиться через **mono substitution**: для
  `f[K=str]` вызов `k.eq(key)` → `str.eq(...)` (static dispatch).
- Сейчас: bound-method calls внутри generic body fallback'ят на default
  return-type (часто `nova_int` для unknown), что ломает control flow:
  `if k.eq(key) { ... }` → "if condition must be bool, got nova_int".
- **Pre-existing impact:** std/collections/hashmap.nv:326 (`k.eq(key)`),
  set.nv (`item.eq(...)`), priority_queue.nv (`a.cmp(b)`), linkedlist
  (`item == prev`). 8 файлов CC-FAIL в `std/collections` regression
  (см. Ф.5 chunk session 2026-05-15).
- **Fix:** В emit_call / infer_expr_c_type, когда method call на receiver
  типа = type-param K, и K имеет bound P — резолвить method через
  `protocol_method_table[P][method]`. В mono'д контексте substitute
  K → concrete C-type и звать concrete method (static dispatch).
- **Acceptance:** std/collections type-check 8 FAIL → 50/50 PASS;
  hashmap/set/lru — реально компилируются в C без CC-FAIL.
- **Это первый production-blocker:** без Ф.7.7 collections не работают,
  что несовместимо с «production-ready Nova». Plan §«Protocol-bounded
  generics» обещал static dispatch — обещание не выполнено.

**Ф.7 — что НЕ входит (по-прежнему V2):**
- Spawn closure-capture в mono ([M-spawn-closure-capture-mono])
- Mono-spawn forward-decls ([M-mono-spawn-fwd-decls])
- []fn()->T внутри generic
- cancellation.nv within/race (заблокирован spawn-баг'ами)

---

### Ф.3 — Generic records/sum-types mono (2026-05-15, добавлено как V1 production work)

> **Разблокирует Ф.7.3, Ф.7.4, Ф.7.7** — все три упираются сюда.
>
> Корень проблемы: generic records/sum-types сейчас emitted as erased
> (`void*` fields для type-params). `k` из pattern `Occupied { key: k }`
> имеет тип `void*`, поэтому `k.eq(key)` → branch 4b → `NULL`.

**Цель**: `HashMap[str, int]` → конкретный `Nova_HashMap__nova_str__nova_int`
struct per (K,V) instance. `Slot[str, int]` → конкретный
`Nova_Slot__nova_str__nova_int` union. Методы на generic records
mono'дятся аналогично free-fn mono.

**Approach**:

**A. Новые поля CodegenState:**
- `generic_type_templates: HashMap<String, TypeDecl>` — хранит template вместо emit
- `generic_type_instance_info: HashMap<String, (String, Vec<String>)>` — mangled → (base, args)
- `generic_type_worklist: Vec<(String, Vec<String>, String)>` — (base, args_c, mangled)
- `emitted_generic_type_instances: HashSet<String>` — уже emitted
- `generic_type_methods: HashMap<String, Vec<FnDecl>>` — методы per generic type template

**B. `emit_type_decl` для generic**: хранит template, return Ok(()).
Регистрирует методы из FnDecl scan в `generic_type_methods`.

**C. `type_ref_to_c` для `HashMap[str, int]`**:
- compute mangled: `Nova_HashMap__nova_str__nova_int`
- if not emitted → enqueue in worklist
- register in `generic_type_instance_info`
- return `"Nova_HashMap__nova_str__nova_int*"`

**D. `drain_generic_type_worklist`**: loop until empty.
- emit_generic_type_instance(record) → concrete struct + record_schemas
- emit_generic_type_instance(sum) → concrete tag enum + union + sum_schemas +
  constructor fns + `record_variant_field_types` / `record_variant_field_order`
- both: использует `current_type_subst` = {K→nova_str, V→nova_int}
- forward-decl `typedef struct NAME NAME;` сначала (до union body)

**E. Методы на generic type instances в `emit_call`**:
- В "5. User-defined method call": если `rt` not in method_overloads,
  lookup в `generic_type_instance_info` → base+args
- Найти FnDecl метода в `generic_type_methods[base]`
- Construct type_subst = zip(base_generics, args)
- Register + emit mono method instance
- Call: `Nova_HashMap__nova_str__nova_int_method_find_slot(obj, args)`

**F. Drain integration**: drain_generic_type_worklist() вызывается:
1. Каждый раз после drain mono_worklist (потому что mono'd fn bodies
   могут порождать новые generic type usages)
2. Forward-decls для generic type instances в pre-pass (emit_module)

**Acceptance**: hashmap/priority_queue CODEGEN-FAIL → PASS.

**Dependencies**: нет жёстких зависимостей. Ф.7.3+7.4+7.7 зависят отсюда.

**Риски**:
- Circular generic types (`type Tree[T] | Leaf | Node { left Tree[T], right Tree[T] }`)
  → forward-decl перед body решает
- Recursive monomorphization (`Option[Option[T]]`) → `emitted_generic_type_instances` guard
- Methods на generic records потребуют receiver-type extraction из C name

---

## Что НЕ входит

- **Явный `dyn`-режим** (Rust `dyn Trait` / erased runtime-полиморфизм
  для гетерогенных коллекций) — отдельная фича/D-decision. Plan 48 убирает
  *неявную* erasure; *явный* opt-in erased — future work.
- **GC-shape sharing** (Go-style дедуп инстанциаций по layout ради
  размера бинарника) — оптимизация code-bloat'а; V2, только если bloat
  станет реальной проблемой (см. R1).
- **Раздельная компиляция** инстанциаций — bootstrap компилирует
  whole-program, worklist видит всю программу. Incremental — не сейчас.
- **Polymorphic recursion** без границы — см. R3; V1 ставит лимит глубины
  с понятной ошибкой.
- **Cancel-throw routing** — Plan 49, ортогонально.

---

## Риски

**R1 — Code bloat.** Полная мономорфизация дублирует код per type
(проблема Rust). Митигации: (a) дедуп идентичных инстанциаций по
MonoKey — `within[int]` эмитится один раз сколько бы вызовов; (b)
большинство Nova-программ используют конечный небольшой набор типов;
(c) V2-оптимизация — GC-shape sharing (Go-style) если bloat измеримо
кусается. V1 принимает bloat как Rust — это правильный trade-off
(перформанс > размер для AOT-языка без JIT).

**R2 — Объём работы.** Больше исходного erasure-плана (~1130 → ~1900
LOC). Митигация: Ф.0-Ф.2 (функции+методы+замыкания) — самодостаточный
срез, разблокирует Plan 47 Ф.5 и `retry.nv`. Ф.3 (generic records) можно
вынести в V2 если объём не влезает. Erasure-эмиттеры удаляются целиком —
часть «работы» это чистое удаление.

**R3 — Polymorphic recursion → бесконечный worklist.** `fn f[T](x T) =>
f[Box[T]](...)` порождает `f[T]`, `f[Box[T]]`, `f[Box[Box[T]]]`, ∞.
Rust ловит это лимитом рекурсии инстанциаций. Митигация: счётчик глубины
инстанциации, при превышении — понятная compile-error «instantiation
depth exceeded (polymorphic recursion?)». Не молчаливый hang.

**R4 — Регрессия существующего generic-кода.** Erasure сейчас «работает»
для простых generic'ов (identity-like fns, generic records как `void*`).
Переход на мономорфизацию обязан сохранить их поведение. Митигация: Ф.6
regression — полный `nova test`; Ф.5 — боевая stdlib. Erasure-эмиттеры
не удалять пока мономорфные не проходят те же тесты (можно временно
держать оба, флаг — но цель убрать erasure).

**R5 — Type-arg inference неполнота (честно: V1 эргономически слабее
Rust).** Rust выводит type-args полноценным constraint-solver'ом —
turbofish нужен редко. V1 Plan 48 покрывает прямые случаи (голый
`T`-параметр, `[]T`, `fn(..)->T`, turbofish, annotation, return-context);
сложные (`T` выводится только из вложенного/обратного контекста) →
**понятная ошибка** «cannot infer type argument, use turbofish
`f[T](...)`», НЕ тихий `void*`-fallback и НЕ хуже-молча. Это явная
V1-граница: корректность полная (никакого UB), эргономика — на уровне
«turbofish иногда нужен», что строго лучше Go (где до 1.18 generic'ов
не было вовсе) и TS (где всё any), но слабее Rust. V2 — полноценный
bidirectional inference (отдельная под-фича, не блокер Plan 48).

**R6 — Взаимодействие с эффектами в сигнатуре.** Generic-функция с
эффект-row (`fn f[T]() Time Fail[T] -> T`) — мономорфизация по T не
должна ломать эффект-диспетч. Митигация: эффекты ортогональны type-params;
подстановка только в value-типах. Integration-тест в Ф.5.

**R7 — Не сломать protocol-object dispatch.** Если в языке есть
значения с erased-типом за протоколом (vtable-диспетч, `dyn`-аналог) —
мономорфизация generic'ов **не должна** их задеть: это отдельный
механизм (диспетч по vtable значения), не type-параметры функции.
Митигация: Ф.5 явно прогоняет protocol-using тесты. Protocol-**bounded**
generics (`f[T: P]`) — наоборот улучшаются (static dispatch). Риск
именно в том, чтобы случайно не «мономорфизировать» там, где задумана
runtime-полиморфная коллекция — discovery должен триггериться только
на реальных type-параметрах, не на protocol-типах-значений.

---

## Size estimate

| Компонент | LOC |
|---|---|
| Ф.0 — type-arg resolution (turbofish + inference) | ~300 |
| Ф.1 — worklist + мангление + discovery | ~250 |
| Ф.2 — мономорфная эмиссия fn/method + closures | ~400 (−150 удаление erasure) |
| Ф.3 — мономорфные generic records/sum-types | ~400 |
| Ф.4 — within/race восстановить + тесты | ~200 |
| Ф.5 — retry.nv + collections тесты | ~200 |
| Ф.6 — regression + perf-sanity + docs + spec | ~120 |
| **Итого** | **~1900** (Ф.3 ~400 выносимо в V2) |

---

## Acceptance criteria

- [ ] Generic-функция мономорфизируется per concrete type-args; вызов
      эмитит мангленное имя инстанциации, не `void*`-erased stub.
- [ ] Closure-параметр в generic-функции/методе вызывается как обычный
      typed closure-call (`body()` → корректный closure-call, не
      `nova_fn_body()`); zero adapter, zero boxing.
- [ ] `[]fn()->T` внутри generic-функции: `.len()`/`[i]`/`for-in`
      работают (массив конкретного closure-типа).
- [ ] Generic record `Box[T]` → реальный `struct Nova_Box__<T>` с
      типизированными полями (если Ф.3 в V1; иначе зафиксировано в V2).
- [ ] TurboFish — источник type-args, не выбрасывается; inference
      покрывает прямые случаи; непокрытое → понятная ошибка.
- [ ] Polymorphic recursion → compile-error с лимитом, не hang.
- [ ] `std/concurrency/cancellation.nv` (`within`/`race`) компилируется
      и проходит тесты; `[M-race-closure-array]` снят.
- [ ] `std/concurrency/retry.nv` покрыт `retry_test.nv` — проходит.
- [ ] Полный `nova test` (release) — без новых FAIL.
- [ ] Erased-эмиттеры (`emit_generic_fn_erased` / `_method_erased`)
      удалены; `generic_fns`-void*-путь убран.

---

## Связь

- [Plan 47](47-supervised-cancel.md) — Ф.5 (`within`/`race`) разблокируется
  здесь (компиляция).
- [Plan 49](49-cancel-throw-routing.md) — ортогонален; даёт `within`/`race`
  семантику без error-conflation. Plan 48 — компиляция, Plan 49 — семантика.
- [Plan 11](11-overloading-mangling.md) / [Plan 14](14-generics-option.md)
  — closure-инфраструктура (`NovaClosBase`, `fn_param_sigs`, thunk),
  мангление; `NovaOpt_<T>`/`NovaArray_<T>` ленивая инстанциация — паттерн,
  который Plan 48 обобщает с builtin'ов на user-generic'и.
- `std/concurrency/retry.nv` — generic-метод с closure-параметром,
  никогда не codegen-проверявшийся; Ф.5 закрывает пробел.
