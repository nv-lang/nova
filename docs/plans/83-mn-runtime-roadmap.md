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

## ACTION: изучить Go C-era M:N + подтянуть Nova

**`[M-83-study-go-c-mn]`** (P1 research → impl): взять **рабочий M:N из C-исходников Go** (последняя
C-версия рантайма — Go ≤1.4, перед 1.5-трансляцией: `src/pkg/runtime/proc.c`, `runtime.h`, sched/G-P-M,
work-stealing, park/ready, sysmon), **сравнить с текущим M:N Nova** (Plan 82 fiber arena + 83.x scheduler),
и **реализовать улучшения, если наш M:N уступает** (work-stealing balance, sysmon-preemption, P-local
runqueues, spinning/parking эвристики). Особое внимание: открытые race'ы 83.10.4/83.11 — посмотреть, как
Go решал аналогичные (grow-vs-wake, slot-reuse) в C-рантайме.

Источник: github.com/golang/go история (тег go1.4, `src/pkg/runtime/`). Go C-runtime — MIT-совместимая
лицензия (BSD), идеи/алгоритмы переносимы.

---

## Связь
- **Plan 25** G1 (single-threaded N:1 scheduler) / G5 (preemption budget) / G6 (cancel propagation) → закрываются здесь.
- **Plan 82** — fiber arena foundation.
- `[M-opt-preempt-strided-loop]` (backlog) — short-term; signal-preemption (этот план) — long-term.
- Open races: `[M-83.10.4-iso-cancel-startup-race]`, `[M-83.11-grow-vs-wake-race]`.
