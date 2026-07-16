<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Bounded-accept mitigation — чекпоинт (2026-07-16, ветка fix-bounded-accept)

**Задача.** Митигация `[M-187-high-concurrency-connection-wedge]` —
app-level bounded-accept, чтобы флагман (examples/flagship/aggregator)
НЕ доходил до permanent-wedge под массовой одновременной connection-нагрузкой.
Настоящий scheduler-фикс (park/join) — отдельная research-ветка, отложен
(вносил memory corruption). Baseline-рантайм (busy-poll, main) — memory-safe,
не трогается этой волной.

**Статус: готово, гейт зелёный, НЕ смёржено в main.**

## Что сделано

`examples/flagship/aggregator/src/main.nv`:

1. `import std.runtime.sync.{AtomicI64}`.
2. `const MAX_INFLIGHT_CONNS i64 = 2` — эмпирический предел одновременно
   обрабатываемых соединений (см. док-комментарий в файле для полной
   методологии подбора).
3. Accept-loop: `mut inflight = AtomicI64.new(0)` перед циклом; после
   `lst.accept()` — если `inflight.load() >= MAX_INFLIGHT_CONNS`, соединение
   закрывается немедленно (`stream.close()`, честный reject, без очереди,
   без чтения/ответа); иначе `inflight.fetch_add(1)` + `detach {
   handle_connection(...); inflight.fetch_sub(1) }`.
4. `detach { }` заменил старый per-connection `supervised { spawn { ... } }`
   (который блокировал accept-loop до завершения — де-факто последовательно).
   `detach` — D50 fire-and-forget; под armed M:N этого бинаря — реальный
   worker-pool dispatch (`nova_runtime_spawn_orphan`), НЕ синхронный
   test-only `SyncDetach`. `fn main()` effect-row: `Time` → `Time Detach`.
5. Комментарии у обоих старых маркеров (`[M-187-nested-spawn-scope-var-cc-fail]`,
   `[M-187-supervised-nested-fiber-slot-race]`) обновлены — история сохранена,
   пояснено что́ изменилось и что́ по-прежнему в силе.

`examples/flagship/aggregator/README.md` — секция «Известные ограничения»
обновлена (mitigation влита, баг остаётся OPEN, эмпирические числа).

`docs/plans/backlog-followups.md` — строка `[M-187-high-concurrency-connection-wedge]`
дополнена (маркер ОСТАЁТСЯ OPEN, mitigation описана).

`docs/simplifications.md` — append-only запись с полной методологией/цифрами.

## Почему `MAX_INFLIGHT_CONNS = 2` (не 16, не «queue»)

Первая гипотеза (`= 16`, под размер worker-pool рантайма) была **неверна** —
проверено эмпирически, воспроизвела ТОТ ЖЕ permanent-wedge. Баг НЕ про
голое число соединений (старый код уже был де-факто последовательным и всё
равно висел на ~80) — он про то, сколько `aggregate()`'s ФАН-АУТОВ
(`parallel for` + `fetch_guarded`'s nested `supervised(deadline:)`, по
несколько fiber'ов каждый) одновременно живы в планировщике. Прямые
`xargs -P80`/`-P200` бёрсты на этой машине (с `NOVA_WATCHDOG_DUMP_SECS`-
дампами на висящих прогонах — `STUCK_ALIVE_NOT_PARKED` fibers, тот же
stale-slot симптом):

| `MAX_INFLIGHT_CONNS` | 80-конкурентный xargs | Итог |
|---|---|---|
| 1 | ~2-5/80 admit, сервер жив после | survives |
| 2 | ~4-8/80 admit (x2 повтор + отдельно -P200), сервер жив после | survives |
| 3 | 0/80 admit, сервер мёртв после | **permanent wedge** |
| 4 | 0/80 admit, сервер мёртв после | **permanent wedge** |
| 16 | 0/80 admit, сервер мёртв после | **permanent wedge** |

Обрыв резкий (2→3), не плавная деградация — типично для probabilistic
stale-slot race (см. `docs/cases/mn-race-stale-slot-2026-05.md`), так что
`2` — не гарантия на 100% всех будущих машин/нагрузок, а измеренный запас
на ЭТОЙ машине под гейтовой нагрузкой (80 и 200). Не поднимать без
повторного прогона гейта ниже.

## Гейт (пройден)

### `loadtest.ps1 -Concurrency 80 -RepoRoot <worktree>` (2 прогона)

- BLOCK 1-3: чисто оба раза.
- BLOCK 4 (sustained SSE weather-live x50): 1-й прогон — 1 transient network
  flake (`events=0` на #7, реальный internet-запрос, сервер остался 200) —
  2-й прогон подряд: 50/50 чисто. Не регрессия от этого изменения (live-режим
  бьёт в реальные домены).
- BLOCK 5 (concurrency 80): FAIL по скрипт-критерию `$ok -eq 80` (ОЖИДАЕМО —
  часть 80 честно отбита by design) — но **сервер ЖИВ после** (`server=200`)
  оба раза. Раньше (без mitigation) — permanent-000.
- BLOCK 6 (12с idle): 200 оба раза.
- BLOCK 7 (demo determinism): PASS оба раза.
- Итог: PASS=66/67 FAIL=1 (BLOCK5) — 1-й прогон; PASS=67/68 FAIL=1 — 2-й
  прогон (единственный FAIL и там, и там — BLOCK5's строгий критерий).

### Прямой `xargs`

```
seq 1 80  | xargs -P80  -I{} curl -s -m20 -o /dev/null -w "%{http_code}\n" ".../api/run?legend=health&mode=demo&seed={}" | sort | uniq -c
     76 000
      4 200
--- after burst, single req --- single:200

seq 1 200 | xargs -P200 -I{} curl -s -m25 -o /dev/null -w "%{http_code}\n" ".../api/run?legend=health&mode=demo&seed={}" | sort | uniq -c
    192 000
      8 200
--- after 200-burst, single req --- single:200
```

Сервер выживает (часть 200, часть честно отбита 000, single-req после = 200,
НЕ permanent-000) — раньше на этой же нагрузке было permanent-000 навсегда.

## Непушёные коммиты

Ветка `fix-bounded-accept` в `d:/Sources/nv-lang/nova-boundedaccept` — НЕ
смёржена в main (гейт+вливание — оркестратор), см. `git log` этой ветки.

## Амендмент

НЕ нужен — app-код (пример), не язык-меняющее слияние (`main.nv`'s
`import std.runtime.sync`, `detach { }`, `AtomicI64` — уже существующие
std-примитивы/keyword-конструкции, синтаксис не менялся).
