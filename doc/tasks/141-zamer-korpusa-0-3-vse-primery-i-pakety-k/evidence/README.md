# Улики замера 141

Контур: **patched-scratch-driver-141** — копия nova-driver @ `0b01c838b` с
пинами на локальный сид из Карины HEAD (`scratch-driver-141.patch`; стейджинг
embed-sidecar уже штатный в main, задача 136 — заплатка 135 за него не
понадобилась). Сборки: Windows, clang, Boehm/libuv из vendored подмодулей.
Сырые байты логов — в scratch сессии, здесь всё нормализовано
(`$WT` = worktree 141, `$S` = scratch сессии, `$INSTALL` = $S/install141,
`$REPOS` = каталог спутников, `$TMP`/`$NOVA_HOME` — машинные temp/кеши).

| Файл | Что |
|---|---|
| `table-118.tsv` | 118 примеров: вердикт, build rc/сек, run rc, sha256 exe (16 hex), первая строка stdout, класс отказа |
| `logs-118/` | нормализованные build+run логи каждого входа (L001–L118) |
| `fail-diagnostics-118.tsv` | первая диагностика каждого из 55 FAIL + сверка с логом 135 того же входа (колонка same_as_135: SAME ×55) |
| `packages.tsv` | 19 runnable входов пакетов: repo, head, tag, вердикт, класс |
| `logs-packages/` | нормализованные логи 19 пакетных входов |
| `manifests.md` | sha256 всех артефактов контура + дескриптор сида (stage oracle-a) |
| `oracle-check-string-literals.md` | сверка единственного PASS-NEW с оракулом: найдено CRLF-расхождение (реестр 221.1, строка №TBD) |
| `scratch-driver-141.patch` | diff scratch-драйвера против nova-driver @ 0b01c838b (нормализованный b/-заголовок) |
| `satellite-lock-status.md` | состояние 9 спутников после замера: совпало с записью 135 |

Порядок входов table-118.tsv == порядок table-118.tsv 135 (та же нумерация
L001–L118), что даёт построчную сверку по имени и по номеру.
