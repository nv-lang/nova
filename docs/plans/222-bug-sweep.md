<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# План 222 — Bug Sweep (единый реестр зачистки бэклога дефектов)

**Статус:** 🔨 В РАБОТЕ (заведён 2026-07-21 по запросу владельца: «есть план, в котором
правятся все эти баги?» — не было; теперь single source для всей бэклог-очистки).
**Приоритет:** P1-часть = релиз-сопряжённая (v0.1, план 221); остальное — фоновая
программа после релиза. **Правило:** запись здесь = указатель; полная история дефекта —
в `docs/plans/backlog-followups.md` (или реестре семьи, напр. 196-one-truth-closeout.md).
Статусы обновляет интегратор при вливании волн.

Легенда: ✅ закрыт · 🔨 волна в полёте · ⏳ очередь (порядок сверху вниз) · 🧊 после релиза.

## Ф.1 — P1 (все живые)

| # | Маркер | Статус |
|---|---|---|
| 1 | [M-http-compress-errorkind-crosspkg-collision] | 🔨 волна (sonnet): расширение D381-карты коллизий на внешние пакеты; БЛОКЕР тегов v0.1 (решение владельца) |
| 2 | [M-protocol-embed-vtable-missing-method] | 🔨 волна (sonnet): vtable зеркалит flatten_protocol_methods |
| 3 | [M-match-arm-mixed-int-width-sentinel-coerce] | 🔨 волна (sonnet); старейший P1 (172.2, 2026-06-26) |
| 4 | [M-d39-embed-delegation-dispatch-noop] | 🔨 у ПАРАЛЛЕЛЬНОЙ сессии (opus-рекон) — здесь не дублировать |
| 5 | [M-104.10-diag-pipeline-correctness] | ⏳ следующий слот (sonnet) |
| 6 | [M-196-freefn-arity-overload-default-ret-mismatch] | ⏳ (sonnet, 196-семья — карта в реестре 196) |
| 7 | [M-vec-spelling-consume-chain-cap-collision] | ⏳ (sonnet; при приёмке линт-волны new-then-cap проверить связь) |
| 8 | [M-compress-checksum-cleanup] | 🔨 волна (sonnet): синк nova-compress (std-часть закрыта ранее) |
| 9 | [M-d78-dup-decl-type-cross-import-ambiguous] | ⏳ (узкий остаток Plan 202) |

## Ф.2 — P2 свежие (эта неделя)

| # | Маркер/имя | Статус |
|---|---|---|
| 1 | slice-ext-receiver-for-in | ✅? — вероятно закрыт for-in фиксом 2026-07-21 (char.to_str blanket); ПРОВЕРИТЬ репро и закрыть запись |
| 2 | interp-numeric-fallback-silent-garbage | ⏳ проверка: родня закрытого println-Debug (тот же last-resort путь) — прогнать репро, остаток в волну |
| 3 | d424-rawptr-infer-gap (intra-module extern без энфорса) | ⏳ |
| 4 | compound-assign-mul-div | ⏳ |
| 5 | d376-slow-suffix-peer-merge | ⏳ |
| 6 | d289-module-qualified-path-collision (приёмка П19) | ⏳ (после ErrorKind-волны — соседняя тематика квалификации) |

## Ф.3 — 🧊 хвост (после релиза, программой как 196)

- ~14 старых P2-codegen (value-record/net/ffi-семьи, эпоха 172.2/176/178/180) — по одному
  за волну, реестр = backlog-followups.md.
- ~8 tooling/unicode slow-gates.
- 3 × P3 (198-семья, живые репро подтверждены).
- 1 × P4-кандидат (stack-alloc view — по триггеру).
- Архитектурные (НЕ дешёвые, отдельные решения): [M-emit-c-loc-for-span-wrong-file-merged-cu]
  (loc_for_span, process-global); механика отбора фикстур мега-CU раннером (m221-странность);
  196-остатки (реестр 196-one-truth-closeout.md, корень B11q найден — «имя→баунды» 31 сайт).

## Закрыто этой волной плана (2026-07-21, для истории)

217+TcpStream · Ш4+Ш2 (208 целиком) · write-collision · order-cycle · 216-хвосты (Err/tuple)
· d216-паника · oot-дефисы · net-b **[M-boehm-large-buffer-retention-fiber-reuse]** (−87%)
· box-vtable+protocol-box · mut_clock (раннер/ENV-директивы) · println-Debug (cmd_build
inject) · d55-const · match-scope-gap · for-in char.to_str · git-кэш гонка.

**Операционка:** все волны — sonnet по карте (haiku только чистая механика с полным
запрет-набором); worktree-изоляция; мега-CU/авторитетный гейт — интегратор; каждая волна
закрывает маркер в backlog-followups.md той же веткой.
