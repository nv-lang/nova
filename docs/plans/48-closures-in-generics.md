// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 48: Closures in generic functions — higher-order generics

> **Создан 2026-05-14.**
>
> **СТАТУС:** план, не начат.
>
> **Приоритет:** P1 — разблокирует Plan 47 Ф.5 (`within`/`race`), чинит
> уже-в-репе-но-непротестированный `std/concurrency/retry.nv`, и открывает
> класс «HOF поверх generics» для всей будущей stdlib.
>
> **Предшественники:** нет жёстких. Опирается на существующую closure-
> инфраструктуру (`NovaClosBase`, `fn_param_sigs`, thunk-механизм Plan 11/14).

---

## Проблема

Замыкание, переданное в **generic** функцию (или лежащее в массиве внутри
неё), не кодогенерируется. Всплыло в Plan 47 Ф.5 при попытке написать
stdlib `within[T](ms, body fn() -> T)` и `race[T](competitors []fn() -> T)`.

```nova
export fn within[T](ms int, body fn() -> T) -> Option[T] {
    // ...
    spawn { result = Some(body()) }   // body() → codegen: nova_fn_body()  ✗
    // ...
}
export fn race[T](competitors []fn() -> T) -> T {
    let c = competitors[0]            // []fn()->T эрейзится в void*       ✗
}
```

Generic-функции в Nova кодогенерируются через **type erasure** (не
мон급оморфизация — см. emit_c.rs:1515 «Monomorphization not implemented
yet»): конкретный тип-параметр `T` заменяется на `void*`. Замыкания при
этом ломаются на четырёх независимых местах.

### Корень 1 — closure-параметр не в `fn_param_sigs`

`emit_fn` (обычные функции) регистрирует function-typed параметры в
`fn_param_sigs` (emit_c.rs:4305) — поэтому `body()` диспатчится через
closure-call (`NOVA_CLOS_CALL_*` или arbitrary-sig fallback,
emit_c.rs:6826-6856). `emit_generic_fn_erased` / `emit_generic_method_erased`
этого **не делают** → `body()` падает в «mangle user fn» ветку →
`nova_fn_body()` → undefined symbol.

### Корень 2 — ABI-несовпадение при erased возврате

Даже если зарегистрировать: arbitrary-sig closure-call (emit_c.rs:6849)
эмитит `((ret(*)(void*, params...))(((NovaClosBase*)f)->fn))(...)` где
`ret` — **erased** тип (`void*`). Но реальное замыкание `fn() -> int { 42 }`
имеет `fn` сигнатуры `nova_int(*)(void*)`. Вызов через каст к
`void*(*)(void*)` — вызов function-pointer'а с **несовместимым ABI**:
`int` (4 байта) vs `void*` (8 байт) для возврата — это UB, не «обычно
работает». Аналогично для scalar-аргументов и `f64`-возврата.

### Корень 3 — inline-эрейзинг в `emit_generic_fn_erased`

`emit_generic_method_erased` использует `erased_type_ref_c` (emit_c.rs:3923),
который корректно даёт `[]T` → `NovaArray_nova_int*`. А `emit_generic_fn_erased`
имеет **свою** inline-логику (emit_c.rs:4028-4043) которая всё не-record
эрейзит в `void*`, включая `[]T`. Разъезд: для методов массивы работают,
для free-функций — нет.

### Корень 4 — массив замыканий не имеет element-представления

`NovaArray_*` поддерживает scalar-элементы (`nova_int`/`nova_str`/...) и
указатели. Замыкание — это `NovaClosBase` (struct `{fn, env}`) либо
указатель на него. Нужно зафиксировать: массив замыканий хранит
`NovaClosBase*` (указатели) элементами, и `.len()` / `[i]` / `for-in`
по нему резолвятся.

---

## Архитектурное решение: call-site erasure adapter + uniform erased ABI

**Ключевое наблюдение.** В точке **вызова** generic-функции codegen знает
ОБА куска информации: (1) конкретный тип замыкания (из call-site AST),
(2) что callee-параметр — generic (`fn() -> T`). Значит именно call-site
может вставить **адаптер эрейзинга**.

**Erased closure ABI.** Замыкание, передаваемое в generic-функцию,
оборачивается в адаптер с **единообразной pointer-sized сигнатурой**:
каждый аргумент и возврат — `void*` (scalar'ы боксируются: `nova_int`/
`nova_bool` через `(void*)(intptr_t)`, `f64` через heap-box, не-scalar'ы
уже указатели). Внутри generic-функции ВСЕ замыкания вызываются
единообразно через arbitrary-sig path, всё `void*` — ABI consistent.

Это согласуется с тем, что generic-функция **уже** возвращает `void*` и
caller un-эрейзит результат (`emit_erased_return`, emit_c.rs:4056). Если
замыкание тоже работает в erased ABI — вся цепочка сходится.

**Почему адаптер, а не uniform ABI для ВСЕХ замыканий.** Делать все
замыкания pointer-sized — регрессия производительности для не-generic
HOF (`xs.map(inc)` на конкретных типах). Адаптер платится только когда
замыкание реально пересекает generic-границу.

**Почему не мон급оморфизация.** Мон급оморфизация `within[int]` /
`within[str]` дала бы реальные сигнатуры без ABI-возни — но это
архитектурный разворот всего generic-codegen'а (отдельный мега-план).
Erasure + call-site адаптеры — incremental, совместимо с текущим кодом.

---

## Фазы

### Ф.0 — Erased closure ABI: helpers + спецификация

- `nova_rt/nova_rt.h`: задокументировать erased closure ABI —
  `void*(*)(void*env, void* a0, void* a1, ...)`. Добавить box/unbox
  helpers: `nova_box_int(nova_int) -> void*` / `nova_unbox_int(void*) ->
  nova_int`, аналогично `bool`; `nova_box_f64` / `nova_unbox_f64` через
  GC-heap (f64 не влезает в указатель на всех ABI — боксим в `nova_alloc`).
  Не-scalar типы (`nova_str`, `Nova_X*`, `NovaArray_X*`) — уже
  pointer-sized либо struct-by-value, для них box = identity / address.
- `NovaClosErased` тип = `NovaClosBase` (уже `{void* fn; void* env}`) —
  переиспользуем, фиксируем что `fn` имеет erased-сигнатуру.
- Spec: новая D-decision (следующий свободный номер) — «closures across
  generic boundaries: call-site erasure adapter».

### Ф.1 — Регистрация closure-параметров в generic-функциях

- `emit_generic_fn_erased`: для каждого param с `TypeRef::Func` —
  зарегистрировать в `fn_param_sigs` с **erased** сигнатурой (все
  param-типы и ret → `void*`). Зеркалит логику `emit_fn` (emit_c.rs:4285-
  4305), но с erased-типами.
- `emit_generic_method_erased`: то же.
- После этого `body()` внутри generic-функции диспатчится через
  closure-call с erased-сигнатурой → arbitrary-sig path → корректный
  `((void*(*)(void*))(((NovaClosBase*)body)->fn))(env)`.
- Внутри generic-функции возвращаемое замыканием `void*` un-боксится
  по месту использования. На первом шаге — там где результат сразу
  используется как `T` (= `void*` в erased-контексте) — un-box не нужен,
  значение уже `void*`. Un-box нужен только если внутри generic-функции
  результат используется как конкретный scalar — редко; зафиксировать
  как ограничение либо обработать через `infer`.

### Ф.2 — Call-site erasure adapter

- В `emit_call`, когда аргумент — closure-value (lambda / free-fn-value /
  closure-typed var) И callee-параметр имеет generic тип (`fn(...) -> T`
  где T — type-param callee): эмитить **erasure-adapter thunk**.
- Адаптер: `static void* <closure>_erased_adapter(void* env, void* a0, ...)`
  — un-боксит аргументы из `void*` в конкретные типы, зовёт реальное
  замыкание, боксит результат обратно в `void*`. Генерируется один раз
  per (concrete-sig, arity) — кэш в `emitted_erased_adapters: HashSet`.
- Call-site эмитит `NovaClosBase`-значение `{fn: <adapter>, env: <real
  closure>}` — env адаптера = само исходное замыкание (так адаптер
  достаёт реальный `fn` + реальный `env`).
- Определение «callee-параметр generic» — нужна сигнатура callee. У free
  fn — `user_fn_sigs` + список generics из FnDecl; у методов — аналогично.
  Возможно потребуется отдельная мапа `generic_fn_param_kinds`.

### Ф.3 — Массивы замыканий в generic-функциях

- `emit_generic_fn_erased`: заменить inline param-эрейзинг (emit_c.rs:
  4028-4043) на вызов `erased_type_ref_c` — устраняет Корень 3, `[]T`
  становится `NovaArray_*` единообразно с методами.
- Зафиксировать element-представление для `[]fn() -> T`: массив хранит
  `NovaClosBase*` (указатели на erased-замыкания). Нужен
  `NovaArray_NovaClosBase` (или переиспользовать `NovaArray_nova_int*` с
  cast'ами — решить в Ф.3, чище — отдельный element-tag).
- `.len()` / индексация `[i]` / `for-in` по массиву замыканий — должны
  резолвиться. `for comp in competitors` внутри generic-функции:
  iterator-resolution видит `NovaArray_<closure>` вместо `void*`.
- Call-site: литерал массива замыканий `[fn(){...}, fn(){...}]`,
  передаваемый в `race[T]([]fn()->T)` — каждый элемент оборачивается в
  erasure-adapter (Ф.2), массив собирается из `NovaClosBase*`.

### Ф.4 — Разблокировать Plan 47 Ф.5: `within` / `race`

- Восстановить `std/concurrency/cancellation.nv` с `within[T]` и `race[T]`
  (были удалены в Plan 47 как нерабочие).
- `within[T](ms int, body fn() Time Fail[any] -> T) -> Option[T]` —
  watchdog + work в `supervised(cancel: tok)`.
- `race[T](competitors []fn() Time Fail[any] -> T) -> T` — index-обход
  массива замыканий, self-cancel победителя.
- Тесты `nova_tests/concurrency/cancellation_stdlib_test.nv`: within
  (success/timeout), race (первый победитель / единственный competitor).
- Снять `[M-race-closure-array]` из simplifications.md.
  `[M-within-error-conflation]` остаётся (это про cancel-throw-routing,
  отдельная ось — см. Plan 47).

### Ф.5 — Валидация на `retry.nv` (реальная stdlib)

- `std/concurrency/retry.nv` — `RetryPolicy @execute[T, E](body fn() ...)`
  — generic-МЕТОД с closure-параметром. Сейчас type-check'ается, но НИ
  РАЗУ не был codegen-проверён (нет тестов).
- Написать `nova_tests/concurrency/retry_test.nv`: exponential/fixed
  backoff, max-attempts, успех после N попыток, исчерпание попыток.
- Это покрывает `emit_generic_method_erased` путь (Ф.1) на боевом коде.

### Ф.6 — Regression + docs

- Полный `nova test` (release) — без новых FAIL.
- Возможные edge-cases для отдельных тестов: closure с `f64`-возвратом
  через generic; closure с multiple args; nested generic HOF
  (`map` поверх `within`); free-fn-value (не lambda) через generic.
- `docs/project-creation.txt` + `docs/simplifications.md` — закрытие
  плана; снять `[M-race-closure-array]`.
- spec D-decision (Ф.0) финализировать.
- discussion-log private-репы.

---

## Что НЕ входит

- **Полная мон급оморфизация** generic-функций — отдельный архитектурный
  план, если когда-нибудь. Plan 48 работает В РАМКАХ текущей type-erasure
  модели.
- **Cancel-throw routing fix** (`[M-within-error-conflation]`) — отдельная
  ось, см. Plan 47 §«Что НЕ входит». Plan 48 разблокирует `within` как
  код, но конфляция реальной ошибки и timeout'а в нём остаётся до того
  фикса.
- **Specialization / inlining** замыканий в generic-функциях ради
  производительности — Plan 48 даёт корректность, не скорость. Адаптер +
  boxing — измеримый overhead на generic-HOF пути; оптимизация позже.
- **Closures с эффект-полиморфизмом** в сигнатуре (`fn() E -> T` где E —
  effect-параметр) — если всплывёт, отдельная under-плана.

---

## Риски

**R1 — ABI function-pointer cast.** Центральный риск. Вызов замыкания
через каст function-pointer'а к другой сигнатуре — UB по стандарту C,
хотя на x86-64 SysV/Win64 pointer-sized args/ret обычно проходят.
Митигация: erasure-adapter (Ф.2) делает ABI **точным** — адаптер имеет
ровно ту сигнатуру, через которую его зовут (`void*(*)(void*...)`), а
реальное замыкание зовётся внутри адаптера с его настоящей сигнатурой.
Никаких «каст-и-молись».

**R2 — `f64` boxing.** `f64` не влезает в указатель на 32-бит ABI (Nova
целится в x86-64, но не хардкодить). Box через `nova_alloc(sizeof(f64))`.
Overhead приемлем — generic-HOF и так не hot path. GC видит box (alloc'нут
через GC).

**R3 — определение «callee-параметр generic».** Call-site должен знать,
что параметр callee — type-параметр, а не конкретный `fn(int)->int`.
Требует доступа к FnDecl callee + его generics на call-site. Для free fn
— через `user_fn_sigs` + расширить мапой generic-kind. Для методов —
сложнее (overload resolution). Митигация: начать с free-fn (покрывает
`within`/`race`), методы — в Ф.5 на `retry.nv`.

**R4 — closure-as-array-element typing.** `NovaArray` element-теги
ограничены primitive-набором. Добавление closure-element-тега — точечное
расширение array-машинерии; риск задеть существующие array-пути.
Митигация: отдельный `NovaArray` element-вариант, не трогать существующие.

**R5 — interaction с spawn-capture.** Замыкание, захваченное в `spawn`
внутри generic-функции (как в `within`: `spawn { body() }`) — `body`
captured by-pointer в SpawnCtx, потом вызывается. Цепочка: capture →
ctx-field → call. Уже частично работает (Plan 47 уперлось именно сюда
после Ф.1-фиксов scan/buffer). Нужен integration-тест именно этого
пути в Ф.4.

---

## Size estimate

| Компонент | LOC |
|---|---|
| Ф.0 — erased ABI helpers + spec | ~120 |
| Ф.1 — fn_param_sigs регистрация (2 generic-эмиттера) | ~80 |
| Ф.2 — call-site erasure adapter + кэш | ~250 |
| Ф.3 — closure-массивы (erased_type_ref_c + element-tag + iter) | ~250 |
| Ф.4 — within/race восстановить + тесты | ~200 |
| Ф.5 — retry.nv тесты | ~150 |
| Ф.6 — regression + docs | ~80 |
| **Итого** | **~1130** |

---

## Acceptance criteria

- [ ] Closure-параметр в generic free-функции вызывается корректно
      (`body()` → erased closure-call, не `nova_fn_body()`).
- [ ] Closure-параметр в generic-методе — то же (`retry.nv` `@execute`).
- [ ] Call-site erasure adapter: замыкание с scalar-возвратом
      (`fn() -> int`), переданное в `generic[T]`, вызывается с корректным
      ABI (адаптер боксит/un-боксит).
- [ ] `[]fn() -> T` внутри generic-функции: `.len()` / `[i]` / `for-in`
      резолвятся.
- [ ] `std/concurrency/cancellation.nv` восстановлен: `within` + `race`
      кодогенерируются и проходят тесты.
- [ ] `std/concurrency/retry.nv` покрыт `retry_test.nv` — проходит.
- [ ] Полный `nova test` (release) — без новых FAIL.
- [ ] `[M-race-closure-array]` снят из simplifications.md.

---

## Связь

- [Plan 47](47-supervised-cancel.md) — Ф.5 (`within`/`race`) отложена сюда;
  Plan 48 её разблокирует.
- [Plan 11](11-overloading-mangling.md) / [Plan 14](14-generics-option.md)
  — closure-инфраструктура (`NovaClosBase`, thunk-механизм, `fn_param_sigs`,
  `user_fn_sigs`), на которой Plan 48 надстраивается.
- `std/concurrency/retry.nv` — существующий generic-метод с closure-
  параметром, никогда не codegen-проверявшийся; Ф.5 закрывает этот пробел.
