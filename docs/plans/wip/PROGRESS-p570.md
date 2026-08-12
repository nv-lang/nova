<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# PROGRESS — p570-generic-refusal

Бриф: `docs/plans/wip/brief-p570-generic-refusal.md` (главное дерево, только
чтение). Дерево: `d:/Sources/nv-lang/nova-p262`, ветка `p570-generic-refusal`.
Модель: sonnet.

## Статус: ЗАВЕРШЕНО, все коммиты сделаны

Прерывание (серверный сбой) случилось в момент ДО-и-ПОСЛЕ проверки
`nova check std/src`; после рестарта sql.nv восстановлен, ДО/ПОСЛЕ
перепроверены, дальше по списку «осталось сделать» — саботаж пройден в обе
стороны, греп по corpus (std+spec_tests+examples) дал 0 посторонних
generic-эффектов/операций, и всё закоммичено шестью коммитами в порядке
чекпойнт → спека → компилятор → std → доки/планы → фикстуры:
`7531fefae` → `a6371c555` → `99b9d13c6` → `4ab68ba72` → `515ce6f31` →
`dc57cc349`. Рабочее дерево чистое (`git status --porcelain` пусто).
Остался только отчёт владельцу.

## Что сделано (реализация + верификация пройдены)

1. **Компилятор** (`compiler-codegen/src/types/mod.rs`):
   - Новая функция `check_effect_generic_refusal` (перед
     `check_handler_op_declarations`, вызывается из `check_module_impl`
     сразу после неё, ~строка 2119).
   - Ось 2 (`type X[T] effect`) → `[E_EFFECT_GENERIC_UNSUPPORTED]` на
     `td.generics[0].span`.
   - Ось 1 (`op[T](...)` внутри НЕ-generic эффекта) →
     `[E_EFFECT_OP_GENERIC_UNSUPPORTED]` на `m.generics[0].span`, по одному
     диагнозу на операцию.
   - Обе ссылаются на Q6 (`spec/open-questions.ru.md#q6`) текстом.
   - **Найдено эмпирически (build-verified, не рассуждением):**
     `std/src/prelude/effects.nv:48` — `export type Fail[E] effect { fail(e
     E) -> never }` — компилятор-интринсик, sugar-цель `throw`/`!!`, НЕ
     идёт через общий handler-literal vtable-путь. Первая сборка с
     отказом БЕЗ исключения ломала ВЕСЬ `std` (`Fail[E]` используется
     почти в каждой fallible-функции языка). Исключение — переиспользован
     существующий `arity_exempt(name)` (тот же список `Fail`/`Effect` уже
     используется чекером как «built-in эффект, гибкая арность», see
     `types/mod.rs` рядом с `check_module_impl`). `Ask`/`Alloc` из того же
     списка не декларируются через `Item::Type` и в проход не попадают
     вовсе — экзепшен их не касается практически, только по имени.
   - `check_handler_op_declarations` (соседний, существовавший проход):
     схема эффекта строится ТОЛЬКО для НЕ-generic `TypeDeclKind::Effect`
     (`if td.generics.is_empty()`) — иначе ветка (а) №614 (обработчик с
     подставленным типом) получала БЫ ВТОРОЙ диагноз
     (`E_HANDLER_OP_RETURN_TYPE_MISMATCH`) поверх названного отказа. Одна
     причина — одно сообщение; проверено фикстурой `reg614_...`, см. ниже.
   - Билд чистый (`bash scripts/tools/build-compiler.sh`, дважды: до и
     после Fail-исключения).
2. **`std/src/data/sql.nv`** — `Db.in_transaction[T, E]` (была строка 137)
   снята из `type Db effect { ... }`; на месте — комментарий-указатель на
   №570 и план 273 §4-тер (форма `Tx` не спроектирована — работа окна 268).
3. **Спека:**
   - `spec/decisions/04-effects.md` D456 — абзац-исключение переписан:
     было наблюдение «генерики не работают», стало — решение владельца
     2026-08-12, обе оси, оба кода ошибок, исключение `Fail[E]`, судьба
     `Db.in_transaction`. Новый D-блок НЕ заведён (амендмент существующего).
   - `spec/effects.ru.md` (~строка 499) — тот же перевод сути на русский
     обзорный документ.
   - `spec/effects.md` (английский) — **НЕ трогал**: у него в принципе НЕТ
     раздела "border of the effect" (весь блок, которому в ru.md
     соответствуют строки ~480-505, в английской версии отсутствует
     целиком — не только абзац про generic). Разрыв ДОСЛОВНО не про
     генерики, чинить его — целый раздел переводить, вне периметра этого
     окна; не решал за владельца, просто НЕ создавал новую ложь (нечего
     было чинить на английской стороне).
4. **Планы (2 упоминания брифа + 1 найденное третье):**
   - `docs/plans/177-fallible-result-everywhere.md:471` — снята
     `sql.in_transaction` из списка forwarding-`Fail` носителей, помечено
     «снят 2026-08-12, №570».
   - `docs/plans/wip/197-audit-progress.md` — амендмент-заметка добавлена
     В НАЧАЛО файла (append-стиль, как остальной документ — журнал per-
     заход, историю не переписывал): исторические упоминания
     `Db.in_transaction` (repro бага #2, `orm_decorators.nv:145`) — не
     трогал, это протокол, не текущее состояние API.
   - **НАЙДЕНО ПОПУТНО, не названо брифом:** `docs/plans/177-...md:265` —
     ТОТ ЖЕ carrier-claim, что и :471, дублирован в более раннем сводном
     абзаце того же файла. Поправлен той же правкой (иначе план бы
     противоречил сам себе — один абзац поправлен, другой нет).
5. **Фикстуры (созданы, `nova check` подтверждён):**
   - `spec_tests/conformance/neg/reg570_effect_op_generic_neg.nv` —
     `E_EFFECT_OP_GENERIC_UNSUPPORTED`, `nova check` КРАСНЫЙ на
     объявлении (не build).
   - `spec_tests/conformance/neg/reg614_effect_generic_neg.nv` —
     `E_EFFECT_GENERIC_UNSUPPORTED`, аналогично.
   - `spec_tests/conformance/pos_effect_nongeneric_handler_value.nv` —
     обычный эффект `Answer`, обработчик, `with`, `println(Answer.ask())`
     → `EXPECT_STDOUT 42`. Проверено ИЗОЛИРОВАННОЙ копией в scratchpad
     (`nova test` полный build+run, PASS, напечатал 42) — НЕ гонял `nova
     test` на самой `spec_tests/conformance` (там 3236 тестов = мега-CU,
     брифом запрещено); маркер в самом файле поправлен на конвенцию БЕЗ
     двоеточия (`EXPECT_STDOUT 42`, не `EXPECT_STDOUT: 42` — реальный
     парсер, `test_runner.rs::parse_expect`, двоеточие не ест).
6. **`nova check std/src` ДО/ПОСЛЕ (дословно, дважды прогнано):**
   - ДО (временный `git checkout --` sql.nv → HEAD, но С НОВЫМ чекером):
     `PASS: 152  FAIL: 28  WARN: 62`; сам `sql.nv` — FAIL с ИМЕННО
     `[E_EFFECT_OP_GENERIC_UNSUPPORTED]` на строке 137 (двойной листинг —
     std проверяется в двух проходах, оба поймали).
   - ПОСЛЕ (sql.nv восстановлен с моей правкой): `PASS: 154  FAIL: 26
     WARN: 62`; `sql.nv` — `ok`. Дельта ТОЧНО −2 FAIL/+2 PASS (одно и то
     же вхождение, дважды посчитанное) — больше НИЧЕГО не сдвинулось.
   - **Побочно подтверждено:** 154/26/62 совпадает С БАЗОВОЙ линией
     соседнего окна `p616-mode-modifiers` (`docs/plans/wip/
     PROGRESS-p616.md` §7 — тот же `nova check std/src` ДО/ПОСЛЕ, уже в
     `main`), то есть моя правка возвращает std РОВНО к состоянию ДО
     этого окна — ноль незамеченных побочных эффектов где-либо ещё в
     std.

## Всё сделано

1. **Саботаж (пройден в обе стороны):** `return;` первой строкой
   `check_effect_generic_refusal`, пересборка, обе neg-фикстуры →
   `ok:`/`PASS: 1 FAIL: 0` (зелёные). Возврат правки, пересборка, обе
   снова `FAIL:` с точно теми же кодами (`E_EFFECT_OP_GENERIC_UNSUPPORTED`
   / `E_EFFECT_GENERIC_UNSUPPORTED`) на тех же строках — дословно те же
   сообщения, что и до саботажа.
2. **Греп по corpus (точный счёт):** `nova check` (не мега-CU, лёгкий
   статический проход) на `std/src`, `spec_tests/conformance`, `examples`
   целиком — `grep -c` по обоим кодам ошибки: `std/src` — 0 (кроме
   `Fail[E]`, который экземпт и не диагностируется вовсе);
   `spec_tests/conformance` — 2 (ровно мои `reg570_...`/`reg614_...`);
   `examples` — 0. **Итог: 0 посторонних объявлений** нигде в
   `std/spec_tests/examples`. Декларативный `Grep` по `type \w+\[...\]
   effect` подтверждает то же самое отдельно. Найдено, но ВНЕ периметра
   брифа: `bench/corpus/04_effects_handlers.nv:13 type Cache[K, V]
   effect` — это `bench/`, не входит в перечень
   `std/spec_tests/examples`; НЕ правил, назван в отчёте владельцу.
3. **Коммиты (6, в порядке):** `7531fefae` чекпойнт → `a6371c555` спека →
   `99b9d13c6` компилятор → `4ab68ba72` std → `515ce6f31` доки/планы →
   `dc57cc349` фикстуры. Перед каждым — греп конфликт-маркеров в
   staged-диффе в ОДНОЙ команде с `git commit`, все дали `0`.
4. **Отчёт владельцу** — следующий (последний) шаг, по формату брифа
   (§ФОРМАТ ОТЧЁТА), включая находки `Fail[E]`-исключения и
   `bench/corpus` как вещи, решённые на месте (реализационные, не
   языковые), а не поднятые владельцу заранее.

## Файлы, тронутые в этом окне (git diff --stat на момент обрыва)

    compiler-codegen/src/types/mod.rs            | 141 ++++++++++++++
    docs/plans/177-fallible-result-everywhere.md |   9 +-
    docs/plans/wip/197-audit-progress.md         |  10 ++
    spec/decisions/04-effects.md                 |  51 +++++---
    spec/effects.ru.md                           |  20 ++--
    std/src/data/sql.nv                          (модифицирован, не входил
                                                    в diff --stat снимок —
                                                    восстановлен ПОСЛЕ)
    + 3 новых файла (?? в git status):
      spec_tests/conformance/neg/reg570_effect_op_generic_neg.nv
      spec_tests/conformance/neg/reg614_effect_generic_neg.nv
      spec_tests/conformance/pos_effect_nongeneric_handler_value.nv
