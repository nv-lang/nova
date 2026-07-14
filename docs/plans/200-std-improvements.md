<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 200 — зонтичный план улучшений std

**Статус:** 📋 ЖИВОЙ РЕЕСТР — **НЕ закрывать** (владелец 2026-07-12: «будем добавлять много новых штук»).
**Приоритет:** ниже Plan 196 (196 — высший, без остановки). Пункты 200
**file-disjoint** от компилятора (правят `std/*.nv` + спеку/конвенцию), могут идти параллельно/после 196
отдельными дешёвыми коммитами. Каждый пункт = свой D-реф, своя приёмка, свой коммит; НЕ мега-коммит.

## Назначение

Единая точка сбора и секвенирования эргономических/корректностных улучшений stdlib. Реестр: по мере
появления новых std-улучшений — пункт сюда (с D-рефом и приёмкой), не плодить микро-планы.

---

## Пункт 1 — `Vec[T].new(cap int = 0)` точный pre-alloc конструктор

**Статус:** ✅ СДЕЛАНО 2026-07-12 (plan-200 агент [sonnet]) — `[M-vec-new-cap-default-arg-backfill]`
зачинен в РЕЗОЛВЕ (не заплатка), код внедрён. **Спека:** D372-amend2 (`spec/decisions/02-types.md`),
таблица `03-syntax.md`, пример `syntax.md`.

**Бывший блокер `[M-vec-new-cap-default-arg-backfill]` — ЗАКРЫТ, корень НЕ в 196-зоне.** Диагностика
подтвердила: класс бага **НЕ** `infer_call_ret_c`/mono-clone (196.2 W2) — это чисто callnorm-classification
gap, вне запретной зоны. Root cause: `try_normalize_call` (`callnorm.rs`) резолвит callee → `FnDecl.params`
ОДНИМ classify-match'ем (`Ident`→free / `Path`(2)→static / `Member`→instance) — единственный резолв-путь,
которым ЛЮБОЙ вызов (включая обычные static/free) доходит до `param.default` перед backfill'ом. Для
generic-static-ctor (`Type[Args].method(...)` турбофиш, `[]T.method(...)` slice-sugar D38/D239) парсер кладёт
turbofish/`__array`-Path ОДНИМ уровнем ВНУТРИ `Member.obj`, а не на верхнем уровне `func.kind` — тот же
`Member`-арм, что и обычный `obj.method()`. Этот арм БЕЗ рассмотрения этой формы шёл сразу в
`instance_by_name` (там нет ctor-имени "new") → резолв обрывался → default НЕ подставлялся. Фикс — ТА ЖЕ
точка: `Member`-арм теперь СНАЧАЛА проверяет, не является ли `obj` type-position turbofish/`__array`-путём
(если да — тот же `static_methods`, что уже используют `Path`-static-вызовы), и только иначе падает в
`instance_by_name` — сходимость на существующий резолв-путь, не новый резолвер. Отдельно найден и зачинен
ВТОРОЙ, независимый источник арности "too few arguments — cap": 3 места в `emit_c.rs`
(`try_emit_typed_vec_literal` — литерал `[...]`; `ParallelFor`-аккумулятор; rest-bind `[a, ...rest]`),
которые синтезируют `Vec.new`-вызов НАПРЯМУЮ C-строкой (нет Nova-AST `Call`-узла вообще → callnorm их не
видит по построению) — арность руками приведена к новой C-сигнатуре (`(0)`). Оба класса вне заявленной
196-зоны (`infer_call_ret_c` 46293-48883 / mono-clone) — 196-агент не тронут.

**Что:** заменить 0-арг `Vec[T].new()` на `Vec[T].new(cap int = 0) -> Self` (default-аргумент, ОДНА
функция — не overload). `new()` = пусто (cap 0, без аллокации, как сейчас); `new(cap: 1024)` = ровно 1024
слота, len 0 (именованная форма — самодокументирует намерение; позиционная `new(1024)` тоже легальна).

**Семантика (владелец 2026-07-12) — три разных намерения:**
- `new(cap)` и `@cap(n)` — **ТОЧНАЯ** ёмкость, без округления.
- `@reserve(additional)` — **амортизированный** рост, округление ВВЕРХ до степени 2 (8→16→32…).
  Уже такова (`std/collections/vec/core.nv:305`) — НЕ трогать.

**Правки кода (СДЕЛАНО):**
- `std/collections/vec/core.nv:98` — `Vec[T].new()` → `Vec[T].new(cap int = 0)`; тело: `cap == 0` → пустой
  путь (`null_buf`); `cap > 0` → `alloc_buf[T](cap)`, `len 0`, `cap` (одна аллокация).
- `std/collections/vec/core.nv:198` — `from`: `Vec[T].new().cap(items.len()).extend(items)` →
  `Vec[T].new(cap: items.len()).extend(items)`.
- Doc-комментарии `new` обновлены.
- Компиляторный фикс (см. выше): `compiler-codegen/src/callnorm.rs` (`try_normalize_call` classify-match) +
  `compiler-codegen/src/codegen/emit_c.rs` (3 hand-formatted ctor-call сайта).

**Риск/секвенс — снят.** `new(cap int = 0)` — default-arg, ОДНА функция → НЕ триггерит
`[M-vec-new-static-arity-overload]` (тот overload-класс отдельный, `from_raw_parts` остаётся именованным до
своего фикса — ниже).

**Связанный фолд (ОТДЕЛЬНО, в Plan 196.2 W2, НЕ этим коммитом):**
`fn Vec[T].from_raw_parts(ptr *T, len int, cap int) -> Self`
⇒ `fn Vec[T].new(ptr *mut T, len int, cap int) -> Self require cap >= len`
(3-арг overload + `*mut T` напрямую вместо `unsafe { ptr as *mut T }` + контракт `cap >= len`). Возможен
ТОЛЬКО после фикса `[M-vec-new-static-arity-overload]` (arity-overload cross-wiring — ДРУГОЙ класс: codegen
второе окно путает 0-арг/3-арг перегрузки; НЕ то же самое, что default-arg backfill выше, и НЕ закрыт этим
коммитом). До фикса `from_raw_parts` остаётся именованным. Отслеживается 196.2, НЕ здесь.

**Приёмка (все зелёные 2026-07-12):** conformance PASS 3/3 (single-CU; включая НОВЫЙ regression-тест на
`[M-vec-new-cap-default-arg-backfill]` — `spec_tests/conformance/d372_canonical_new_defaults.nv` +
`types_generic_static_ctor.nv` peer; red-before/green-after подтверждён на baseline-бинаре); byte-parity
(`std/collections` — единственный CC-FAIL `vec_lazy` pre-existing δ0, см. Plan 172.12); `nova test
std/collections/vec` — PASS (`access`, тот самый repro-кейс); `nova test std/runtime/string` — чистая
компиляция; D372-amend2 в спеке (сделано, расхождение спека/код снято).

---

## Пункт 2 — `priv(type)` → `priv` для итераторов

**Статус:** ✅ СДЕЛАНО 2026-07-12 (в main `c81d28419`). **Спека/конвенция:** `docs/nv-coding-style.md:221`.
`str` (`std/prelude/core.nv:211`) сознательно НЕ тронут — lang-item (ABI-мост к `nova_str`, bootstrap
pre-method, Plan 139.1); больший blast-radius, приёмочных тестов нет → **открытый вопрос владельцу.**

**Что:** итераторные типы задают поля через `value priv(type)`; правильно — field-level `priv` (D281
module-boundary). Мотивация `priv(type)` для полей итератора отсутствует и ложно-строга: по **D267** любой
модуль может писать extension-методы для типа → `priv(type)`-поля обходимы; реальная внешняя граница — `priv`
(module). Коллекционные итераторы (`VecIter` и др.) уже на field-`priv` — эталон.

**Правки:**
- `docs/nv-coding-style.md:221` — исправить правило (итераторы → `priv`, не `priv(type)`).
- 5 `*Iter`-типов на field-level `priv`:
  - `CharsIter` (`std/runtime/string/chars.nv:58`)
  - `CharIndicesIter` (`std/runtime/string/chars.nv:155`)
  - `GraphemesIter` (`std/runtime/string/unicode/graphemes.nv:84`)
  - `SentencesIter` (`std/runtime/string/unicode/sentences.nv:81`)
  - `WordsIter` (`std/runtime/string/unicode/words.nv:219`)
- **Проверить `str`** (`std/prelude/core.nv:211` `type str value priv(type)`) — ОТДЕЛЬНО: возможно намеренно
  (тип-обёртка над буфером с инвариантами), решить по месту, не менять слепо.

**Приёмка:** `nova test std` без новых фейлов.

---

## Пункт 3 (КАНДИДАТ, НЕ согласовано) — единый префикс протоколов `As*`

Владелец (2026-07-12) высказал мысль: протоколы именовать `As…` (`AsEqual`/`AsCompare`/`AsHash`/…, по образцу
`AsSlice`). Только идея. Объём большой — затрагивает `Equal`/`Compare`/`Hash`/`Display`/`Clone`/… (D229/D230/
D262 и весь std + conformance). Требует отдельного решения (D + миграция). Держу как открытый вопрос до go.

---

## Пункт 4 — полная миграция на `.new(cap int=0)` (D9 «один путь») — НАЙДЕНО 94 сайта

**Статус:** ✅ СОГЛАСОВАНО 2026-07-12, В РАБОТЕ [sonnet, фон] (владелец: «сделай в фоне, не останавливаясь»).
**Не блокер** (chain `.new().cap(n)` легален по D372-amend2), приводим к единой канонической форме.

**Найдено:** **94× `.new().cap(...)` в 42 файлах** (std/tests/conformance). НЕ все Vec — много HashMap(13)/
Set/Queue/StringBuilder/WriteBuffer.

**Два шага к D9 «один путь»:**
1. **Расширить `new(cap int=0)`** на остальные cap-типы: `StringBuilder`, `WriteBuffer`, `HashMap[K,V]`,
   `Set[T]`, `Queue[T]` (семантика `cap(n)` того типа — D372-amend2 «по мере миграции»). Компилятор теперь
   поддерживает (callnorm-фикс `ebcefc703`). Для каждого — регресс-тест (0-арг backfill).
2. **Мигрировать** все 94 `.new().cap(n)` → `.new(cap:)` (Vec + новые cap-типы). Per-файл.

**Приёмка:** `.new().cap(` → 0 вне самих сеттеров; `nova test std` без регресса; conformance 95/0.
**Приоритет:** ниже 196; можно агентом (механика + API-расширение), когда скажешь.

---

## Пункт 5 — фолд `from_raw_parts` → `new(ptr, len, cap)` overload (D372-amend1)

**Статус:** ✅ СДЕЛАНО 2026-07-12 (форс-фикс, sonnet, worktree `nova-capmig`) — `[M-vec-new-static-arity-overload]`
ЗАКРЫТ. Разблокирован по указанию владельца 2026-07-12.

**Что сделано:**
```nova
export fn Vec[T].new(ptr *mut T, len int, cap int) -> Self
    requires len >= 0 && cap >= len
=> { data: ptr, len, cap }
```
3-арг overload конструктора `new` рядом с `new(cap int = 0)`, `*mut T` НАПРЯМУЮ (без reinterpret-cast в теле —
кастует теперь caller, см. `str.@bytes()`). `from_raw_parts` удалён; единственный call-сайт
(`std/runtime/string/core.nv` `str @bytes()`) переведён на `Vec[u8].new(unsafe { @ptr as *mut u8 },
@byte_len(), @byte_len())`.

**Бывший блокер `[M-vec-new-static-arity-overload]` — ЗАКРЫТ, ВНЕ зоны 196.2/W2** (не `infer_call_ret_c`,
а два co-located name-only arity-blind overload-резолва):
1. `compiler-codegen/src/callnorm.rs` — `Sigs::static_methods` раньше ФИЛЬТРОВАЛ прочь любой `(type,method)`
   c >1 сигнатурой (default-arg backfill просто пропускался для overloaded ctor → 0-арг `new()`-вызовы
   доходили до codegen БЕЗ backfill'а `cap=0`). Фикс: хранить ВСЕ overload'ы + новая `pick_static_params`
   дизамбигуирует по `bind_call_args`-совместимости на каждом call-site (fast-path `candidates.len()==1`
   byte-identical со старым поведением).
2. `compiler-codegen/src/codegen/emit_c.rs` — ветка «1b» (`Type[Args].method(...)` turbofish static-ctor
   call, ~emit_call 32577) резолвила `generic_type_methods[base].find(name)` первым совпадением ПО ИМЕНИ
   (детерминированно — первый `new` в файле, т.е. 0-арг), тогда как соседняя ветка «5b» (instance-method
   generic dispatch) уже имела арность+param-type дизамбигуацию (`[M-138.2-generic-method-overload-mono]`).
   Фикс: та же схема портирована в «1b» (арность → param-C-type → `resolved_callees`-span чекера) +
   per-overload `__<paramtype>` суффикс у mono C-имени (иначе оба overload'а всё равно схлопнулись бы в
   ОДИН C-символ даже при правильном выборе `FnDecl`).

**Приёмка (все зелёные 2026-07-12):** `nova test --full std/collections/vec` — vec-folder без cross-wiring
(PASS 2/2); `vec_of_empty_panic` neg-тест зелёный; `nova test --full std/collections` — PASS 14/14, SKIP 6
(compile-only, без регресса); `nova test --full std/checksums std/crypto` (упражняют `str.@bytes()` через
folded `new`) — PASS 7/7; conformance single-CU (`--positive --compile-error spec_tests/conformance`) —
PASS 95/0; спека D372-amend1 обновлена (`spec/decisions/02-types.md`, «ПОПРАВКА 2» — снята пометка
«ОТКАЧЕНО»).

---

## Подплан 200.1 — скорость `nova test std`

**Статус:** 📋 согласован 2026-07-13, вынесен в [200.1-std-test-speed.md](200.1-std-test-speed.md)
(папочные компайл-юниты для std-тестов по образцу conformance + кеш сборки + профиль медленных
тестов + бенч; критерий — полный std ≤ 15–20 мин без потери покрытия).

---

## Пункт 6 — переименование поля `Vec.data` → `Vec.ptr` (консистентность имени)

**Статус:** ⏸️ НА ПАУЗЕ 2026-07-13 (владелец). **НЕ чистый std-рефактор — это компиляторная ABI-правка**
(см. «Уточнение объёма» ниже). Ценность косметическая (priv-поле, API/поведение не меняются), риск CC-FAIL
реальный → отложено до отдельного решения. **Приоритет:** ниже 196.

**★ Уточнение объёма (recon 2026-07-13) — план ранее мис-скоуплен как «дешёвый агент».** Имя Nova-поля
`data` — неявный **C-ABI контракт**: struct-поля эмитятся ВЕРБАТИМ из Nova-имён
([emit_c.rs:14688-14693](../../compiler-codegen/src/codegen/emit_c.rs), `mangle_field_name` трогает только
C-ключевые слова), а codegen ХАРДКОДИТ `->data` в рукописных быстрых путях Vec: index-write (~25573),
index-read (зеркало), slice/copy-within (~24161-24180), `(({})->data)` (~25510/25512), boxed-element (~25617),
ParallelFor-аккумулятор. Переименование Nova-поля → C-поле `ptr` → все хардкод-`->data` ссылаются на
несуществующий член → **CC-FAIL**. Значит переименование ОБЯЗАНО синхронно править emit_c, разруливая `->data`/
`.data` между ТРЕМЯ структурами: (1) заголовок Vec/массива `{T* data; len; cap}` → переименовать; (2) fat-pointer
протокол-бокса `{.data,.vtable}` (~8974/10486/25195/25200) → НЕ трогать; (3) inline value-массив `{T data[N]}`
(~3320/26385) → НЕ трогать. **haiku/дешёвый агент сюда нельзя** — трап-плотный компиляторный рефактор.
Когда разблокируют — делать самому (или sonnet по этой карте) + rebuild + полная приёмка.

**Что:** поле `mut data *mut T` → `mut ptr *mut T` в `Vec[T]` (и одноимённое поле `data` в `VecIter`,
`std/collections/vec/iter.nv:19`).

**Мотивация — унификация термина.** Сейчас рассинхрон: поле = `data`, аксессор = `@ptr()` (AsSlice/D299),
параметр конструктора = `ptr`, rustc-эталон = `RawVec.ptr`. Переименование поля в `ptr` сводит всё к одному
слову.

**НЕТ коллизии со слотом `@ptr()` (уточнено владельцем):** по property-модели (D84/D409) `@ptr()` (ro-чтение,
`-> *T`), `mut @ptr()` (mut-чтение, `-> *mut T`) и `@ptr` (bare-чтение в теле) — это ВСЕ валидные чтения
свойства поля `ptr` с pointer-вариантностью, как и задумано. Явные аксессоры `@ptr()`/`mut @ptr()`
(access.nv:267/275, тело `=> @data` → станет `=> @ptr`) — это и есть property-read поля, определённый явно ради
D299-вариантности `*T`/`*mut T`.

**Объём (внутренний, поле `priv` → API не меняется):** `@data` → `@ptr` в `access.nv`/`mutate.nv`/`iter.nv`/
`core.nv` (десятки сайтов: `read_at`/`write_at`/`offset`), `other.data` → `other.ptr` (или `other.ptr()`),
поле `VecIter.data`. Явные `@ptr()`/`mut @ptr()` аксессоры остаются (D299 AsSlice).

**Приёмка:** `nova test std/collections/vec` без регресса; conformance δ0; греп `\bdata\b` в vec-модуле = 0
(кроме комментариев/строк). Механика — можно дешёвым агентом.

---

## Пункт 7 — единая поверхность «скаляр → строка»: `@to_str()`, убрать `str.from(scalar)`

**Статус:** ✅ СДЕЛАНО 2026-07-14 [sonnet, worktree `nova-s2s` → влито в main FF `bddc3cf9a`]. `str.from`
удалён ПОЛНОСТЬЮ; публичная поверхность = только бланкет `fn[T] T @to_str() -> str => "${@}"` (ради
цепочных вызовов; `str.from(x)` не чейнился — дубль, D9). **Приёмка:** conformance 469/0+12skip (EXIT=0),
δ0 на encoding/time/data (2 фейла предсуществуют в main). D-амендменты D73/174.1/D410 в том же слиянии
(язык-меняющее). Компиляторный дефект диспетча примитивов (бланкет vs чужой конкретный `@to_str` → SEGV)
починен ОТДЕЛЬНО оркестратором в main — D164, коммит `106ae7207` (guard Plan 164 Ф.3 расширен на примитивы;
регресс `d164_primitive_blanket_dispatch.nv`); neg-тест `int_to_str_effect_collision_neg` конвертирован в
позитив `int_to_str_effect_op_blanket`. Остаток: `nova_tests/**` (str.from) — оставлен Plan 198 (заморожен,
удаляется). Полная запись — `docs/plans/174.2-scalar-to-str-notes.md`.

**Решение владельца 2026-07-14:** `str.from(scalar)` **УБРАТЬ** полностью (не прятать в приватный движок) —
публичная поверхность = только `@to_str()`, ради цепочных вызовов (`x.to_str().pad(…)`; `str.from(x)` не
чейнится). Один путь (D9). **Язык-меняющее** → D-амендмент к 174.1/D410 в ТОМ ЖЕ слиянии.

**Асимметрия (recon 2026-07-13/14):** 174.1 уже мигрировал прямой путь (`str @to_int/@to_f64`, `[]u8 @to_str`,
`@to_version/@to_url`). Обратное «примитив→строка» НЕ мигрировано: `str.from(int/f64/f32/bool)` живы как
`export extern "nova"` (`std/src/runtime/string/from_scalar.nv:23/26/29/32`; `char.nv:25`); только `char` имеет
обёртку `char @to_str() => str.from(@)` (`char.nv:32`) — у `int/f64/f32/bool` `@to_str()` НЕТ. ~56 вызовов
`str.from(scalar)` по std. Тела `@display` примитивов (`prelude/protocols.nv:547/552/567`) аллоцируют
промежуточную `nova_str` (`w.write(str.from(@))`). Интерполяция `${x}` примитива лоуэрится через `str.from`
(`emit_c.rs::emit_interpolated_str ~2428`). Хелперы прямо-в-sink УЖЕ есть (152.7-B): `nova_int_to_str`
(`nova_rt.h`), `nova_bool_to_str/char/f64/f32` (`conv.h ~144-228`).

**Дизайн (владелец 2026-07-14): БЛАНКЕТ, не per-primitive.** Одна базовая реализация на все типы:
`fn[T] T @to_str() -> str => "${@}"` — «строка из значения = подставить значение в интерполяцию». `str.from`
удаляется ПОЛНОСТЬЮ (скалярный И обобщённый — бланкет+интерполяция его заменяют). `.to_str()` чейнится
(`x.to_str().pad(…)`), `str.from(x)` — нет; держать оба = дубль (D9).

**Два инварианта (иначе поломка) — checkpoint+стоп при нарушении:**
- **Рекурсия:** `${примитив}` лоуэрить в Display/прямой рантайм-хелпер, НЕ в `.to_str()` (иначе
  `to_str→"${@}"→to_str` зациклится). float — существующим extern C-хелпером (`nova_f64/f32_to_str`; точный
  round-trip непортируем, §3); НОВЫХ C-хелперов не добавлять.
- **Специализация:** бланкет НЕ должен затирать специфичные `@to_str` с другой семантикой (`[]u8 @to_str()` =
  декод байт как UTF-8, не `"${bytes}"`; `char`/`str`). «Конкретный бьёт общий». Если Nova не поддерживает
  специализацию/бланкет-форму — checkpoint+стоп с отчётом.

**Шаги (каждый = коммит; полный conformance без `--jobs` после каждого):**
0. Сверить форму `fn[T] T @to_str()` и специализацию по `spec/decisions/`+`examples/` (не выдумывать синтаксис);
   невалидно → checkpoint+стоп.
1. Добавить бланкет `fn[T] T @to_str() -> str => "${@}"`. **Язык-меняющее** → D-амендмент 174.1/D410 в том же слиянии.
2. Лоуэринг `${примитив}` + `@display` примитивов — прямо-в-sink через Display/хелпер (с `str.from`), НЕ через
   `.to_str()`. **Уточнение по факту (2026-07-14):** примитивы и ДО этого шли напрямую через `nova_int_to_str`
   и т.п. (Plan 152.7-B), в стороне от `str.from`; снята лишь редундантность двух путей (убран Path-form
   `str.from` dispatch). Глубокий долг `[M-152.7.2-interp-direct-primitives]` (лишняя аллокация внутри
   `emit_interpolated_str`) **НЕ закрыт** — требует НОВЫХ sink-write C-хелперов (вне рамок задачи).
3. Мигрировать ~56 потребителей `str.from(scalar)` → `.to_str()` (data/semver, encoding/serde·json·url, io/error,
   time/civil/*, text/markdown_minimal, runtime/defaults·string/transform, _experimental/crypto, prelude/*).
4. Удалить `str.from` целиком (`from_scalar.nv` + `char.nv` скаляр-overload'ы + обобщённый, если есть); негатив-тест:
   `str.from(5)` недоступен.
5. Обновить D73/174.1/D410: канон = бланкет `@to_str()` + интерполяция-через-Display; `str.from` ретрактирован.

**Приёмка (оркестратор):** conformance полный без `--jobs` (мега-CU ~450с); `std/src/encoding + time + data` δ0;
byte-parity НЕ требуется (аллокация убрана — `.c` меняется законно), тесты зелёные; D-амендмент в том же слиянии;
греп конфликт-маркеров одной командой с коммитом. **Checkpoint при обрыве:** `docs/plans/174.2-scalar-to-str-notes.md`.

---

## Пункт 8 — унифицированный Formatter (`@display(mut f Fmt)`, байтовый `Write`, zero-alloc)

**Статус:** 📋 ДИЗАЙН — вынесен в отдельный **[Plan 201 «Unified Formatter»](201-unified-formatter.md)** (объём
язык-меняющий: D422 + амендменты D419-retract/D374/D237/D229/D179; свой дизайн-док + карта миграции). Полный
алгоритм (без спека / со спеком через `pad_in_place`), разбор «что на nv / компилятор-синтез / C», converged-решения
и открытые вопросы — там. Триггер: разбор форматирования при закрытии Пункта 7 (str.from-ретракция) выявил дубль
`@display`/`@display_fmt` и str-центричный `Write`; владелец задал редизайн под единый Rust-подобный Formatter,
но лучше Rust (нет footgun'а «забыл pad», радикс без плодения трейтов, C только на float-body).

---

## Кандидаты на будущее

_(сюда — новые std-эргономические/корректностные улучшения по мере появления; каждый с D-рефом и приёмкой)_
