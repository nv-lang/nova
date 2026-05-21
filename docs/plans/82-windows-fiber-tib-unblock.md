// SPDX-License-Identifier: MIT OR Apache-2.0
# Plan 82: Windows fiber stack — обход TIB-блокера (разблокировка 44.3)

> **Создан:** 2026-05-21.
> **Статус:** 📋 proposed, не начат. Требует Ф.0 (research spike) +
> решения автора по avenue (A / B / C).
> **Приоритет:** P2 — Windows работает на calloc-fallback (корректно,
> но без lazy-commit / growable stacks); это долг, не блокер релиза.
> **Родитель:** [Plan 44](44-mn-runtime-roadmap.md) (M:N runtime).
> **Связь:** [Plan 44.3](44.3-fiber-arena-windows.md) — наивный подход
> (`MCO_USE_ASM` + custom VirtualAlloc arena) признан 🔒 fundamentally
> blocked после 4 попыток. **Plan 82 ≠ ещё одна попытка того же** —
> это исследование путей ВОКРУГ TIB-блокера, которые 44.3 не пробовал.
> 44.3 остаётся историческим документом «почему лобовой путь мёртв».

---

## Почему 44.3 заблокирован (точная формулировка)

Блокер — **не «arena»**. Блокер — «запуск fiber'а на не-дефолтном стеке
бэкендом, который не синхронизирует Windows TIB».

- minicoro на Windows x64 выбирает `MCO_USE_ASM` (`minicoro.h:378-388`):
  asm-переключение контекста, меняет только RSP + callee-saved.
- Этот бэкенд **не обновляет** `NT_TIB.StackBase` / `StackLimit` /
  `DeallocationStack` в TEB потока.
- Fiber на стеке из `VirtualAlloc` → любой код, читающий границы стека
  из TIB (SEH unwinding, stack-overflow detection, `/GS` cookies,
  guard-page logic), видит диапазон **оригинального OS-стека** →
  mismatch → hang.
- Вторично: Boehm conservative scan всей arena range → Windows
  commit-charge коммитит страницы, которые не decommit'ятся как Linux
  `MADV_DONTNEED`.

**Вывод:** любой подход `MCO_USE_ASM` + не-OS-managed стек упрётся в то
же самое. Нужно либо отдать TIB операционной системе, либо чинить TIB
вручную. Это и есть развилка плана.

---

## Три avenue (выбор в Ф.0)

### Avenue A — `MCO_USE_FIBERS` (Windows native fiber API) ⭐ предпочтительный кандидат

minicoro **уже содержит** этот бэкенд (`minicoro.h:1382-1400` —
`ConvertThreadToFiber` / `SwitchToFiber` / `CreateFiber`). Windows
native fibers: **ОС сама владеет TIB** — `SwitchToFiber` корректно
обновляет `StackBase`/`StackLimit`/`DeallocationStack`. SEH работает
из коробки.

- **Бесплатно даёт:** корректный TIB, lazy commit (`CreateFiberEx` с
  `dwStackCommitSize` ≪ `dwStackReserveSize` — reserve много, commit
  мало), guard page, overflow detection.
- **Теряем vs custom arena:** стеки — отдельные OS-аллокации, не один
  contiguous arena → нет «single GC root». Нужно решить Boehm-
  регистрацию per-fiber-stack (Ф.3).
- **Цена:** `SwitchToFiber` чуть медленнее raw-asm (user-mode, но с
  TIB-bookkeeping). Для M:N приемлемо — переключения не самый горячий
  путь.
- **Почему предпочтителен:** перекладывает ровно ту проблему, что
  убила 44.3, на ОС. Бэкенд уже vendored — это смена `#define`, не
  новый код переключения контекста.

### Avenue B — `MCO_USE_ASM` + ручной TIB-fixup

Оставить быстрый asm-бэкенд, но в пути resume/yield вручную писать
`NtCurrentTeb()->NT_TIB.StackBase / StackLimit / DeallocationStack`
(+ exception chain). Так делает Boost.Context на Windows — то есть
**доказуемо возможно**.

- **Даёт:** asm-перформанс + полный контроль arena (single GC root
  достижим).
- **Цена:** хрупко — патчить TIB на каждом resume, восстанавливать на
  yield; несколько полей TEB; `/GS` `GuaranteedStackBytes`;
  взаимодействие с guard-page. Самый сложный путь; класс именно этих
  фиксов — там, где 44.3 спотыкался.

### Avenue C — оставить calloc-fallback, понизить амбиции

Не «чинить arena», а **разблокировать M:N на Windows без arena**:
Windows остаётся на дефолтном minicoro calloc-пути (текущее
состояние), но снимаются ограничения, мешающие M:N работать
многопоточно (Boehm thread registration, лимит GC roots).

- **Даёт:** M:N concurrency на Windows на **fixed 56 KB × N** стеках —
  без lazy commit, без growable, без guard page.
- **Открытый вопрос для Ф.0:** нужен ли arena для полезности Windows
  M:N вообще, или fixed-stack M:N — достаточный первый шаг?

---

## Декомпозиция (фазы)

### Ф.0 — Research spike + выбор avenue (decision point, ~2-3 дня)

**Главное отличие от 44.3:** 4 провалившиеся попытки шли **сразу в
интеграцию** в runtime Nova. Plan 82 начинается с **изолированного
spike** — минимальные standalone C-харнессы, вне runtime Nova.

Для каждого avenue (A, B; C — тривиально, уже работает) собрать
минимальный харнесс: запустить корутину на не-дефолтном стеке и
намеренно спровоцировать —
1. SEH / C++ exception unwind через границу fiber-стека;
2. stack overflow (упереться в guard page);
3. Boehm-style conservative scan диапазона стека.

Подтвердить: **нет hang**, корректное поведение. Замерить стоимость
переключения контекста (A vs B vs baseline asm).

**Выход Ф.0:** решение A / B / C + обоснование. Если A и B оба
проваливают spike — честный исход «только avenue C» или «подтверждённо
заблокировано, вот более глубокое почему».

### Ф.1 — Прототип выбранного avenue end-to-end (~2-3 дня)

В spike-харнессе довести до полного: lazy commit + guard page + GC-
взаимодействие доказаны в изоляции, до интеграции.

### Ф.2 — Интеграция в runtime Nova (~2-4 дня)

Windows-ветка в `fibers.h` / `fiber_arena.*`. Сейчас
`NOVA_FIBER_ARENA_ENABLED = 1` только для `__linux__`/`__APPLE__`
(`fiber_arena.h:50-54`); `_NOVA_MCO_DESC_INIT` использует arena-
callbacks только под Linux/macOS (`fibers.h:91-109`). Добавить
Windows-путь (для avenue A — вероятно отдельный флаг
`NOVA_WIN_NATIVE_FIBERS`, не общий arena-флаг).

### Ф.3 — GC-взаимодействие (~2 дня)

Boehm conservative scan vs Windows commit-charge. Для avenue A —
per-fiber-stack регистрация (`GC_register_my_thread` / push-roots)
вместо single contiguous root. Подтвердить отсутствие commit-charge
взрыва (вторичный блокер 44.3, попытки 1-2).

### Ф.4 — Включить M:N на Windows + тесты (~2 дня)

Сейчас M:N тесты платформо-агностичны — на Windows arena off, API
возвращают honest-sentinel `0` (`fiber_arena_stats.nv` ветвится по
`fibers.slot_count() == 0`). Сделать так, чтобы Windows реально
исполнял многопоточный M:N путь; тесты `nova_tests/concurrency/
mn_runtime_*` должны его реально упражнять. Stress: N fibers, no
hang, no leak.

### Ф.5 — spec + закрытие 44.3 (~0.5 дня)

- Обновить D97 (fiber arena spec) — Windows-стратегия.
- В 44.3 добавить шапку-ссылку «superseded by Plan 82» (если 82
  успешен) либо «Plan 82 подтвердил — остаётся avenue C» (если нет).
- `docs/simplifications.md`, `project-creation.txt`.

---

## Acceptance

- Windows исполняет M:N fiber-workload без hang; SEH unwinding и
  stack-overflow detection корректны на fiber-стеках.
- Lazy commit измеримо работает (avenue A/B) — ИЛИ задокументировано,
  почему arena отложена и M:N работает на fixed-стеках (avenue C).
- 0 регрессий в полном прогоне (Windows + Linux).

## Честная рамка риска

План **может снова провалиться** — если в Ф.0 spike avenue A и B оба
неработоспособны, честный исход: «avenue C only» либо «подтверждённо
заблокировано». Plan 82 это **допускает** — Ф.0 front-load'ит
изолированный spike, чтобы провал был дешёвым и диагностичным, в
отличие от 44.3 (4 интеграционных провала). Avenue A (`MCO_USE_FIBERS`)
оценивается как наиболее вероятный к успеху: бэкенд уже vendored,
ОС берёт TIB на себя — ровно та проблема, что убила 44.3.

## Связь

- [Plan 44.3](44.3-fiber-arena-windows.md) — наивный подход (blocked);
  Plan 82 — обход. 44.3 = «почему лобовой путь мёртв».
- [Plan 44.2](44.2-fiber-arena-posix.md) — Linux/macOS arena (✅
  закрыт); даёт reference-семантику (lazy commit, single GC root),
  которую Windows должен догнать.
- [Plan 44.4](44.4-mn-runtime-stage0.md) / [44.5](44.5-work-stealing-scheduler.md)
  — M:N runtime; Windows сейчас на single-threaded cooperative
  fallback, Plan 82 Ф.4 включает реальный multi-threaded путь.
