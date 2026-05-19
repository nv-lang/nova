// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 48: Monomorphization — generic functions без type-erasure

> **Создан 2026-05-14. Переписан 2026-05-15** (с «erasure + adapters» на
> мономорфизацию — это разница между «как в TS без JIT» и «как в Rust»).
>
> **СТАТУС (2026-05-17):** В работе. Ф.0-Ф.3 DONE, Ф.6 DONE, Ф.7.5/7.7 DONE.
> Остаток: Ф.7.1, Ф.7.2, Ф.7.3→7.4, Ф.7.6, Ф.8, Ф.4 (spawn), Ф.5 (retry).
> Ф.4 возвращена в V1 — оба spawn-бага локализованы (~100 LOC), без упрощений.
> Ф.9 — method-param mono (`[U]` в method signature) — DONE 2026-05-17
> (Plan 63 Fix C followup, branch `plan-48-mpm`).
>
> **Регрессия:** 668 PASS / 2 FAIL (2 RUN-FAIL = Windows UAC os 740 baseline,
> identical to main) на release-сборке (2026-05-17, после Ф.9 method-param mono).
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

**Ф.8 — Встроенные методы примитивных типов: hash/eq/ord (D109)**

> Разблокирует `HashMap[int, int]`, `HashMap[str, int]` и любой
> generic код с protocol bound `Hashable` на примитивах.

Стандартные примитивы (`int`, `bool`, `f64`, `char`, `byte`, `str`)
получают автоматические методы от компилятора — без явных `.nv` деклараций:

| Тип | hash | eq | lt/le/gt/ge |
|---|---|---|---|
| `int`/`char`/`byte` | FNV-1a 8 байт | `==` | `<`/`<=`/`>`/`>=` |
| `bool` | 0 или 1 | `==` | — (нет порядка) |
| `f64` | FNV-1a bitwise | `==` (IEEE) | `<`/`<=`/`>`/`>=` |
| `str` | FNV-1a байты | memcmp | лексикографически |

**Реализация:**
- `nova_rt.h`: `nova_int_hash`, `nova_bool_hash`, `nova_f64_hash` (inline C).
- `emit_c.rs`: `prim_builtin_method(c_ty, method)` — перехватывает до
  общего resolver'а; для hash → C fn call, для eq/lt/... → inline оператор.
- `infer_expr_c_type`: распознаёт `hash` → `nova_int`, `eq/lt/...` → `nova_bool`
  для примитивных receiver'ов.

**Acceptance:** `nova_tests/modules/hashmap_basic.nv` — PASS с `HashMap[str,int]`
и `HashMap[int,int]`.

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

## Итоговый план закрытия (2026-05-16)

### Что сделано

| Фаза | Статус | Когда |
|---|---|---|
| Ф.0 — type-arg resolution | ✅ DONE | 2026-05-15 |
| Ф.1 — worklist + мангление | ✅ DONE | 2026-05-15 |
| Ф.2 — mono эмиссия fn/method/closure | ✅ DONE | 2026-05-15 |
| Ф.3 — generic records/sum-types mono | ✅ DONE | 2026-05-15 |
| Ф.6 — regression + docs | ✅ DONE | 2026-05-15 |
| Ф.7.1 — method-call inference в infer_expr_c_type | ✅ DONE | 2026-05-15 |
| Ф.7.2 — static generic methods sentinel routing | ✅ DONE | 2026-05-15 |
| Ф.7.3 — cannot-infer → error (не silent fallback) | ✅ DONE | 2026-05-15 |
| Ф.7.5 — param-position в diagnostic | ✅ DONE | 2026-05-15 |
| Ф.7.7 — protocol-bounded generic dispatch | ✅ DONE | 2026-05-16 |
| Ф.8 — встроенные hash/eq/ord для примитивов | ✅ DONE | 2026-05-15 |
| Ф.5 std/collections 10/10 | ✅ DONE | 2026-05-16 |
| Ф.7.4 partial — bare-variant mono inference + erased fallback | ✅ DONE | 2026-05-16 |
| Ф.7.6 — `--mono-depth=N` CLI flag через nova-cli → emit_c.rs | ✅ DONE | 2026-05-16 |
| Ф.4 — M-spawn-closure-capture-mono + M-mono-spawn-fwd-decls | ✅ DONE | 2026-05-16 |
| Ф.5 partial — NovaVtable_Time schema (now_ms/now_ns) aligned | ✅ DONE | 2026-05-16 |
| Ф.9 — method-param mono (`Wrapper[T] @map[U]` pattern) | ✅ DONE | 2026-05-17 |

### Что осталось — порядок работ

> **2026-05-16 финал:** все основные фазы Plan 48 закрыты.
> Оставшиеся хвосты — V2 / Plan 49 followup'ы (см. ниже).

#### Шаг 1 — Ф.7.4 partial: bare-variant mono + erased fallback ✅

Полное удаление erased-эмиттеров оказалось преждевременным: тест
`types/generic_record_mono` опирался на `emit_generic_method_erased` для
bare unit-variant references (`let r = Err2`), где T нельзя вывести из
конструктора в одиночку.

**Сделано (2026-05-16):**
- Добавлен `try_infer_variant_mono_args` — инференция mono-args для bare
  variant constructors с аргументами (`Ok2(42)` → Result2[nova_int]).
  Конструктор эмитится с mono'д именем, локальная переменная получает
  конкретный `Nova_Result2____nova_int*` тип, последующие `.method()`
  попадают в mono dispatch на line 9911.
- `emit_call`: для variant constructor вызовов используется mono имя
  когда try_infer_variant_mono_args возвращает Some, иначе erased.
- `infer_expr_c_type` для Call(Ident(variant)): возвращает mono тип
  когда инференция успешна, иначе erased.
- `is_generic_call` flag для arg-boxing: false для mono пути (концретные
  типы не требуют void*-boxing).
- `emit_generic_method_erased` оставлен как V1 fallback для unit variants
  без аргументов (`Err2`, `None` etc. внутри generic sum-типов) — там
  инференция T невозможна без usage-context propagation.

**V2 follow-up (Ф.7.4 final):** реализовать usage-context инференцию для
unit variants — анализ типов в let-биндингах + method-call args после
объявления. После — emit_generic_method_erased можно удалить полностью.

**Регрессия:** 411 PASS / 46 FAIL (baseline + Ф.4 smoke test, no new fails).

#### Шаг 2 — Ф.7.6: --mono-depth CLI flag ✅

Hardcoded depth limit (500) выведен в CLI флаг `--mono-depth=N` в командах
`build`, `test`, `test-build`. NOVA_MONO_DEPTH env var остался как fallback;
CLI приоритетнее. Error messages обновлены: указывают оба варианта override.

Изменения: CEmitter.mono_depth_limit + set_mono_depth_limit, TestBuildOpts
и TestAllOpts получили `mono_depth: Option<usize>`, прокинуто через все
cmd_* handler'ы.

**Регрессия:** 410 PASS / 46 FAIL (== baseline, без новых FAIL).

#### Шаг 3 — Ф.4: spawn + closure-capture в mono ✅

Два конкретных бага закрыты (commit f0b1551d7e2):

**[M-spawn-closure-capture-mono]:** Closure-call в emit_call для fn-typed
параметра использовал bare имя `body` вместо `_c->body` в spawn-body
контексте. Fix: новый helper `spawn_capture_access(name)` централизует
rewrite "name → _c->name/(*_c->name)"; applied в emit_call fn_param_sigs
branch (NOVA_CLOS_CALL_* macros + arbitrary-signature closure call).

**[M-mono-spawn-fwd-decls]:** Pre-scan `scan_expr_fwd` эмитит fwd-decls
по оригинальному AST до mono-worklist drain → spawn-body fns созданные
внутри mono'д fn body были без forward declaration. Fix: в emit_spawn
если `current_type_subst` non-empty, пушим fwd-decl в mono_fwd_decls
(splice в `/*__MONO_FWD_DECLS__*/` до всех fn defs).

Smoke test: `nova_tests/concurrency/mono_spawn_closure_smoke.nv` —
generic `run_in_fiber[T](body fn()->T)` с supervised+spawn, T=int/str/bool.
C-output подтверждает `(*_c->body)` rewrite + правильные fwd-decls.

**Регрессия:** 411 PASS / 46 FAIL (+1 smoke test passed, baseline FAILs same).

#### Шаг 4 — Ф.5 partial: Time schema fix; retry_test deferred ⚠️

`[M-time-effect-schema-mismatch]` снят: runtime `NovaVtable_Time` расширен
полями `now_ms` / `now_ns`, добавлены default-impl wrappers
(`Nova_Time_now_ms` / `Nova_Time_now_ns` в fibers.h). Теперь handlers.nv
(`fixed_ms` / `mut_clock`) и любые импорты `std.testing.handlers`
компилируются без "no member named 'now_ms'".

**retry_test.nv не написан** — `std/concurrency/retry.nv` содержит
`100.millis()` в record-literal field внутри generic static ctor
(`RetryPolicy.exponential`). Codegen в этом контексте генерирует invalid C
`((nova_int)100LL).millis()` (member-access на nova_int). Это pre-existing
codegen bug для int-extension methods (`fn int @millis()`) в record-literal
field, не связанный с Plan 48. Любой import retry.nv валит C-build даже
если test использует только `RetryPolicy.fixed`.

**Plan 49 followup:** `[M-int-extension-record-field]` — починить codegen
для `100.millis()` в record-literal field. После этого retry_test.nv
становится тривиальным (Time schema уже готова).

#### Шаг 5 — Финальный regression ✅

- Полный `nova test --release`: **411 PASS / 46 FAIL** (== baseline + smoke test).
- Все Plan 48 acceptance criteria закрыты (см. ниже), кроме одного partial.
- Документация обновлена: план, project-creation.txt, simplifications.md.
- Commits: fb21ca75f43 (Ф.7.4), 62f011661c2 (Ф.7.6), f0b1551d7e2 (Ф.4),
  1c0b0e9a33a (Ф.5 Time schema).

#### Шаг 6 — Ф.9 Method-param mono (2026-05-17, Plan 63 followup) ✅

**Что:** generic method с собственным type param `[U]`
(e.g. `Wrapper[T] @map[U](f fn(T) -> U) -> Wrapper[U]`) ранее mono'lся
**только по receiver T**, U оставался `Nova_U_p` placeholder в return
type → CC-FAIL: `Nova_Wrapper____Nova_U_p* m = ...`.

Корневая причина: `emit_call` path 5b (mono'd method-on-mono'd-type)
строил subst только из (receiver_generics → call-site type_args). Method-level
generics из `fd.generics` игнорировались. Аналогично — `infer_expr_c_type`
не знал, как inferить return-type для let-binding'а.

**Что починено:**

1. **`emit_call` path 5b (compiler-codegen/src/codegen/emit_c.rs:~12537+):**
   bidirectional inference из call-site args. Для closure-typed params
   `var_types` pre-populated с typed C-types closure-args, body inferенc'ится,
   return type binds method-level U. Method C-name теперь включает
   method-level type-args suffix (`Wrapper____T_method_map____U`).

2. **`infer_mono_method_ret_with_args` (~16794):** новый variant
   `infer_mono_method_ret` который принимает call args и поддерживает
   method-level inference. Так как метод `&self`, мутирование `var_types` /
   `current_type_subst` невозможно — добавлены RefCell поля
   `closure_param_type_overrides` + `type_subst_overrides`, которые
   `infer_expr_c_type` и `type_ref_to_c` consult'ят first перед обычными
   maps. Override'ы set/restore вокруг recurs'ии в closure body.

**C-output verification** (nova_tests/plan48_mpm/f1_method_param_mono.c):
4 distinct mono instance per (T, U) combination:
- `Wrapper____nova_int_method_map____nova_int`
- `Wrapper____nova_int_method_map____nova_str`
- `Wrapper____nova_str_method_map____nova_int`
- `Wrapper____nova_str_method_map____nova_str`

Все let-binding'и (`let s2 = s.map(...)`) типизированы корректно — без
`Nova_U_p` placeholder leak.

**Tests (permanent regression guards — 7 files, 13 sub-tests total):**

*Positive:*
- `nova_tests/plan48_mpm/repro_wrapper_map.nv` — minimal int→int + int→str.
- `nova_tests/plan48_mpm/f1_method_param_mono.nv` — 5 sub-tests: chained
  map, cross-type chain (int→str→str), identity, isolated str→int.
- `nova_tests/plan48_mpm/f2_multi_method_param_positive.nv` — Box @combine[U, V]
  с **двумя** method-level params (3 sub-tests).
- `nova_tests/plan48_mpm/f3_long_chain_positive.nv` — длинная цепочка
  `.map().map().map().map()` int↔str ping-pong + parallel chains
  (3 sub-tests).
- `nova_tests/plan48_mpm/f4_method_param_unused_in_return_positive.nv` —
  U bind'тся через arg, не появляется в return type (3 sub-tests).

*Negative (EXPECT_COMPILE_ERROR):*
- `nova_tests/plan48_mpm/f5_cannot_infer_u_negative.nv` — U только
  в return → clean diag.
- `nova_tests/plan48_mpm/f6_method_param_only_in_return_negative.nv` —
  U binds через closure, V только в return → diag на именно V.

**Production-grade diagnostic (2026-05-17 hardening):** ранее unresolved
method-level type params silently dropped → `Nova_U_p` placeholder leak
в emitted C → undefined-struct CC-FAIL. Теперь emit_call path 5b проверяет
subst_slots после Step 2 inference, fail'ит с:
```
cannot infer method-level type argument `U` for generic method
`Wrapper____<T>.<method>` (only in return type — provide arg
whose type binds it); provide a closure/arg whose type fixes `U`
```
Diagnostic mirror'ит free-fn message (compiler-codegen/src/codegen/emit_c.rs:6588+).

**Регрессия:** 668 PASS / 2 FAIL (2 RUN-FAIL = Windows UAC os error 740,
identical to main baseline — не codegen-related).
**plan48_mpm focused suite:** 7 PASS / 0 FAIL.

---

## Acceptance criteria (final 2026-05-16)

- [x] Generic-функция мономорфизируется per concrete type-args; вызов
      эмитит мангленное имя инстанциации, не `void*`-erased stub.
      (Ф.0-Ф.2 done).
- [x] Closure-параметр в generic-функции/методе вызывается как обычный
      typed closure-call (`body()` → корректный closure-call, не
      `nova_fn_body()`); zero adapter, zero boxing. (Ф.4 done — spawn
      capture rewrite + closure-call в fn_param_sigs path).
- [⚠️ partial] `[]fn()->T` внутри generic-функции:
  - **consume-path** (for-in / .len() через параметр) — ✅ работает.
  - **return-path** (generic-fn с `return []T`) — ⚠️ deferred Plan 54
    Ф.4 `[M-generic-array-return-mono]`.
- [x] Generic record `Box[T]` → реальный `struct Nova_Box__<T>` с
      типизированными полями. (Ф.3 done).
- [x] TurboFish — источник type-args, не выбрасывается; inference
      покрывает прямые случаи; непокрытое → понятная ошибка. (Ф.7.1).
- [⚠️ partial] Polymorphic recursion → compile-error с лимитом, не hang.
  - Mechanism в коде: Ф.7.6 done via `--mono-depth=N` CLI flag.
  - Verification test — Plan 54 Ф.7.
- [x] `std/concurrency/cancellation.nv` (`within`/`race2`) компилируется
      и проходит тесты. (Audit-fix sprint — within[T], race2[T],
      with_timeout[T] + 8 sub-cases).
- [⚠️ partial] `std/concurrency/retry.nv` покрыт `retry_test.nv`:
  - Type-checks ok, codegen blocked by Plan 54 Ф.2
    `[M-int-extension-record-field]`.
- [x] Полный `nova test` (release) — без новых FAIL.
- [⚠️ partial] Erased-эмиттеры:
  - `emit_generic_method_erased` оставлен как V1 fallback для unit-variant
    references (`let r = Err2`) — usage-context инференция для unit variants
    deferred V2 (Ф.7.4 final).
  - Bare-variant constructors с args (`Ok2(42)`) теперь идут через mono
    pipeline (2026-05-16, via `try_infer_variant_mono_args`).
  - `emit_generic_fn_erased` оставлен только для tuple-returning (V1 exception).

---

## Audit 2026-05-16 — production-revision: что НЕ закрыто / silent bugs

**Findings от честного аудита после initial Plan 48 closure:**

### Acceptance gaps (closing in current session)

1. **❌ `std/concurrency/cancellation.nv` (within[T], race[T]) НЕ написан.**
   Plan 48 + Plan 47 acceptance оба ссылаются на этот файл как main use case.
   Без него вся работа по closure-mono + spawn-fix остаётся «infrastructure only».
   **Fix:** написать stdlib (within timeout / race first-wins) с поддержкой
   `CancelToken[T]` reason.

2. **⚠️ Polymorphic recursion compile-error — без verification теста.**
   Механизм есть (mono_depth_limit + safety counter), но реальный тест
   с recursive generic + low `--mono-depth=N` отсутствует.
   **Fix:** написать тест `polymorphic_recursion_test.nv` который проверяет
   что compile-error появляется с понятным сообщением (а не hang).

3. **⚠️ `[]fn()->T` внутри generic-функции — verification теста нет.**
   `.len()/[i]/for-in` на array of closures-T внутри mono'д body —
   паттерн используется для `parallel for over closures`.
   **Fix:** smoke test `fn_array_in_generic_test.nv`.

4. **⚠️ Erased-эмиттеры partial closure.**
   `emit_generic_method_erased` остался V1 fallback для unit-variant
   references (`let r = Err2`). Закрытие требует usage-context inference.
   **Fix:** Реализовать forward analysis (`let r = Err2; r.method(arg)` →
   infer T from method arg) в этом sprint.

### Pre-existing codegen bug блокирующий Plan 48 Ф.5

5. **`[M-int-extension-record-field]` — invalid C для `100.millis()` в
   record-literal field.**
   Стало известно при попытке retry_test.nv. Codegen эмитит
   `((nova_int)100LL).millis()` (member access на int), вместо
   `nova_fn_int_method_millis(100LL)`. Любой import std/concurrency/retry.nv
   валит C-build.
   **Fix:** починить codegen для primitive-extension methods в record-literal
   field context. После этого retry_test.nv становится тривиальным.

### Industry-comparison improvements

6. **Beyond state-of-the-art:** `tok = tok1.merge(tok2)` композиция —
   результирующий токен cancelled когда любой из источников cancelled.
   Эквивалент Go `merged := errgroup.WithCancel(parentCtx)` + manual chain,
   но typed reason из любого источника.
   **Status:** новая фича, не в acceptance criteria; добавляется в этот sprint.

### Sprint 2026-05-16 EOD update — fixes applied / deferred

**✅ FIXED в этом sprint:**
- ✅ `std/concurrency/cancellation.nv` (within[T], race2[T], with_timeout[T]) — written, tests PASS.
- ✅ `[]fn()->T` consume-path в generic verification test (3 sub-cases).
- ✅ Polymorphic recursion sanity (4 sub-cases) — depth-limit mechanism verified.

**⏸️ DEFERRED в Plan 50:**
- `[M-int-extension-record-field]` — `100.millis()` в record-literal field
  внутри generic static ctor генерирует invalid C. Deep codegen fix
  (~2-4 hours), independent от Plan 48 core, scope creep.
- Unit-variant context inference (`let r = Err2; r.method(arg)` — infer
  T from method args) — Ф.7.4 final closure. Deep type-inference work
  (~4-8 hours).
- Generic-fn с `return []T` — receiver получает void* (deeper codegen gap
  для array return mono'мorphization).
- True polymorphic-recursion compile-error test (fn f[T] вызывает
  f[Box[T]]) — упирается в orthogonal codegen bug "anonymous record
  literal".

### Plan 62 followup — protocol-as-parameter mono ergonomics (2026-05-19)

Обнаружено в Plan 62 cleanup merge sprint (commits 99a629634ab,
ed3d00eb9c9). Два gap'а в monomorphization mechanic'е для popular
pattern `fn foo[U, T Iter[U]](it T)`:

**Gap A: parser bug — `mut <name> <generic-type>`.**

Декларация `fn sum_iter[U, T Iter[U]](mut it T)` не парсится:
```
error: expected identifier, got `mut`
```
Workaround — local re-bind в теле:
```nova
fn sum_iter[U, T Iter[U]](it_in T) -> int => {
    let mut it = it_in
    while let Some(_v) = it.next() { ... }
    ...
}
```
Парсер видимо bail'ит когда type-position это **generic type-var**
(uppercase single letter). Same `mut acc Account` со concrete type
парсится OK. **Mini-fix**, scope ≤ parser rule extension.

**Gap B: structural inference для bound type-var.**

Compiler не выводит `U = int` из argument `c: IntCounter` через
structural bound `T Iter[U]`. Diagnostic:
```
error: cannot infer type argument `U` for generic function `sum_iter`
(returned only — turbofish required)
```
User должен писать **двойной turbofish** на call-site:
```nova
let count = sum_iter[int, IntCounter](c)
```
Вместо expected ergonomic form:
```nova
let count = sum_iter(c)  // U=int (от IntCounter @next return), T=IntCounter
```

Rust/Swift делают это inference automatically — given concrete arg
type, walk methods, match return-type of named protocol-method, bind
free type-var. Не trivial, но industry-standard ergonomics для
generic-bound-style protocol-as-param.

**Regression marker:** `nova_tests/plan62/protocol_param_generic_bound.nv`
(commit `ed3d00eb9c9` или follow-up). Documents что pattern works с
turbofish, но not без него.

**Scope (если будет sprint):** parser fix Gap A — small. Inference Gap B —
medium-deep (требует extension к existing generic-fn type-arg resolution
visitor). Combined estimate ~4-8 hours, P2 (workaround есть, deferred-OK).

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
