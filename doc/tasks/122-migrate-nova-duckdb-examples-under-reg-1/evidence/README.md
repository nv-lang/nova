# Evidence задачи 122 — команды, окружение, версии

Все логи сняты дословно (stdout+stderr, код возврата в последней строке `rc=N`)
2026-10-26, круг доработки 1. ОС: Windows 11, toolchain clang, mode=Dev.

## Окружение (общее)

- Основной репозиторий (компилятор/std): worktree
  `D:\Sources\nv-lang\worktrees\nova-opencode-122-migrate-nova-duckdb-examples-under-reg-1`,
  ветка `task/122-migrate-nova-duckdb-examples-under-reg-1`, HEAD на момент снятия
  логов — `3860414d6` (main 3b13fd3c5 + коммиты задачи).
- Oracle-бинарь: `<worktree>\nova-cli\target\release\nova.exe`, собран
  2026-10-26 01:53 из исходников этого worktree (`cargo build --release` в nova-cli).
- Satellite: `D:\Sources\nv-lang\nova-duckdb`, ветка
  `task/122-migrate-under-reg-1637`, HEAD `a59a07328030ed736041c50031bd0eff9e45d594`
  (коммиты 0f82981, a59a073), дерево чистое (`git status` — только untracked
  `.vscode/` и автогенерированные `examples/*/nova.lock.toml`, см. ниже).
- Переменные Oracle-прогонов (скрипт `run.sh`/`check-example.sh` в скрэтче
  `D:\Sources\.opencode-data\opencode\scratchpad\ses_mvoer3w487ts9tapsz\122\`):
  `NOVA_STD_PATH=<worktree>/std/src`, `NOVA_RT_DIR=<worktree>/compiler-codegen/nova_rt`,
  `NOVA_CG_INCLUDE=<worktree>/compiler-codegen`,
  `NOVA_GC_LIB_DIR`/`NOVA_GC_INCLUDE_DIR`=`D:/Sources/nv-lang/nova/compiler-codegen/vcpkg_installed/x64-windows-static/{lib,include}`,
  `TMPDIR=D:/Sources/nv-lang/.nova-tmp`.
- Резолв зависимости примеров: `examples/*/nova.override.toml` (D420, не
  коммитится): `[replace] duckdb = { path = "../.." }` — то есть ЛОКАЛЬНЫЙ
  мигрированный satellite, не git; в 03-with-tls дополнительно
  `tls = { path = "../../../nova-tls" }` (файл отсутствовал на момент начала
  круга доработки — восстановлен, проверка 03 снова зелёная).

## Команды и файлы

| Файл | Команда | cwd |
|---|---|---|
| `oracle-check-src-before.txt` | `run.sh check src` ДО фикса (sqltag.nv откачен к 6 веткам) | `D:\Sources\nv-lang\nova-duckdb` |
| `oracle-check-src-d491-narrowing.txt` | промежуточный замер: 14 веток без `as i64` → E_IMPLICIT_NARROWING | то же |
| `oracle-check-src.txt` | `run.sh check src` после фикса | то же |
| `oracle-test-src.txt` | `run.sh test src` после фикса | то же |
| `oracle-check-example-01-hello.txt` | `check-example.sh 01-hello` → `nova check .` | `examples\01-hello` |
| `oracle-check-example-02-appender-history.txt` | `check-example.sh 02-appender-history` | `examples\02-appender-history` |
| `oracle-check-example-03-with-tls.txt` | `check-example.sh 03-with-tls` | `examples\03-with-tls` |
| `sabotage-arm-red.txt` | убрана ветка `U32` из match → `run.sh check src` красный (E_MATCH_NON_EXHAUSTIVE «не покрывает вариант: U32») | пакет |
| `sabotage-value-red.txt` | `U32` → `Value.UInt((n as u64) + 1)` → `run.sh test src --filter sqltag` красный (`assert failed: n == want`, sqltag_test.nv:32) | пакет |
| `sabotage-value-green.txt` | то же после восстановления `Value.UInt(n as u64)` — зелёный; зелёный саботажа ветки = `oracle-check-src.txt`/`oracle-test-src.txt` (те же финальные байты, diff пуст) | пакет |
| `resolution-proof-oracle.txt` | в `src/types.nv` добавлена строка-мусор → `check-example.sh 01-hello` красный с путём `.\../..\src\types.nv` — пример резолвит duckdb в ЛОКАЛЬНЫЕ мигрированные исходники | пример 01 |
| `resolution-proof-carina.txt` | тот же сломанный types.nv под Кариной: вывод не меняется — сид pinned B1 не читает источники зависимости ВООБЩЕ (ни локальные, ни git), это и есть блокер | пример 01 |
| `resolution-proof-restored.txt` | types.nv восстановлен (diff пуст), пример 01 снова зелёный | пример 01 |
| `carina-build-01-hello.txt` | `carina-example.sh 01-hello hello_duckdb.nv` → см. ниже | скрэтч |
| `carina-build-02-appender-history.txt` | `carina-example.sh 02-appender-history appender_history.nv` | скрэтч |
| `carina-build-03-with-tls.txt` | `carina-example.sh 03-with-tls with_tls.nv` | скрэтч |
| `1928-enum-payload-array-literal.txt` | пробник bytesprobe: источник + дословный вывод (троичный контраст позиций) | скрэтч `122\bytesprobe` |

## Команда Carina-сборки (скрипт `carina-example.sh`)

Упакованный install собран `scripts/package-release.ps1` из этого worktree и
распакован в скрэтч: `…\122\install\nova-0.3.0-task122-windows-x86_64\`
(дальше `$ROOT`). Окружение: `NOVA_STD_PATH=$ROOT/std`, `NOVA_CG_INCLUDE=$ROOT`,
`NOVA_RT_DIR=$ROOT/nova_rt`, `NOVA_GC_LIB_DIR=$ROOT/gc/lib`,
`NOVA_GC_INCLUDE_DIR=$ROOT/gc/include`. Команда:

```
$ROOT/nova.exe build D:\Sources\nv-lang\nova-duckdb\examples\<dir>\<entry.nv> \
  -o <скрэтч>\<dir>.exe --mode dev --toolchain clang --timeout 300
```

Первая строка каждого лога — вердикт сид-гейта:
`carina-gate verify-seed PASS: carina-b windows-x86_64
sha256=7e8264fd7b72d4005ca9bde585a6a732d3e7d6c248b65fd5787cd4a766e69cdb
source=3729abcce42ec2d94efc898ec6eafe91ff6210d5` (собрано опубликованным
Oracle 0.1.0). Все три сборки: rc=1, `Carina B1 emit failed` — E_NOVAC_SUBSET,
включая `undeclared type name QueryResult/Connection/DbOptions … in the
declarations handed to novac`: сид не получает модули зависимостей вовсе
(program.rs: «external packages … the pinned B1 program walk does not link») —
внешний блокер, не дефект миграции.

## Про nova.lock.toml в examples/

Каждый прогон (и Oracle, и драйвер) пересоздаёт `examples/*/nova.lock.toml`,
прибитый к `git … version = "0.1.1" commit = "ba7b8a1…"` — ВОПРЕКИ
`[replace]` в `nova.override.toml` (наблюдение интегратору: lockfile::sync
игнорирует override). Файлы untracked, на прогоны не влияют: Oracle-резолв идёт
по override (доказано `resolution-proof-oracle.txt`), Карина источники
зависимости не читает вовсе (доказано `resolution-proof-carina.txt`).
