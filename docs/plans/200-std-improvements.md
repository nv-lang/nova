<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 200 — зонтичный план улучшений std

**Статус:** 📋 ЖИВОЙ РЕЕСТР — **НЕ закрывать** (владелец 2026-07-12: «будем добавлять много новых штук»).
**Приоритет:** ниже Plan 196 (196 — высший, без остановки). Пункты 200
**file-disjoint** от компилятора (правят `std/*.nv` + спеку/конвенцию), могут идти параллельно/после 196
отдельными дешёвыми коммитами. Каждый пункт = свой D-реф, своя приёмка, свой коммит; НЕ мега-коммит.

## Назначение

Единая точка сбора и секвенирования эргономических/корректностных улучшений stdlib.

**Зон-координация мульти-агентных периодов (2026-07-21, договорённость сессий):** 200-агенты берут
ТОЛЬКО `std/src/**` ВНЕ горячих чужих зон; на момент записи заняты: `runtime/fmt_buf*` +
`runtime/string_builder*` (Ф.4R/208-семья), `types/mod.rs`/`emit_c.rs`/`callnorm.rs` (196/217/фикс-волны),
`nova-cli`+`scripts/` (release-волна). Пункт требует codegen-правку → НЕ делать её 200-волной:
маркер в backlog → очередь компиляторной сессии (два свипа в одном файле запрещены). Реестр: по мере
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
pre-method, Plan 139.1). **Хвост ЗАКРЫТ владельцем 2026-07-20 (`61c43b564`): `priv(type)` НАВСЕГДА** —
методы str живут в другом модуле (runtime.string) + 38 полевых чтений: field-`priv` их сломал бы;
коммент-обоснование поставлен у деклы.

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
- **`str` — РЕШЕНО 2026-07-20 (владелец, вопрос закрыт): `priv(type)` ОСТАЁТСЯ НАВСЕГДА.** Причина —
  топология, не вкус: методы str в ДРУГОМ модуле (runtime.string, 38 прямых чтений полей) — field-priv
  сломал бы их; перенос деклы = bootstrap-цикл. Коммент у деклы поставлен. ~~Проверить `str`~~ (возможно намеренно
  (тип-обёртка над буфером с инвариантами), решить по месту, не менять слепо.

**Приёмка:** `nova test std` без новых фейлов.

---

## Пункт 3 — единый префикс протоколов `As*` — ❌ ОТКЛОНЁН 2026-07-20 (владелец)

Поверхность протоколов с 12-го устоялась (D422 Display/Debug, Compare в операторах, D429 #coerce);
рен-волна всего std+спеки дорога, выгода — только единообразие префикса. Не делаем.

Владелец (2026-07-12) высказал мысль: протоколы именовать `As…` (`AsEqual`/`AsCompare`/`AsHash`/…, по образцу
`AsSlice`). Только идея. Объём большой — затрагивает `Equal`/`Compare`/`Hash`/`Display`/`Clone`/… (D229/D230/
D262 и весь std + conformance). Требует отдельного решения (D + миграция). Держу как открытый вопрос до go.

---

## Пункт 4 — полная миграция на `.new(cap int=0)` (D9 «один путь») — НАЙДЕНО 94 сайта

**Статус:** ✅ СДЕЛАНО 2026-07-20 (закрыто интегратором). Код-сайты: 9/9 мигрированы ещё 16-го;
финальный греп показал — остальные 19 хитов НЕ код: doc-строки (поправлены на `new(cap: n)` — hashmap/
queue/set/SB/WB/mutate) + НАМЕРЕННЫЕ фикстуры-пины chain-формы (d372/d38/dispatch — chain легален по
D372-amend2, пины остаются) + исторические ex-with_capacity заметки (остаются).
**Не блокер** (chain `.new().cap(n)` легален по D372-amend2), приводим к единой канонической форме.

**Найдено (по grep):** **94 упоминания `.new().cap(...)` в 30 файлах**, из них:
- **9 кодовых сайтов в spec_tests/conformance** (затронуто: d38_array_creation.nv, dispatch_receiver_type_vs_name.nv, write_constructors.nv, d372_canonical_new_defaults.nv) — ✅ МИГРИРОВАНО
- **14 в комментариях/документации std/** (примеры/docstrings в hashmap.nv, queue.nv, set.nv, vec/core.nv, string_builder.nv, write_buffer.nv и т.д.) — НЕ трогаем (история)
- Остальные в spec_decisions/, docs/plans/, docs/prompts/ (reference/примеры)

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

**Статус:** ⏸️ НА ПАУЗЕ 2026-07-13, ПОДТВЕРЖДЕНА 2026-07-20 (владелец). **НЕ чистый std-рефактор — это компиляторная ABI-правка**
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
удаляется). Полная запись — `docs/plans/wip/174.2-scalar-to-str-notes.md`.

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
греп конфликт-маркеров одной командой с коммитом. **Checkpoint при обрыве:** `docs/plans/wip/174.2-scalar-to-str-notes.md`.

---

## Пункт 8 — унифицированный Formatter (`@display(mut f Fmt)`, байтовый `Write`, zero-alloc)

**Статус:** 📋 ДИЗАЙН — вынесен в отдельный **[Plan 208 «Unified Formatter»](208-unified-formatter.md)** (объём
язык-меняющий: D422 + амендменты D419-retract/D374/D237/D229/D179; свой дизайн-док + карта миграции). Полный
алгоритм (без спека / со спеком через `pad_in_place`), разбор «что на nv / компилятор-синтез / C», converged-решения
и открытые вопросы — там. Триггер: разбор форматирования при закрытии Пункта 7 (str.from-ретракция) выявил дубль
`@display`/`@display_fmt` и str-центричный `Write`; владелец задал редизайн под единый Rust-подобный Formatter,
но лучше Rust (нет footgun'а «забыл pad», радикс без плодения трейтов, C только на float-body).

---

## Пункт 9 — `clamp` + scope-local `const` в `shim_error` (nova-tls) — эргономика/DRY

**Статус:** ✅ СДЕЛАНО 2026-07-15. Мелкая читаемость-правка в `../nova-tls/src/stream.nv` (внешний TLS-модуль).

**Что:** в `shim_error` число `256` (ёмкость буфера ошибки шима) встречалось 3× (`resize`/`tls_last_error`-cap/
верхняя граница), а clamp был ручным `if`-каскадом. Свёл к:
- **scope-local `const ERR_BUF_CAP = 256`** (функцио-локальный const — поддержан, спека 02-types.md:13790 /
  03-syntax.md:1117 «module-level + scope-local»);
- `buf.resize(ERR_BUF_CAP, 0 as u8)` / `tls_last_error(h, buf.ptr(), ERR_BUF_CAP)`;
- **`ro m = n.clamp(0, ERR_BUF_CAP)`** вместо `if n < 0 { 0 } else if n > 256 { 256 } else { n }`
  (`int @clamp`, std/runtime/defaults.nv:180).

**Приёмка:** `nova check`/сборка nova-tls зелёная; поведение идентично (clamp(0,256) == прежний каскад).

---

## Пункт 10 — duration.nv: локальные i64-арифм-хелперы → встроенные (Plan 206 бланкеты + i64.MAX/@clamp)

**Статус:** ✅ СДЕЛАНО 2026-07-16 (worktree `nova-200dur`, ветка `p200-duration-chain`, ПОСЛЕ фикса 196.8/196.9 —
`[M-primitive-receiver-bounded-blanket-dispatch]` закрыт, блокер снят). Модель: sonnet.

**Что (карта):**
- **Убрать 6 локальных хелперов** (стали редундантны после Plan 206 / встроенных): `i64_max()` (361),
  `i64_min()` (365), `clamp_i64()` (368), `checked_add_i64()` (379), `checked_sub_i64()` (381),
  `checked_mul_i64()` (386).
- **Замены во всех сайтах:** `i64_max()`→`i64.MAX`; `i64_min()`→`i64.MIN`; `clamp_i64(r,lo,hi)`→`r.clamp(lo,hi)`;
  `checked_add_i64(a,b)`→`a.checked_add(b)` (+ `_sub`/`_mul`). (`i64.MAX/MIN` встроены — verified; `@clamp` на
  `int`, i64 через коэрсию на 64-бит — verified тип-чеком.)
- **Оставить** `checked_neg_i64` (383) и `checked_div_i64` (388) — стандартного `@checked_neg`/`@checked_div`
  бланкета НЕТ (owner 2026-07-15); их внутренние `i64_min()`→`i64.MIN`.
- **`sat_add/sub/mul_i64` оставить** (доменная сатурация к кастомным `[lo,hi]` ≠ `saturating_add`); внутренности
  `checked_*_i64(a,b)`→`a.checked_*(b)`.

**Приёмка:** `i64_max()`/`i64_min()`/`clamp_i64()` удалены → `i64.MAX`/`i64.MIN`/`@clamp`; `checked_add_i64`/
`checked_sub_i64`/`checked_mul_i64` wrapper-функции удалены, call-сайты → `a.checked_add(b)` напрямую;
`checked_neg_i64`/`checked_div_i64`/`sat_add/sub/mul_i64` оставлены (нет бланкета neg/div; кастомный `[lo,hi]`).
`nova check std/src/time` (targeted, без codegen) — зелёный на момент коммита. **Полный `nova test`/conformance —
НЕ прогнан этой сессией** (CPU занят гейтом интегратора) — авторитетный гейт остаётся за оркестратором.

**ДОПОЛНЕНИЕ (волна «числовой паритет», 2026-07-19, worktree `nova-numparity`, ветка
`p-numeric-parity`, модель sonnet).** Прогнан отложенный полный `nova test std/src/time`:
**PASS: 6  FAIL: 0  SKIP: 1** (`cron` — SKIP, нет test-блоков/`fn main()`, ожидаемо) —
блокер снят подтверждённо, `sat_add_i64`/`sat_sub_i64`/`duration/core.nv` работают на
`r.clamp(lo, hi)` без изменений (Ints-бланкет `@clamp`, std/src/prelude/protocols.nv,
уже покрывал `i64` до этой волны — см. `docs/plans/wip/numeric-parity-notes.md`).
Талли `std/src/time` — ЗЕЛЁНАЯ, пункт закрыт фактом прогона.

Побочная находка (НЕ регрессия этой волны, НЕ трогал): `nova test
std/src/time/overflow_safe_test.nv` (standalone-таргет ЭТОГО ОДНОГО файла) падает
компиляторным ICE `[P67-LEGACY] Path call return type unknown for method=to_nanos`
(emit_c.rs:52548) — воспроизведено И на полностью откаченном HEAD (до всех правок этой
волны), т.е. pre-existing, из известного класса P67-LEGACY (generic-return-type-inference
gap, уже описан в protocols.nv рядом с `@checked_div`/`@checked_neg`). При штатном прогоне
`nova test std/src/time` (весь каталог, а не один файл) этот файл почему-то не входит в
отчёт вовсе (не PASS/не FAIL/не SKIP — тихо отсутствует) — та же судьба у
`std/src/time/rt/*.nv` и `std/src/time/civil/rt/*.nv`/`civil/neg/*.nv` (легаси
`EXPECT_RUNTIME_PANIC`/`EXPECT_COMPILE_ERROR` фикстуры, отдельная конвенция запуска — см.
`docs/test-conventions.md` §rt/neg). Не относится к Пункту 10 (`sat_add_i64` живёт в
`duration/core.nv`, не в `overflow_safe_test.nv`) — отдельная P67-LEGACY заметка, не
добираю в этой волне (не numeric-parity scope).

---

## Пункт 11 — duration.nv: `@as_*()` → голое имя (хвост D410-миграции `[M-d410-as-to-migration]`)

**Статус:** ✅ СДЕЛАНО 2026-07-16 — вобрано в Пункт 12 (см. ниже), делалось одним проходом.

**Почему:** D410 упразднил префикс `as_` ([nv-coding-style.md:33](../nv-coding-style.md)), но 11 методов в
`duration.nv` остались — пропущенный хвост миграции `[M-d410-as-to-migration]`.

**Какое имя (§1а «четыре направления»):** голое существительное (категория «вид/линза», O(1) i64, без аллокации,
инфаллибельно — как `byte_len()`/`len()`; НЕ `to_*`, тот для аллоц/fallible/O(n)):
- `@as_nanos/micros/millis/secs/mins/hours/days()` → `@nanos()`/`@micros()`/`@millis()`/`@secs()`/`@mins()`/
  `@hours()`/`@days()`;
- `@as_unix_secs/millis/nanos()` → `@unix_secs()`/`@unix_millis()`/`@unix_nanos()`.

**Проверить:** коллизию `@nanos()` с авто-property приватного поля `nanos` (D84/D409); и не столкнётся ли
`Duration @nanos() -> i64` (getter) с `int @nanos() -> Duration` (fluent-конструктор, см. обсуждение
`from_nanos`) — разные ресиверы, но одно имя.

**Приёмка:** `nova test std/time` зелёный; conformance один-CU δ0 (переименование, поведение идентично); греп
`@as_` в `duration.nv` = 0. **Модель:** дешёвый агент по карте (после 10), CPU-дисциплина, гейт — оркестратор.

---

## Пункт 12 — единицы времени: getter=голое / конструктор=`to_`, ретракт `Duration.from_*` (вбирает Пункт 11)

**Статус:** ✅ СДЕЛАНО 2026-07-16 (worktree `nova-200dur`, ветка `p200-duration-chain`). Модель: sonnet.

**Правило (§1а):** направление задаёт форму. **Единица — полным словом** (nanos/micros/millis/seconds/minutes/
hours/days).
- **Getter** (`Duration → i64`, O(1)-скаляр = голое имя): `d.nanos()`/`micros()`/`millis()`/`seconds()`/`minutes()`/
  `hours()`/`days()`; Timestamp `d.unix_seconds()`/`unix_millis()`/`unix_nanos()` (это = миграция `@as_*` Пункта 11).
- **Конструктор** (`int → Duration`, конверсия = `to_*`): ретрактировать ВСЕ `Duration.from_*` И заменить bare-fluent
  `int @seconds()` на **ДЖЕНЕРИК-бланкет** `fn[T Ints] T @to_seconds() -> Duration` (владелец 2026-07-15: один вход на
  ВСЕ int-ширины i8..i64/u*, DRY, зеркалит Plan 206 `checked_add`). → `5.to_seconds()`, `100.to_millis()`,
  `n.to_nanos()`. Timestamp: `n.to_unix_seconds()` (int→Timestamp, вар. (а)).
- **Float — отдельно** (f64 ∉ Ints, в бланкет не входит): `f64 @to_seconds()` (заменяет `from_secs_f`;
  `1.5.to_seconds()`). Только секунды.
- **Singular** `int @second()`/`@minute()` — убрать (DRY; `1.to_seconds()`).
- **Свободные обёртки-дубли убрать** (владелец 2026-07-16): `fn sleep(d Duration)` (duration.nv:263 — однострочный
  делегат в `d.sleep()`) и `fn sleep_until(deadline Monotonic)` (:280 → метод `Monotonic @sleep_until()`) — §3
  nv-coding-style (surface = методы) + D9; канон `5.to_seconds().sleep()` / `deadline.sleep_until()`. Effect-op
  `Time.sleep(ms int)` (prelude/effects) НЕ трогать — слой примитива, не пользовательский surface. Мигрировать
  немногие call-сайты свободных форм (std/examples, единицы).
- **⚠ Зависимость:** `[T Ints] @to_seconds()` на примитивном ресивере — ровно механизм
  `[M-primitive-receiver-bounded-blanket-dispatch]` (dispatch-баг). Делать **ПОСЛЕ** его фикса (196.8).
- **Коллизия снята:** getter `d.nanos()` (голое) vs конструктор `5.to_nanos()` (`to_`) — разные имена.

**Приёмка:** getter/конструктор реализованы по карте (см. duration-chain-progress.md за деталями). Греп
`@as_`/`Duration.from_`/`Timestamp.from_unix_`/singular-алиасов = 0 по `std/examples/spec_tests` (repo-wide,
подтверждено grep'ом). `nova check` (targeted) зелёный на `std/src/time`, `std/src/time/civil`,
`std/src/concurrency` на момент соответствующих коммитов. **Полный `nova test`/conformance — НЕ прогнан
этой сессией** (CPU занят гейтом интегратора) — авторитетный гейт остаётся за оркестратором. D-амендменты:
D410 (03-syntax.md), D317 (04-effects.md) — добавлены.

## Пункт 13 — разбить duration.nv: Timestamp/Monotonic в отдельные файлы (D78 co-equal)

**Статус:** ✅ СДЕЛАНО 2026-07-16 (worktree `nova-200dur`, ветка `p200-duration-chain`). Модель: sonnet.

**Что:** `Timestamp` вынесен в `std/src/time/timestamp.nv`, `Monotonic` — в `std/src/time/monotonic.nv`
(co-equal файлы модуля `time.duration` — оба объявляют `module time.duration`, как `std/src/time/civil/*.nv`
все объявляют `module time.civil`; import-путь `std.time.duration` не меняется). `Duration` + module-private
overflow-safe хелперы (`sat_add_i64`/`checked_neg_i64`/`f64_nanos_or_trap`/и т.п., общие для всех трёх типов)
остались в `duration.nv`. Чистая текстовая экстракция (head/tail по проверенным границам), без изменения тел
методов.

**⚠ НЕ ВЕРИФИЦИРОВАНО КОМПИЛЯЦИЕЙ** (запрет на `nova.exe` в конце сессии из-за CPU-контеншна с гейтом
интегратора) — `[M-200-duration-chain-verify]`. Уверенность высокая (архитектурный precedent
module-private-функций, видимых межфайлово в том же модуле, подтверждён на `std/net/ffi.nv`↔`std/net/addr.nv`
(`net_addr_parse` объявлен в `ffi.nv`, вызывается из `addr.nv`, оба `module std.net`); границы среза
перепроверены построчно до/после разреза), но **первым делом авторитетного гейта — собрать
`nova test std/src/time` и убедиться, что `timestamp.nv`/`monotonic.nv` реально резолвятся как co-equal
файлы модуля `time.duration`.**

**Приёмка (авторитет — оркестратор):** `nova test std/time` зелёный; import-пути неизменны; conformance δ0.

---

## Пункт 14 — комбинаторы `Option`/`Result`: `flat_map` (обе стороны) + `filter` (Option)

**Статус:** ✅ РЕАЛИЗОВАНО этим заходом 2026-07-16 [sonnet, worktree `nova-200p14`]. Research-основание —
`docs/research/2026-07-16-option-result-combinators.md` (владелец, коммиты `f74cea01c` + `c24c5cae4`).

**Что:** три Nova-body метода в `std/src/prelude/core.nv` (рядом с `map`/`ok_or`/`or`/`map_err`):
- `fn Option[T] @flat_map[U](flat_map_fn fn(T) -> Option[U]) -> Option[U]`
- `fn Result[T, E] @flat_map[U](flat_map_fn fn(T) -> Result[U, E]) -> Result[U, E]`
- `fn Option[T] @filter(pred fn(T) -> bool) -> Option[T]`

**Почему именно эти три (D86-философия отбора, в обратную сторону от ретракта unwrap-twins —
Пункт `[M-unwrap-twins-retraction]`):** value-fallback у Nova принципиально операторный (`??`/`!!`),
поэтому в prelude добавляется ТОЛЬКО то, что операторами и `.map`/`match` невыразимо:
- **`flat_map`** — единственный канонический комбинатор, снимающий вложенность `M[M[U]]` (bind
  fallible-шагов); `.map` этого не даёт.
- **`filter`** — единственный способ вернуть `None` из `Some` по предикату без явного `match`.
- **НЕ добавлены** (тот же класс, что ретрактированные `unwrap_or`/`unwrap_or_else`, выразимы
  существующими средствами): `or_else` (`?? f()`, `??` эмпирически право-ассоциативен — плоская
  цепочка `a ?? b ?? c` без вложенности), `unwrap_or[_else]` (уже ретрактированы D86), `map_or[_else]`
  (`.map(f) ?? d`). Имя **`flat_map`**, не `and_then`/`then` (сиблинг `map`, без булевого багажа
  `and`/`or`, без коллизии с Promise-`then`/`bool::then`) — решение владельца, зафиксировано в research.

**Спека:** амендмент-нота тем же коммитом — [D26 (08-runtime.md)](../../spec/decisions/08-runtime.md#d26-базовая-stdlib-и-prelude)
(канонический каталог методов + закрытие «Q-monadic-api» частично) и [D86 (04-effects.md)](../../spec/decisions/04-effects.md#d86-coalesce-оператор--fallback-для-resultoption)
(cross-ref на философию отбора). Не язык-меняющее (методы над существующими типами, без нового
синтаксиса/семантики) — но амендмент-нота внесена по конвенции std-API-добавлений.

**Тесты:** `spec_tests/conformance/plan200_14_option_result_flat_map_filter.nv` (13 test-блоков:
flat_map Some/None/short-circuit/type-changing на Option и Result, filter pass/fail/None,
композиция `filter().flat_map()`, `?? default` цепочка через `flat_map`).

**Приёмка:** функционально верифицировано min-фикстурой (nova build + запуск, exit 0: flat_map
Some/None, filter pass/fail, Result short-circuit); conformance-фикстура закоммичена и прогоняется
авторитетным гейтом интегратора (мега-CU в заходе агента не гонялся — CPU-дисциплина).
**Остаток приёмки (из согласования владельца):** первый потребитель — `resolve_port` флагмана
(`env(...).flat_map(|s| s.to_int().ok()).map(|n| n as u16) ?? DEFAULT_PORT`) — мигрировать
демонстрацией; отдельный мелкий заход.

---

## Пункт 15 — type-set'ы: единое множественное число (`UnsignedInt` → `UnsignedInts`) + doc «Ints включает unsigned»

**Статус:** ✅ СДЕЛАНО 2026-07-20 (интегратор; объём оказался шире записи — жив был и `SignedInt`):
ОБА сета → мн.число (`SignedInts`/`UnsignedInts`), doc-предупреждение «Ints ВКЛЮЧАЕТ беззнаковые» у деклы,
комменты-ссылки обновлены (protocols/parse/string_test), 8 conformance-фикстур (bound-сайты + d310-коммент;
локальный `type UnsignedInt` в p172_3 намеренно оставлен — свой тип). Гейты: мега-CU PASS 1/0 полным
фильтром, neg-пара EXPECT_COMPILE_ERROR валидна, checksums δ0.

**Что:** в `std/src/prelude/protocols.nv` два сета с рассогласованными именами: `UnsignedInt` (ед. число, :830)
и `Ints` (мн., :842). D310-сет — буквально МНОЖЕСТВО типов → семейство во множественном числе:
- **переименовать `UnsignedInt` → `UnsignedInts`** (+ все bound-сайты `[T UnsignedInt]`);
- **doc-строка к `Ints`**: явно «включает unsigned» (ловушка читателя: по аналогии с `int` можно решить, что
  только знаковые; Go-прецедент `constraints.Integer` — тоже все целые);
- будущие сеты — тем же паттерном: `SignedInts`, `Floats`. Голое `Signed` (Rust/Go) НЕ брать — float'ы тоже
  знаковые, имя вводит в заблуждение.

**Сверка с другими языками:** Go `constraints.Integer/Signed/Unsigned` (ед., trait-стиль); Swift
`SignedInteger/UnsignedInteger`; Rust num `PrimInt/Signed/Unsigned`. У них bound = trait (способность) → ед.
число; у нас D310-сет (перечень типов) → мн. число честнее и короче: `[T Ints]` = «T из целых».

**Приёмка:** греп `UnsignedInt\b` = 0 (кроме исторических доков); conformance δ0 (переименование);
`nova test std` без новых фейлов. **Модель:** haiku по карте (греп+переименование+doc), гейт — оркестратор.

---

## Пункт 16 — ретракт `Vec[T].from(items []T)` (пятая дверь §1а) с развязкой типо-направленной роли

**Статус:** ✅ СДЕЛАНО 2026-07-20 (sonnet, по карте владельца) — **ПОЛНЫЙ ретракт**.

Три роли `from` мигрированы по канону (карта подтверждена на факте — ВСЕ живые сайты вне nova_tests
оказались литеральными; role-2/role-3 подтверждены на отдельных фикстурах):
1. **литерал** (`Vec[int].from([1,2,3])`, `Vec[f32].from([1.5,2.5])`) → **`Vec[T].of(...)`** — 59 сайтов в
   `std/src/collections/vec/{access,iter,mutate,protocols,restructure,views}.nv` + `vec_lazy.nv` (doc-примеры
   и inline-тесты) + 6 conformance-файлов (`repro_control`, `repro_explicit`, `repro_param`,
   `self_nested_repro`, `t8_arg_vec_accepts_literal`, `vec_f32_chained_debug`);
2. **same-T конверсия** (`[]int.from(src)`) → **`src.clone()`** — 2 сайта, `spec_tests/conformance/
   d259_vec_of_vs_from.nv` (единственный файл, реально бивший эту роль; переписан ЦЕЛИКОМ с сохранением
   покрытия — `of`/`new`/`clone`-independence, D259 header переписан на текущий канон);
3. **width-конверсия** (`Vec[u8].from(int_vec)`) — сайтов с фактическим вызовом НЕ найдено (только
   объясняющий NOTE-комментарий в core.nv про НЕ-bulk-copy); карта пункта 3 остаётся справочной на случай
   будущего сайта (явный поэлементный цикл, не одна фраза).

Декла `Vec[T].from` снесена ПОСЛЕДНИМ коммитом из `std/src/collections/vec/core.nv` (вместе с обновлением
doc-комментария `of`, который ссылался на `from` для сравнения). `[M-lint-findings-static-conversion]`
Vec.from-часть закрыта (маркер остаётся открыт для остальных 20 сайтов — `docs/plans/backlog-followups.md`).

**Спека тем же слиянием:** `spec/decisions/02-types.md` (D259 AMEND-блок + `README.md` индекс-строка + D232
construction-таблица + D230 shallow-copy таблица + NovaArray-блокер item 5 помечен MOOT), `docs/nv-coding-style.md`
§1а item 4, `docs/collections/vec-owned.md` (Construction-таблица + секция `of` vs `.clone()` переписана),
`docs/vec-lazy.md` (пример кода).

**Побочная находка (зафиксирована и исправлена в этой же волне):** при standalone-верификации
`vec_f32_chained_debug.nv` обнаружен НЕ связанный с Vec.from, самостоятельный pre-existing баг — вызовы
`.debug(a)`/`.display(a)` передавали голый `StringBuilder` вместо `FmtCtx.bare(a, mark, is_debug)` (устаревшая
call-форма, оставшаяся от ДО Plan 208 Ф.2/Ф.3 (D422) миграции `Write`→`Fmt`; воспроизведён на ГОЛОМ
`Vec[int]` без единого `.of()/.from()` — не регрессия миграции). Исправлено на канон-форму (по образцу
`std/src/collections/vec/protocols_test.nv` / `std/src/time/duration/core.nv`); подтверждено 5/5 в изоляции.

**Прогоны:** `nova test std/src/collections/vec` PASS (весь folder-module, access/core/iter/mutate/protocols/
restructure/views + пир-тесты, 26 test-блоков); `nova test std/src/collections/vec_lazy.nv` PASS; `nova test
std/src/checksums` δ0 (3 PASS / 3 SKIP, без изменений); все 7 мигрированных conformance-файлов — standalone
(изолированный module-namespace, обходя shared-module merge всей папки conformance) 7/7 PASS. Финальные грепы
`Vec[…].from(` живых сайтов и декла = 0 вне nova_tests и вне исторических комментариев.

**Находка про карту:** `spec_tests/conformance` НЕ допускает истинно изолированный per-file прогон обычным
путём — все ~150 файлов папки (кроме `standalone/`) делят `module spec_tests.conformance` и мега-CU
собирается целиком при передаче ЛЮБОГО одного из них (авторитетный гейт интегратора, не std-агента);
для «standalone-прогона» карты фактическая техника — временная module-переименованная копия в
`standalone/`, прогнанная и удалённая (не коммитится).

Модель: sonnet по карте (роль-2 требует суждения по месту), гейт финального мега-CU — оркестратор/интегратор.

---

## Пункт 17 — инлайн-тесты в файлах имплементации → пир-файлы `*_test.nv` (конвенция)

**Статус:** ✅ 9/9 ГОТОВО (6/9 — 2026-07-20 haiku, влито `72394c274`, collections PASS 13/0;
+2/9 — 2026-07-20 sonnet, worktree `nova-p17rest`/ветка `p200-17-rest`;
+1/9 — 2026-07-20 haiku, П17 9/9). Конверсия файл-модуль →
папка-модуль (`X.nv` → `X/core.nv` + `X/core_test.nv`, прецедент time/duration/): **hashmap(10),
range(21), set(5), vec_iter(16), vec_lazy(5), vec_seq(5), base64(9), handlers(20), fmt_buf(8)** = 99 тестов
в пирах, 0 в имплементации, каждый модуль зелёный.

**Нарушение** [test-conventions.md:125](../test-conventions.md): позитив-тесты std-модуля живут
ПИР-файлом `<имя>_test.nv` (тот же module-декларатор), НЕ инлайном в имплементации.

**Инвентарь (греп `^test "` вне `*_test.nv`, std/src): 16 файлов имплементации.**
`collections/`: hashmap, range, set, vec_iter, vec_lazy, vec_seq · `encoding/`: base64,
compress/{checksum,deflate,gzip,inflate,zlib} · `runtime/`: fmt_buf · `testing/`: handlers (20),
property (6) · `time/`: duration (**33** теста). **НЕ нарушители** (легальны по конвенции):
`neg/`-фикстуры (standalone-CU `module neg.*`, :133-136) и `rt/`-trap-фикстуры (тот же standalone-класс —
сверить формулировку в конвенции, при отсутствии — дописать).

**Карта (per-file, механика):** вырезать `^test "`-блоки → пир `<имя>_test.nv` с ТЕМ ЖЕ `module`-
деклараторов + перенести только нужные тестам import'ы; имплементация без тестов. **Секвенс:**
`time/duration.nv` — ПОСЛЕ приземления folder-split (`time/duration/{core,timestamp,monotonic}.nv`,
ветка orphan-фикса) — иначе двойной конфликт.

**Приёмка:** греп `^test "` вне `*_test.nv`/`neg/`/`rt/` = 0 по std/src; `nova test std/<затронутые>`
таргетно зелёные; полный гейт — CI. **Модель:** haiku по списку (механика вырезать-перенести), duration —
sonnet (координация со split).

---

## Пункт 18 — UTF-8 кодпоинт-логика: 4 копии → один приватный источник

**Статус:** ✅ СДЕЛАНО 2026-07-19 (worktree `nova-p20018`, ветка `p200-18-utf8`, слияние
`p-tuple-fixarr` → П18; sonnet). Найдено при вопросе владельца про `char_utf8_len`.
Блокер `[M-tuple-fixarr-typedef-order]` (typedef-порядок для `(T, [N]U)`) закрыт тем же
заходом (`compiler-codegen/src/codegen/emit_c.rs`, унифицированный topo-sort tuple+fixarr
typedef'ов) — см. `docs/history/simplifications-closed.md`. Приёмка ниже.

**Что:** лестница `cp < 0x80 / 0x800 / 0x10000` (длина и/или кодирование кодпоинта в UTF-8)
размножена по std в ЧЕТЫРЁХ местах:
- `runtime/string_builder.nv:146` — bit-shift кодирование в `@append(c char)`;
- `runtime/string_builder.nv:281+296` — `char_utf8_len` + `char_utf8_bytes` (коммент 208 Ф.1
  честно признаётся: «duplicated (not shared)» — обход для raw-pointer записи pad_in_place);
- `runtime/defaults.nv:100` — та же длина-лестница;
- `runtime/write_buffer.nv:120` — ещё одно кодирование.

**Дизайн (владелец 2026-07-18, кортеж + находка про len_utf8):** `defaults.nv:100` — это
ПУБЛИЧНЫЙ `char @len_utf8()` (Rust-парити), т.е. приватник string_builder дублирует public
API. Единственный носитель лестницы:
- `export fn char @encode_utf8() -> ([4]u8, int)` — (bytes, len) кортежем (перевёрнут владельцем 2026-07-20: данные-потом-длина, конвенция {ptr,len}; ветка, записавшая
  байты, сама знает длину — отдельного вычисления не остаётся); Rust-парити
  `char::encode_utf8`; прецеденты кортежа: `decode_utf8 -> (int, int)`, `ro (a, b) =`;
- `export fn char @len_utf8() -> int => @encode_utf8().0` — публичная len-дверь становится
  делегатом (цена на len-only сайтах — стековые 4 байта, холодно);
- дом — `defaults.nv` рядом с `len_utf8` (методы char; отдельный utf8.nv НЕ нужен);
- `char_utf8_len`/`char_utf8_bytes` (string_builder) сносятся целиком; `@append`/
  `pad_in_place`/write_buffer → `ro (n, b) = c.encode_utf8()` (один проход лестницы вместо
  двух). `#no_prelude`-зона — проверить импорт-циклы (string_builder/write_buffer → defaults).

**Координация (обязательно):** 208 Ф.4 (снос conv.h → буфер-примитивы) работает в ТОЙ ЖЕ зоне
— выполнять ЛИБО как подготовку Ф.4, ЛИБО после неё, не параллельно (иначе двойная правка
одних тел).

**Приёмка:** греп лестницы `< 0x800` вне `char @encode_utf8()` = 0 (кроме first_invalid_utf8
— это ДЕКОДЕР, другая ось); `char_utf8_len|char_utf8_bytes` грепом = 0; таргетно
string_builder/write_buffer тесты + checksums-CU; байт-паритет вывода pad/append на
существующих фикстурах.
**Модель:** haiku по этому списку (механика), координацию с 208 Ф.4 решает интегратор.

**Приёмка (закрытие 2026-07-19, sonnet, бинарь на коммите `703b525b7`):** оба грепа по
`std/`: `char_utf8_len|char_utf8_bytes` = 0 ✓; `< 0x800` = 1 совпадение, ровно в
`defaults.nv:106` внутри `char @encode_utf8()` ✓. Таргетно: `std/src/runtime/string_builder_test.nv`
PASS 1/0, `std/src/runtime/char_test.nv` PASS 1/0, `std/src/checksums` PASS 3/0 SKIP 3
(non-test модули). `nova test std/src/runtime` целиком (директория) не гоняется — folder-module
CU того же каталога тянет `sync_test.nv`, который падает ПРЕД-СУЩЕСТВУЮЩИМ (не связанным)
ICE `[P67-LEGACY] Ident 'guard' not in var_types` — воспроизведён СТАНДАЛОНЕ и на чистом
неизменённом main (та же строка, оба бинаря); отдельный дефект, вне объёма П18, заведён как
`[M-runtime-sync-guard-consume-p67]` (backlog-followups.md, возможная регрессия
2026-07-17..2026-07-19). write_buffer.nv не имеет отдельного `_test.nv` (нет прямых тестов
по имени в std) — покрыт транзитивно (string_builder/интерполяция).

---

## Пункт 19 — `[N]T @ptr()` / `@len()` — аксессоры фикс-массива (зеркало Vec/str)

**Статус:** ✅ СДЕЛАНО 2026-07-21 (worktree `nova-p19`, ветка `p200-19-fixarr`, sonnet, коммиты
`4c022a1ec` (чекпоинт: checker+codegen синтез) + `01a0ea94c` (фикстуры + E_UNKNOWN_METHOD гейт +
D431)). Влито в main интегратором: merge `1a7296c73` (рассинхрон-фикс 2026-07-21 по
аудиту — фраза «не влито» писалась до мёржа и не была обновлена).

**Ш0-вердикт (первым делом, подтвердил дизайн):** проба `fn [4]u8 @probe() -> int => 4` — **НЕ
парсится** (`error: expected identifier, got int literal` на `4` внутри `[4]`). Путь —
КОМПИЛЯТОР-СИНТЕЗ, как и предполагала архитектурная записка (const-generic `N` на уровне
метода в языке нет).

**Механизм («одно окно» с `@index`, D238-семья):** тот же структурный приём, каким `arr[i]`
уже резолвится (checker `ExprKind::Index`-арм, `types/mod.rs` ~8991; codegen
`parse_mono_fixed_array_name` + `.data`/`->data`, `emit_c.rs` ~32415). Checker:
`peel_fixed_array`+`fixed_array_accessor_return` (types/mod.rs) — вызываются из ДВУХ уже
существующих продюсеров (`infer_expr_type`'s Call-арм и `infer_method_call_channel_type`), НЕ
новый резолв-путь; `is_mut` через существующий `is_through_ro_binding` (D175/D326). Codegen:
`emit_call`'s `Member`-арм, синтез напрямую через ТУ ЖЕ `parse_mono_fixed_array_name`, что и
`arr[i]`-чтение (`len()` → компайл-тайм литерал `N`; `ptr()` → адрес `data[0]`, `const`-квалификация
по `is_place_mutable` — тот же predicate, что Vec-overload'ы Plan 135/138.4). Побочная находка (в
scope этой же волны, не отдельный маркер — единственный, кто вводит РЕАЛЬНУЮ FixedArray-метод-
поверхность): `check_instance_overload`'s `E_UNKNOWN_METHOD`-гейт исторически скипал Array/
FixedArray-ресиверы целиком ("Vec" не в `is_primitive_recv_name`) → typo на `[N]T` падал
внутренним ICE `[P67-LEGACY]`, не чистой диагностикой; добавлен FixedArray-специфичный гейт
(любой метод кроме `len`/`ptr` на `TypeRef::FixedArray` → чистый `[E_UNKNOWN_METHOD]`; `Array`/
`[]T`, реально `Vec[T]`, не затронут).

**D431** (`spec/decisions/03-syntax.md`) — полный decision-блок после D27 + amendment-заметка в
самом D27.

**Фикстуры:** pos `spec_tests/conformance/d431_fixarr_len_ptr.nv` (3 test-блока: `.len()` на
трёх разных N; `unsafe { RawMem.copy(arr.ptr(), dst.ptr(), arr.len()) }` round-trip; mut-
перегрузка — запись через `.ptr().write(...)` видна в `arr`); neg
`spec_tests/conformance/neg/d431_fixarr_unknown_method_neg.nv` (typo `.lenx()` →
`EXPECT_COMPILE_ERROR E_UNKNOWN_METHOD`, было ICE до гейта).

**Верификация (5/5 зелёных):** `nova test std/src/collections/vec` PASS 1/0; `nova test
std/src/runtime/string_builder_test.nv` PASS 1/0; `nova test std/src/checksums` PASS 3/0 SKIP 3;
pos-фикстура (standalone-изолированная копия) PASS; neg-фикстура PASS (negative). Байт-паритет:
2 нетронутых фикстуры (`d216_ptr_methods_174_5.nv` — Vec `.ptr()`, НЕ FixedArray;
`d27_fixed_array.nv` — FixedArray-индексация без `.len()`/`.ptr()`-вызовов), SHA-256 сгенерированного
`.c` идентичен между этой веткой и базовым коммитом `f0eba7b5f` (throwaway reference worktree,
удалён после сверки): `parity_a.c` `7b8fde0493b7c0c2145c4e9e482223ba785a1088415208bb3d028ccaf36ff115`,
`parity_b.c` `b8e3e0a6c73276f4a4cd23f43b6836345193a9d2c8a89e69529a99fb8f858979`.

**Остаток по тексту пункта (сознательно НЕ в этой волне):** миграция трёх мотив-сайтов
(pad `fill_bytes` → `RawMem.copy`; `encode_utf8`-потребители → копия среза; `@display`-fallback
примитива → стек-буфер) — отдельный шаг после приземления.

**Мотив (три реальных упора одной недели):** (1) pad-оптимизация: `RawMem.copy` из
`fill_bytes [4]u8` несобираем — нет указателя-источника; (2) потребители `encode_utf8`
пишут байты циклом push вместо копии среза; (3) generic-fallback `@display` примитива:
стек-буфер `[32]u8` невыразим без каста — вынужденный heap-Vec в холодной ветке.
Обход существует уже сегодня: `&arr as *T` (прецедент `from_bits`), но это каст-жест на
каждом сайте вместо именованной двери.

**Форма (зеркало Vec/str):**
```nova
fn [N]T @len() -> int            // компайл-тайм N, O(1)
fn [N]T @ptr() -> *T             // + mut-перегрузка -> *mut T
```
Контракт как у `str @ptr()`: значение указателя safe, разыменование/lifetime — обязательство
вызывающего; для СТЕК-массива view не должен переживать скоуп (доккомент).

**Архитектурная записка (исполнителю проверить первым шагом):** ресивер-формы `fn [N]T @m()`
в std нет ни одной; method-level const-generic `N` в языке отсутствует → почти наверняка это
НЕ .nv-декларация, а КОМПИЛЯТОР-СИНТЕЗ (класс магии `@index` на FixedArray, D238-семья:
индексация `arr[i]` на [N]T уже работает компилятором). Шаг 0 исполнителя: 5-строчная проба
`fn [4]u8 @probe() -> int => 4` — если конкретно-инстансный ресивер парсится/резолвится,
рассмотреть .nv-путь для литеральных N; иначе — синтез в чекере/кодгене по образцу
FixedArray-`@index`. Одно окно: `@len`/`@ptr` должны идти тем же резолв-механизмом, что
существующая магия FixedArray, НЕ отдельной name-keyed веткой (§3).

**Приёмка:** фикстуры pos ( `arr.len() == 4`; `unsafe { RawMem.copy(arr.ptr(), dst, arr.len()) }`
round-trip; mut-перегрузка запись) + neg (view-эскейп не пинуем — контракт доккоментом, как у
str @ptr); перевод трёх мотив-сайтов (pad fill_bytes → RawMem.copy; display-fallback → стек)
— ОТДЕЛЬНЫМ шагом после приземления, не в этой волне. conformance δ0; checksums/SB таргетно.

---

## Пункт 20 — StringBuilder → `consume value`: минус GC-объект на каждую сборку строки

**Статус:** 📋 СОГЛАСОВАН 2026-07-21 (владелец; вопрос «SB оптимизируется в значение на стеке?» —
сегодня нет). **ПОСЛЕ слияния Ф.4R** (208 §10R — string_builder.nv у той волны живой; не параллелить).

**Что:** `type StringBuilder consume { mut buf []u8 }` (heap-запись: GC-заголовок SB + Vec-заголовок
внутри) → **`consume value`** — структ из одного Vec-указателя ПО ЗНАЧЕНИЮ на стеке; GC-объект SB
исчезает из каждой интерполяции/сборки. Прецедент комбинации — `TcpStream consume value` (tcp.nv:39,
с mut-методами). Линейность запрещает копии (consume = move); все мутации (push/advance/reserve,
вкл. реаллокацию data) живут ВНУТРИ общего Vec-заголовка — SB-структ их не хранит.

**Условия звучности (проверить исполнителю, №1 уже выполняется в main 2026-07-21):**
1. НОЛЬ реассайнов поля `@buf` (греп `@buf =` = 0; последний убран advance-волной `d5e6eed16`;
   появление нового реассайна при value-семантике разъединит копии — в doc-коммент типа вписать
   ЗАПРЕТ с обоснованием);
2. FmtCtx-взаимодействие: SB — поле FmtCtx; с value-SB инлайнится структ-копия — безопасно ровно
   при №1; прогнать формат-эталоны Ф.4R Ш0 (байт-в-байт) + протокольный путь (FmtCtx.rich-фикстуры);
3. `mut buf` ось поля: при value-типе перепроверить D246-оси (binding-mut поля больше не нужен,
   если реассайн запрещён — возможно `buf []u8` без mut; решить по чекеру).

**Спека:** D179-амендмент (тип-декла меняется) В ТОМ ЖЕ слиянии.
**Гейты:** SB-тест 1/0; checksums 3/0; формат-эталоны Ш0 байт-в-байт; мега-CU полным фильтром;
флагман strict (CI). **Модель:** sonnet (поведенческая правка типа, не механика).

---

## Пункт 21 — civil DateTime: композит-конструктор с default-временем (Python-эргономика без потери звучности)

**Статус:** 📋 СОГЛАСОВАН 2026-07-21 (владелец: «можно в DateTime добавить параметры по времени
со значением по умолчанию»). Мотив: `datetime(2026, 6, 8, 10, 0, tzinfo=MSK)` у Python — один
вызов; у нас — три `!!` и вложенный `TimeOfDay.new`.

**Что (обе — канонические арность-перегрузки с default-аргами, прецедент `new(cap = 0)`):**
```nova
// 1) полный композит (арность-сиблинг существующего DateTime.new(date, time)):
export fn DateTime.new(y i32, m Month, d i32,
                       h int = 0, min int = 0, s int = 0, ns int = 0)
    -> Result[DateTime, DateError]
// DateTime.new(2026, Jun, 8)            == полночь
// DateTime.new(2026, Jun, 8, 10, 0)     == Python-паритет одним вызовом

// 2) снятие вложенности у ступенчатой формы (сиблинг at(TimeOfDay)):
export fn Date @at(h int, m int = 0, s int = 0, ns int = 0) -> Result[DateTime, DateError]
// Date.new(2026, Jun, 8)!!.at(10, 0)!!
```
**Зафиксировано:** месяц ОСТАЁТСЯ `Month`-enum (класс багов «6 — месяц или день?» невозможен);
ошибки ОСТАЮТСЯ `Result[_, DateError]`; таймзона — прежним явным `to_zoned(tz)` (+Disambiguation).
Разные возвраты у перегрузок по арности (`at(TimeOfDay) -> DateTime` infallible vs
`at(h,…) -> Result`) — прецедент `new(cap)->Self` / `new(ptr,len)->ro Self`. D9-оговорка
«композит поверх ступеней» осознана — прецедент `new(cap:)` поверх `new().cap()`.

**Спека:** D320/D321-амендмент (civil-конструкторы) В ТОМ ЖЕ слиянии. **Приёмка:** civil-тесты
(пиры) + новые кейсы (дефолт-полночь, Python-паритет, невалидные h/min → Err) + `nova test
std/src/time/civil` зелёный; conformance δ0 (аддитив). **Модель:** sonnet.

---

## Кандидаты на будущее

_(сюда — новые std-эргономические/корректностные улучшения по мере появления; каждый с D-рефом и приёмкой)_
