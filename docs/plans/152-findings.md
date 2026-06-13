<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 152 — Findings (execution log)

> Находки исполнителя по ходу Plan 152. Фиксируются здесь (а не молча решаются),
> когда спека/шпаргалка расходится с кодом или план неоднозначен. Создан 2026-06-13,
> worktree `nova-p152` (база `main` cc73f0f5).

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
- Роль реестра (`runtime_registry.rs`) сейчас: встроенное знание компилятора для
  **C-dispatch** методов (`eq`/`lt`/`le`/`gt`/`ge`/`hash`, `nova_body: None`) + type-info
  до парсинга std. Для Nova-body методов реестр дублирует `.nv` (вестигиально).

**Следствие:** str-методы можно править/добавлять/переносить на Nova-стороне свободно;
реестр держать консистентным только для C-only методов (eq/lt/…/hash). Регенерацию
стабов из реестра НЕ запускать (перезатрёт hand-written хелперы).

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
