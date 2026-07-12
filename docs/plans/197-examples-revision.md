<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 197 — examples/ ревизия: снести устаревшее, пересобрать канон, дом для 187

**Статус:** 🚧 Ф.1/Ф.2 ГОТОВЫ (2026-07-12) — аудит всех 29 файлов + чистка мёртвой
поверхности выполнены, см. [197-audit-progress.md](197-audit-progress.md). Ф.3
(канонический showcase-набор), Ф.4 (дом 187) исполнение, Ф.5 (CI-гейт) — не начаты.
**Приоритет:** P2 (user-facing витрина).
**Связано:** [187](187-flagship-concurrency-demo.md) (флагман — куда его селить),
[193](193-nova-tls-repo.md)/[195](195-native-modules-c-not-rust.md) (паттерн отдельной
репы для native-модуля), [198](198-nova-tests-triage.md) (та же семья «legacy-корпус
под ревизию»).

## Проблема

`examples/` — **user-facing** (доки, лендинг, «за 5 секунд»), поэтому stale-пример
ХУЖЕ stale-теста: юзер его видит и копирует. Аудит 2026-07-11: **28 `.nv`-файлов**
(уточнено 2026-07-12: **29**), из них ≥4 несли **мёртвую поверхность** (`with Detach`
/ `effect Detach`-хендлеры — ретрактированные имена, dead handler surface).

Нет **CI-гейта компиляции examples** → они молча гниют (в отличие от std/conformance).

**Обновление 2026-07-12 (Ф.1 доследование + Ф.2 исполнено):** после мержа main
(196.2/196.3 волна) два toolchain-бага, блокировавшие аудит 2026-07-11 (ICE на
любом hello-world + `Result.map`-инференс), оказались уже исправлены апстримом —
полный переаудит стал возможен. Итог: 4 файла удалены (`real_world/audit.nv`,
`real_world/oxsar_port.nv` — явно non-compiling reading-only content;
`effects/gc_coroutines_test.nv`, `effects/with_tests.nv` — не-user-facing/не по
месту); 6 файлов перемещены в `examples/_wip/` (effect_density-семья, 5 файлов;
`typed_pointers/unsafe_block.nv`) до переписи начисто; `real_world/orm_decorators.nv`
и `real_world/orm_demo.nv` получили полный проход dead-syntax→канон (`with Detach`→
`SyncDetach`, `use std.X`→`import std.data.X.{...}`, bare `assert`→`assert(...)`,
sql-tag `SqlValue`-обёртки, multi-line-signature/leading-operator фиксы) — оба
теперь содержательно чисты, но заблокированы ДВУМЯ НОВЫМИ подтверждёнными
compiler-багами вне скоупа этого плана (`.map()` generic-inference ICE, `with`
внутри handler-method body не парсится — оба с synthetic repro, см.
[197-audit-progress.md](197-audit-progress.md)). Мёртвая поверхность в оставшемся
дереве (вне `_wip/`) = 0; 16 из 19 файлов реально компилируются сегодняшним
`nova.exe`, 3 — блокированы известными compiler-issue.

## Фазы

- **Ф.1 — аудит per-file (read-only):** ✅ ГОТОВО (2026-07-12, полный переаудит
  после фикса toolchain-багов апстримом). Таблица — [197-audit-progress.md](
  197-audit-progress.md). Doc-ссылки проверены (grep `examples/` в docs/ +
  `www`) — удалённые/перемещённые файлы упоминались только в исторических
  plan-докax/spec-history, не на лендинге/гайдах.
- **Ф.2 — исполнить триаж:** ✅ ГОТОВО (2026-07-12). FIX-CHEAP починены на канон
  (включая полный проход `orm_decorators.nv`/`orm_demo.nv`); DELETE-STALE удалены
  (4 файла); RECREATE → `examples/_wip/` (6 файлов, см. `_wip/README.md`; **решён
  открытый вопрос #2 ниже — `_wip/`, не снос**). Мёртвая поверхность = 0.
- **Ф.3 — канонический набор витрины:** определить эталонный showcase-набор,
  выровненный с текущим языком: `basics/` (hello/records/match/strings) · `effects/`
  (актуальная effect-модель, БЕЗ ретрактов) · `concurrency/` (spawn/supervised/
  parallel-for/cancel) · `ffi/` (native-паттерн 195) · `real_world/` (1-2 честных
  сквозных). Каждый — минимальный, компилящийся, прокомментированный.
- **Ф.4 — дом для 187 (флагман):** ✅ РЕШЕНО (владелец 2026-07-11): моно-репа.
  **`flagship/` = КАТЕГОРИЯ-тир, каждый демо — именованная подпапка** (масштаб на N
  флагманов, а не одиночный `flagship/`): бек 187 → **`examples/flagship/aggregator/`**
  (Nova-код в моно-репе — гоняет настоящий std/http + concurrency, гейтится вместе с
  языком, не гниёт). Категория — `flagship/` (тир) или `showcase/` (нейтральнее);
  демо-имя `aggregator`/`fanout`. **Фронт/лендинг → репа `www`**
  (`nv-lang.org`, уже сиблинг-worktree `d:/Sources/nv-lang/www`; фронт не на Nova —
  §159 «язык не про UI»); живое демо хостится с лендинга. Альтернатива — отдельная
  showcase-репа (как nova-tls/193), если хотим «скачал одну репу и запустил» без
  моно-репы; но флагман тесно завязан на неустоявшийся std → in-repo надёжнее до
  стабилизации. Sign-off владельца в §Открытые.
- **Ф.5 — CI-гейт компиляции:** новый job `examples-compile` — `nova build` по всем
  `examples/**` (кроме `_wip/`) должен быть зелёным. Anti-rot: пример, переставший
  компилиться, валит PR. (Не гейт корректности рантайма — только компиляция + для
  помеченных `// EXAMPLE_RUN` быстрый прогон.)

## Гейты

Каждый оставленный/новый пример компилится через C-codegen (`nova build`); ноль
мёртвой поверхности (grep `with Detach`/retracted/`str.len` = 0 в examples вне `_wip`);
doc-ссылки на examples целы; `examples-compile` CI-job зелёный; conformance δ0.

## Открытые решения (sign-off до старта)

1. ✅ РЕШЕНО (владелец 2026-07-11): **Дом 187 = `examples/flagship/`** (бек в моно-репе)
   + фронт/лендинг в репе `www`. (См. Ф.4.)
2. ✅ РЕШЕНО (исполнено 2026-07-12): **`_wip/`** — RECREATE-примеры (6 файлов)
   перемещены в `examples/_wip/` с README, не снесены. Обоснование: концепт
   ценен (effect_density — измерение плотности эффектов; unsafe_block —
   демо unsafe-блоков), переписать дешевле с сохранённым контекстом, чем с
   нуля; `_wip/` не участвует в Ф.5 CI-гейте.
3. **Глубина Ф.5-гейта** — только компиляция, или для части ещё `// EXAMPLE_RUN`
   быстрый рантайм-прогон в CI?

## Границы

Не про язык-фичи (только демо существующих). Фронт 187 — в `www`, не здесь. Native-
модуль-паттерн примеров — из [195](195-native-modules-c-not-rust.md).
