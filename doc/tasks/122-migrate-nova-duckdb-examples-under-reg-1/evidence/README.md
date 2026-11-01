# Evidence задачи 122 — команды, окружение, версии

Сырые логи лежат ДОСЛОВНО (без потери байтов) в `docs/plans/repro/122/` —
исключение стража №698 для улик (путь к машине там — часть улики). Здесь —
ссылки, команды и окружение с заполнителями. Перенос доказан хешами:
`hashes-before.txt` == `hashes-after.txt` (sha256, 17 файлов, круг доработки 2).

Заполнители:

- `ROOT` — корень worktree основного репозитория (ветка
  `task/122-migrate-nova-duckdb-examples-under-reg-1`, HEAD на момент снятия
  логов — `3860414d6`, main 3b13fd3c5 + коммиты задачи);
- `SAT` — рабочая копия satellite-репозитория nova-duckdb (ветка
  `task/122-migrate-under-reg-1637`, HEAD `a59a07328030ed736041c50031bd0eff9e45d594`,
  коммиты 0f82981, a59a073; дерево чистое — untracked только `.vscode/` и
  автогенерированные `examples/*/nova.lock.toml`, см. ниже);
- `SCRATCH` — скрэтчпад сессии (скрипты `run.sh`, `check-example.sh`,
  `carina-example.sh`, `sabotage-run.sh`, `resolution-probe.sh` — там);
- `REPRO` — `docs/plans/repro/122` (внутри репозитория).

Снято 2026-10-26 (круг доработки 1; перенос — круг 2). ОС: Windows 11,
toolchain clang, mode=Dev.

## Окружение (общее)

- Oracle-бинарь: `ROOT/nova-cli/target/release/nova.exe`, собран 2026-10-26
  01:53 из исходников worktree (`cargo build --release` в `nova-cli/`).
- Переменные Oracle-прогонов (скрипты в `SCRATCH`):
  `NOVA_STD_PATH=ROOT/std/src`, `NOVA_RT_DIR=ROOT/compiler-codegen/nova_rt`,
  `NOVA_CG_INCLUDE=ROOT/compiler-codegen`,
  `NOVA_GC_LIB_DIR`/`NOVA_GC_INCLUDE_DIR` — vcpkg-installed главной копии
  (`x64-windows-static/{lib,include}`), `TMPDIR` — каталог `.nova-tmp` рядом с
  репозиториями.
- Резолв зависимости примеров: `SAT/examples/*/nova.override.toml` (D420, не
  коммитится): `[replace] duckdb = { path = "../.." }` — ЛОКАЛЬНЫЙ мигрированный
  satellite, не git; в 03-with-tls дополнительно `tls = { path = "../../../nova-tls" }`
  (файл отсутствовал на момент начала круга 1 — восстановлен, проверка 03
  снова зелёная).

## Команды и файлы (все логи — `REPRO/<имя>`)

| Файл в `REPRO` | Команда | cwd |
|---|---|---|
| `oracle-check-src-before.txt` | `run.sh check src` ДО фикса (sqltag.nv откачен к 6 веткам) | `SAT` |
| `oracle-check-src-d491-narrowing.txt` | промежуточный замер: 14 веток без `as i64` → E_IMPLICIT_NARROWING | `SAT` |
| `oracle-check-src.txt` | `run.sh check src` после фикса | `SAT` |
| `oracle-test-src.txt` | `run.sh test src` после фикса | `SAT` |
| `oracle-check-example-01-hello.txt` | `check-example.sh 01-hello` → `nova check .` | `SAT/examples/01-hello` |
| `oracle-check-example-02-appender-history.txt` | `check-example.sh 02-appender-history` | `SAT/examples/02-appender-history` |
| `oracle-check-example-03-with-tls.txt` | `check-example.sh 03-with-tls` | `SAT/examples/03-with-tls` |
| `sabotage-arm-red.txt` | убрана ветка `U32` из match → `run.sh check src` красный (E_MATCH_NON_EXHAUSTIVE «не покрывает вариант: U32») | `SAT` |
| `sabotage-value-red.txt` | `U32` → `Value.UInt((n as u64) + 1)` → `run.sh test src --filter sqltag` красный (`assert failed: n == want`, sqltag_test.nv:32) | `SAT` |
| `sabotage-value-green.txt` | то же после восстановления `Value.UInt(n as u64)` — зелёный; зелёный саботажа ветки = `oracle-check-src.txt`/`oracle-test-src.txt` (те же финальные байты, diff пуст) | `SAT` |
| `resolution-proof-oracle.txt` | в `SAT/src/types.nv` добавлена строка-мусор → `check-example.sh 01-hello` красный с ОТНОСИТЕЛЬНЫМ путём `.\../..\src\types.nv` — пример резолвит duckdb в локальные мигрированные исходники | пример 01 |
| `resolution-proof-carina.txt` | тот же сломанный types.nv под Кариной: вывод БАЙТ-В-БАЙТ тот же, что `carina-build-01-hello.txt` (один sha256 — см. hashes-*.txt): сид pinned B1 не читает источники зависимости ВООБЩЕ (ни локальные, ни git) — это и есть блокер | пример 01 |
| `resolution-proof-restored.txt` | types.nv восстановлен (diff пуст), пример 01 снова зелёный | пример 01 |
| `carina-build-01-hello.txt` | `carina-example.sh 01-hello hello_duckdb.nv` → см. ниже | `SCRATCH` |
| `carina-build-02-appender-history.txt` | `carina-example.sh 02-appender-history appender_history.nv` | `SCRATCH` |
| `carina-build-03-with-tls.txt` | `carina-example.sh 03-with-tls with_tls.nv` | `SCRATCH` |
| `1928-enum-payload-array-literal.txt` | пробник bytesprobe: источник + дословный вывод (троичный контраст позиций) | `SCRATCH/bytesprobe` |

## Команда Carina-сборки (скрипт `carina-example.sh` в `SCRATCH`)

Упакованный install собран `scripts/package-release.ps1` из worktree и
распакован в `SCRATCH/install/nova-0.3.0-task122-windows-x86_64/` (дальше
`$PKG`). Окружение: `NOVA_STD_PATH=$PKG/std`, `NOVA_CG_INCLUDE=$PKG`,
`NOVA_RT_DIR=$PKG/nova_rt`, `NOVA_GC_LIB_DIR=$PKG/gc/lib`,
`NOVA_GC_INCLUDE_DIR=$PKG/gc/include`. Команда:

```
$PKG/nova.exe build SAT/examples/<dir>/<entry.nv> \
  -o SCRATCH/<dir>.exe --mode dev --toolchain clang --timeout 300
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

Каждый прогон (и Oracle, и драйвер) пересоздаёт `SAT/examples/*/nova.lock.toml`,
прибитый к `git … version = "0.1.1" commit = "ba7b8a1…"` — ВОПРЕКИ
`[replace]` в `nova.override.toml` (наблюдение интегратору: lockfile::sync
игнорирует override). Файлы untracked, на прогоны не влияют: Oracle-резолв идёт
по override (доказано `REPRO/resolution-proof-oracle.txt`), Карина источники
зависимости не читает вовсе (доказано `REPRO/resolution-proof-carina.txt`).
