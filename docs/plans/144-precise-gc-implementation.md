<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 144: Precise GC implementation — Boehm replacement

> **Создан:** 2026-06-11 (extracted из Plan 83.13 research deliverable).
> **Статус:** 📋 PROPOSED — implementation plan, ещё не декомпозирован по фазам.
> **Приоритет:** P3 — long-horizon, v1.0 production-blocker (не блокирует текущую работу).
> **Оценка:** крупная многофазная работа (codegen + runtime), ~6-12 mo по оценкам 83.13.
> **Родитель:** [Plan 83.13](83.13-precise-gc-roadmap.md) (research/decision), [Plan 25 G3b](25-production-readiness-roadmap.md).
> **Зависимости:** независим от M:N-работы; codegen-prerequisites (stack maps).

---

## 1. Почему отдельный top-level план (а не 83.x)

Plan 83.13 по своему scope — **ТОЛЬКО research** («decision document, НЕ implementation»).
Реализация выделена в отдельный top-level план, потому что:

1. **Compiler-spanning, не runtime-only.** Точный GC требует, чтобы codegen
   эмитил pointer-maps / stack-maps — работа в самом компиляторе, шире чем
   umbrella «83 = M:N runtime».
2. **Масштаб v1.0-блокера.** Замена Boehm — крупная самостоятельная многофазная
   работа; прятать её под `83.x` занижает видимость.
3. **Чистая структура.** research (83.13) → decision → implementation (144) разнесены.

## 2. Контекст и решение из research (Plan 83.13)

Decision-документ: [`docs/research/precise-gc-decision-2026.md`](../research/precise-gc-decision-2026.md)
(5801 слов, 8 секций; merge `d743a77f21b`, 2026-05-26).

**Рекомендация 83.13:** **Option B (Hybrid Boehm + Nova-managed arenas)** на v1.0,
с post-v1.0 миграцией на **Option A (MMTk)**.

Boehm conservative GC — production-blocker для:
- **Dynamic stack growth** (Go-style 2KB→grow): conservative GC не может move
  pointers → Nova держит 8MB virtual reserve на fiber.
- **Concurrent GC** (sub-1ms STW): Boehm STW phase scales O(heap).
- **Per-thread fast-path**: даже с Plan 83.5 THREAD_LOCAL_ALLOC — fallback на global heap.

Codegen prerequisites (оценки 83.13): stack maps ~3-6mo, write barriers ~1-2mo,
per-fiber maps ~1-2mo.

## 3. Go 1.4 precedent (связь с Plan 83 / `[M-83-study-go-c-mn]`)

Go ≤1.4 (последняя C-версия рантайма) имел **точный (precise) parallel-mark
stop-the-world** GC на C (`mgc0.c`, `mheap.c`, tcmalloc-style `mcache`/`mcentral`):
- **Точность** обеспечивалась pointer-bitmaps на heap-объекты + precise stack maps,
  эмитимыми компилятором Go.
- **Важно для Nova:** точный GC с явной регистрацией fiber-stack роутов **обошёл бы
  Windows fiber-stack-scanning проблему Boehm** (`SuspendThread` conservative-скан не
  видит minicoro-стеки — корень race-бага, см. Plan 83.11 §12.23 / reference-mn-race
  case study). Это сильный аргумент в пользу precise GC именно для M:N-рантайма Nova.
- Concurrent tri-color GC появился только в go1.5 (вместе с C→Go переводом) — в
  C-версии его нет; для concurrent нужен Option A (MMTk) или собственная реализация.

**Лицензия:** Go под BSD-3-Clause (совместима с Nova MIT OR Apache-2.0). Алгоритмы
переносимы свободно; при близком порте кода — атрибуция + сохранение copyright-нотиса.

## 4. Scope (high-level — детальная декомпозиция в отдельной сессии)

Implementation Option B (Hybrid), фазы-кандидаты (черновик, не финал):

1. **Codegen: stack maps** — emit precise pointer maps для stack frames (крупнейший prereq).
2. **Codegen: heap object layout bitmaps** — type → pointer-offset bitmap.
3. **Codegen: write barriers** — для будущего incremental/concurrent.
4. **Runtime: Nova-managed arenas** — bump-allocator арены для известных-layout объектов;
   Boehm остаётся для conservative-fallback (closures, unknown layout).
5. **Runtime: precise root registration** — fiber-stack роуты через stack maps (закрывает
   Windows fiber-stack issue).
6. **Migration + bench** — gradual rollout, perf vs Boehm baseline.

> **NB:** это умышленно НЕ детальная декомпозиция. Перед стартом implementation —
> отдельная design-сессия (Opus + Thinking ON), которая превратит §4 в Ф.1..Ф.N с
> acceptance criteria, опираясь на 83.13 decision-документ.

## 5. Связь
- **Plan 83.13** — research/decision (Option B Hybrid), source of truth для стратегии.
- **Plan 25 G3b** — production-readiness roadmap (Boehm replacement = v1.0 blocker).
- **Plan 83 / `[M-83-study-go-c-mn]`** — Go 1.4 C-runtime precedent (precise GC как
  кандидат, закрывает Windows fiber-stack issue).
- **Plan 83.5** — Boehm THREAD_LOCAL_ALLOC (interim perf win, не заменяет Boehm).
