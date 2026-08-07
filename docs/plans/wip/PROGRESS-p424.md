# PROGRESS — окно p424-d310-amendment

**Модель:** Sonnet 5 (sonnet), thinking effort medium-high (не задавался явно — дефолт окна).
**Ветка:** `p424-d310-amendment`, worktree `d:/Sources/nv-lang/nova-p424`.
**Задача:** D310-амендмент, реестр `docs/plans/221.1-bug-sweep.md` №424 — (А) снять
`E_TYPE_SET_MIXED_SIGNEDNESS`, (Б) разрешить вложенные type-set'ы. Один атом, один цикл кода,
спек-амендмент в том же слиянии (язык-меняющее).

---

## 0. Ход окна (для протокола — обрыв связи среди сессии)

Связь оборвалась один раз в середине работы (после фазы А+Б компилятора, до фикстур/спеки) —
восстановлено по сообщению координатора. Чекпоинт-коммиты делались отдельно на каждую фазу,
незакоммиченной работы на момент обрыва не было (первым действием после восстановления —
`git add`+`commit` текущего состояния, затем продолжение).

`main` дважды уезжал вперёд, пока окно работало (52, затем ещё 12 коммитов) — оба раза слито
(`git merge main --no-edit`) БЕЗ конфликтов; `compiler-codegen/src/types/mod.rs` апстримом не
трогался в обоих окнах дрейфа (проверено `git diff <merge-base> main -- .../types/mod.rs` —
пусто первый раз, только `nova_rt/effects.c/.h` второй раз). Второе слияние принесло фикс
№433 (`std/src/encoding/json.nv`, `Lexer.char_at` → `Option`), который до этого блокировал
ЛЮБОЙ прогон мега-CU `spec_tests/conformance` (см. §3).

---

## 1. Компилятор — что сделано

### (А) Снятие `E_TYPE_SET_MIXED_SIGNEDNESS`

`compiler-codegen/src/types/mod.rs`, функция `check_generic_bound_declarations`. Удалено:
`signed_ints`/`unsigned_ints` списки, `signed_seen`/`unsigned_seen` HashSet'ы, их заполнение,
блок `is_full_union` и сама диагностика `E_TYPE_SET_MIXED_SIGNEDNESS`. Исключение
`is_full_union` (костыль под снимаемый запрет) удалено ЦЕЛИКОМ, а не оставлено мёртвым кодом.

### (Б) Вложенные type-set'ы

В том же блоке — из списка запрещённых kind'ов члена убран `"type_set"` (остались только
`"protocol"`/`"effect"`). Разворачивание реализовано отдельным разделяемым helper'ом
`expand_type_set_members` (DFS, `memo` + recursion `stack`, cycle-safe через `Result<_,
Vec<String>>` с путём цикла в `Err`), применённым в ТРЁХ точках:

| Точка применения | Файл / метод | Что делает |
|---|---|---|
| Декларация | `check_generic_bound_declarations` | На каждый объявленный type-set — фиксирует `E_TYPE_SET_CYCLE` при цикле (свежий `stack`+общий `memo` на файл). |
| Membership / `E_TYPE_NOT_IN_SET` | `BoundCtx::build` → поле `type_sets: HashMap<String, Vec<(TypeRef, Option<String>)>>` | Хранит РАЗВЁРНУТЫЙ (не сырой) список членов с origin-тегом (`Some(via)` — пришёл через вложенный набор, `None` — объявлен напрямую). При цикле (уже задиагностированном на декларации) — деградирует к пустому списку, не паникует и не зацикливается. |
| Numeric fast-path (172.1.2 Binary-bounds) | `TypeCheckCtx::typeset_all_scalar` (новый метод) | Рекурсивно проверяет, что ВСЕ (транзитивные) члены bound'а — скалярные примитивы; без этого `x + x`/`T.MAX` над `type Q set SignedInts | UnsignedInts` не попадали бы на быстрый арифметический путь (использует `self.types`, отдельный от `BoundCtx.type_sets` — другая структура, другой момент компиляции). |

`E_TYPE_NOT_IN_SET`-сообщение (`check_satisfaction`) теперь показывает происхождение члена:
`Allowed members: {i32 (via SignedInts), u32 (via UnsignedInts), ...}` вместо голого списка —
без этого сообщение ссылалось бы на имя, отсутствующее в тексте объявления набора.

`~underlying` НЕ введён — вложенность = композиция явно перечисленных множеств, не
structural-matching.

---

## 2. Три диагностики (снятая / изменённая / новая)

| Код | Статус | Было | Стало |
|---|---|---|---|
| `E_TYPE_SET_MIXED_SIGNEDNESS` | **СНЯТА** | `set i32 \| u32` (частичный микс) — compile error; `Ints` (полный union) — единственное исключение | Код диагностики удалён из компилятора целиком (см. проба §3). Частичный и полный микс оба легальны. |
| `E_TYPE_SET_MEMBER_NOT_CONCRETE` | **ИЗМЕНЕНА** | Член — protocol / effect / **другой type-set** — все три отклонялись | Отклоняются только protocol / effect. Вложенный type-set как член — легален (разворачивается). |
| `E_TYPE_SET_CYCLE` | **НОВАЯ** | не существовала | Транзитивный цикл разворачивания (`type C set C`; `type A set B` + `type B set A`) — диагностика с путём цикла (`C -> C`, `A -> B -> A`). |

---

## 3. Проба «подсунь заведомо негодное»

```
grep -rn "E_TYPE_SET_MIXED_SIGNEDNESS" compiler-codegen/src   → 0 совпадений
grep -rn "is_full_union" compiler-codegen/src                 → 0 совпадений
```

Оба грепа гоняны ПОСЛЕ финального коммита компиляторной фазы — НОЛЬ вхождений, включая
doc-комментарии (первый черновик случайно процитировал старый код диагностики в новом
комментарии у поля `type_sets` — найдено этим же грепом и переписано без буквальной строки
отдельным коммитом `60fe4be5a`).

Остаточные текстовые упоминания `MIXED_SIGNEDNESS` вне `compiler-codegen/src` — ТОЛЬКО как
явно помеченный исторический контекст («правило было — снято амендментом»):
`spec/decisions/04-effects.md` (амендмент-абзац D423 §R1), `spec/decisions/README.md`
(D423-строка индекса), `spec_tests/conformance/mixed_signedness.nv` (комментарий файла,
объясняющий его происхождение из `neg/`), `docs/plans/backlog-followups.md` /
`docs/plans/172.3-type-set-bounds.md` / `docs/plans/221.1-bug-sweep.md` (планы/реестр —
историческая запись процесса, не источник истины о текущем поведении; не редактировались).

---

## 4. Приёмка — вердикт по всем 8 пунктам

**Как верифицировано:** `spec_tests/conformance` — folder-module, ОДИН compile-unit на 1155+
файлов; любой путь, переданный `nova test`, всё равно тянет ВСЮ директорию. До второго
слияния с `main` это упиралось в пред-существующий блокер №433 (`E_BANG_REQUIRES_FAIL` в
`std/src/encoding/json.nv`, зарегистрирован интегратором независимо от этого окна) — красный
на ЛЮБОМ файле мега-CU, включая нетронутый `d310_type_set_bound.nv` (проверено ДО фикса).
После слияния №433 фикс подъехал, но полная сборка мега-CU (check + codegen + clang) не
уложилась в 10-минутный потолок инструмента (не финализирована зелёной — это ожидаемо: авторитетный
гейт мега-CU целиком — за координатором, мне предписано `--filter`/точечные прогоны).
Поэтому пункты 1-4 и регрессы верифицированы ДВАЖДЫ: (a) точной байт-копией содержимого
целевого/новых файлов в изолированный standalone-модуль (`module p424.*`, вне
`spec_tests.conformance` — тот же приём, что test-conventions предписывает для РАЗРАБОТКИ
нового D-теста до мержа в общий CU) — компиляция и исполнение через `nova test`/`--strict-effects`;
(b) neg-фикстуры (5,6,7) — напрямую в реальном месте, т.к. `module neg.*` уже standalone-CU
по конвенции, мега-CU их не касается.

| № | Пункт | Вердикт | Как проверено |
|---|---|---|---|
| 1 | `type P set i32 \| u32` + обобщённое тело, вызванное на ОБОИХ членах | ✅ PASS | `spec_tests/conformance/mixed_signedness.nv` (`D310MixedSet`/`d310_mixed_twice`), байт-копия в изолированном модуле: `nova test`+`--strict-effects` зелёное, оба члена (`i32`/`u32`, explicit type-args И через инференс) реально вызваны и дают верный результат. |
| 2 | `type Q set SignedInts \| UnsignedInts` + тело над `Q` | ✅ PASS | `spec_tests/conformance/d310_type_set_nested.nv` (`D310NestedQ`/`d310_nested_q_twice`), байт-копия изолированно: зелёное, члены из ОБОИХ вложенных наборов (`i32`/`i64` via SignedInts, `u32`/`u8` via UnsignedInts) вызваны и дают верный результат. |
| 3 | `type R set Ints \| i32` — дедупликация не ломает мономорфизацию | ✅ PASS | Тот же файл (`D310NestedR`/`d310_nested_r_twice`): `i32` (дублируемый — direct AND via `Ints`) и `u16` (только via `Ints`) оба мономорфизуются верно. |
| 4 | Без регресса: `Ints` (protocols.nv), D423 `checked_add`, `d310_type_set_bound.nv`, `primitive_bounded_blanket_dispatch.nv` | ✅ PASS | (a) `nova check std/src` — 0 находок с кодом `E_TYPE_SET_*`, диффренциально против стального main-бинаря (главный репо, бинарь собран ДО фикса №433) — единственная дельта в 5 файлов (`json`/`jwt`) объяснена устаревшим бинарём, НЕ моей правкой (таймстемпы: бинарь 17:03:58, коммит №433-фикса 17:11:27). (b) `d310_type_set_bound.nv` и `primitive_bounded_blanket_dispatch.nv` — байт-копии (только `module`-строка заменена) в изоляции: оба зелёные (`i64.checked_add` дисп. на блан­кет `Ints`, `T.MAX`-резолв per-member на `D310Ints`). |
| 5 | `type C set C` → `E_TYPE_SET_CYCLE` | ✅ PASS | `spec_tests/conformance/neg/type_set_cycle_self.nv` (`D310SelfCycle` — переименовано с `C`, см. §5 «побочная находка»). `nova check`/`nova test --compile-error` — ОДНА диагностика `E_TYPE_SET_CYCLE`, путь `D310SelfCycle -> D310SelfCycle`. |
| 6 | `type A set B` / `type B set A` → `E_TYPE_SET_CYCLE` | ✅ PASS | `spec_tests/conformance/neg/type_set_cycle_mutual.nv` (`D310CycleA`/`D310CycleB`). Обе декларации диагностируются отдельно: `D310CycleA -> D310CycleB -> D310CycleA` и `D310CycleB -> D310CycleA -> D310CycleB`. |
| 7 | `neg/member_not_concrete.nv` по-прежнему красная, БЕЗ правок файла | ✅ PASS | Файл НЕ тронут (git diff пуст). `nova test spec_tests/conformance/neg/member_not_concrete.nv --compile-error` — PASS (то есть по-прежнему корректно красная на `E_TYPE_SET_MEMBER_NOT_CONCRETE`, protocol-член). |
| 8 | `type Y set SomeEffect` → `E_TYPE_SET_MEMBER_NOT_CONCRETE` | ✅ PASS | Изолированный пробник (`type SomeP424Effect effect {...}` + `type YEffectSet set SomeP424Effect`) — `nova check`: ровно `E_TYPE_SET_MEMBER_NOT_CONCRETE`, текст диагностики называет kind `effect`. Постоянной фикстуры не заводил — не требовалась (не НОВЫЙ код, уже покрыт `neg/member_not_concrete.nv` для protocol-случая по тому же коду). |

**Neg-фикстура на новый код (правило 5 test-conventions.md, обязательна для гейта
`check-test-fixture-coverage.sh`):** ЕСТЬ, причём ДВЕ (`type_set_cycle_self.nv` +
`type_set_cycle_mutual.nv`) — сверх минимума, для покрытия self- и mutual-топологии цикла.

**Побочная находка при написании неg-фикстур (не баг компилятора, отчёт для протокола):**
первая версия использовала однобуквенные имена (`type C set C`, `type A set B`/`type B set A`)
— `nova check` вернул НЕ ТОЛЬКО `E_TYPE_SET_CYCLE`, но и `E_TYPE_NAME_TOO_SHORT` (D30 lint) И,
для `C` конкретно, каскад `E_PREFIX_SHADOWS_NAMED_TYPE` из `std/src/collections/vec_iter/core.nv`
(там `fn[C Iter[I], ...]` использует `C` как ИМЯ ГЕНЕРИКА повсеместно — мой top-level `type C`
из std-prelude-видимой области реально с ним схлопывался). И `--compile-error`-тест, и сама
диагностика `E_TYPE_SET_CYCLE` при этом работали правильно (гейт всё равно проходил — раннер
матчит код диагностики, не считает диагностики), но лишний шум маскировал бы регрессию другого
рода. Переименовано в domain-prefixed (`D310SelfCycle`/`D310CycleA`/`D310CycleB`) — чисто.

---

## 5. Спека — карта затронутых мест

| Место | Действие |
|---|---|
| `spec/decisions/02-types.md` §D310 «Синтаксис» (:16186 на момент брифа) | **ИЗМЕНЕНО** — дополнено вложенностью/разворачиванием/дедупликацией/циклами (`E_TYPE_SET_CYCLE`)/происхождением члена/явной фразой про `~underlying`. |
| `spec/decisions/02-types.md` §D310 «Знаковость» (:16190) | **УДАЛЕНО ЦЕЛИКОМ** (правило снято). |
| `spec/decisions/02-types.md` §D310 «Члены — по ИДЕНТИЧНОСТИ» | **ИЗМЕНЕНО** — вложенный type-set легален как член; protocol/effect остаются запрещены. |
| `spec/decisions/02-types.md` §D310 «Проверки/диагностика» (:16195) | **ИЗМЕНЕНО** — убран `E_TYPE_SET_MIXED_SIGNEDNESS`, добавлен `E_TYPE_SET_CYCLE`, переформулирован `E_TYPE_SET_MEMBER_NOT_CONCRETE`. |
| `spec/decisions/02-types.md` §D310 «Почему» | **ИЗМЕНЕНО** — убран пункт про «знаковость на уровне декларации», добавлен про композицию вместо копирования. |
| `spec/decisions/02-types.md` §D310 — новый подраздел «Амендмент (D310 amendment, Plan p424)» | **ДОБАВЛЕНО** — обоснование обоих изменений одним текстом (self-contradiction argument + Go-прецедент для вложенности). |
| `spec/decisions/02-types.md` :42 (индекс-таблица) | **ПРОВЕРЕНО БЕЗ ИЗМЕНЕНИЙ** — не ссылается на снятое правило. |
| `spec/decisions/02-types.md` :4045 | **ПРОВЕРЕНО БЕЗ ИЗМЕНЕНИЙ** — определение bound'а, не зависит от знаковости. |
| `spec/decisions/02-types.md` :7296-7303 | **ПРОВЕРЕНО БЕЗ ИЗМЕНЕНИЙ** — про `~underlying`/Q-representation-bound, независимо от снятого правила. |
| `spec/decisions/02-types.md` :16243 (связь-список) | **ПРОВЕРЕНО БЕЗ ИЗМЕНЕНИЙ** — упоминает `SignedInt`/`UnsignedInt` как пример, не ссылается на правило. |
| `spec/decisions/04-effects.md` D423 §Статус | **ИЗМЕНЕНО** — фраза про «full-union exemption от `E_TYPE_SET_MIXED_SIGNEDNESS`» переформулирована, ссылка на D310-амендмент. |
| `spec/decisions/04-effects.md` D423 §R1 | **ИЗМЕНЕНО** — добавлен амендмент-абзац: та же R1-мотивировка стала доводом снять правило D310; сам D423 (`Ints`/`checked_add`/trap-политика) НЕ меняется по существу. |
| `spec/decisions/README.md` D423-строка индекса | **ИЗМЕНЕНО** — краткая синхронизация с 04-effects.md. |
| `spec/syntax.md` :1462-1467 | **ИЗМЕНЕНО** — один абзац на оба изменения (вложенность + снятие «Знаковости»), явная фраза про `~`. |
| `spec/syntax.en.md` (зеркальный абзац) | **ИЗМЕНЕНО** — синхронно с syntax.md. |
| `spec/syntax.md` :631 (таблица токенов `set`) | **ПРОВЕРЕНО БЕЗ ИЗМЕНЕНИЙ** — не зависит от правила. |
| `std/src/prelude/protocols.nv` :683-684 | **ИЗМЕНЕНО** — doc-комментарий над `SignedInts`/`UnsignedInts` ссылался на снятый код; переписан (раздельность = удобство, не обязательность). |
| `std/src/prelude/protocols.nv` `type Ints set ...` (:691) | **ПРОВЕРЕНО БЕЗ ИЗМЕНЕНИЙ (сознательно)** — НЕ переписан на `set SignedInts \| UnsignedInts`, хотя новая фича это бы позволила: минимизация риска для foundational-prelude в language-changing слиянии; список членов идентичен, поведение подтверждено регрессом (п.4). |
| `docs/guide/**` | **НЕ НАЙДЕНО** — 0 упоминаний `MIXED_SIGNEDNESS`/специфики D310-правила (интегратор уже проверил по брифу; повторный грep подтвердил). |
| `docs/plans/172.3-type-set-bounds.md`, `docs/plans/backlog-followups.md`, `docs/plans/221.1-bug-sweep.md` | **НЕ ТРОНУТЫ (сознательно)** — исторические планы/реестр, не источник истины о текущем поведении (тот — `spec/decisions/`); реестр правит интегратор. |

---

## 6. №425 (доложить, не чинить)

Расхождение имён спека/std, уже заведённое интегратором как №425 (`SignedInt`/`UnsignedInt` в
спеке vs `SignedInts`/`UnsignedInts` в `std/src/prelude/protocols.nv`), **воспроизведено и НЕ
чинилось** (не в объёме этого окна — интегратор уже знает). В моих НОВЫХ спек-правках (§D310
амендмент-абзац, `syntax.md`/`syntax.en.md`) единственную форму имени я не унифицировал — там,
где текст ИЛЛЮСТРИРУЕТ общий принцип (не цитирует конкретный prelude-тип), использована
СПЕКА-форма (`SignedInt`/`UnsignedInt`, единственное число) для консистентности с ОСТАЛЬНЫМ
текстом того же D-блока; там, где текст явно указывает на `std/prelude`-декларацию (амендмент
02-types.md, комментарий в `protocols.nv`) — STD-форма (`SignedInts`/`UnsignedInts`,
множественное число). Других новых расхождений спека↔std имён не нашёл при этом проходе.

---

## 7. Коммиты окна

```
b8e9650c7 compiler(p424): D310-amendment (А+Б) — снять E_TYPE_SET_MIXED_SIGNEDNESS, разрешить вложенные type-set'ы
19259473f Merge branch 'main' into p424-d310-amendment          (52 коммита апстрима, types/mod.rs не задет)
60fe4be5a compiler(p424): убрать последнее текстовое упоминание E_TYPE_SET_MIXED_SIGNEDNESS
846887c08 test(p424, D310-amendment): фикстуры на снятие MIXED_SIGNEDNESS + вложенные type-set + E_TYPE_SET_CYCLE
617ef9712 spec(p424): D310-амендмент — снятие «Знаковости» + вложенные type-set (02-types.md, syntax.md/.en.md, 04-effects.md, README.md)
b938b14e5 Merge branch 'main' into p424-d310-amendment          (12 коммитов апстрима, включая фикс №433; только nova_rt/effects.c/.h пересеклись, без конфликта)
```

Не пушено — по инструкции окна, доклад координатору для решения по мега-CU гейту.

---

## 8. Что НЕ сделано / известные ограничения

- **Мега-CU `spec_tests/conformance` целиком не прогнан зелёным моими руками** — авторитетный
  гейт закреплён за координатором; после второго слияния (фикс №433) полная сборка не
  уложилась в 10-минутный потолок инструмента (обычная длительность для 1155+-файлового CU,
  не признак дефекта — см. §4 методологию замены изоляцией).
- **`emit_c.rs`'s `type_set_members` (bounded-blanket dispatch, Plan 196.8)** — НЕ трогал: эта
  структура строится из СЫРОГО (нерасширенного) списка членов и обслуживает ТОЛЬКО
  primitive-receiver dispatch на `#impl(TypeSetName)`-блан­кетах (напр. `i64.checked_add(...)`
  через `Ints`). Ни один из тестов приёмки её не задействует (все — прямые generic-вызовы, не
  blanket-impl), и брифом эта зона explicitly НЕ упомянута среди мест правки в `types/mod.rs`.
  **Известный архитектурный гэп, НЕ в этом окне:** если когда-нибудь заведут ВЛОЖЕННЫЙ
  type-set КАК bound blanket-impl'а (`#impl(SomeNestedSet)`), primitive-receiver dispatch на
  него сегодня не сработает — `type_set_members` увидит имя вложенного набора вместо его
  листовых членов. Не заводил отдельным номером (не проверил практическую нужность); докладываю
  для интегратора на случай, если он сочтёт нужным завести маркер.
