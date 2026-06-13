<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 144: Precise GC implementation — Boehm replacement

> **Создан:** 2026-06-11 (extracted из Plan 83.13 research deliverable).
> **Статус:** 🟡 DECOMPOSED — механизм выбран (Henderson shadow-stack); фазы Ф.0–Ф.8 (non-moving → moving), см. §7–§8.
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

> **NB (обновлено 2026-06-13):** декомпозиция выполнена — см. **§8**. Механизм
> зафиксирован в **§7** (Henderson shadow-stack). §4 оставлен как исторический
> high-level черновик; источник истины по фазам — §8.

## 5. Связь
- **Plan 83.13** — research/decision (Option B Hybrid), source of truth для стратегии.
- **Plan 25 G3b** — production-readiness roadmap (Boehm replacement = v1.0 blocker).
- **Plan 83 / `[M-83-study-go-c-mn]`** — Go 1.4 C-runtime precedent (precise GC как
  кандидат, закрывает Windows fiber-stack issue).
- **Plan 83.5** — Boehm THREAD_LOCAL_ALLOC (interim perf win, не заменяет Boehm).
- **[Plan 146](146-growable-fiber-stacks.md)** — растущие стеки; **copying-вариант GATED
  на этот план** (нужны precise stack-maps для релокации указателей при копировании стека —
  Boehm conservative не может). Точный GC здесь разблокирует и copying-стеки.
- Порядок исполнения семейства: [83-mn-runtime-roadmap.md](83-mn-runtime-roadmap.md) §«Порядок».

## 6. Реализация-заметки (design-discussion 2026-06-12)

- **Техника root-scan: precise-maps + shadow-stack, НЕ handles.** Nova компилируется в C →
  clang владеет раскладкой стека → точные stack-карты «из коробки» недоступны. Решение —
  **shadow-stack** (компилятор сам ведёт точный список адресов GC-указателей: push/pop кадра
  на границах функций; GC сканит его, не C-стек). Handles (двойная косвенность `*T`→`**T`)
  отвергнуты — налог на КАЖДЫЙ доступ; неприемлемо для systems-языка. Прецеденты shadow-stack:
  OCaml C-FFI (`CAMLlocal`/`caml_local_roots`), V8 `HandleScope`, JNI refs (на границе);
  whole-program — LLVM `ShadowStackGC` + GC-языки на WASM (категория Nova).
- **Heap-сторона достижима** (Nova знает layout типов → per-type pointer-bitmap); **stack-сторона**
  — главная трудность compile-to-C, решается shadow-stack'ом.
- **Оправдание = СВЯЗКА**, не только стеки: movable/compacting + быстрее GC + дорога к concurrent
  GC + **убирает Windows fiber-stack conservative-scan** (источник M:N-race'ов).
- **Стратегический корень:** настоящий Go-уровень (миллионы fiber'ов + precise/concurrent GC +
  async-preempt) в пределе требует **владеть кодогеном** (LLVM IR statepoints / свой backend)
  вместо emit-в-C. Решение масштаба v2.0. До тех пор precise GC = shadow-stack поверх C.

## 7. Выбранный механизм: Henderson shadow-stack (design 2026-06-13)

**Решение:** precise roots реализуются техникой **Henderson** — «Accurate Garbage
Collection in an Uncooperative Environment» (F. Henderson, 2002). Компилятор Nova
эмитит для каждой функции **frame-struct** с известным layout, содержащий её
heap-корни, и **связывает кадры в цепочку** (per-fiber shadow-stack). GC сканит
цепочку — точно — вместо консервативного скана C-стека / fiber-arena.

Это стандартный путь precise GC при компиляции в C (Mercury, ряд ML→C / Scheme→C).
Альтернатива handles (`*T`→`**T`, налог на каждый доступ) — отвергнута (§6).

### 7.1. Runtime-структуры

Каноничная раскладка — **inline-roots**: корни лежат СРАЗУ за заголовком (как в
оригинальном Henderson), поэтому поле-указатель `roots` НЕ нужно — GC вычисляет
адрес массива как `(void**)(f + 1)`. Экономия: −8 байт на кадр и −1 запись на вызов.

```c
typedef struct NovaFrame {
    struct NovaFrame* prev;   // кадр вызывающего (цепочка)
    uint32_t          nroots; // число root-слотов; сами слоты — сразу за заголовком
    // void* slot[nroots];    // (концептуально) корни: (void**)(this + 1)
} NovaFrame;

// Инвариант codegen↔runtime: между заголовком и слотами НЕТ padding,
//   т.е. offsetof(combined, slot) == sizeof(NovaFrame). Выполняется т.к. и prev,
//   и void*-слоты выровнены на 8; закрепить статически:
_Static_assert(sizeof(NovaFrame) % sizeof(void*) == 0,
               "roots must start right after header, no padding");

// ВЕРШИНА цепочки — per-FIBER, не per-OS-thread:
//   swap'ается на каждом fiber-switch (как NT_TIB.StackBase в fiber_arena).
// Каждый OS-worker и каждый blocking{}-offload-поток имеют СВОЮ вершину.
```

> **Почему не `alloca`.** `nroots` известен СТАТИЧЕСКИ на каждую функцию (число
> локалов фиксировано на этапе компиляции) → достаточно обычного локального struct'а
> с inline-массивом `{ NovaFrame hdr; void* slot[N]; }`: смежная раскладка по
> известному смещению, без runtime-вычисления размера, дружелюбнее оптимизатору.
> `alloca` понадобился бы только при динамическом числе корней — которого нет.

### 7.2. Codegen-паттерн (на функцию с heap-корнями)

```c
ReturnType f(args) {
    struct { NovaFrame hdr; void* slot[2]; } fr;  // slot[] смежно за hdr (компилятор сам)
    fr.slot[0] = NULL; fr.slot[1] = NULL;    // (1) ОБНУЛИТЬ до первого safe-point
    fr.hdr.prev   = nova_shadow_top;         // (2) push
    fr.hdr.nroots = 2;                       //     roots-поле убрано (inline за hdr)
    nova_shadow_top = &fr.hdr;
    ...
    fr.slot[0] = live_ptr;                   // (3) write-back ПЕРЕД safe-point
    call_that_may_gc();                      //     safe-point
    // moving-режим: live_ptr = fr.slot[0];  // (4) reload ПОСЛЕ (адрес мог сдвинуться)
    ...
    nova_shadow_top = fr.hdr.prev;           // (5) pop на КАЖДОМ выходе (return/panic/defer)
    return ...;
}
```

GC-скан (корни читаются как `(void**)(f + 1)`):
```c
for (NovaFrame* f = top; f; f = f->prev) {
    void** roots = (void**)(f + 1);
    for (uint32_t i = 0; i < f->nroots; i++)
        if (roots[i]) mark(roots[i]);
}
```

### 7.3. Четыре инварианта (источник всех багов техники)

1. **Init-before-safe-point** — root-слоты обнулены до первого alloc/вызова, иначе GC
   читает мусор.
2. **Write-back-before-safe-point** — любой *живой-через-safe-point* heap-указатель
   записан в кадр до точки. Для **non-moving** этого достаточно (лишние копии в
   регистрах безвредны — объект помечен через кадр).
3. **Reload-after-safe-point** — **только moving/compaction**: перечитать из кадра, т.к.
   GC обновил адрес. Это и есть пессимизация оптимизатора вокруг heap-ptr.
4. **Pop-on-every-exit** — вершина возвращается на `prev` по ВСЕМ путям выхода
   (early return, `?`/Bang error-path, panic-unwind, defer). Самая рискованная зона
   при компиляции в C — крепить pop в существующий epilogue/unwind.

### 7.4. Что специфично для M:N-рантайма Nova

- **Fiber-switch swap.** `nova_shadow_top` сохраняется в состояние fiber'а при park
  и восстанавливается при resume — рядом со swap'ом стека (аналог NT_TIB). Без этого
  GC увидит чужую цепочку.
- **Unified roots registry.** Для STW-скана нужен реестр ВСЕХ живых вершин: по одной
  на fiber + по одной на каждый worker/offload-поток. Это закрывает Go-allg-вдохновлённый
  `[M-mn-gc-root-unified-stack-registry]` и **снимает Windows fiber-stack
  conservative-scan** (корень race'ов — Plan 83.11 §12.23 / reference-mn-race).
- **Cooperative safe-points.** GC стартует только в safe-point'ах (call/alloc/loop
  back-edge) — между write-back и use GC сработать не может.

### 7.5. Оптимизации (тиры O0–O3)

**Фундаментальный потолок (honest limit).** У Go write-back **ленивый** — статические
карты дают GC найти указатель *где он лежит* (регистр/слот) в момент сборки. У нас карт
нет → write-back **энергичный** (eager): GC найдёт корень только там, куда мы его *явно
положили*. Оптимизации уменьшают **число** энергичных write-back'ов, **не делают** их
ленивыми. Этот зазор — цена компиляции-в-C.

**Кто оптимизирует.** clang НЕ уберёт наши `fr.slot[i]=…` сам: `nova_shadow_top=&fr.hdr`
публикует адрес кадра в глобал, читаемый GC → для clang записи в `fr` имеют видимый
side-effect (escape). **Все оптимизации ниже обязан делать codegen Nova на своём IR ДО
эмита C**, не надеясь на clang.

Стоимость схемы: (A) push/pop кадра, (B) обнуление слотов, (C) write-back перед
safe-point, (D) reload после (moving), (E) доступ к TLS-вершине.

| # | Оптимизация | Срезает | Сила |
|---|---|---|---|
| 1 | Нет кадра, если нет корня живого через safe-point | A+B+C функции | 🔥 макс |
| 2 | **Effect-driven safe-points**: вызов `#no_gc`/pure — НЕ safe-point | C | 🔥 макс |
| 3 | Per-safepoint live-set: писать только корни, живые в ЭТОЙ точке | C | 🔥 |
| 4 | Slot coloring: непересекающиеся live-range делят слот → меньше `nroots` | B+C+размер | ⚙ |
| 5 | Store sinking + LICM для write-back | C (не-достигнутые пути / циклы) | ⚙ |
| 6 | Frame coalescing: leaf-кластер пушит ОДИН кадр | A | ⚙ |
| 7 | Single-epilogue + unwinder-reset для pop | A на error/panic/defer + риск | ⚙ |
| 8 | Вершина в регистре (трюк Go `g`→r14) | E (TLS-load) | 🔧 adv |
| 9 | Selective reload: только movable-слоты | D | 🔧 moving |
| 10 | Fold `shadow_top` в fiber-CB (рядом со swap стека) | E + кэш-линия | free |

**Ключевое #1+#2.** Большинство мелких функций корней-через-safe-point не имеют → **0
операций**. А `#no_gc`/pure-вызовы (система эффектов Nova!) перестают быть safe-point'ами →
write-back перед ними не нужен. Монтоморфизация делает анализ точным (нет виртуального
dispatch). Здесь Nova **обыгрывает наивный Henderson** — за счёт своей системы эффектов.

```c
int64_t nova_len(NovaStr* s) { return s->len; }   // leaf, не аллоцирует → кадра НЕТ
// ...
NovaObj* a = nova_make_obj();
fr.slot[0] = a;            // нужно: впереди аллокация
nova_log_int(a->id);       // #no_gc, pure → НЕ safe-point → write-back НЕ нужен
nova_other_alloc();        // safe-point → a уже в слоте ✓
```

**#7 — снятие риска инварианта pop.** Pop не в каждой точке выхода (легко забыть на
`?`/panic), а единый epilogue + сброс вершины в unwinder при раскрутке (как `longjmp`
у Henderson) → на исключительных путях pop вообще не пишем.

```c
NovaObj* ret;
if (err) { ret = NULL; goto out; }
ret = r;
out:
    nova_shadow_top = fr.hdr.prev;   // ЕДИНСТВЕННОЕ место pop; panic-путь чинит unwinder
    return ret;
```

**Тиры (привязка к §8):**

| Тир | Оптимизации | Фаза | Зачем |
|---|---|---|---|
| **O0** | наивно: кадр на каждую функцию с корнями, write-back перед каждым вызовом | Ф.2 (эталон) | корректность, baseline |
| **O1** | #1 + #2 + #3 (frame-elision + effect-safe-points + per-safepoint live-set) | Ф.2 завершение | **условие жизнеспособности** — без них каждый вызов = safe-point с write-back, перф «съеден» |
| **O2** | #4 + #5 + #6 + #7 (coloring, sink/LICM, coalescing, единый epilogue) | Ф.4–Ф.5 | перф-полировка + снятие риска pop |
| **O3** | #8 + #9 + #10 (регистр-вершина, selective reload, fold в CB) | Ф.6+ / флаг | финальный TLS/moving-перф |

> **O0→O1 — НЕ «оптимизация ради скорости», а условие жизнеспособности схемы.** Без #1/#2
> каждый вызов становится safe-point'ом с write-back. Поэтому Ф.2 **обязана** довести до
> O1; O2/O3 — последующая полировка. AC Ф.5 (перф vs Boehm) меряется на **O1+**, не на O0.

## 8. Декомпозиция по фазам

Два полукорпуса: **non-moving precise (Ф.0–Ф.5)** — даёт точную пометку, убирает
false-retention и integer-как-указатель, разблокирует generational; **moving (Ф.6–Ф.8)**
— компактификация + растущие/копирующие стеки. Не-moving — самостоятельная веха с
бóльшей частью пользы при минимальной пессимизации; делать первым.

| Ф | Цель | AC |
|---|------|-----|
| **Ф.0** GATE | Заморозить ABI shadow-frame (§7.1), определение safe-point, протокол swap вершины на fiber-switch, взаимодействие с blocking{}-offload, дисциплина pop на error/panic/defer-путях. Спроектировать unified roots registry. | Design-doc + spec D-блок (D2xx) + open-questions; реестр вершин описан; решение non-moving-first зафиксировано. Без кода. |
| **Ф.1** Heap bitmaps | Codegen эмитит per-type pointer-offset bitmap; allocator пишет layout-id в заголовок каждого объекта. (Лёгкая сторона — layout известен.) | Каждая heap-аллокация несёт layout-id; precise heap-tracer метит объект точно при заданных корнях; unit-тест на 3-4 типах (record/sum/nested-ptr). |
| **Ф.2** Codegen shadow-frame (non-moving) | Эмит frame-struct + push/pop + write-back перед safe-point'ами для функций с heap-локалами, живущими через safe-point. Инварианты 1,2,4. Pop на всех exit-путях (incl. `?`/panic/defer). **Довести до тира O1** (§7.5: frame-elision + effect-safe-points + per-safepoint live-set) — не опционально, а условие жизнеспособности. | Кадр — авторитетный источник: тест, где Boehm conservative ложно удерживает (integer похож на указатель / dead-but-on-stack) → точная сборка. Pop-discipline тест: early-return + panic + defer не ломают цепочку. O1-проверка: leaf/`#no_gc`-вызовы НЕ порождают кадр/write-back. |
| **Ф.3** Runtime precise root-scan | GC обходит shadow-цепочку + unified registry вместо консервативного скана C-стеков/fiber-arena. Swap вершины на fiber-switch. | Boehm stack-scan отключён (свой root-provider); полная регрессия зелёная; **закрывает reference-mn-race** (fiber-stack scan больше не консервативен) — стресс-фикстура M:N зелёная без `GC_DONT_GC`. |
| **Ф.4** Safe-point completeness | Гарантировать safe-point'ы на call/alloc/loop-back-edge; GC стартует только в них (cooperative). Интеграция с preempt. | Стресс: GC не может сработать между write-back и use; ноль use-after-free под `NOVA_GC_STRESS` (collect на каждом safe-point). |
| **Ф.5** Non-moving precise GC online ✦ | Precise mark-sweep на shadow-stack + heap-bitmaps; conservative fallback только для подлинно-unknown layout. O2-полировка (§7.5: coloring, sink/LICM, coalescing, единый epilogue). **ВЕХА.** | Полная nova test регрессия зелёная; bench vs Boehm baseline (не хуже X%) **меряется на O1+, не на O0**; false-retention бенч улучшен; 8MB fiber-reserve можно НЕ трогать (это Ф.7). |
| **Ф.6** Moving/compaction | Codegen: reload-after-safe-point (инвариант 3); GC обновляет root-слоты в кадрах + heap-указатели; bump/compact allocator. | Compaction-тест: фрагментированный heap уплотняется, все адреса обновлены, ноль dangling; перемещаемый-объект тест зелёный. |
| **Ф.7** Растущие/копирующие стеки ✦ | С precise-roots fiber-стек мал и релоцируем → снять 8MB→2KB reserve (связка с [Plan 146](146-growable-fiber-stacks.md) copying-вариант). | 100k+ fiber'ов в бюджете памяти (снимает потолок fiber_arena Plan 82/149); copying-grow с релокацией указателей зелёный. |
| **Ф.8** Generational/concurrent groundwork | Write-barriers (§4.3) для incremental/concurrent; tri-color подготовка. **Post-v1.0, опционально.** | Deferred — отдельная design-сессия; AC определяются тогда. |

✦ = пользовательская веха (milestone).

**Глобальные AC плана:** (1) точность — ноль ложного удержания на curated-наборе; (2)
корректность — полная регрессия зелёная без `GC_DONT_GC`/uncollectable-костылей; (3)
M:N — reference-mn-race воспроизводимо НЕ повторяется; (4) перф — non-moving не хуже
Boehm на bench-наборе сверх согласованного бюджета.

**Порядок / гейты:** Ф.0 GATE первым (design+spec). Стартовать **после** стабилизации
scheduler (умбрелла Plan 83, §«Порядок исполнения»). Ф.6–Ф.8 гейтятся на закрытие Ф.5.
Модель работы для Ф.0: Opus + Thinking ON.
