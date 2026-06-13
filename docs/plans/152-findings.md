<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 152 — Findings (execution log)

> Находки исполнителя по ходу Plan 152. Фиксируются здесь (а не молча решаются),
> когда спека/шпаргалка расходится с кодом или план неоднозначен. Создан 2026-06-13,
> worktree `nova-p152` (база `main` cc73f0f5).

---

## Решения (согласовано с автором, 2026-06-13)

- **D-R1. 152.0 структура** — папка `std/runtime/string/`, co-equal файлы все
  `module runtime.string` (НЕ facade-файл; см. F4). `_buffer` — sibling-модуль
  `runtime.string_buffer`.
- **D-R2. Чистка реестра str (в 152.0)** — удалить **вестигиальные Nova-body**
  записи str из `runtime_registry.rs` (`len`/`byte_len`/`char_len`/`byte_at`/
  `is_empty`/`starts_with`/`ends_with`/`contains`/`find`/`rfind`/`char_at`/`trim`/
  `to_lower`/`to_upper`/`concat`/`plus`/`to_bytes`/`as_bytes`/`to_chars`/`split`/
  `compare`/`parse_int`/`pad_left`/`pad_right`/`repeat`/`replace`). Они НЕ драйвят
  резолв (F2). Интенция автора: всё в `.nv`, компилятор про str-методы не знает.
  Guard: полный `nova test` без новых FAIL (флаги `is_consume`/`is_mut` у str-методов
  отсутствуют → `types/mod.rs:12233` для str ничего не теряет).
- **D-R3. Что компилятор по-прежнему знает про str (НЕ трогаем в 152.0)** — C-операторы
  `==`/`!=`/`+`/`<`/`<=`/`>`/`>=` (хардкод `emit_c.rs:17302` → `nova_str_eq`/`concat`/
  `lt`/…) + `@hash` (`nova_str_hash`, DoS-seed в C — намеренно). Реестровые записи
  `eq`/`lt`/`le`/`gt`/`ge`/`hash` оставить, пока живёт хардкод операторов.
- **D-R4. Декомиссия operator-lowering → 152.5a** — `<`/`<=`/`>`/`>=` синтезировать из
  `@compare` (Compare-протокол) вместо хардкода `nova_str_lt`; `==`/`+` — из `@eq`/`@concat`.
  Удаление реестровых `eq`/`lt`/`le`/`gt`/`ge` — **в одной фазе** с codegen-reroute (без
  промежутка без резолва). **Конечное состояние реестра str: только `@hash`.** Затрагивает
  `emit_c.rs` → отдельная фаза в 152.5a. **Полная декомиссия, БЕЗ perf-retain** (override
  автора проекта 2026-06-13: всё в `.nv`, C-lowering НЕ оставлять). **Perf — через
  RawMem-примитивы в Nova-body:** `@compare`→`RawMem.compare` (memcmp), `@concat`→
  `RawMem.copy_nonoverlapping` (memcpy) ≈ C-скорость без byte-loop. Бенч — подтвердить
  паритет, не решать. Маркер `[M-139.1-operator-lowered-methods]`.

- **D-R5. `_buffer` = `Vec[u8]`, отдельный `StrBuf`/`string_buffer.nv` НЕ вводится**
  (Ф.1/Ф.3/Ф.4, исполнение 2026-06-13). План задавал отдельный internal-модуль
  `string_buffer.nv` (`StrBuf` на RawMem). Но **`Vec[u8]` уже И ЕСТЬ RawMem-буфер**
  (Plan 131): `@with_capacity`/`@append` (`RawMem.copy` memmove, НЕ push-loop, vec_owned.nv:568)/
  `from_bytes_unchecked_steal` (reuse buffer, без второй копии). И **`StringBuilder` уже
  тонкая обёртка** над `{mut buf []u8}` (= Ф.4 де-факто выполнен). Вводить `StrBuf` —
  дублировать grow/alloc/NUL Vec'а (нарушает DRY + минимализм API, хендофф 153 п.3).
  **Решение:** `Vec[u8]` — единственный «дом буфера»; Ф.3 = заменить push-loop'ы
  (`trim`/`concat`/`to_bytes`) на `Vec.@append` (bulk memmove) + `from_bytes_unchecked_steal`.
  Цель автора («builder-логика в одном месте на RawMem, ноль push-loop-копипаста»)
  достигнута через Vec, без нового типа. **AMEND 152.0 Scope:** `string_buffer.nv` строка
  снята; `_buffer` ≡ `Vec[u8]`. (`to_lower`/`to_upper`/`from_bytes_lossy` сохраняют свои
  loop'ы — это per-byte ТРАНСФОРМ, не copy-paste alloc/grow/NUL.)

> **Q1/Q2 апрув автора (2026-06-13):** Q1 → **вариант C** (`CharsView` → `CharsIter`
> `{buf str, pos int}`, `Next[char]`; нет позиционных `at`/`len`-коллекции; codepoint-count
> = `as_chars().count()`; нет `char_len`/`char_at` на `str`). D250 переопределён в
> [152.1](152.1-coordinate-model-lenses.md). Q2 → D-R4 апрув с perf-гардом (выше).
> Автор НЕ редактирует файлы 152 (во избежание конфликтов) — все правки вносит исполнитель.

---

## F1 — База: `main`, не `plan-138.1` (шпаргалка устарела) ✅ resolved

Шпаргалка/память: «str-инфра в `plan-138.1`, 139.x НЕ в main, сверься с базой».
**Эмпирически неверно сейчас:**

- `git merge-base --is-ancestor plan-138.1 main` → **YES** (`plan-138.1` 7a237011 —
  предок `main`; 0 коммитов впереди main).
- `main` содержит `type str value priv {…}` в [core.nv](../../std/prelude/core.nv#L193);
  `std/runtime/string.nv` в main на **+316 строк** больше, чем в `plan-138.1`.

**Решение:** worktree `nova-p152` создан от `main` (cc73f0f5). plan-138.1 не нужен.

---

## F2 — `string.nv` — hand-maintained форк, НЕ регенерируется из реестра ✅ resolved

Заголовок [string.nv](../../std/runtime/string.nv) говорит «AUTO-GENERATED … Source of
truth: runtime_registry.rs». Реально:

- `nova-codegen emit-runtime-stubs --check` на main **падает**: `string.nv` И `char.nv`
  «diverge from registry». Т.е. CI-guard не блокирует — файлы давно форкнуты вручную
  (содержат `cp_to_char`/`validate_utf8`/`from_bytes_*`/`@sub_view`/`replace`/
  `try_parse_int`/`ParseIntError`, которых нет в реестре).
- **Резолв str-методов идёт из распарсенного `.nv`, НЕ из реестра**: probe-метод
  `@probe_byte_len`, добавленный только в `.nv` (без записи в реестре), резолвится и
  работает (Ф.0.0).
- **Резолв str-методов = из распарсенного `.nv`, НЕ из реестра** (verified 2026-06-13):
  метод `get` имеет **0** записей в реестре (живёт только в `string.nv`) — и
  резолвится+работает **без импорта** (`nova test` PASS). str-методы доступны глобально
  потому, что prelude **загружает** модуль `runtime.string` (`export import
  std.runtime.string.{…}` парсит файл), а type-directed method-resolution находит ВСЕ
  методы типа str в загруженном модуле (независимо от списка имён в import).
- Реестр (`runtime_registry.rs`) для str-Nova-body записей (`find`/`len`/`split`/…) —
  **вестигиален**: единственный потребитель `types/mod.rs:12233` читает их только ради
  `is_consume`/`is_mut`-флагов, которых у str-методов нет (str иммутабелен, не consume).
  Плюс мёртвая stub-gen (divergent).
- Что компилятор **реально** ещё хардкодит про str (не в `.nv`): операторы
  `==`/`!=`/`+`/`<`/`<=`/`>`/`>=` лоуэрятся напрямую в C `nova_str_eq`/`concat`/`lt`/…
  ([emit_c.rs](../../compiler-codegen/src/codegen/emit_c.rs) BinOp, option (b)) +
  `@hash` (`nova_str_hash`, security/DoS-seed) + C-helpers `nova_str_index_panic`/
  `slice_panic`/`terminated_ptr`. Маркер `[M-139.1-operator-lowered-methods]` —
  декомиссия operator-lowering (future).

**Следствие:** str-методы можно править/добавлять/переносить на Nova-стороне свободно
(реестр НЕ драйвит резолв). **Авторская интенция (2026-06-13):** все str-методы — в
`.nv`, компилятор про них не знает (кроме `@hash` + C-операторов). → 152.0 МОЖЕТ
заодно вычистить вестигиальные str-Nova-body записи из реестра (безопасно: единственный
консумер `types/mod.rs:12233` для str ничего не вставляет). Регенерацию стабов из
реестра НЕ запускать (перезатрёт hand-written `.nv`).

---

## F3 — facade `export import` + кросс-модульная type-method privacy работают ✅ resolved

- `export import` уже в проде: [prelude.nv:162](../../std/prelude.nv#L162) реэкспортит
  str-методы из `std.runtime.string`.
- str-методы уже живут в **4 модулях** (`runtime.string`, `runtime.char`, `ffi.cstr`,
  `prelude.protocols`) — N>2 для str-методов уже факт.
- Ф.0.0 probe (PASS через C-codegen `nova test`): str-метод в **отдельном модуле-файле**
  читает priv `@len` И `@ptr` (type-method privacy — type-based, кросс-модульна); facade
  `export import` этих методов работает.

---

## F4 — Резолвер запрещает `file + folder` с одним именем; решено задуманной моделью «папка = один модуль» ✅ resolved

> **РЕШЕНО (2026-06-13, подсказка автора):** дизайн facade из 152.0 не нужен. Модель
> модулей Nova **изначально** допускает: папка = ОДИН модуль, много **равноправных**
> файлов, все объявляющие один `module` name. Прецедент в std: `sync.nv` + `sync_test.nv`
> — оба `module runtime.sync` (flat). Ф.0.0 подтвердил то же для **папки**: `string/core.nv`
> + `string/search.nv`, оба `module runtime.string`, методы из обоих резолвятся (PASS, C-codegen).
>
> **Итоговая структура 152.0 (заменяет facade-дизайн):** удалить файл `std/runtime/string.nv`;
> создать папку `std/runtime/string/` с файлами `core.nv`/`search.nv`/`transform.nv`/`parse.nv`/
> `chars.nv` — **все `module runtime.string`** (сливаются в один модуль). Internal `_buffer` —
> отдельный модуль (`runtime.string._buffer` или sibling `runtime.string_buffer`), не
> реэкспортируется. **Преимущества:** полный слоистый сплит (главная цель 152.0); существующие
> `import std.runtime.string.{X}` работают БЕЗ изменений (один модуль); реестр
> `module: "std.runtime.string"` валиден (ноль миграции); конфликта F4 нет (нет файла
> `string.nv`). Опции A/B/C из черновика ниже — отвергнуты в пользу этого. **AMEND плана
> 152.0:** «facade `string.nv` + папка» → «папка `string/`, co-equal файлы `module runtime.string`».

### (исторический черновик — почему facade-дизайн невалиден)

**Ф.0.0 (первый вариант, как в плане):** `string.nv` (module `runtime.string`) +
папка `std/runtime/string/{probe.nv}` (submodule `runtime.string.probe`) →

```
CODEGEN-FAIL: ambiguous module 'std.runtime.string':
              both single-file and folder-module exist
```

**Модель модулей Nova:** папка = namespace; каждый `.nv` внутри = submodule
(`folder.file`, как `std/collections/{vec,hashmap,…}.nv`). **Facade-файла для самой
папки НЕТ** (`_module.nv` — только носитель prelude-атрибутов, Plan 107/D174; контент
не несёт). `string.nv` и `string/` рядом — запрещено.

**Дизайн 152.0 «facade `string.nv` (= `runtime.string`) + папка `string/{core,search,
transform,parse,chars,_buffer}`» НЕВАЛИДЕН as-written.**

### Опции (выбор определяет всю реализацию 152.0)

| | Опция A — folder-only | Опция B — sibling-files (fallback плана) |
|---|---|---|
| Структура | удалить `string.nv`; `runtime.string` = namespace-папка с submodules `core/search/transform/parse/chars/_buffer` | `string.nv` остаётся (= `runtime.string`); рядом sibling-файлы `string_buffer.nv` (internal `_buffer`), опц. `string_chars.nv` |
| Слоистый сплит | ✅ полный (главная цель 152.0) | ⚠ частичный (string.nv остаётся монолитом методов) |
| RawMem `_buffer` + ноль копипаста | ✅ | ✅ |
| StringBuilder dedup на `_buffer` | ✅ | ✅ |
| Миграция | 13 import-сайтов (1 prelude + 12 тестов `ParseIntError`) → submodules; prelude re-export; реестр `module:` поля | нет (string.nv = `runtime.string` как в реестре) |
| Риск | средний (связка с реестром `module: "std.runtime.string"` не до конца де-рискнута; широкий blast как фундамент) | низкий (реестр-консистентность сохраняется) |
| План | главная цель | явно разрешён как fallback «при ограничении резолвера» |

**Опция C (язык):** расширить модель модулей до folder-facade (язык-фича). Самый
верный долгосрочно, но крупнейший scope (трогает резолвер) — отдельный language-план.

**Рекомендация исполнителя:** **B** для фундамента сейчас (низкий риск, разблокирует
152.1+ немедленно, достигает всех инженерных целей 152.0; folder-сплит — позже, если
выберем C/амендмент модели). F4 фиксируется → дизайн facade в 152.0 надо амендить.
