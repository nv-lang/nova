# PROGRESS — окно p-vela (№173/№243/№162)

## №162 — ЗАКРЫТ (найдено уже влитым до ветвления p-vela)
Код (`nova_sched.h` диагностика + self-slot фикс `9c2b1da13`) и спека
(D439, `spec/decisions/06-concurrency.md`, коммит `6329a4c21`) уже смёржены
в main ДО того, как этот worktree ответвился (`398c6366e` — ancestor HEAD).
Единственная недоделка — устаревшая строка в `docs/plans/221.1-bug-sweep.md`
(всё ещё показывала 🔴 P1). Обновлена. Пин-фикстура
`spec_tests/conformance/standalone/m162_supervised_body_direct_park.nv` —
**30/30 PASS** (перепроверено).

## №173/№243 — STOP (ОДНА зона, root cause НЕ закрыт)
База: `probe173_atomic_checker_clean.nv` (AtomicInt-only, чекер-чистая,
n=64×rounds=20) — **20/30 PASS** изолированно. `race_detected==0` ВСЕГДА
(опровержение прошлого окна переподтверждено). Slot-collision — исключена.
Cancel-kind миссклассификация — исключена. Decisive SEQ_CST+5ms-sleep
эксперимент **исключает memory-ordering/visibility** как причину — запись
(`child_error[slot].published`) физически ещё не произошла в момент, когда
её decrement (`pending_remote`/`pending_sweeps`) уже учтён owner'ом.
Опробованный ACQ_REL-фикс decrements — статистически НЕ отличим от базы
(19/30) — **отkачен, в main НЕ попал**, codebase в этом worktree = pristine
для этих трёх файлов (`sync.h`, `fibers.h`, `emit_c.rs`).

Полная методика + рабочая (недоказанная) гипотеза cross-round-contamination
+ следующие шаги — `docs/plans/wip/d416-serialization-repro/README.md`.

## Гейты (все чистые, изменения кода = НОЛЬ для 173/243, только doc-правки)
- `cargo build --release` (nova-cli) — чисто, только pre-existing warnings.
- `nova check std/src` — **PASS 147 / FAIL 26 / WARN 60** — канон совпал.
- polaris `nova.exe test src --strict-effects` — **PASS 37 / FAIL 0 / SKIP 18**
  — канон совпал.
- `scripts/guards/arch-ratchet.sh` — `lines=64505 <= 64505`,
  `infer=348 <= 348` — на потолке, не превышен.
- Мега-CU / флагман — НЕ прогонялись этим окном (нет behavior-changing
  правок для интеграции; интегратор при приёмке, per бриф).

## Ветка
`pvela` в `d:/Sources/nv-lang/nova-vela` — НЕ вливать, сдаётся интегратору.
Единственные фактические правки: `docs/plans/221.1-bug-sweep.md` (записи
№162/№173/№243) + `docs/plans/wip/d416-serialization-repro/` (README +
новая проба). Кода — не тронуто (после отката).
