# План 10: PGO integration (stub / future)

**Статус:** stub / future. Полный план будет написан после
завершения плана 09 (Clang migration).
**Дата создания:** 2026-05-08.
**Зависимости:** [Plan 09](09-clang-migration.md) — миграция на Clang.

---

## Цель

**Profile-Guided Optimization** для Nova C-backend через
two-stage build:

1. `nova build --pgo-instrument foo.nv` — собирает инструментированный
   binary с counter'ами на каждой ветке.
2. Запуск training workload — собирает `foo.profdata` с реальной
   статистикой ветвлений / hot path'ов.
3. `nova build --pgo-use=foo.profdata foo.nv` — собирает финальный
   оптимизированный binary, где Clang переставляет код / inline'ит
   hot fns / отбрасывает cold ветки.

Ожидаемый прирост: **15-30%** на hot path'ах. Замерено в
production'ах Rust/Go/Chrome.

---

## Почему stub, а не полный план

1. **Зависит от плана 09.** PGO на MSVC слабее чем на Clang
   (LLVM IR-based профили vs MSVC старое COFF-based PGO). Нет
   смысла писать детальный план PGO до миграции на Clang.

2. **Зависит от benchmark suite.** План 09 Ф.6 создаёт `bench/`
   с representative workloads. **Тот же suite** становится
   training workload'ом для PGO. Делать дважды бессмысленно.

3. **Зависит от `nova-codegen compile` интеграции.** Сейчас
   compiler-codegen **эмитит .c**, runner (PowerShell скрипт)
   собирает binary. PGO требует **two-stage** в одном workflow —
   удобнее когда `nova-codegen compile` управляет invocation
   компилятора напрямую. Это будущее улучшение CLI.

---

## TBD (что будет в полном плане)

После плана 09 этот документ расширится:

- **Ф.1** — `--pgo-instrument` flag в `run_tests.ps1` /
  `nova-codegen compile`. Эмитит binary с
  `clang -fprofile-generate=<dir>`.
- **Ф.2** — Workflow для training run. Используем `bench/`
  suite (план 09 Ф.6) как training workload.
- **Ф.3** — `--pgo-use=<profile>` flag → собирает финальный binary
  с `clang -fprofile-use=<profile>`.
- **Ф.4** — `llvm-profdata merge` для агрегации профайлов.
  Документировать workflow.
- **Ф.5** — Tests/benchmarks: измерить прирост vs `--release`
  (без PGO) на тех же workloads.
- **Ф.6** — Documentation: README раздел "Production builds with PGO".
- **Ф.7** — `simplifications.md`: запись `[P-pgo-default]` или
  оставить как opt-in для критичных к перфу проектов.

---

## Acceptance criteria (предварительные)

- ✅ Two-stage build работает: instrument → training → use.
- ✅ Прирост perf vs `--release` без PGO **измерим** (15-30% на
  benchmark suite).
- ✅ Документировано как пользователь может включить PGO для своих
  проектов.
- ✅ Не ломает `--dev` / `--release` modes из плана 09.

---

## Open questions

- **Profile в репо или нет?** Cargo рекомендует не коммитить
  `.profdata` (специфичный для платформы). Программист сам делает
  training run. Альтернатива — коммитить и обновлять при заметных
  изменениях. Решается при написании полного плана.
- **AutoFDO** (`-fprofile-sample-use`) vs обычный PGO?
  AutoFDO работает с `perf` sampling вместо инструментирования —
  быстрее training run, но требует `perf` tooling.
- **PGO и LTO** — комбинируются ли? Да, и взаимоусиливаются
  (Clang делает inlining через границы файлов с учётом профиля).
  Используем оба.

---

## Связь

- [Plan 09](09-clang-migration.md) — Clang migration. **Должен быть
  завершён до плана 10.**
- [Plan 02](02-codegen-c-backend.md) — C backend архитектура.
- `spec/open-questions.md` → `Q-build-pgo` — соответствующий
  open-question (зафиксирован 2026-05-08).
- `docs/simplifications.md` → `[P-no-pgo-integration]` — пометка
  про текущее отсутствие.

---

## Ссылки

- [Clang PGO docs](https://clang.llvm.org/docs/UsersManual.html#profile-guided-optimization)
- [LLVM source-based vs IR-based profiles](https://llvm.org/docs/InstrProfileFormat.html)
- Rust PGO опыт: rustc 12-14% прирост на bootstrap (известные
  результаты команды Rust).
- Chrome PGO: 15-25% на core rendering.
