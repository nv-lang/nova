# Отчёт по задаче 118 «first test run of duckdb postgres sdl»

Дата: 2026-10-25. Исполнитель: nova.worker (ses_mvnwgvq5vj9fx42v96).
Оракул: `nova.exe` (release), собран из дерева `3445f8bb3` = main `992b9c24040c32cfedbba1387bbd49de6d8dceed` + коммит с доками задачи (компилятор не тронут). Сборка: `cd nova-cli && cargo build --release` — rc=0, 5m 33s, лог `build-oracle.log`.
Окружение прогонов (как в `scripts/gate.sh`, шаг «пакетные репозитории»): `NOVA_STD_PATH=<wt>/std/src`, `NOVA_RT_DIR=<wt>/compiler-codegen/nova_rt`, `NOVA_CG_INCLUDE=<wt>/compiler-codegen`, `NOVA_GC_LIB_DIR`/`NOVA_GC_INCLUDE_DIR` — из главной копии (`D:/Sources/nv-lang/nova/compiler-codegen/vcpkg_installed/x64-windows-static/...`), `TMPDIR=D:/Sources/nv-lang/.nova-tmp`. У всех трёх реп есть `src/` и `[lib] src = "src"` — команда везде `nova test src`.

## Таблица результатов

| репа | ветка | SHA до | SHA после | команда | вердикт | время |
|---|---|---|---|---|---|---|
| nova-duckdb | main | 674b12aac4d2bbdb817a9116ed3267d4d60c78fb | 674b12aac4d2bbdb817a9116ed3267d4d60c78fb | `nova test src` | **FAIL** (`PASS: 4  FAIL: 1  SKIP: 6`) | 43 с |
| nova-postgres | main | 8a46520269cf430e3f9d7a78ad9ae729ed23af8a | 8a46520269cf430e3f9d7a78ad9ae729ed23af8a | `nova test src` | **PASS** (`PASS: 1  FAIL: 0  SKIP: 1`) | 26 с |
| nova-sdl | master | e636ac9286dfdc303c3b56c386241fda9d25dbd9 | e636ac9286dfdc303c3b56c386241fda9d25dbd9 | `nova test src` | **PASS** (`PASS: 0  FAIL: 0  SKIP: 4` — тестовых блоков нет, все 4 модуля скомпилированы OK) | 28 с |

Чистота рабочих копий: до и после совпадает. nova-duckdb: единственное отличие от HEAD — untracked `.vscode/` (было до прогона, не моё). nova-postgres, nova-sdl: clean до и после. SHA/ветки не изменились; код не правился, коммитов и пушей в пакетных репах нет.

Время включает разовые постройки внутри прогона: у nova-sdl — libuv (~30 с), у всех — архив `libnova_rt` под свой конфиг.

## FAIL nova-duckdb — первые строки ошибок (дословно)

```
CODEGEN-FAIL   src/sqltag  # D:\Sources\nv-lang\nova-duckdb\src\sqltag.nv:40:5: error: [E_MATCH_NON_EXHAUSTIVE] `match` по сумме `SqlValue` не покрывает варианты: `Int`, `I8`, `I16`, `I32`, `I64`, `U8`, `U16`, `U32`, `F32`, `F64`, `Text`, `Bool`. Непокрытый вариант дал бы ТИХО неверный результат во время исполнения (D65-амендмент 2026-08-16, реестр №703). Добавь недостающие ветки — либо `_ => …`, если остаток действительно безразличен.
40 |     match v {
  |     ^^^^^^^^^
```

Разбор (факты, без починки): `src/sqltag.nv` написан под старые шесть вариантов `SqlValue` (`I/F/S/B/Bytes/Null` — комментарий файла дословно: «`SqlValue` is deliberately small — six shapes»). Реестр №1637 (решение владельца 2026-10-02, влито `7c7bd21af`) дал `SqlValue` вариант на каждый числовой тип — 14 вариантов; пакет в ту волну не вошёл. Отказ громкий — энфорсер исчерпаемости D65-амендента (реестр №703).

Дефект заведён в `docs/plans/221.1-bug-sweep.md` строкой `№TBD` (номер присваивает интегратор; `check-registry-entry-shape` на TBD красный по дизайну — это и есть механизм выдачи номера перед гейтом). `check-registry-single-verdict` и `check-registry-routes` на дереве с записью — ok.

## Логи (scratch сессии)

`D:\Sources\.opencode-data\opencode\scratchpad\ses_mvnwgvq5vj9fx42v96\118\`

- `build-oracle.log` — сборка оракула;
- `test-nova-duckdb.log` — FAIL, полный вывод;
- `test-nova-postgres.log` — PASS;
- `test-nova-sdl.log` — PASS;
- `run-pkg.sh` — скрипт прогона с окружением;
- `repos-before.txt`, `repos-after.txt` — SHA/ветка/статус до и после.

## По пунктам приёмки проекта

- **criteria** — названы дословно в задании и выполнены: SHA+ветка до/после, команда, вердикт, первые строки ошибок, таблица с временем, логи.
- **fixture** — не применимо: правки кода нет, задача — измерение; утверждаемое поведение — сами строки `PASS:/FAIL:` в логах выше.
- **sabotage** — не применимо: фикса нет, ломать/возвращать нечего. Обратная проба была по среде: первый заход без сабмодуля libuv дал `FATAL libuv submodule not initialized` за 4 с (красный), после `git submodule update --init` — настоящие прогоны (зелёный канал) — то есть отказ по среде отличён от вердикта по коду.
- **class** — класс назван: «пакет не мигрирован на `SqlValue` вариант-на-тип (№1637)»; в той же сессии проверены остальные две репы из задания — не затронуты; сверка остальных пакетов 274.16 записана в оговорку о классе в реестре.
- **registry** — запись `№TBD` в `docs/plans/221.1-bug-sweep.md` в этом же коммите; тесты не ослаблены и не удалены; новых E_/W_ нет.
- **spec** — не применимо: язык не меняется.
- **std-check** — не применимо: правки компилятора и `std/` нет (только строка реестра и доки задачи); `nova check std/src` не гонялся — менять его результату нечем.
- **invariant** — не применимо: новых инвариантов и стражей не заведено.
- **gate** — не применимо к моей части: локально `gate.sh` не гонялся (запрещено брифом), вердикт — CI на кандидате при приёмке.
- **ci** — кандидат собирает и пушит приёмщик; моя ветка `task/118-first-test-run-of-duckdb-postgres-sdl` в worktree, пуш не делался (границы задачи).
- **commits** — один коммит на английском с `Signed-off-by`, `git add` по именам, `--only -- <файлы>`.
- **report** — этот файл; строка результата ниже.

## Строка результата (window-brief-template §6)

```
РЕПРО: подтверждён — nova-duckdb FAIL воспроизводится оракулом main 992b9c240 (E_MATCH_NON_EXHAUSTIVE, src/sqltag.nv:40); nova-postgres и nova-sdl — PASS
ТОЧКА: nova-duckdb/src/sqltag.nv:40 — match по SqlValue покрывает 6 старых вариантов из 14 (№1637)
ФИКС: нет (границы задачи: ничего не чинить); заведена запись №TBD в docs/plans/221.1-bug-sweep.md
ФИКСТУРЫ: нет новых; улика — `nova test src` в nova-duckdb (лог test-nova-duckdb.log)
САБОТАЖ: среда без libuv — «FATAL libuv submodule not initialized» (красный) / после init — настоящий прогон (вердикты выше)
std/src: не применимо (правки компилятора/std нет)
ВЕТКА/КОММИТ: task/118-first-test-run-of-duckdb-postgres-sdl <хэш коммита этого отчёта>
ЧТО НЕ СДЕЛАНО: починка nova-duckdb (вне границ), сверка остальных шести пакетов 274.16 (вне задания)
```
