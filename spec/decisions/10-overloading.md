# Overloading — перегрузка функций и методов

Решения этой группы описывают единый механизм перегрузки в Nova: **по
receiver-типу, по типам аргументов, по типу результата, по арности** —
одно правило резолва, общее для свободных функций, методов и
static-функций на типе.

| # | Решение |
|---|---|
| [D84](#d84-перегрузка-функций-и-методов-четыре-оси-резолв-по-самому-специфичному-матчу) | Перегрузка функций и методов: четыре оси, резолв по самому специфичному матчу |
| [D285](#d285--receiver-compatibility-rule--blanket-dispatch-priority-plan-164-ф3) | Receiver-compatibility rule: blanket dispatch приоритизирует по receiver-совместимости |
| [D464](#d464--бáунд-как-фильтр-отбора-отсев-без-ранжирования-2026-08-16) | Бáунд как фильтр отбора: кандидат, чей бáунд не держится, отпадает; выживших больше одного — неоднозначность, без ранжирования |

Связанные решения в других файлах:
- [D46](03-syntax.md#d46) — operator overloading через `@plus`/`@times` (частный случай по receiver).
- [D69](03-syntax.md#d69) — variadic-параметры `...args []T`.
- [D73](08-runtime.md#d73) — `From[T]`/`Into[T]` (частный случай по аргументу/результату).
- [D77](08-runtime.md#d77) — `TryFrom`/`TryInto` (то же).

---

## D84. Перегрузка функций и методов: четыре оси, резолв по самому специфичному матчу

### Что
Имя функции или метода может быть **перегружено** — одно имя описывает
несколько сигнатур, и компилятор выбирает нужную по контексту вызова.
Перегрузка работает по **четырём осям**:

1. **По receiver-типу** — `fn int @m()` и `fn str @m()` — разные методы.
2. **По типам аргументов** — `fn f(s str)` и `fn f(b []u8)` — разные функции.
3. **По типу результата** — `fn Celsius @into() -> Fahrenheit` и `fn Celsius @into() -> Kelvin` — выбираются по ожидаемому типу из контекста.
4. **По арности** — `fn exit(code int)` и `fn exit(code int, msg str)` — разное число параметров.

Все четыре оси работают одновременно для **свободных функций**,
**методов** (с `@`-receiver'ом) и **static-функций** на типе (`T.name(...)`).

### Правило

#### Декларация

Программист пишет несколько определений с одним именем. Сигнатуры должны
**различаться** хотя бы по одной из осей — иначе compile error
«duplicate definition».

```nova
// По типам аргументов
fn parse(s str) Fail[ParseError] -> int  => ...
fn parse(b []u8) Fail[ParseError] -> int => ...

// По арности
fn exit(code int) -> never              => ...
fn exit(code int, msg str) -> never     => ...

// По receiver-типу (методы)
fn int @double() -> int  => @ * 2
fn f64 @double() -> f64  => @ * 2.0

// По типу результата (static / instance)
fn Celsius @into() -> Fahrenheit => ...
fn Celsius @into() -> Kelvin     => ...
```

#### Резолв на call-site

Компилятор резолвит вызов в **четыре фильтра**, применяемые по порядку:

**Фильтр 1 — арность.** Отбрасываются кандидаты с числом параметров,
не совпадающим с числом аргументов на call-site. Variadic-кандидат
([D69](03-syntax.md#d69)) принимает любое число аргументов ≥ числа
non-variadic параметров.

**Фильтр 2 — типы аргументов.** Каждый аргумент проверяется против
типа соответствующего параметра. Если тип аргумента — подтип типа
параметра (или совпадает), кандидат остаётся; иначе отбрасывается.

**Фильтр 3 — тип результата.** Если контекст вызова задаёт ожидаемый
тип (см. ниже «Источники контекста для result-резолва»), отбрасываются
кандидаты, тип результата которых не совпадает / не приводится.

**Фильтр 4 — самый специфичный матч.** Из оставшихся кандидатов
выбирается тот, у которого сигнатура **самая специфичная** по правилам:

1. **Concrete побеждает generic.** `fn f(v int)` выбирается раньше,
   чем `fn f[T](v T)`, при вызове `f(42)`.
2. **Non-variadic побеждает variadic.** `fn f(s str)` выбирается раньше,
   чем `fn f(...args []str)`, при вызове `f("hello")`.
3. **Subtype побеждает supertype.** При иерархии `int < any` для
   аргумента типа `int` выбирается `fn f(v int)`, а не `fn f(v any)`.
4. Если ни один кандидат не доминирует — compile error «ambiguous
   overload» с перечислением кандидатов и hint'ом про cast/turbofish.

#### Источники контекста для result-резолва

Тип результата подсказывает компилятору, какую перегрузку выбрать.
Контекст приходит из:

- **`let x T = expr`** — тип из аннотации.
- **Возврат из функции** — `return expr` в функции с известным
  return-type.
- **Аргумент функции** — `f(c.into())` где `f(x Fahrenheit)`.
- **Поле record-литерала** — `{ temp: c.into() }` где `temp Fahrenheit`.

Если контекста нет — compile error:

```nova
ro x = c.into()                    // ❌ нет ожидаемого типа
//      ^^^^^^^^ cannot resolve overload `Celsius.@into()`:
//               candidates: -> Fahrenheit, -> Kelvin
//               hint: add type annotation `let x Fahrenheit = ...`

ro x Fahrenheit = c.into()         // ✅ контекст из аннотации
```

#### Turbofish не обходит concrete

Вызов `f[T_value](args)` — turbofish задаёт значение generic-параметра,
но **не меняет** правила резолва. Concrete-перегрузка для конкретного
типа доминирует над generic-перегрузкой даже при явном turbofish.

**Следствие:** `f[u8](7)` ≡ `f(7 as u8)`. Обе формы резолвятся в одну и
ту же overload — concrete если она существует, иначе generic с `T = u8`.

```nova
fn job[T Numeric = f64](a T) => a * 10        // generic
fn job(a u8) Fail => throw "error"             // concrete u8

job(7 as u8)        // → throw "error" (concrete)
job[u8](7)          // → throw "error" (concrete, не generic)
job(5.0)            // → 50.0 (generic, T = f64)
job(7)              // → 70 (generic, T = int — нет concrete для int)
```

Это согласовано с принципом «concrete побеждает generic» (фильтр 4
выше): автор API, объявивший concrete-перегрузку, делает это
**специально** — generic-версия для этого типа обходится. Turbofish
не обходит этот контракт.

**Когда нужна именно generic для конкретного типа** при существующей
concrete — переименовать generic или вынести её в отдельный модуль.
В Nova нет специального синтаксиса «вызови именно generic».

#### Mangling

Компилятор использует name mangling для C-emit: каждая перегрузка
получает уникальное C-имя, в которое закодированы типы параметров и
receiver'а. Программист этого не видит — на уровне Nova-кода имя одно.

**Схема mangling.** Первая перегрузка использует короткое имя
(backward-compat): `Nova_T_method_m` / `Nova_T_static_m`. Вторая+ — с
param-types suffix: `Nova_T_method_m__nova_str`, `Nova_T_method_m__nova_int`.
Общее правило: `<original>__<param_type_1>_<param_type_2>_...`.

Это распространяет существующий механизм Plan 11 (mangling для
методов) на свободные функции и static-функции на типе.

#### Bootstrap-status (Plan 11)

- ✅ **static** overload по типу аргумента (`T.from(int)` vs `T.from(str)`)
  работает в bootstrap-codegen через `method_overloads` registry +
  C-name mangling по param types.
- ✅ **instance** overload по типу аргумента (`@write(str)` vs
  `@write([]u8)`).
- ✅ **arity** overload (`@log(msg)` vs `@log(level, msg)`).
- ✅ Одноимённые методы на разных типах (`Box1.make()` vs `Box2.make()`)
  не конфликтуют — multi-key registry `(type, name) → Vec<Sig>`.
- ✅ **Free-functions** (без receiver'а) — overload работает (2026-05-10):
  тот же `method_overloads` registry с sentinel-key `("", name)`,
  C-mangling по param-types. Резолв на call-site по статическим типам
  args. Тест: `nova_tests/syntax/overload_free_fn.nv`.
- ⚠️ **Result-type overload** (ось 3 D84) — type-checker регистрирует
  overloads с разным return-type, но codegen на call-site **не делает**
  expected-type propagation: при двух кандидатах с одинаковыми
  arg-types и разным return-type возникает ambiguity error. Реализация
  требует context-driven resolve через let-аннотации, return-position,
  argument-types вызывающей функции. Отложено как Q-overload-result-type.
- ✅ **Method values** как first-class (`Type.@m` unbound, lambda `|| obj.m()`) —
  Plan 11 Ф.4 + Plan 132 (bound `obj.@m` removed). См. D35 «Method values».
- ✅ **Disambiguation через `as fn(...)`** для overloaded method values —
  Plan 11 Ф.5. Annotation на cast или на let-binding type определяет,
  какой overload выбрать.
- ✅ **Receiver-mutability overload** (`fn T @m()` vs `fn T mut @m()` —
  одинаковые param-типы, разный receiver-mut) — Plan 135 (2026-06-09):
  `fn T @m()` получает C-имя `Nova_T_method_m` (первая perегрузка),
  `fn T mut @m()` — `Nova_T_method_m__mut`. Call-site dispatch по
  мутабельности receiver'а: `ro a; a.m()` → ro overload,
  `mut b; b.m()` → mut overload. Аналог C++ `const`-overloading.
  Тесты: `nova_tests/plan135/` (8/8 PASS).
- ✅ **Generic-type method overloads в монорфизации** (`fn Vec[T] @cap()` vs
  `fn Vec[T] mut @cap(n int)` — арность; `fn Box[T] @tag(int)` vs `@tag(str)` —
  arg-type) — Plan 153.1 / `[M-138.2-generic-method-overload-mono]` (2026-06-13):
  раньше mono-диспатч коллапсировал overloads first-by-name (`v.cap(10)` → 0-арг
  геттер → «too many args»). Теперь call-site дизамбигуирует по арности → param-
  C-типам (через side-map `mono_method_fndecl_for_name`), а return-type inference
  для **chained** receiver'а (`v.cap(n).push(x)`) резолвит `@`/Self тем же arity-
  aware выбором (Ф.3-fallback). Тесты: `nova_tests/plan153_1/generic_overload.nv`
  (3/3: арность + param-type + chain), `core_api.nv` (fluent). Caveat: вызов
  overload'а **без совпадения** по арности всё ещё CC-FAIL'ит (codegen fall-
  through к первому кандидату), а не чистый type-check error — followup
  `[M-138.2-overload-no-match-typecheck]`.

#### Strict matching типов

**No implicit conversions.** `buf.write(42)` где `42 int` — error если
нет `@write(int)`. Программист пишет `buf.write(42 as char)` или
`buf.write(str.from(42))`. Это часть правила «самый специфичный матч»:
implicit-конверсия размывает специфичность.

#### Примеры — методы

```nova
fn Buffer mut @write(s str) -> ()
fn Buffer mut @write(b []u8) -> ()
fn Buffer mut @write(c char) -> ()

fn Logger @log(msg str) -> ()
fn Logger @log(level int, msg str) -> ()         // arity overload
```

Resolution на call-site по статическим типам аргументов:

```nova
buf.write("hello")        // → @write(str)
buf.write([0xDE, 0xAD])   // → @write([]u8)
buf.write('A')            // → @write(char)

log.log("ok")             // → @log(str) — arity 1
log.log(2, "ok")          // → @log(int, str) — arity 2
```

При ambiguity (≥2 кандидатов после фильтрации) — compile error
с suggestion'ом disambiguate через lambda с явными типами аргументов:

```nova
ro f = fn(s str) -> int => t.m(s)
```

#### Дисамбигуация программистом

Когда автоматический резолв даёт ambiguous error, программист может
явно указать выбор:

- **Cast аргумента:** `f(42 as i32)` — выбирает `fn f(v i32)`, если
  кандидат был `fn f(v int)` или `fn f(v i32)`.
- **Turbofish для generic:** `parse[int]("42")` — фиксирует
  generic-параметр.
- **Аннотация результата:** `let x Fahrenheit = c.into()` — фиксирует
  тип результата.

#### Ось режима {ro, mut, consume} — единая для ресивера и параметров (Plan 184, амендмент D326-Р13/Р14)

> **Статус:** ✅ IMPLEMENTED (Plan 184 заход-6, 2026-07-07; правило выбора consume-версии
> уточнено заходом-7, №309/221.1, окно p-lang, 2026-08-04 — см. правило 3 ниже: заход-6
> сверял только временность значения, не форму привязки, вопреки тексту правила).
> Режим параметра — ось перегрузки наравне с receiver-mut (прецедент Plan 135). Факты
> реализации:
> - **Ключ перегрузки (чекер):** `types/mod.rs` — dup-детект сравнивает режим каждого
>   параметра (`is_mut` + `consume`) наряду с типом/арностью/возвратом/receiver-mut;
>   `f(x T)` / `f(mut x T)` / `f(consume x T)` — раздельные перегрузки, не «duplicate».
> - **Мангл (раздельные C-символы):** `MethodSig.param_modes: Vec<u8>` (0=ro/1=mut/2=consume,
>   `sig_registry.rs`). Мангл-суффикс режима (`__r`/`__m`/`__c` по параметрам) добавляется
>   ТОЛЬКО при реальной коллизии — когда сиблинг с ИДЕНТИЧНЫМИ `param_c_types`, но другим
>   режимом уже занял имя (кучевой случай Р6: `ref H ≡ H` — mut/consume не меняют ABI
>   handle). Чисто-`ro`/type-различённые наборы byte-identical (существующие символы не
>   тронуты — проверено). `mangle_fn` резолвит тело каждой перегрузки под своим символом
>   пассом по `param_modes`.
> - **Диспатч (`emit_c.rs::narrow_by_param_mode`):** среди кандидатов, совпавших по
>   `param_c_types`, сужение по режиму от способности аргумента-биндинга (см. правило ниже).
>   Хук на call-site свободной функции И instance-метода (ортогонален receiver-mut tiebreak).
> - **Гейты:** conformance 56/0; `nova_tests/inout_ref` 17/0 (`p184_mode_overload_pos`
>   7/7 runtime: тройная ось + пара ro/mut + смешение `fn T @m(x T)`/`fn T mut @m(mut x T)`);
>   широкая byte-identity vs main-бинарь = 0.
> - **Ограничение (scoped follow-up `[M-184-value-mut-mode-overload-abi]`):** для
>   VALUE/примитивного `T` перегрузка с `mut`-режимом меняет ABI параметра на указатель
>   (`T*`, Р10), что требует согласования адресации на call-site с выбором перегрузки —
>   кучевой `T` (канонический in-out, Р6) покрыт полностью; value/prim `mut`-mode-overload
>   — отдельный заход. Одиночный value/prim `mut x T` (не перегруженный) работает как в
>   заходе-4.

**Амендмент (2026-07-06, sign-off владельца; см. [ревизию D326](02-types.md#ревизия-d326-plan-184-ref-t--ограниченный-тип)).**
Ось «изменяемость ресивера» (Plan 135: `fn T @m()` vs `fn T mut @m()`, bootstrap-статус выше,
строка «Receiver-mutability overload») **обобщается** до единой тройной оси режима
`{ro, mut, consume}`, действующей одинаково для **ресивера** (нулевой параметр) и **обычных
параметров**. Это делает `@` формой параметра (Р14): различимы `@m` / `mut @m` / `consume @m`
И `f(x T)` / `f(mut x T)` / `f(consume x T)`.

**Декларация.** Одноимённые функции/методы, различающиеся только режимом одного параметра
(включая нулевой — ресивер), — **легальные перегрузки** (Р13, по прецеденту ресивера). До
Plan 184 `consume` в оси перегрузки НЕ описан (пар `mut`/`consume` одного имени в std нет) —
расширение чистое, backward-compatible.

**Резолв на call-site (правило выбора, Р14).** Дополнительный фильтр перед «самым
специфичным матчем» (фильтр 4), применяемый пер-параметр (и к ресиверу):

1. **ro-аргумент** (аргумент — `ro`-биндинг / rvalue / не в последней точке использования) →
   выбирается **только ro-версия** параметра.
2. **mut-биндинг-аргумент** → **mut-версия приоритетнее ro** (прецедент ресивера, Plan 135:
   `mut b; b.m()` → mut-overload).
3. **consume-версия** участвует в резолве, когда аргумент — **временное значение (owned-
   rvalue) ИЛИ именованная `consume`-привязка** (дословное правило владельца, 2026-08-03:
   «временное ИЛИ забирающее — перемещается, остальное копируется»); тогда consume
   специфичнее mut. Иначе (`ro`-/`mut`-привязка) consume-кандидат исключён — детерминизм без
   молчаливого потребления живого биндинга, объявленного НЕ как `consume`.
   *Реализация (заход-6, owned-временное; заход-7 №309/221.1 2026-08-04, именованная
   `consume`-привязка):* consume-версия выбирается для **owned-временного** аргумента
   (rvalue: результат вызова / литерал / конструктор — заведомо последняя точка,
   `is_rvalue_temp`) **ИЛИ** для аргумента-`Ident`, чья привязка объявлена `consume x = …`
   (`LetDecl.consume`, чекер `types/mod.rs` уже гарантирует D133/D156-обязательство её
   потребления — форма привязки сама несёт факт «это единственная точка использования»,
   отдельного last-use-анализа для ЭТОГО случая не нужно). Заход-6 сверял только временность
   значения — именованная `consume`-привязка (`consume t = 7; b.put(t)`) ошибочно попадала в
   ro/mut-версию (дефект №309, реестр 221.1). **Отличие от `[M-184-consume-dispatch-named-
   lastuse]`** (`docs/plans/184-ref-type-revision.md`): тот маркер — про ЛЮБОЙ (в т.ч. `ro`/
   `mut`) именованный биндинг в его последней точке через полный consume-чекер-анализ;
   правило владельца ýже — специфично только для привязки, ОБЪЯВЛЕННОЙ `consume`, без
   last-use-анализа произвольных биндингов. Маркер `[M-184-consume-dispatch-named-lastuse]`
   остаётся отдельным открытым follow-up для более общего случая.

**Неоднозначность невозможна по построению.** `mut`-версия годна только для изменяемого
места, `consume`-версия — только для owned-временного; классы аргумента взаимоисключающи, а
`ro` всегда годна, но наименее специфична. Значит для набора, различающегося режимом ОДНОГО
параметра, победитель единственен. Для (редкого) набора с противоположной вариацией режима
двух параметров, дающего ничью специфичности, диспатч **реджектит** вызов как ambiguous с
перечислением кандидатов (общий канон D84, фильтр 4) — молчаливого выбора нет.

> **Реализовано 2026-09-02 (реестр №857).** До того дня абзац был неисполнен:
> тайбрейкер складывал специфичность в ОДНО число, а любая скалярная шкала
> разрешает несравнимые пары: `f(consume a, b, c)` против `f(a, mut b, mut c)`
> давали 3 против 4, и второй молча побеждал (печаталось `2`). Сравнение
> теперь по **доминированию** позиционного вектора режимов (победитель обязан
> быть не ниже везде и выше где-то); нет доминирующего — отказ
> **`[E_OVERLOAD_AMBIGUOUS_MODE]`** с перечнем кандидатов, как требует этот абзац.
> Фикстуры: `spec_tests/conformance/neg/overload_mode_axis_tie_neg.nv` (отказ)
> и два позитивных контроля рядом: одиночный кандидат и настоящее
> доминирование по-прежнему ВЫБИРАЮТСЯ, а не отвергаются.

**Следствие для Р10 (форма параметра одна на режим).** Поскольку `mut x T` — единственная
форма mut-параметра (ret. `mut ref`, D326-ревизия Р3), исходная неоднозначность
`f(x T)` vs `f(mut x T)` (D326-старый R4: неразличимы на вызове без маркера) **исчезает по
построению**: различие несёт изменяемость аргумента-биндинга, как у ресивера. Прежний код
`E_OVERLOAD_REF_AMBIGUOUS` не нужен.

### Почему

#### Зачем перегрузка вообще

В существующей Nova-практике перегрузка **уже используется** — Plan 11
закрыл её для методов, D73 для `From`/`Into`, D46 для операторов.
Stdlib-типы вроде `StringBuilder` опираются на это:

```nova
external fn StringBuilder mut @append(s str)  -> ()
external fn StringBuilder mut @append(c char) -> ()
```

Запрет на перегрузку для **свободных функций** оставался искусственным
ограничением, не имеющим обоснования в дизайне. D84 устраняет
несимметрию и формализует все четыре оси одним правилом.

#### Почему четыре оси, а не три

Тип результата (ось 3) часто упускают, но в Nova он **уже работает**
для `@into()` через context-driven dispatch ([D73](08-runtime.md#d73)).
Без него `Celsius @into() -> Fahrenheit` и `Celsius @into() -> Kelvin`
было бы нельзя различить. Включение оси 3 в общее правило формализует
существующее поведение.

#### Почему «самый специфичный матч»

Это согласованное правило в большинстве языков с overloading
(Java, Swift, C#, Scala, Rust trait selection). Альтернативы:

- **Last-wins** (текущий bootstrap для свободных функций) — проще
  имплементировать, но создаёт hidden surprises: добавление новой
  перегрузки молча меняет поведение существующего кода.
- **First-wins** — то же, в обратную сторону.
- **Ambiguous → error** без подсказки — не помогает программисту
  выбрать.

«Самый специфичный + ambiguous → error с hint'ом» — баланс между
автоматизмом и предсказуемостью.

#### Почему concrete побеждает generic

Программист пишет конкретную перегрузку, чтобы **специализировать**
дженерик для конкретного типа: например, `fn f[T Hash](v T)` —
общая реализация, `fn f(v str)` — оптимизированная для строк. Если бы
generic выигрывал, специализация не работала бы.

#### Почему non-variadic побеждает variadic

`f("hello")` для `fn f(s str)` и `fn f(...args []str)` — оба подходят.
Variadic — это «catch-all» на произвольную арность; non-variadic
сигнатура **конкретно совпадает по форме** и поэтому специфичнее. Это
естественно отражает намерение программиста: писать non-variadic = «у
меня ровно столько-то аргументов», писать variadic = «может быть любое
количество».

#### LLM-критерий

Перегрузка повышает риск, что LLM сгенерирует код, который компилируется,
но вызывает не ту перегрузку. Mitigation:

- **Все перегрузки имени должны быть в одном модуле** (или явно
  re-exported в одно место). Не разрешается, чтобы модуль `A` определил
  `f(int)`, а модуль `B` (который импортирует `A`) — `f(str)` с тем же
  именем. Это даёт locality: LLM, читая модуль `A`, видит **все**
  перегрузки `f` в нём.
- **Hover в LSP** показывает все доступные перегрузки с их сигнатурами.
- **Compile error при ambiguity** включает список кандидатов — LLM
  видит конкретный путь починки.

### Что отвергнуто

- **Перегрузка только по receiver-типу (текущее частичное состояние).**
  Несимметрия со static и свободными функциями; D73 уже работает иначе.
- **Last-wins резолв.** Hidden surprises при добавлении перегрузок.
- **Перегрузка только через protocol-based dispatch (variant 4).**
  Покрывает большинство случаев, но требует явного protocol-объявления
  для тривиальной перегрузки (`exit(int)` / `exit(int, str)` —
  излишне). Protocol-dispatch остаётся как **идиоматичный** путь для
  расширяемых перегрузок (новые типы могут добавлять реализации), но
  не как **единственный** механизм.
- **Перегрузка через namespace-prefix** (`exit::with_msg(code, msg)`).
  Замена синтаксиса — не решение задачи.

### Связь

- [D46](03-syntax.md#d46) — operator overloading: частный случай
  перегрузки методов с фиксированными именами (`@plus`, `@times`).
- [D69](03-syntax.md#d69) — variadic-параметры `...args []T`: D84
  фиксирует правило резолва между variadic и non-variadic перегрузками.
- [D73](08-runtime.md#d73) — `From`/`Into`: формализованный частный
  случай перегрузки `T.from(...)` по типу аргумента и `@into()` по
  типу результата.
- [D77](08-runtime.md#d77) — `TryFrom`/`TryInto`: то же для fallible
  конверсий.
- [D35](03-syntax.md#d35) — методы и static-функции на типе.
- [D40](03-syntax.md#d40) — «один способ делать одно»: D84 не нарушает,
  потому что разные перегрузки решают **разные задачи** (разные типы),
  а не одну.

### Эволюция

- **Q-overloading** в open-questions: статус был ⚠️ PARTIALLY CLOSED —
  методы через Plan 11, свободные функции запрещены. D84 закрывает
  Q-overloading полностью.
- **Plan 11** реализовал перегрузку методов через C-name mangling +
  strict resolution по статическим типам. D84 переиспользует этот
  механизм для свободных функций и static-функций.
- **D73** ввёл context-driven dispatch для `@into()`. D84 формализует
  это как ось 3 общего правила.
- **2026-05-10**: добавлен раздел «Turbofish не обходит concrete» —
  уточнение что `f[T_value](args)` ≡ `f(arg as T_value)`, обе формы
  резолвятся одинаково и concrete-перегрузка доминирует. Триггер —
  обсуждение D87/D88 (specialization для конкретного типа vs generic
  с default).

---

## D267. Method coherence: extension — да, override чужого метода — `E_METHOD_REDEFINITION`

> **Статус:** принято и реализовано ([Plan 154.0](../../docs/plans/154.0-method-override-coherence.md),
> umbrella [154](../../docs/plans/154-no-silent-dispatch.md), 2026-06-13). Закрывает
> `[M-method-override-silent-noop]`.

### Что

У Nova **нет orphan rule** ([02-types §«Структурная проверка вместо impl»](02-types.md))
— метод на типе можно объявить из любого модуля (extension-методы). Но
**переопределение существующего метода** `fn T @m` с **той же сигнатурой**
(receiver-type + arity + arg-types + return + receiver-mut, все оси D84), что у
метода `T.@m`, **уже определённого в другом модуле** (std/prelude/импорт), —
**compile-error `E_METHOD_REDEFINITION`**.

**Исключение:** если пользователь **сам объявил `type T` локально** (shadow всего
типа), методы на этом T — его (Plan 62 user-wins, не override чужого).

### Почему

Раньше это был **silent no-op**: type-check классифицировал дубль как prelude-shadow
(user-wins в `env.fns`), но codegen `method_overloads` резолвит call-site
**first-match**, а prelude/std prepend'ится первым → выигрывает существующее
определение, тело пользователя **никогда не вызывается**. Программист уверен, что
переопределил `str.to_lower`, а поведение не меняется (мёртвый код). Худший исход —
не ошибка, не override.

Глобальный override built-in/std метода к тому же **coherence-хазард**: stdlib и
чужие библиотеки, зовущие `to_lower` внутри, получили бы нелокальный сюрприз
(проблема monkey-patching из JS-прототипов). Прецедент строгости: Rust orphan rule,
Kotlin/C#/Swift (member/inherent wins, extension не переопределяет), Go (нельзя
добавлять к чужим типам).

### Как сделать «свой» метод (разрешённые пути)

- **Extension с другим именем:** `fn str @shout()` — ок (нет orphan rule).
- **Overload по сигнатуре:** `fn T @m(int)` + `fn T @m(str)` (D84).
- **Receiver-mut overload:** `fn T @m()` + `fn T mut @m()` ([D-Plan 135](#)).
- **Newtype + own-method:** `type Locale { use _ str }` + `fn Locale @to_lower()` —
  override-precedence ([02-types §«Override через own-methods»](02-types.md)).
- **Локальный re-decl типа:** `type Range {...}` + `fn Range @step_by(...)` (Plan 62
  user-wins; receiver-тип объявлен локально).

### Реализация

[types/mod.rs](../../compiler-codegen/src/types/mod.rs) `check_module`, `Item::Fn`:
для метода (`receiver.is_some()`), у которого `classify_dup` = `Some(_)` (shadow'ит
prelude/merged) и receiver-тип **не** в `user_declared_types` (типы из entry-peers) —
`E_METHOD_REDEFINITION`. Site type-check → компиляция падает до codegen.

### Связь

- [D84](#d84) — оси перегрузки (override = совпадение по всем осям; overload = хотя
  бы одна различается → разрешён).
- **Plan 62** — prelude-shadow для type/const/free-fn + локальный type re-decl
  (user-wins + W_PRELUDE_SHADOW) — **не задет** (фикс только для методов на не-локальных
  типах).


## D268. Opt-in конформность протоколов: `#impl(P)` на метод-декларации

**Plan 154.1 Ф.2.** Ведущий атрибут `#impl(P1 + P2 + ...)` (D186, ранее только перед
`type`-декларацией) распространён на **метод**-декларации:

```nova
#impl(Display)
fn int @display(mut sb StringBuilder) -> () { sb.append(@) }
```

### Семантика — opt-in, НЕ номинальная

Конформность остаётся **структурной**: «есть подходящий метод → тип годится для бонда
`[T Display]`». `#impl(P)` — **необязательная** пометка, которая:

1. **проверяет подпись** метода против объявления `@m` в протоколе `P` (параметры +
   возврат + receiver-mutability, с подстановкой `Self` ↔ receiver-тип);
2. **явно привязывает** `P` к receiver-типу `T` — наполняет `type_impl_protocols[T] += P`
   (та же карта, что у type-уровня `#impl`), как если бы `P` был перечислен на
   `type`-декларации `T`.

Отсутствие `#impl` — **не ошибка**: структурная конформность по-прежнему удовлетворяет
бонд. `#impl` только ДОБАВЛЯЕТ проверку + привязку; миграция существующего кода не нужна.
Путь к required-конформности (как в Rust) — отдельным шагом (`[M-154.1-required-conformance]`).

### Три кода ошибок (checker, на методе `fn T @m` с `#impl(P)`)

| код | условие |
|-----|---------|
| `E_IMPL_UNKNOWN_PROTOCOL` | `P` — не известный тип ИЛИ не протокол |
| `E_IMPL_NOT_A_PROTOCOL_METHOD` | имя `@m` ∉ методов `P` (или у `fn` нет receiver'а) |
| `E_IMPL_SIGNATURE_MISMATCH` | подпись `@m` (параметры / возврат / receiver-mut) ≠ объявлению `P` |

Renamed-протоколы (D237) дают `E_PROTOCOL_RENAMED` с подсказкой.

### Реализация

- **AST:** `FnDecl.impl_protocols: Vec<String>` ([ast/mod.rs](../../compiler-codegen/src/ast/mod.rs)).
- **Парсер:** `#impl` разрешён перед `type` И `fn` (`#from_fields`/`#zero_on_move` —
  по-прежнему только перед `type`); `impl_protocols` прокидывается в `parse_fn`
  ([parser/mod.rs](../../compiler-codegen/src/parser/mod.rs)).
- **Чекер:** `verify_method_impl_protocols` в `check_module`
  ([types/mod.rs](../../compiler-codegen/src/types/mod.rs)) — переиспользует
  `check_signature_match` / `check_receiver_mut_match` от type-уровня.
- **Codegen:** метод-уровень `#impl` биндит `type_impl_protocols[recv.type_name] += P`
  ([emit_c.rs](../../compiler-codegen/src/codegen/emit_c.rs) рядом с type-уровневым
  populate'ом).

### Связь

- [D186](../decisions/) (Plan 91.9) — type-уровневый `#impl` + bare-call synthesis gate.
- [D237](#) — renamed-протоколы (Display/Debug/Equal/...).
- [D269](#d269) — первый потребитель: конкретные Display/Debug примитивов.


## D269. Конкретные Display/Debug примитивов + element-dispatch hardening

**Plan 154.1 Ф.1 + Ф.3.** Закрывает класс silent-mis-dispatch «primitive-element
protocol-метод».

### Проблема

Mono-тело контейнера `Vec[T] @debug` вызывает `@data[i].debug(sb)` на ЭЛЕМЕНТЕ. Когда
`T` — примитив без конкретного `@debug`, single-key `method_receivers`-fallback в codegen
тихо роутил вызов в erased-стаб самого контейнера (`Nova_Vec_method_debug`,
type-confused no-op) — `vec_debug_pos` выдавал мусор.

### Fix 1 — громкая ошибка (Ф.1)

В instance-call dispatch ([emit_c.rs](../../compiler-codegen/src/codegen/emit_c.rs),
зеркало static-guard'а): если receiver — примитивный C-тип, а single-key fallback
резолвит `method` в generic-тип (`is_instance && is_generic_type`), вызов невозможен →
`E_PRIMITIVE_NO_PROTOCOL_METHOD` (вместо эмиссии неверного/no-op вызова).

### Fix 2 — конкретные impl примитивов (Ф.3, Variant B)

`int / f64 / bool / char / str` получают конкретные `#impl(Display)` + `#impl(Debug)`
([std/prelude/protocols.nv](../../std/prelude/protocols.nv)). Тогда element-dispatch
резолвит `int.debug` через direct-dispatch (`Nova_int_method_debug`) — мис-диспатч
устранён «даром», и guard для `int` не срабатывает (метод есть).

- **Тела:** `sb.append(@)` через типизированные `@append`-overload'ы StringBuilder
  (`@append(int)`/`@append(f64)`/`@append(bool)` добавлены; `str`/`char`/`[]u8` были) —
  без промежуточной `nova_str` от interp-temp. Debug `char`/`str` (кавычки+escape)
  использует interp `${@:?}` → conv.h `nova_*_to_debug_str` напрямую (не рекурсирует:
  interp для примитивов зовёт форматтер, не метод).
- **f32** (followup 2026-06-14, `e38f30ee`): догнал остальные — conv.h `nova_f32_to_str`/
  `_to_debug_str` (widen→double + `%g`, 6-знач прячет f32→f64 хвост), `@append(f32)`
  (`x as f64`), ветка `nova_f32` в interp display+debug map + exclusion list. (Остался
  отдельный коэрсинг-баг `Vec[f32].from([f64-литералы])` — `[M-154.1-f32-literal-coercion]`,
  не про печать.)

### Инвариант interp

`${x}` / `${x:?}` для примитивов по-прежнему идут через conv.h-форматтеры напрямую
(interp-путь исключает примитивы из method-dispatch), поэтому конкретные методы их не
перехватывают и не ломают.

### Followups codegen-hardening (2026-06-14)

Два общих codegen-фикса, всплывших при доведении f32:

1. **Self-method-call overload по типам аргументов** (`e38f30ee`). Вызов `@m(args)` (receiver
   `@`, внутри тела метода) при нескольких overload'ах тайбрейкал **только** по
   receiver-mutability (Plan 135 Ф.2), игнорируя типы аргументов. С `@append`-overload'ами (все
   `mut`) `@append(x as f64)` брал ПЕРВЫЙ (базовый `str`) → C передавал `nova_f64` в `nova_str`-
   параметр. Фикс: сперва сузить own-instance overload'ы по `param_c_types == arg_c_types`, потом
   тайбрейк по `recv_mutable`. (Existing self-`@append` всегда целили `str`-базу → латентный баг.)
2. **`E_UNKNOWN_STATIC_METHOD`** (`99dee599`, `[M-154.1-static-call-unresolved-loud]`). Вызов
   `Prim.method(...)` на примитиве, дошедший до codegen fall-through (все валидные primitive
   static-методы/интринсики — `str.from`, `str.from_bytes_lossy` — резолвятся раньше), раньше
   эмитил `nova_fn_<prim>_<method>` → undefined-символ на линковке. Теперь — громкий compile-error.
   Узко: только примитив-ресиверы (модуль-qualified free-fn и user-типы не задеты).

### Связь

- [D268](#d268) — `#impl` opt-in (механизм привязки протокола).
- [D229](#) / [D237](#) — Display/Debug протоколы + `${:?}` interp-spec.
- [D267](#d267) — sibling из той же зоны диспатча (cross-module override coherence).

---

## D263. Vec restructure-ops + оператор `+` (`@plus` ≡ `@concat`)

**Plan 153.5** (commit `e8f700e4`). `Vec[T]`/`[]T` получают слой **restructure-ops** —
операции, строящие НОВЫЙ вектор из существующих данных или переставляющие целые прогоны
элементов. Реализовано на Nova-body поверх bulk `RawMem.copy`
([std/collections/vec/restructure.nv](../../std/collections/vec/restructure.nv), co-equal файл
folder-модуля `collections.vec`). Закрывает **Q-vec-operator-plus** (см.
[open-questions.md](../open-questions.md)).

### Решение

| Метод | Сигнатура | Семантика |
|---|---|---|
| **concat** | `Vec[T] @concat(other Vec[T]) -> Vec[T]` | non-mutating join: одна аллокация ровно на `a+b` + два bulk-copy; операнды нетронуты |
| **оператор `+`** | `Vec[T] @plus(other Vec[T]) -> Vec[T] => @concat(other)` | `a + b` = НОВЫЙ Vec (как str `@plus`, [D46](03-syntax.md#d46)); `a += b` ≡ `a = a + b` |
| **rotate_left** | `Vec[T] mut @rotate_left(n int) -> @  requires n >= 0` | циклический сдвиг влево in place; `n mod len`; O(len) time, O(min(n,len−n)) scratch |
| **rotate_right** | `Vec[T] mut @rotate_right(n int) -> @  requires n >= 0` | сдвиг вправо ≡ left на `len − k` |
| **drain** | `Vec[T] mut @drain(range Range) -> Vec[T]  requires start>=0 && end>=start && end<=@len` | вырезать `[start,end)`, вернуть удалённое владеемым; суффикс сдвигается вниз; `self` короче на `range.len()` |
| **insert_slice** | `Vec[T] mut @insert_slice(i int, sl []T) -> @  requires 0<=i && i<=@len` | вставить срез на `i` (`i==len`=append); делегирует в `@splice` (под [D239](02-types.md#d239-t--синтаксический-псевдоним-vect) `[]T` ЕСТЬ `Vec[T]`) |

**Принципы.**
- **`+` не мутирует** (как Kotlin/Python/Ruby `+`): `a + b` — свежий буфер, операнды нетронуты.
  Рост `a` in place — отдельный `a.append(b)` (mutate.nv); `a += b` лоуэрится в свежий concat-Vec,
  НЕ в in-place append (`+=` ≡ `=`-через-`+`, единая семантика оператора).
- **`@concat` ≠ `@append`.** `@append(Vec[T])` (mutate.nv) — in-place bulk-merge в `self`;
  `@concat` — новый Vec. Один слой = одна семантика (инвариант I4 плана).
- **`@insert_slice` = `[]T`-вариант `@splice`** (берёт `Vec[T]`): имя документирует slice-аргумент
  (Rust `Vec::splice` / Go `slices.Insert`), тело — делегация (overlap-safe → self-insert корректен).
- **Контракты `requires`** (rotate/drain/insert_slice) — out-of-bounds/отрицательный сдвиг →
  runtime-panic (D13-семантика контрактов).

### Codegen (operator `+` / `+=`)

`@plus`/`@concat` — Nova-body; **operator-lowering** `+`/`+=` добавлен в
[emit_c.rs](../../compiler-codegen/src/codegen/emit_c.rs) двумя минимально-таргетными точками:
1. `Stmt::Assign`: `a += b` / `a -= b` на типе с method-лоуэрингом `+` (`nova_str`, `Nova_Vec____*`,
   любой `Nova_*`-record с `@plus`) десугарится в синтез-`Binary{Add}` → re-emit через полный
   binop-dispatch (а не сырой C `a += b` на struct/pointer операнде → CC-FAIL). Перегружаемы только
   `Add`/`Sub`.
2. `BinOp::Add`: `Vec[T] + Vec[T]` → `vec_method_call(.., "plus", ..)` ПЕРЕД generic-`Nova_*`-sum-pointer
   Add-arm (тот эмитит голый `_method_plus(l,r)` без инстанцирования mono-тела → undefined symbol на
   линковке); `vec_method_call` регистрирует mono-инстанс первым.

### AMEND (2026-06-14, `plan-153.5-restructure` commits `1c323d0e` + `16753d23`): flatten реализован

`[][]T.flatten() -> []T` **больше НЕ отложен** — реализован вместе с фундаментом
**вложенных generic-ресиверов произвольной глубины** (см. [D145 AMEND](02-types.md#d145-fnt-префикс--receiver-generic-decl--bounds-plan-101),
2026-06-14). Маркер `[M-153.5-flatten-nested-receiver]` **РАЗРЕШЁН**.

| Метод | Сигнатура | Семантика |
|---|---|---|
| **flatten** | `Vec[Vec[T]] @flatten() -> Vec[T]` (≡ `[][]T @flatten() -> []T` под D239) | конкатенация внутренних рядов в один новый `Vec[T]`; pre-size `with_capacity(Σ inner.len())` + bulk `@append(inner)` на ряд (copy-fast-path, операнды нетронуты); пустые ряды/внешний — корректно |

Реализация — Nova-body в
[std/collections/vec/restructure.nv](../../std/collections/vec/restructure.nv):
сперва суммирует все `inner.len()` для точной пре-аллокации `out`, затем bulk-копирует
каждый ряд `out.append(inner)` (тот же `RawMem.copy` fast-path, что `@concat`/`@append`).
Production-форма — **carrier** `Vec[Vec[T]] @flatten()` (совпадает с записью stdlib).

**Что разблокировало (корень — D145 AMEND):** обе принимаемые формы ресивера раньше
теряли вложенность — `Vec[Vec[T]]` ПАРСЕР отвергал в carrier-слоте, `[][]T` монорфизатор
биндил `T` в *непосредственный* элемент (`Vec[int]`), не во *внутренний* (`int`). Фикс —
**структурная унификация typevar для вложенных ресиверов на любой глубине** в ОБОИХ
парсере (carrier-слот принимает `parse_type` + сбор free-typevars; slice-форма считает
глубину `[]` и спускается до внутреннего `Named`) И монорфизаторе (рекурсивный
`infer_type_param_binding` биндит receiver-typevar в element-of-element… до самого
внутреннего, depth-agnostic). Cross-cutting: тот же `[]T`-method-dispatch путь, через
который идут все slice-методы stdlib — flat `[]T` (depth 1) остался byte-identical.

### Связь

- [D46](03-syntax.md#d46) — operator overloading через `@plus` (str-прецедент; этот D-блок
  распространяет на `Vec[T]`).
- [D239](02-types.md#d239-t--синтаксический-псевдоним-vect) — `[]T ≡ Vec[T]` (insert_slice-аргумент; `Vec[Vec[T]] ≡ [][]T` для flatten).
- [D145](02-types.md#d145-fnt-префикс--receiver-generic-decl--bounds-plan-101) AMEND — вложенные generic-ресиверы произвольной глубины (фундамент `@flatten`).
- Plan 153.1 mutate.nv — `@append`/`@splice` (in-place; restructure НЕ дублирует; `@flatten` переиспользует bulk `@append`).

---

## D285 — Receiver-compatibility rule — blanket dispatch priority (Plan 164 Ф.3)

**Status:** ACTIVE (Plan 164 Ф.3, 2026-06-16). **Зависит от:** [D355](02-types.md#d355--blanket-protocol-receiver-methods-plan-161-2026-06-15) (blanket protocol-receiver), [D84](#d84-перегрузка-функций-и-методов-четыре-оси-резолв-по-самому-специфичному-матчу) (dispatch priority). **Маркеры:** `[M-codegen-blanket-generic-param-order]` ✅ CLOSED Plan 164 Ф.2; `[M-153.2-drop-z-prefix]` ✅ CLOSED Plan 164 Ф.4; `[M-impl-attr-generic-protocol]` ✅ CLOSED Plan 164 Ф.1.

**§1 Проблема (root cause).** До Plan 164 Ф.3 codegen при резолве метода по имени применял стратегию «last-wins» из `method_receivers` (HashMap, один FnDecl на ключ `(type, name)`). При наличии конкретного метода с тем же именем на несовпадающем типе (напр., `CharsIter.count()` из `std/strings`) он побеждал над blanket-методом `fn[I Next[T]] I @count()` для `FilterIter` — потому что конкретный регистрировался позже и перезаписывал запись. Результат: `FilterIter.count()` диспетчился в `CharsIter.count()` → CC-FAIL.

**§2 Правило (receiver-compatibility).** При резолве вызова `recv.method(args)`:
1. Ищется **конкретный** метод с точным совпадением receiver-типа (`method_receivers[base_type]` или `method_overloads`). Если найден — используется. Конкретный метод всегда приоритетнее blanket (D84 §«по receiver-типу»).
2. Если конкретный не найден — ищется **blanket**-метод (typevar-ресивер, bound = протокол). Receiver считается совместимым, если его тип зарегистрирован в `type_impl_protocols[base_type]` для соответствующего протокола.
3. **Concrete метод на НЕСОВПАДАЮЩЕМ типе не может победить blanket на СОВМЕСТИМОМ типе.** «Несовпадение» = base_type в `method_receivers` не совпадает с actual receiver.

**§3 Алгоритм (codegen, Plan 164 Ф.3) — СОСТОЯНИЕ ОРАКУЛА НА ДАТУ, НЕ ПРАВИЛО (амендмент 2026-08-16, [D464](#d464--бáунд-как-фильтр-отбора-отсев-без-ранжирования-2026-08-16)).** Нормативно здесь §2; §3 описывает, КАК легаси-кодоген `emit_c.rs` восстанавливает решение из уже утраченной информации — разбором мангл-имени, потому что чекер ему решения не передавал. Это обход, а не норма: проверка бáунда живёт в чекере, кодоген читает принятое решение (D310/D122/D458, D464). Оставлено как запись реализации оракула. Перед lookup в `method_receivers` (last-wins single-key fallback):
1. Извлечь base-type из `obj_ty` (strip `Nova_`/`NovaValue_` prefix, `*`, часть до первого `____`).
2. Если не примитив → сканировать `mono_method_decls` по имени метода, найти blanket-FnDecl (typevar-key длиной ≤2 символов, all-uppercase).
3. Проверить, что все bounds blanket-fn удовлетворены: для каждого bound `Proto` проверить `type_impl_protocols[base_type]` через `impl_spec_base_name`.
4. Если все bounds OK → диспетчить через blanket (bind receiver typevar → concrete type). Иначе — продолжить в fallback.

**§4 Приоритет impl-протоколов — МЕХАНИЗМ ОРАКУЛА НА ДАТУ, НЕ ПРАВИЛО (амендмент 2026-08-16, тот же класс, что §3; вскрыто вопросом окна 274 и аудитом спеки).** Фраза «отсутствие = тип не участвует в blanket» описывала, как НАПОЛНЯЕТСЯ таблица `type_impl_protocols` в `emit_c.rs` (Plan 164 Ф.1, `impl_spec_args_text`) — и, прочитанная как норма, противоречила [D186](02-types.md#d186--impip1--p2---opt-in-annotation-для-protocols) («через bound / coercion использование structurally-подходящего типа работает — `#impl` не требуется») и [D464](#d464--бáунд-как-фильтр-отбора-отсев-без-ранжирования-2026-08-16). НОРМА: участие типа в blanket-отборе — СТРУКТУРНОЕ (D53/D186/D464), проверяется чекером из бáунда; `#impl` остаётся воротами ambient-синтеза и проверкой декларации (D186), к отбору отношения не имеет. Ограничение оракула «blanket-диспетч видит только #impl-регистрации» — состояние реализации; novac реализует норму (чекер → канал → кодоген), это не расхождение. Исходный текст §4 сохранён строкой ниже как запись реализации: `type_impl_protocols[T]` пополняется из атрибута `#impl(P[U])` на декларации метода; наличие записи = декларация намерения; отсутствие = оракул (на 2026-08-16) не ведёт тип в blanket-диспетче.

**§5 Ограничения V1.** (a) Алгоритм §3 сканирует `mono_method_decls` O(N) по имени — достаточно для текущего масштаба. (b) Только один blanket-кандидат per-name per-receiver предполагается (конфликт двух blanket = `E_BLANKET_CONFLICT`, D355 §5). (c) Receiver-type с несколькими `#impl` протоколами корректен — каждый bound проверяется независимо.

**Кросс-ссылки:** [D355](02-types.md#d355--blanket-protocol-receiver-methods-plan-161-2026-06-15) (blanket-receiver dispatch), [D84](#d84-перегрузка-функций-и-методов-четыре-оси-резолв-по-самому-специфичному-матчу) §1 (concrete > blanket приоритет по receiver-типу), [D268](#d268-method-coherence-extension--да-override-чужого-метода--e_method_redefinition) (method coherence — запрет override чужого метода).

**Реализовано в.** `compiler-codegen/src/codegen/emit_c.rs` (~line 25289): блок receiver-compatibility dispatch (Plan 164 Ф.3, commit `af33bc76`). `compiler-codegen/src/parser/mod.rs`: `impl_spec_base_name()` / `impl_spec_args_text()` helpers (commit `3846a976`). `compiler-codegen/src/types/mod.rs`: `verify_impl_protocols` + `check_signature_match_with_subst` + `normalize_type_str` (commit `3846a976`). `std/collections/vec_iter_zc.nv`: `#impl(Next[T/U])` annotations на adapter `@next()` методах; `VecIter[T]` `@next()` также annotated (commit `70079788`). Тесты: `nova_tests/plan164/` (6/6 PASS).

---

## D464 — Бáунд как фильтр отбора: отсев без ранжирования (2026-08-16)

**Статус:** accepted (по запросу окна 274 перед этапом Э2-б3; решение
интегратора в рамках плана 274, владелец не возражал). **Зависит от:**
[D72](02-types.md#d72-generic-bounds-через-t-protocol--protocol-как-тип)
(бáунд как protocol-тип), [D84](#d84-перегрузка-функций-и-методов-четыре-оси-резолв-по-самому-специфичному-матчу)
(фильтры резолва), [D285](#d285--receiver-compatibility-rule--blanket-dispatch-priority-plan-164-ф3)
(конкретное над бланкетом), [D310](02-types.md#d310-type-set-bounds-plan-1723) /
[D122](02-types.md#d122) / [D458](02-types.md#d458) (звучность в чекере,
лоуэринг в кодогене).

### Зачем этот блок

До него в спеке жили ДВА режима бáунда, оба нормативные, и ни один блок
не говорил, когда работает который:

* [D72](02-types.md#d72-generic-bounds-через-t-protocol--protocol-как-тип):
  бáунд — **ошибка**, если конкретизация ему не удовлетворяет;
* [D285](#d285--receiver-compatibility-rule--blanket-dispatch-priority-plan-164-ф3) §2
  и [D84](#d84-перегрузка-функций-и-методов-четыре-оси-резолв-по-самому-специфичному-матчу):
  бáунд — **отпадение кандидата** при отборе перегрузок.

Разница видна пользователю. Два кандидата подходят по имени, один не
удовлетворяет бáунду: компилятор либо молча берёт оставшегося, либо говорит
«неоднозначно», либо ошибается на бáунде. Это поведение языка, а не деталь
реализации, — и по правилу проекта спека пишется до реализации.

### Правило

Бáунд — **фильтр отбора**, а не ось ранжирования. В четырёх фильтрах
[D84](#d84-перегрузка-функций-и-методов-четыре-оси-резолв-по-самому-специфичному-матчу)
он стоит **между фильтром 2 (типы аргументов) и фильтром 3 (тип результата)**
и звучит так:

> **Фильтр 2б — бáунды.** Для каждого кандидата с generic-параметрами
> подстановка, выведенная из аргументов, проверяется против бáундов этих
> параметров. Кандидат, у которого хотя бы один бáунд не держится,
> **отбрасывается**. Кандидат без generic-параметров фильтр проходит.

Дальше — как записано в D84 без изменений: фильтр 3 (тип результата),
фильтр 4 (самый специфичный по трём **структурным** правилам: конкретное над
generic, non-variadic над variadic, подтип над супертипом), и если ни один не
доминирует — `ambiguous overload` с перечислением кандидатов.

Три следствия, и каждое отвечает на конкретный вопрос окна 274:

1. **Один выживший — берётся молча.** Отсев по бáунду — обычное поведение,
   не предупреждение: автор бланкета с бáундом сказал ровно это.
2. **Выживших больше одного и никто структурно не доминирует —
   неоднозначность.** Никакого «чей бáунд уже, тот лучше»: ранжирования по
   бáундам **нет**. Ср. rustc и Go; ранжирование по «наилучшему совпадению»
   дало Swift экспоненциальный резолвер, C# — спеку на десятки страниц.
   Переход «ошибка → ранжирование» позже возможен без ломки программ,
   обратный — нет; в v0.1 дверь держится открытой в ту сторону, куда можно
   будет пойти.
3. **Ни одного выжившего — это и есть D72.** Ошибка бáунда возникает, когда
   после отсева не осталось кандидатов, и текст ошибки называет бáунд,
   который не удержался (`E_BOUND_NOT_SATISFIED`, D72), а не «имя не найдено».
   Отдельного «режима D72» нет — это пустой результат фильтра.

**Единственная ось приоритета, которая ОСТАЁТСЯ, — структурная:** конкретный
метод на точно совпадающем receiver побеждает бланкет
([D285](#d285--receiver-compatibility-rule--blanket-dispatch-priority-plan-164-ф3) §2 п.1,
D84 фильтр 4 п.1). Это не ранжирование по бáундам, а правило «специфичное
имя против обобщённого», и его отменять нельзя: первая же реализация «без
ранжирования вообще» сломала бы `FilterIter.count()`, ради которого D285 и
появился.

### Где проверяется — норма

Проверка бáунда — **в чекере**, до мономорфизации, из самого бáунда:
`[T Display]` решается по объявленному контракту, ему не нужен
инстанцированный код. Кодоген получает **уже принятое решение** (какой
кандидат выбран, какая подстановка) через канал и лоуэрит его; он не место,
где ошибка бáунда возникает впервые. Ровно так формулируют
[D310](02-types.md#d310-type-set-bounds-plan-1723) / [D122](02-types.md#d122) /
[D458](02-types.md#d458), и так же — эталон rustc: ошибка бáунда есть ошибка
типизации, а не инстанцирования (в отличие от шаблонов C++).

Непроверяемо до мономорфизации только **несвязанное** `T` — у которого бáунда
нет вовсе (D72: «структурное соответствие при использовании»). Оговорка
амендмента 221.1 в D186 «bound-satisfiability is a mono-time concern» говорила
именно об этом случае и была записана шире, чем следовало; поправлена той же
волной.

**Разбор мангл-имени назад в тип** для восстановления решения — не норма
никогда. [D285](#d285--receiver-compatibility-rule--blanket-dispatch-priority-plan-164-ф3)
§3 описывал именно такой обход в легаси-кодогене оракула (`emit_c.rs`:
снять префиксы, взять кусок до первого `____`, найти бланкет по typevar
«≤2 символов, all-uppercase»); он переведён в раздел «Реализовано в» с
пометкой «состояние оракула на дату, не правило». Реализация, идущая
чекер → канал → кодоген без разбора имён, **исполняет норму**, а не
расходится с оракулом — заводить расхождение в реестр не нужно.

### Что НЕ решает

* Не вводит ранжирование по бáундам — намеренно (см. следствие 2).
* Не меняет `#impl`: он остаётся воротами ambient-синтеза (D186), к отбору
  по бáунду отношения не имеет.
* Не трогает `E_BLANKET_CONFLICT` ([D355](02-types.md#d355--blanket-protocol-receiver-methods-plan-161-2026-06-15) §5):
  два бланкета на одном имени и receiver — по-прежнему ошибка декларации,
  а не неоднозначность вызова.

### Реализовано в

Оракул: `compiler-codegen/src/types/mod.rs::verify_impl_protocols` /
`check_signature_match_with_subst` (чекер), receiver-compatibility в
`emit_c.rs` (~25289, обход — см. D285 «Реализовано в»). Novac: этап Э2-б3
(план 274), проверка в чекере, решение публикуется в канал.
