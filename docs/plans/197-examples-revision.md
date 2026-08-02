<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 197 — examples/ ревизия: снести устаревшее, пересобрать канон, дом для 187

**Статус:** 🚧 Ф.1/Ф.2/Ф.3 ГОТОВЫ (Ф.3 — 2026-07-21, worktree `nova-ex197`/
branch `p197-final`): витринная карта [`examples/README.md`](
../../examples/README.md) (8 категорий, нарастающая сложность,
build-only/run пометки, Known gaps), полный переаудит без регрессий
(включая новые `tour/**`×13+`mini_aggregator.nv`, появившиеся между 07-17
и сегодня — все зелёные). Найдена НОВАЯ регрессия: `net/echo_client.nv`/
`echo_server.nv` (undefined symbol `Nova_TcpStream_consume_cleanup` —
`consume TcpStream` внутри `spawn{}`, codegen-пробел, воспроизведено в
главной репе тем же релизным `nova.exe`) — эти два файла ВХОДЯТ в текущий
5-целевой флагман-гейт `nova-gate.yml`, из чего следует, что гейт
**вероятно красный прямо сейчас** (см. [197-audit-progress.md](
wip/197-audit-progress.md) §«Заход 2026-07-21» — владельцу стоит
проверить последний CI-прогон/`workflow_dispatch`). Классифицирован как
KEEP-blocked-by-toolchain (не перенесён в `_wip/` — тот же прецедент, что
`orm_decorators.nv`/`orm_demo.nv`: готовый контент, не concept-to-rewrite).
Ф.5-подготовка: точный список путей для расширения CI-гейта на ВЕСЬ
`examples/**` — [197-f5-gate-list.txt](wip/197-f5-gate-list.txt)
(BUILD/CHECK по каждому файлу + предложенный текст шага, `nova-gate.yml`
НЕ менялся). Ранее (2026-07-12), дожаты 2026-07-17 (рестарт
погибшей волны, worktree `nova-197r`/branch `p197-examples-rest`) — аудит
всех 29 файлов + чистка мёртвой поверхности + переверификация 19
не-`_wip`-файлов сегодняшним `nova.exe` (ноль регрессий от промежуточных
lang-волн), см. [197-audit-progress.md](wip/197-audit-progress.md) §«Заход
2026-07-17». Итог 07-17: **18 из 19** файлов реально компилируются
(`--strict-effects`) — было 16/19; один из трёх известных toolchain-багов
(extern-FFI tuple-return codegen, `sqlite_mini.nv`) подтверждённо
пофикшен апстримом, снят с очереди; попутно найден+исправлен независимый
реальный баг содержимого в `orm_decorators.nv` (`.filter()` без явного
`import std.collections.vec_seq.{filter}` резолвился в чужой тип);
оставшиеся 2 файла (`orm_decorators.nv`, `orm_demo.nv`) блокированы
ДРУГИМИ (переформулированными, точнее локализованными) toolchain/std-багами
вне объёма этой волны — `SyncDetach` отсутствует в std bootstrap
(подтверждено комментарием в `spec_tests/conformance/
detach_effect_ok_test.nv`) и `Repo[T].bulk_load[K]`+`Vec[K].map()`
C-type-inference (`E7001`, класс `M-196.5-b3-closure-param-bind`). Ф.3
(канонический showcase-набор — явный reshape папок) — сознательно НЕ
начата: директива владельца на этот заход была узко «почини или удали
ненужные старые примеры по вердиктам аудита Ф.1», не редизайн структуры;
текущее дерево (`basics/effects/ffi/real_world/typed_pointers/net/tls/
plan110`) уже фактически покрывает намеченные Ф.3 категории. Ф.4 (дом
187) — дизайн решён И ИСПОЛНЕН фактически (`examples/flagship/
aggregator/` — реализованный проект с src/frontend/тестами, Plan 187).
**Ф.5 (CI-гейт) — ГОТОВ (2026-07-16, ветка
`ci-gate-workflow`):** `.github/workflows/nova-gate.yml` — расширенный CI-гейт
(директива владельца 2026-07-16, «авторитетный гейт переезжает с локальной
машины на CI»), не только `examples-compile` из исходного Ф.5-описания, а ОБЕ
половины авторитетного merge-гейта (test-conventions.md §«Авторитетный
merge-гейт», прецедент Plan 206): (1) `spec_tests/conformance` (`--positive
--compile-error --timeout 300 --jobs 4`, один CU) + (2) флагман-examples-build
под `--strict-effects` (`examples/flagship/aggregator` + `examples/net` и
`examples/tls` echo_server/echo_client пары, 5 целей). Триггеры: `push`
(main) + `pull_request` + `workflow_dispatch`; один job, кэш cargo по
`nova-cli/Cargo.lock`; последние 100 строк упавшего лога — в
`GITHUB_STEP_SUMMARY`. **Первая верификация — на живом пуше** (не прогонялось
локально: задача была yml-only, без сборок). Находка при построении: `examples/
nova.toml`'s `http = { path = "../../nova-http" }` — НЕ git-зависимость (в
отличие от `tls`/`compress`) → на чистом CI-чекауте нужен sibling-checkout
`nova-http` рядом с `nova` (шаг `Checkout nova-http sibling` в воркфлоу),
иначе весь flagship-job падает на резолве манифеста `examples` (пакет один,
резолвится целиком даже для `echo_server.nv`, который `http` не импортирует).
`nova test std` (5 pre-existing Linux-red — `docs/guide/linux-build.md` §«Known
gap») сознательно НЕ в этом гейте (std-специфично, conformance/флагмана не
касается).

### Бейдж для README (черновик — владелец/интегратор переносит в README.md
### при включении гейта)

```md
[![nova-gate](https://github.com/nv-lang/nova/actions/workflows/nova-gate.yml/badge.svg)](https://github.com/nv-lang/nova/actions/workflows/nova-gate.yml)
```

### Тир-гейты (черновик — переносится в `docs/dev/test-conventions.md` при включении)

Авторитетность гейта зависит от того, ЧТО меняет слияние — три тира:

| Тир | Что меняется | Гейт |
|---|---|---|
| **docs-only** | Только `*.md`/doc-комментарии, без `.nv`/Rust | Без гейта (CI не запускается вовсе, либо только markdown-lint при наличии) |
| **`.nv`-only** | Только `.nv`-файлы (std/examples/spec_tests/nova_tests) | Таргетно ЛОКАЛЬНО у исполнителя: `nova test spec_tests/conformance` (+ затронутый `std`-модуль/`examples`-цель под `--strict-effects` при необходимости) — не весь CI-гейт, целевой прогон дешевле полного CI-цикла |
| **Rust (`compiler-codegen`/`nova-cli`)** | Компилятор/раннер/кодоген | **CI-авторитет + постоянный мониторинг**: `nova-gate.yml` ОБЯЗАТЕЛЕН зелёным перед merge (conformance + флагман-examples-build); мониторинг — badge в README + `workflow_dispatch` для ad-hoc перепрогона на подозрении о флаке |

Правило перехода: любое Rust-слияние, меняющее поведение (не чисто-рефакторинг
с byte-identical выходом), обязано дождаться зелёного `nova-gate` (push на
main) ПЕРЕД тем, как считаться влитым-и-проверенным — красный `nova-gate` на
main = стоп-сигнал уровня «красный conformance», немедленно чинить или
откатывать.
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
[197-audit-progress.md](wip/197-audit-progress.md)). Мёртвая поверхность в оставшемся
дереве (вне `_wip/`) = 0; 16 из 19 файлов реально компилируются сегодняшним
`nova.exe`, 3 — блокированы известными compiler-issue.

**Обновление 2026-07-17 (дожатие Ф.3/Ф.4-остатка, см. «Статус» вверху и
[197-audit-progress.md](wip/197-audit-progress.md) §«Заход 2026-07-17»):**
переверификация сегодняшним `nova.exe` — ноль регрессий на 16 ранее
зелёных файлах; extern-FFI tuple-return баг (был #3) подтверждённо
пофикшен, `sqlite_mini.nv` → KEEP; в `orm_decorators.nv` найден+исправлен
независимый content-баг (`.filter()` без импорта `vec_seq.filter`
резолвился в чужой тип через single-key fallback); `with`-в-handler-method
баг (был #2) тоже подтверждённо пофикшен, но это вскрыло ГЛУБЖЕ лежащий
баг — `SyncDetach` не реализован в std bootstrap (7-12 фикс полагался на
неверную предпосылку); `.map()`-ICE (был #1) сменил форму на чистую
диагностику `E7001`, локализован до `Repo[T].bulk_load[K]`+function-value
`.map()`. Итог: **18 из 19** файлов дерева (вне `_wip/`) реально
компилируются; 2 (`orm_decorators.nv`, `orm_demo.nv`) остаются
заблокированы переформулированными toolchain/std-багами, переданы
следующей волне с точной локализацией.

## Фазы

- **Ф.1 — аудит per-file (read-only):** ✅ ГОТОВО (2026-07-12, полный переаудит
  после фикса toolchain-багов апстримом). Таблица — [197-audit-progress.md](
  wip/197-audit-progress.md). Doc-ссылки проверены (grep `examples/` в docs/ +
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
  **2026-07-17: сознательно НЕ начата** — директива владельца на заход
  07-17 узко ограничила объём «почини или удали ненужные старые по
  вердиктам Ф.1» (без reshape папок); текущее дерево фактически уже
  покрывает намеченные категории (`basics/effects/ffi/real_world` есть,
  `effects/spawn_demo.nv` — готовый кандидат в отдельную `concurrency/`,
  перенос не делался). Явный reshape — отдельная директива при желании.
  **✅ ГОТОВО 2026-07-21** (worktree `nova-ex197`/branch `p197-final`):
  явный reshape папок НЕ сделан (директива этого захода — карта, не
  редизайн дерева), но эталонный showcase-набор явно зафиксирован как
  курируемая карта — [`examples/README.md`](../../examples/README.md),
  8 пунктов нарастающей сложности (hello → getting_started →
  mini_aggregator → tour/ → effects/ → net+tls → ffi/typed_pointers →
  flagship/aggregator), с build-only/run-пометками и разделом Known gaps
  (ссылки на битые/заблокированные исключены из витрины). Полный
  переаудит всех файлов вне `_wip/` подтверждён `nova build/check
  --strict-effects` сегодняшним релизным `nova.exe` — см.
  [197-audit-progress.md](wip/197-audit-progress.md) §«Заход 2026-07-21».
- **Ф.4 — дом для 187 (флагман):** ✅ РЕШЕНО (владелец 2026-07-11) И ИСПОЛНЕНО фактически:
  `examples/flagship/aggregator/` существует как реализованный проект (src/frontend/loadtest/regressions) —
  моно-репа. **`flagship/` = КАТЕГОРИЯ-тир, каждый демо — именованная подпапка** (масштаб на N
  флагманов, а не одиночный `flagship/`): бек 187 → **`examples/flagship/aggregator/`**
  (Nova-код в моно-репе — гоняет настоящий std/http + concurrency, гейтится вместе с
  языком, не гниёт). Категория — `flagship/` (тир) или `showcase/` (нейтральнее);
  демо-имя `aggregator`/`fanout`. **Фронт/лендинг → репа `www`**
  (`nv-lang.org`, уже сиблинг-worktree `d:/Sources/nv-lang/www`; фронт не на Nova —
  §159 «язык не про UI»); живое демо хостится с лендинга. Альтернатива — отдельная
  showcase-репа (как nova-tls/193), если хотим «скачал одну репу и запустил» без
  моно-репы; но флагман тесно завязан на неустоявшийся std → in-repo надёжнее до
  стабилизации. Sign-off владельца в §Открытые.
- **Ф.5 — CI-гейт компиляции:** ✅ ГОТОВО (2026-07-16, ветка `ci-gate-workflow`,
  `.github/workflows/nova-gate.yml`) — реализовано ШИРЕ исходного описания: не
  отдельный `examples-compile` по ВСЕМУ `examples/**`, а флагман-таргетная
  часть авторитетного merge-гейта (директива владельца 2026-07-16, см.
  «Статус» вверху) — `nova build --strict-effects` по 5 целям
  (`flagship/aggregator` + `net`/`tls` echo_server/echo_client пары), в одном
  workflow вместе с `spec_tests/conformance`. Остальной `examples/**` (basics/
  effects/ffi/real_world/…) полным `examples-compile`-обходом пока НЕ
  покрыт — если понадобится anti-rot гейт на ВЕСЬ каталог (не только
  флагман-таргеты), это отдельное расширение `nova-gate.yml` (новый шаг,
  `nova build`/`nova check` по всем `.nv` в `examples/` кроме `_wip/`), не
  сделанное этой волной. Не гейт корректности рантайма — только компиляция.
  **Ф.5-подготовка 2026-07-21** (worктree `nova-ex197`/branch `p197-final`):
  точный BUILD/CHECK-список по каждому файлу —
  [`197-f5-gate-list.txt`](wip/197-f5-gate-list.txt) (30 целей: 28 BUILD +
  2 CHECK, исключая 4 KEEP-blocked-by-toolchain и 6 `_wip/`-файлов).
  Предложенный текст нового шага `nova-gate.yml` (НЕ применён — `yml` не
  трогался этим заходом, решение об включении за интегратором):
  ```yaml
        - name: Examples anti-rot — build/check ALL non-flagship examples
            (--strict-effects)
          run: |
            set -e
            targets_build=(
              examples/basics/arithmetic.nv examples/basics/demo.nv
              examples/basics/hello.nv examples/basics/match_demo.nv
              examples/basics/records.nv examples/basics/strings.nv
              examples/effects/effects.nv examples/effects/effects_d61.nv
              examples/effects/spawn_demo.nv examples/ffi/ptr_basics.nv
              examples/getting_started.nv examples/mini_aggregator.nv
              examples/plan110/ffi_sqlite_consumable.nv
              examples/tour/*.nv
              examples/typed_pointers/basic_pointer.nv
              examples/typed_pointers/unsafe_fn_keyword.nv
            )
            targets_check=(
              examples/ffi/sqlite_mini.nv
              examples/tour/greeter/core.nv examples/tour/greeter/loud.nv
            )
            # tls/echo_* уже в существующих 5 флагман-целях — не дублировать.
            # net/echo_*, real_world/orm_* — KEEP-blocked-by-toolchain,
            # намеренно исключены (см. 197-f5-gate-list.txt) до фикса апстрима.
            for t in "${targets_build[@]}"; do
              ./nova-cli/target/release/nova build "$t" --strict-effects
            done
            for t in "${targets_check[@]}"; do
              ./nova-cli/target/release/nova check "$t" --strict-effects
            done
  ```
  Точный список целей (с обоснованием каждого исключения) —
  [`197-f5-gate-list.txt`](wip/197-f5-gate-list.txt).

## Гейты

Каждый оставленный/новый пример компилится через C-codegen (`nova build`); ноль
мёртвой поверхности (grep `with Detach`/retracted/`str.len` = 0 в examples вне `_wip`);
doc-ссылки на examples целы; флагман-таргеты `nova-gate.yml` CI-job зелёные
(2026-07-16 — см. «Статус»; полный `examples/**`-обход вне флагмана — открытый
follow-up, не сделан); conformance δ0.

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
