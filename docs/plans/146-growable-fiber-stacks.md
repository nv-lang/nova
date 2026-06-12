<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 146: Growable fiber stacks — lift the ~16k concurrent-fiber ceiling

> **Создан:** 2026-06-11 (из Plan 83-go-cmn Ф.1b scale-теста: 100k/200k fibers
> упёрлись в `fiber_arena exhausted`).
> **Статус:** 📋 PROPOSED — research-first (segmented vs copying — реальная развилка).
> **Приоритет:** P2 — снимает фундаментальный потолок масштаба; не блокирует текущее.
> **Оценка:** крупная (compiler + runtime); research ~1-2 dev-day, impl зависит от выбора.
> **Родитель:** [Plan 82](82-windows-fiber-arena.md) (эволюция fiber-stack модели),
> [Plan 83](83-mn-runtime-roadmap.md) (M:N umbrella).
> **Зависимость:** **copying-вариант gated на [Plan 144](144-precise-gc-implementation.md)**
> (precise GC / stack-maps); segmented-вариант независим.
> **Маркер:** `[M-146-growable-stacks]`.

---

## 1. Проблема (из Ф.1b scale-теста, 2026-06-11)

Текущая модель (Plan 82 fiber arena): каждый fiber резервирует **фиксированный 8MB
virtual stack** → `NOVA_FIBER_SLOT_COUNT ≈ 16384` слотов/worker → **потолок ~16k
одновременных fiber'ов** (16k × 8MB = 128GB virtual). 100k+ → `fiber_arena exhausted`.

Go держит **миллионы** горутин, потому что стеки **маленькие и растущие** (старт ~2-8KB).

## 2. Прецедент Go (на C) — две эпохи

| Эпоха | Подход | Нужен precise GC? | Проблема |
|---|---|---|---|
| Go 1.0-1.2 | **Segmented** (stack splitting: малый старт + новый сегмент при нехватке; `morestack`) | **НЕТ** (сегменты сканируются как есть) | **hot-split** перф (вызов на границе сегмента в цикле alloc/free) — Go выбросил |
| Go 1.3+ | **Contiguous copying** (растёт → аллок больше + КОПИЯ + починка указателей) | **ДА** (нужны stack-maps, чтобы релоцировать указатели) | требует точного GC |

Обе — в C-эпоху рантайма (C→Go доделан в 1.5).

## 3. Развилка для Nova (decision в research-фазе)

- **Вариант A — segmented.** Не требует precise GC → **можно с Boehm** (conservative
  сканит сегменты). Compiler эмитит stack-split проверку в прологе. Минус: hot-split перф +
  prologue overhead. **Независим от Plan 144.**
- **Вариант B — copying.** Лучше перф, но **жёстко gated на [Plan 144](144-precise-gc-implementation.md)**:
  Boehm conservative **не может релоцировать** указатели (не отличает указатель от int) →
  копирование стека невозможно без precise stack-maps от компилятора.

**Рекомендация (предв.):** research взвешивает A (доступно сейчас, но Go его выбросил за
перф) vs B (правильный, но ждёт Plan 144). Вероятный итог: **B после Plan 144** как
production-цель; A — только если 144 далеко и потолок 16k блокирует раньше.

## 4. Scope (research deliverable)
1. Замер: реальный потолок (NOVA_FIBER_SLOT_COUNT по платформам) + типичные stack-нужды Nova-fiber'ов.
2. A vs B: перф (hot-split), compiler-объём (прологи vs stack-maps), GC-зависимость, риск.
3. Решение + миграционный путь от Plan 82 fixed-8MB-арены.
4. Если B → явный gate на Plan 144 + что именно от 144 нужно (stack-maps).

## 5. Связь
- **Plan 82** — текущая fixed-8MB fiber-arena (заменяется/эволюционирует).
- **Plan 144** — precise GC; **prerequisite для copying-варианта (B)** + tcmalloc-аллокатора.
- **Plan 83-go-cmn** — M:N scheduler; растущие стеки ортогональны планировщику, но снимают
  его потолок масштаба.
- См. порядок исполнения: [83-mn-runtime-roadmap.md](83-mn-runtime-roadmap.md) §«Порядок».

## 6. ВЕРДИКТ (2026-06-12) — растущие стеки НЕ сейчас; cheap-first

После discussion-цепочки (precise GC → copying → handles vs shadow-stack → прецеденты):

**Растущие стеки сейчас делать НЕ стоит.** Причины:
1. **Потребность не доказана** — серверная ниша Nova = тысячи соединений, не сотни тысяч
   fiber'ов. 16k одновременных — уже много; решать «миллионы» до 0.1 = оптимизация под
   несуществующую нагрузку.
2. **Цена огромная, путь редкий** — Nova компилируется в C → нужен **whole-program
   shadow-stack** (категория «GC-язык на WASM» / LLVM ShadowStackGC), per-call оверхед во
   всём коде + большая работа в компиляторе. Текущее (Boehm + fixed-стеки) корректно.

### 6.1 Cheap-first (вместо растущих стеков) → **[Plan 148](148-configurable-fiber-arena.md)**
Дешёвый win, БЕЗ растущих стеков и БЕЗ GC: **уменьшить per-fiber reserve (8MB→4MB) + сделать
стек/макс настраиваемыми (env `NOVA_FIBER_STACK`/`NOVA_MAX_FIBERS` + nova.toml `[runtime]`, авто-
округление вверх + clamp)** → 2× плотность + любой потолок по запросу. **Оформлено как Plan 148**
(2026-06-12, supersedes `[M-fiber-arena-raise-cap]`). ~50-100k fiber'ов за ~полдня-день. Virtual ≈ бесплатен на 64-бит; физическая RAM ленивая (платишь
за реально использованное). Единственный риск — меньший стек → deep-recursion fiber может
переполниться → guard-страница даёт чистый краш (не порчу). Две константы `#define`, алгоритм
не меняется. **Это и есть практический ответ на «растущие стеки» на ближайшую перспективу.**

### 6.2 Если когда-нибудь precise GC (Plan 144)
- **Техника: precise-maps + shadow-stack, НЕ handles** (handles навсегда тормозят каждый
  доступ — неприемлемо для systems-языка).
- **Оправдывать СВЯЗКОЙ выигрышей**, не стеками: movable/compacting GC + быстрее GC + дорога
  к concurrent GC + **убирает Windows fiber-stack conservative-scan** (источник M:N-race'ов —
  реальный correctness-бонус). Стратегическая ставка уровня v1.0+, не тактический фикс.

### 6.3 Стратегический корень
Одна стена всплывает многократно: **«компилируемся в C → не контролируем кодоген/стек»** —
бьёт по precise GC, растущим стекам И по scheduler'у (раскладку стека делает clang). Если Nova
захочет Go-уровень рантайма (миллионы fiber'ов, precise/concurrent GC, async-preempt), настоящий
ответ — **владеть кодогеном** (LLVM IR со statepoints / свой backend), а не эмитить C. Решение
масштаба **v2.0**. До тех пор: compile-to-C + Boehm + настраиваемая арена — прагматично.

**Статус:** 📋 PROPOSED → research/implementation **отложены**; при упоре в потолок — сперва
§6.1 (raise-cap), не растущие стеки.
