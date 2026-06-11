<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 83 — M:N runtime roadmap (umbrella)

> **Создан:** 2026-06-11.  **Статус:** 📋 UMBRELLA (M:N-семейство; работа в 83.x sub-планах).
> **Назначение.** Дом для M:N-runtime работы Nova. Plan 25 G1/G5/G6 ссылаются сюда (раньше —
> на фантомный `23-mn-runtime-roadmap.md`, который никогда не создавался; refs перенаправлены на Plan 83, 2026-06-11).

---

## M:N work (sub-планы 83.x)
Вся M:N-работа идёт здесь:
- **Plan 82** — Windows fiber arena (foundation; minicoro fiber stacks).
- **Plan 83.10.x** — armed M:N routing + STALE-slot races (83.10.1-83.10.5: cancel-timer-hang, per-thread TLS effect registry, fail-handler cross-mco, iso-cancel startup race).
- **Plan 83.11** — centralized IO driver + grow-vs-wake race ([M-83.11-grow-vs-wake-race] OPEN).
- **Plan 83.12** — async net stdlib (TcpListener/TcpStream/UdpSocket via libuv).
- **Plan 83.13** — precise GC roadmap research.
- **Plan 83.4.x** — coordination primitives (Semaphore/Barrier/CountDownLatch/Condvar).

Открытые M:N race'ы (backlog P1): `[M-83.10.4-iso-cancel-startup-race]`, `[M-83.11-grow-vs-wake-race]`
(множество фикс-попыток провалено — нужен архитектурный подход; **DO NOT repeat tactical attempts**).

---

## Go precedent — M:N в C-рантайме (исследование 2026-06-11)

| Фича | Go-версия | В C-коде рантайма? |
|---|---|---|
| **M:N scheduler** (goroutines, G-P-M) | Go 1.0 (2012) / refine 1.1 (2013) | **ДА** — рантайм Go был на C до 1.5 |
| **Signal-based async preemption** | Go 1.14 (фев 2020) | **НЕТ** — рантайм уже на Go (C→Go в 1.5, авг 2015) |

**Выводы для Nova (рантайм на C):**
- **M:N: прецедент доказан** — Go реализовал полноценный M:N work-stealing scheduler в **C-рантайме**
  (Go 1.0-1.4, 2012-2015, до self-hosting). Значит M:N в C-рантайме Nova **доказуемо возможен** — не
  блокируется тем, что рантайм на C.
- **Signal-preemption**: Go сделал его уже на Go (1.14), но механизм (OS-сигналы SIGURG + safe-point
  метаданные) — **OS-уровневый, язык-агностичный** → реализуем и в C-рантайме Nova. Снимает per-loop
  `nova_preempt_check` (см. `[M-opt-preempt-strided-loop]`).

## ACTION: изучить Go C-era M:N + подтянуть Nova → **[Plan 83-study-go-c-mn](83-study-go-c-mn.md)**

**`[M-83-study-go-c-mn]`** (P0 research → impl): ✅ **research+декомпозиция выполнены 2026-06-11**
→ см. **[83-study-go-c-mn.md](83-study-go-c-mn.md)** (8-фазный production-grade план порта).

Research-workflow (11 агентов) зафетчил Go 1.4 C-рантайм (`src/pkg/runtime/proc.c`, `runtime.h`,
`netpoll*.c`, `time.goc`, `lock_sema.c`), смапил текущий Nova M:N, выдал gap-анализ (9 gaps).
**Главная находка:** grow-vs-wake — баг **реаллокации**, не memory ordering; Go fixed `runq[256]`
(стабильный адрес, never realloc) структурно его исключает. **Закрывает оба открытых маркера:**
`[M-83.11-grow-vs-wake-race]` (Ф.1+Ф.2+Ф.3), `[M-83.10.4-iso-cancel-startup-race]` (Ф.5).

Источник: github.com/golang/go тег go1.4 (последняя C-версия, перед go1.5 C→Go). Go BSD-3-Clause —
совместима с Nova; алгоритмы переносимы свободно, при близком порте — атрибуция Google + BSD-нотис.

---

## GC связь (precise GC из Go 1.4 — кандидат)

Go ≤1.4 C-рантайм имел **точный (precise) parallel-mark STW** GC на C (`mgc0.c`,
tcmalloc-style `mcache`/`mcentral`), с pointer-bitmaps + precise stack maps от компилятора.
**Релевантно M:N:** точный GC с явной регистрацией fiber-stack роутов **закрыл бы Windows
fiber-stack-scanning проблему Boehm** (`SuspendThread` conservative-скан не видит
minicoro-стеки — корень race-бага Plan 83.11 §12.23). Concurrent tri-color GC появился
только в go1.5 (C→Go), в C-версии его нет.

**НЕ scope M:N-работы** — GC вынесен в **[Plan 144](144-precise-gc-implementation.md)**
(implementation; родитель — [Plan 83.13](83.13-precise-gc-roadmap.md) research, рекомендация
Option B Hybrid). M:N-планировщик портируется с Boehm как есть; precise GC — независимая работа.

## Связь
- **Plan 25** G1 (single-threaded N:1 scheduler) / G5 (preemption budget) / G6 (cancel propagation) → закрываются здесь.
- **Plan 82** — fiber arena foundation.
- **Plan 83.13** (precise GC research) → **Plan 144** (precise GC implementation) — закрыл бы Windows fiber-stack issue Boehm.
- `[M-opt-preempt-strided-loop]` (backlog) — short-term; signal-preemption (этот план) — long-term.
- Open races: `[M-83.10.4-iso-cancel-startup-race]`, `[M-83.11-grow-vs-wake-race]`.
