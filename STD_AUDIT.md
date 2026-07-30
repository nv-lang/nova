# STD_AUDIT.md — Реестр обходов (workarounds) в `std/src`

> Дата аудита: 2026-07-30. Формат: план-минутки. Каждая запись — файл:строка | форма | канон | маркер-причина | жив? | класс.

---

## Легенда

**Класс обхода:**
| Код | Значение |
|-----|----------|
| §4а | static-fn вместо value-record const / kurz-формы |
| §4б | ручной код / инлайн вместо готового API / итератора |
| §5а | лишняя квалификация / касты / runtime assertion |
| §5б | TODO/FIXME, блокирующий фич-включение |
| §5в | закомментированный код с объяснением дефекта |
| §ист | историческая справка (закрытый дефект, НЕ обход) |
| §конв | осознанное инженерное решение (НЕ обход) |

**ЖИВ?** — актуальна ли причина (маркер/дефект) на 2026-07-30:
- ✅ — причина закрыта → обход **можно снять**
- ⚠️ — причина жива (маркер OPEN / не зафиксен) → обход **ждёт фикса**
- ⚪ — маркер НЕ В РЕЕСТРЕ (ни в `backlog-followups.md`, ни в `221.1-bug-sweep.md`, ни в плане-владельце) → обход **висит в воздухе**
- 🔵 — план-связанный маркер, следящий за собственным дефектом → обход штатный, снимется при реализации волны

---

## 1. Обходы по ключевым словам (46 hits)

### `std/src/time/civil/` — блок Plan 175.1 (✅ SHIPPED 2026-07-10, с маркерами отступлений)

| # | файл:строка | форма обхода | канон | маркер-причина | жив? | класс |
|---|---|---|---|---|---|---|
| 1 | `datetime.nv:17` | `DateTime` объявлен в `time_of_day.nv` (не в `datetime.nv`) | нормальная декларация в своём файле | `[M-175.1-value-in-value-emit-order]` | 🔵 OPEN | §4а |
| 2 | `time_of_day.nv:174` | value-record поля инициализируются через static-fn, а не const | `const MIDNIGHT = {…}` | `[M-175.1-value-in-value-emit-order]` | 🔵 OPEN | §4а |
| 3 | `period.nv:248` | `Period.between(...)` вместо `Date - Date` | операторная форма | `[M-175.1-minus-overload-arg-type]` | 🔵 OPEN | §4а |
| 4 | `errors.nv:107,110` | `RejectMismatch` вместо `Reject` (коллизия имён) | квалиф. `Disambiguation.Reject` как value | `[M-175.1-variant-name-collision]` + `[M-175.1-qualified-variant-value]` | 🔵 OPEN | §5в |
| 5 | `tz.nv:14` | curated-таблица вместо полного IANA-snapshot | полный ~450KB tzdb | `[M-175.1-full-tzdb-embed]` | 🔵 OPEN | §5в |
| 6 | `tzif.nv:13` | TZif-parser + embedded-fallback (не полный snapshot) | полный snapshot | `[M-175.1-full-tzdb-embed]` | 🔵 OPEN | §5в |
| 7 | `zoned.nv:225` | default-параметр через arity-split, не через default-value | `fn f(x, opt = Default)` | `[M-175.1-enum-default-param]` | 🔵 OPEN | §5в |
| 8 | `civil_test.nv:87,303` | bound-local ресивер вместо `DayOfWeek.Sunday.next()` | метод на variant-литерале | `[M-175.1-variant-literal-receiver]` | 🔵 OPEN | §4а |
| 9 | `parse_test.nv:178` | Display-тела проверяются не через интерполяцию | `"${x}"` | `[M-175.1-interp-value-record-display]` | 🔵 OPEN | §5в |
| 10 | `neg/period_not_duration.nv:7-8` | neg-тест на operator-форму (ожидание compile-error) | оператор d + duration | `[M-175.1-operator-arg-type-blind]` + `[M-175.1-minus-overload-arg-type]` | 🔵 OPEN | §5в |
| 11 | `zoned_test.nv:39` | мокабельность через handler-литерал, а не `Offset.local()` | эффект-оп | `[M-175.1-local-offset-effect-op]` | ✅ CLOSED 2026-07-10 | §ист |
| 12 | `zoned_test.nv:176` | обход квалиф. варианта через value | нормальный qualified-variant | `[M-175.1-qualified-variant-value]` | 🔵 OPEN | §5в |
| 13 | `testing/handlers/core.nv:249,276` | slot-coherence + fixed UTC-зона вместо `Offset.local()` | эффект-оп | `[M-175.1-local-offset-effect-op]` | ✅ CLOSED 2026-07-10 | §ист |
| 14 | `testing/handlers/core_test.nv:309` | слот-проверка `Offset.local()` | эффект-оп | `[M-175.1-local-offset-effect-op]` | ✅ CLOSED 2026-07-10 | §ист |

### `std/src/time/duration/` — Plan 175 Ф.1

| # | файл:строка | форма обхода | канон | маркер-причина | жив? | класс |
|---|---|---|---|---|---|---|
| 15 | `core.nv:210-244` | `Duration.from_secs(0)` вместо `Duration.ZERO` | value-record const | `[M-175-value-record-const-ref]` | ⚠️ OPEN (P2, backlog:100) | §4а |
| 16 | `core_test.nv` | inline вызовов вместо generic-tuple-destructure | нормальный generic tuple return | `[M-175-value-in-generic-tuple-return]` | ⚠️ OPEN (P2, backlog:101) | §4б |

### `std/src/runtime/` — Misc codegen gaps

| # | файл:строка | форма обхода | канон | маркер-причина | жив? | класс |
|---|---|---|---|---|---|---|
| 17 | `string/core.nv` | `Str.from_utf8` / static-fn вместо value-record const | `Str.EMPTY` | `[M-175-value-record-const-ref]` | ⚠️ OPEN (backlog:100) | §4а |
| 18 | `string_builder.nv` | `Option[T].new()`→`None` вместо static-fn | generic static method | `[M-91.7-option-new-static]` | ⚪ НЕ В РЕЕСТРЕ (plan 91.7 only) | §5б |
| 19 | `string_builder.nv` | closure capture обходится костылём | полная closure-capture | `[M-str-interp-closure-capture-miss]` | ⚪ НЕ В РЕЕСТРЕ | §5б |
| 20 | `string_builder.nv` | filter-iter closure ctx обходится костылём | полный closure ctx | `[M-splititer-filter-closure-ctx-gap]` | ⚪ НЕ В РЕЕСТРЕ | §5б |

### `std/src/vec/` — Plan 138 / 172 / dd11

| # | файл:строка | форма обхода | канон | маркер-причина | жив? | класс |
|---|---|---|---|---|---|---|
| 21 | `core.nv` | map/filter вручную (инлайн циклы) | итераторные адаптеры | `[M-dd11-opaque-closure-take]` | ✅ CLOSED | §4б |

### `std/src/hashmap/` — Plan 137 / 150 / 138

| # | файл:строка | форма обхода | канон | маркер-причина | жив? | класс |
|---|---|---|---|---|---|---|
| 22 | `core.nv:43` | ручная проверка capacity | `ensure_capacity` | `[M-137-hashmap-ensure-capacity-hidden]` | ✅ CLOSED | §4б |
| 23 | `core.nv:191-194` | entry-API без default-ветки | `or_default` / `and_modify` | `[M-150-entry-or-default-gap]` | ✅ CLOSED | §4б |

### `std/src/io/`

| # | файл:строка | форма обхода | канон | маркер-причина | жив? | класс |
|---|---|---|---|---|---|---|
| 24 | `error.nv:29` | `Duration.from_secs(n)` вместо const | timeout-const | `[M-175-value-record-const-ref]` | ⚠️ OPEN (backlog:100) | §4а |

### `std/src/math/`

| # | файл:строка | форма обхода | канон | маркер-причина | жив? | класс |
|---|---|---|---|---|---|---|
| 25 | `complex.nv` | method-форма вместо generic free-fn (turbofish) | generics с type-param в return | `[M-codegen-method-return-turbofish]` | ✅ CLOSED 2026-07-06 | §5а |
| 26 | `int128.nv` | runtime assertion вместо compile-time проверок | compile-time checks | `[M-176-math-int128-no-intrinsics]` | ⚠️ OPEN (P3) | §5а |

### `std/src/prelude/`

| # | файл:строка | форма обхода | канон | маркер-причина | жив? | класс |
|---|---|---|---|---|---|---|
| 27 | `core.nv:46` | re-export через `pub use` с suppress-lint | нормальный re-export | `[M-175-reexport-rebind-lint]` | ⚠️ OPEN | §5б |
| 28 | `core.nv:56-61` | TODO: type alias assoc-method не работает | включить enable | (нет маркера) | ⚠️ OPEN (нет маркера) | §5б |

### `std/src/encoding/json.nv`

| # | файл:строка | форма обхода | канон | маркер-причина | жив? | класс |
|---|---|---|---|---|---|---|
| 29 | `json.nv` | инлайн io-циклов вместо форварда bounded-generic | bounded-generic forward | `[M-176-io-forward-bounded-generic]` | ⚠️ OPEN (P2, backlog:105) | §4б |
| 30 | `json.nv` | ручной encode-record-field-order (nondeterministic) | field order | `[M-json-encode-record-field-order-nondeterministic]` | ⚠️ OPEN | §4б |
| 31 | `json.nv` | encode-exhaustion с костылём | полная exhaust-check | `[M-175-json-encode-exhaustion]` | ⚠️ OPEN | §4б |
| 32 | `json.nv` | byte-peek вместо нормального cursor | lexer cursor | `[M-json-lexer-byte-cursor]` | ⚠️ OPEN | §4б |

### `std/src/text/regex.nv`

| # | файл:строка | форма обхода | канон | маркер-причина | жив? | класс |
|---|---|---|---|---|---|---|
| 33 | `regex.nv:14` | ожидание bounds на компилятор | compile-time bounds | (нет маркера) | ⚠️ OPEN (нет маркера) | §5б |
| 34 | `regex.nv:38-47` | детерминированное temp-имя вместо O_TMPFILE | `create_temp` API | `[M-176-create-temp]` | ⚠️ OPEN (P3, backlog:94) | §4б |
| 35 | `regex.nv:134-145` | инлайн форвард io-циклов | bounded-generic forward | `[M-176-io-forward-bounded-generic]` | ⚠️ OPEN (P2, backlog:105) | §4б |

### `std/src/net/tcp_share_test.nv`

| # | файл:строка | форма обхода | канон | маркер-причина | жив? | класс |
|---|---|---|---|---|---|---|
| 36 | `tcp_share_test.nv` | workaround для гонки fs-M:N | sched-park-concurrent-fs | `[M-fs-tls-mn-race]` | ✅ CLOSED | §ист |

### `std/src/reflect_test.nv`

| # | файл:строка | форма обхода | канон | маркер-причина | жив? | класс |
|---|---|---|---|---|---|---|
| 37 | `reflect_test.nv` | обход generic-static-dispatch-collision | полный generic-static dispatch | `[M-reflect-generic-static-dispatch-collision]` | ⚠️ OPEN | §4б |

### `std/src/fs/path.nv`

| # | файл:строка | форма обхода | канон | маркер-причина | жив? | класс |
|---|---|---|---|---|---|---|
| 38 | `path.nv:23-24` | CWStr НЕ введён (`CreateFileW` не нужен — libuv) | прямой Win32-биндинг | `[M-176-cwstr-direct-winapi]` | ⚠️ OPEN (P3, backlog:91) | §конв |

### `std/src/time/civil/offset.nv`

| # | файл:строка | форма обхода | канон | маркер-причина | жив? | класс |
|---|---|---|---|---|---|---|
| 39 | `offset.nv:11,61` | vtable-слот `NovaVtable_Time.local_offset_sec` | эффект-оп | `[M-175.1-local-offset-effect-op]` | ✅ CLOSED 2026-07-10 | §ист |

---

## 2. Маркеры не в реестре

Маркеры, отсутствующие одновременно в `backlog-followups.md` (OPEN-view) и `221.1-bug-sweep.md`:

| Маркер | Где найден | Статус |
|--------|-----------|--------|
| `[M-91.7-option-new-static]` | `runtime/string_builder.nv` | plan 91.7 Followups — след в плане есть, но глобальный реестр не знает |
| `[M-str-interp-closure-capture-miss]` | `runtime/string_builder.nv` | **нет нигде** в `docs/plans/` |
| `[M-splititer-filter-closure-ctx-gap]` | `runtime/string_builder.nv` | **нет нигде** в `docs/plans/` |

**Вывод:** маркеры `[M-str-interp-closure-capture-miss]` и `[M-splititer-filter-closure-ctx-gap]` не зарегистрированы ни в одном реестре. Это потерянные маркеры — работают как TODO, но не трекаются. `[M-91.7-option-new-static]` живёт только в плане 91.7, но не вынесен в backlog — легитимен, но прозрачен для глобального трекинга.

---

## 3. Группировка по статусу

### СНЯТЬ СЕЙЧАС (причина закрыта) — 8 шт.

| # | файл | маркер |
|---|------|--------|
| 11 | `zoned_test.nv:39` | `[M-175.1-local-offset-effect-op]` ✅ |
| 13 | `testing/handlers/core.nv:249,276` | `[M-175.1-local-offset-effect-op]` ✅ |
| 14 | `testing/handlers/core_test.nv:309` | `[M-175.1-local-offset-effect-op]` ✅ |
| 21 | `vec/core.nv` | `[M-dd11-opaque-closure-take]` ✅ |
| 22 | `hashmap/core.nv:43` | `[M-137-hashmap-ensure-capacity-hidden]` ✅ |
| 23 | `hashmap/core.nv:191-194` | `[M-150-entry-or-default-gap]` ✅ |
| 25 | `math/complex.nv` | `[M-codegen-method-return-turbofish]` ✅ |
| 36 | `net/tcp_share_test.nv` | `[M-fs-tls-mn-race]` ✅ |
| 39 | `time/civil/offset.nv:11,61` | `[M-175.1-local-offset-effect-op]` ✅ |

### ЖДЁТ ФИКСА (причина жива) — 22 шт.

| # | файл | маркер | приоритет (где указан) |
|---|------|--------|------------------------|
| 15 | `duration/core.nv:210-244` | `[M-175-value-record-const-ref]` | P2 |
| 16 | `duration/core_test.nv` | `[M-175-value-in-generic-tuple-return]` | P2 |
| 17 | `runtime/string/core.nv` | `[M-175-value-record-const-ref]` | P2 |
| 24 | `io/error.nv:29` | `[M-175-value-record-const-ref]` | P2 |
| 26 | `math/int128.nv` | `[M-176-math-int128-no-intrinsics]` | P3 |
| 27 | `prelude/core.nv:46` | `[M-175-reexport-rebind-lint]` | н/д |
| 28 | `prelude/core.nv:56-61` | (нет маркера) | н/д |
| 29 | `encoding/json.nv` | `[M-176-io-forward-bounded-generic]` | P2 |
| 30 | `encoding/json.nv` | `[M-json-encode-record-field-order-nondeterministic]` | н/д |
| 31 | `encoding/json.nv` | `[M-175-json-encode-exhaustion]` | н/д |
| 32 | `encoding/json.nv` | `[M-json-lexer-byte-cursor]` | н/д |
| 33 | `text/regex.nv:14` | (нет маркера) | н/д |
| 34 | `text/regex.nv:38-47` | `[M-176-create-temp]` | P3 |
| 35 | `text/regex.nv:134-145` | `[M-176-io-forward-bounded-generic]` | P2 |
| 37 | `reflect_test.nv` | `[M-reflect-generic-static-dispatch-collision]` | н/д |
| 1-10,12 | `time/civil/*` | `[M-175.1-*]` (8 шт.) | plan-bound |

### НЕ В РЕЕСТРЕ (маркер не трекается) — 3 шт.

| Маркер | Где |
|--------|-----|
| `[M-str-interp-closure-capture-miss]` | `runtime/string_builder.nv` |
| `[M-splititer-filter-closure-ctx-gap]` | `runtime/string_builder.nv` |
| `[M-91.7-option-new-static]` | `runtime/string_builder.nv` (только в плане 91.7) |

### НЕ ОБХОД (осознанное решение / историческая справка) — 2 шт.

| # | файл | обоснование |
|---|------|-------------|
| 38 | `fs/path.nv:23-24` | CWStr не нужен — libuv сам конвертит UTF-8→UTF-16 на Windows |
| 39 | `offset.nv:11,61` | vtable-слот уже закрыт, код — не обход, а штатная реализация |

---

## 4. Топ-маркеров по частоте в `std/src` (только семантически значимые)

| Маркер | Вхождений | Статус |
|--------|-----------|--------|
| `[M-172.1-option-eq-record-structural]` | 147 | составной признак (код ссылается на structural-eq) |
| `[M-fixed-array-value-semantics]` | 146 | составной признак (массивы — value-типы) |
| `[M-ro-launder-via-mut-binding]` | 16 | codegen gap |
| `[M-cas-return-witnessed-value]` | 15 | трекинг CAS |
| `[M-lint-findings-static-conversion]` | 10 | lint suppression |

**Примечание:** `[M-172.1-option-eq-record-structural]` (147) и `[M-fixed-array-value-semantics]` (146) — НЕ маркеры обхода, а контекстные аннотации (формально объясняют, почему код использует record-structural vs builtin-eq, или массивы как value vs reference). Они не порождают технического долга и исключены из списка workaround'ов.

---

## 5. Выводы

1. **Можно снять сейчас** (8 шт.) — обходы, чья причина закрыта. Самый крупный блок — `[M-175.1-local-offset-effect-op]` (4 шт.), остальные — точечные codegen-фиксы.
2. **Ждёт фикса** (22 шт.) — основная масса. Ключевые блокаторы: `[M-175-value-record-const-ref]` (P2, 3 файла), `[M-176-io-forward-bounded-generic]` (P2, 2 файла), и блок Plan 175.1 (8 маркеров, привязаны к D321).
3. **Не в реестре** (3 шт.) — потерянные маркеры в `string_builder.nv`. Требуют заведения в `backlog-followups.md`.
4. **Не обход** (2 шт.) — штатное инженерное решение или история.
5. Общий тренд: workaround'ы сконцентрированы в трёх зонах — **codegen value-record** (175), **io-циклы** (176) и **Plan 175.1 civil-time отступления**.
