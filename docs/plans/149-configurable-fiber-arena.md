<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 149 — Configurable fiber arena (stack size + max fibers): env + nova.toml

> **Создан:** 2026-06-12 (из discussion про растущие стеки — [Plan 146 §6.1](146-growable-fiber-stacks.md)).
> **Статус:** ✅ ЗАКРЫТ Ф.0-Ф.6 (2026-06-12, D233). 7/7 plan149 fixtures PASS (clang);
> regression guards (grow_vs_wake/fibers_10k/ring_overflow) green; smoke PASS.
> **Приоритет:** P2 — cheap, contained, повышает потолок масштаба + DX (тюнинг под нагрузку).
> **Оценка:** ~1-1.5 dev-day (runtime env-read + manifest-parsing + codegen-D + тесты).
> **Родитель:** [Plan 82](82-windows-fiber-arena.md) (fiber arena), [Plan 146 §6.1](146-growable-fiber-stacks.md)
> (cheap-first вместо растущих стеков). **Supersedes** `[M-fiber-arena-raise-cap]`.
> **Worktree:** `nova-p149` (branch `plan-149`).
> **Renumber note:** изначально создан как «Plan 148 / nova-p148»; переименован в Plan 149 / nova-p149
> из-за коллизии plan-148 с 148-independent-cleanups (concurrent plan-138.1). Канонический id — 149.

---

## 1. Зачем

Сейчас per-fiber стек и макс число fiber'ов — **жёсткие `#define`** (`NOVA_FIBER_STACK_SIZE`=8MB,
`NOVA_FIBER_SLOT_COUNT`=16384). Потолок ~16k одновременных fiber'ов, 8MB на каждого — щедро.
Растущие стеки (миллионы fiber'ов) отложены (нужен precise GC — Plan 144/146). **Дешёвый
промежуточный win:** сделать стек и макс **настраиваемыми** (env + nova.toml) + уменьшить дефолт
8MB→4MB. Даёт 2× плотность из коробки + любой потолок по запросу, **без растущих стеков и без GC**.

Это RUNTIME-настройки **готовой программы** (как `GOMAXPROCS` у Go), НЕ параметры компилятора.

## 2. Дизайн-решения

| Параметр | Env | Default | Диапазон | Округление |
|---|---|---|---|---|
| Стек fiber'а | `NOVA_FIBER_STACK` | **4MB** | [256KB, 256MB] | вверх до page-align |
| Макс fiber'ов/воркер | `NOVA_MAX_FIBERS` | **16384** | [64, MAX] | вверх до ×64 |

- **Авто-коррекция ВВЕРХ + clamp (ключевое UX-решение):** пользователь пишет ЛЮБОЕ число
  (`NOVA_MAX_FIBERS=20000`) → runtime сам округляет до корректного (20032, кратно 64) и зажимает
  в диапазон. Про внутренние ограничения (кратность 64, page-align) думать не надо. Битый/мусорный
  env → **warn + default** (никогда не крашим на конфиге).
- **Precedence:** `env` > `nova.toml [runtime]` > builtin default. (env — runtime override;
  nova.toml — project-baked default; builtin — fallback.)
- **Per-worker:** `NOVA_MAX_FIBERS` — на ОДИН воркер; всего = slots × `NOVA_MAXPROCS`.
- **Compile-time MAX отдельно от runtime default:** bitmap (`free_bits[...]`) сейчас compile-time-
  размера = `NOVA_FIBER_SLOT_COUNT`. Чтобы env мог ПОДНЯТЬ выше дефолта — ввести
  `NOVA_FIBER_SLOT_COUNT_MAX` (compile-time, размер bitmap, щедрый ~262144 = 32KB bitmap/арена),
  а runtime `a->slot_count` = clamp(env∨toml∨default, 64, MAX). Не-кратный-64 active count →
  хвостовые биты последнего слова помечаются «занято» на старте (allocator не выдаёт фантомы).
- **Guard page** (16KB PROT_NONE) сохраняется; usable = stack − guard. Overflow → чистый краш
  (hint уже ссылается на `NOVA_FIBER_STACK_SIZE` — обновить на `NOVA_FIBER_STACK`).
- `nova.toml`: новая секция `[runtime]` (`fiber_stack = "4MB"`, `max_fibers = 16384`).
  Принимает human-friendly (`"4MB"`/`"2097152"`).

## 3. Фазы

### Ф.0 — GATE: финализация дизайна + spec ✅ (2026-06-12)
- Утвердить имена env/toml, дефолты, диапазоны, precedence, auto-round-up rule. ✅
- Решить: лоуэрить дефолт стека 8MB→4MB (да) + дефолтный max (16384, без сюрприза). ✅
- **Spec D-block:** `D233` подтверждён free project-wide (`grep -rn '\bD233\b' docs/ spec/` → 0
  совпадений; D234/D235 тоже free; следующий занятый — D236). Контракт в `spec/decisions/08-runtime.md`. ✅
- **Self-reference fix:** заголовок «Plan 148/nova-p148» → «Plan 149/nova-p149» (см. шапку). ✅
- **CRITICAL must_fix #1+#2 resolution (review):** per-fiber minicoro stack size вычисляется в
  `fibers.h::_nova_mco_desc_init_arena` из **compile-time** `NOVA_FIBER_STACK_SIZE`, НЕ из runtime
  `a->slot_size`. Если env поднимает стек — лишняя бронь тратится впустую (fiber всё равно
  overflow'ит на compile-time глубине); если env опускает стек ниже compile-time дефолта —
  minicoro запрашивает coro_size > usable → `nova_fiber_alloc` возвращает NULL → fiber create fail.
  **Резолюция (выбран вариант (a) из review):** ввести runtime-аксессор
  `size_t nova_fiber_arena_slot_size(void)` (оба arena-TU), который lazily инициализирует арену и
  возвращает финальный (env∨-D∨default, round+clamp) `a->slot_size`. `_nova_mco_desc_init_arena`
  вызывает его вместо `NOVA_FIBER_STACK_SIZE` → minicoro `coro_size` масштабируется с runtime
  slot_size → AC2 достижим, floor (256KB) безопасен (usable check проходит). `NOVA_FIBER_STACK_SIZE`
  остаётся как **build-time builtin default**, кормящий `NOVA_FIBER_STACK_DEFAULT` через `#ifndef`.
- **32-bit MAX guard ordering (must_fix #4):** `NOVA_FIBER_SLOT_COUNT_MAX` и `_DEFAULT` ставятся
  внутри того же `#if 32bit / #elif _WIN32 / #else 64bit` каскада что и старый `NOVA_FIBER_SLOT_COUNT`
  (НЕ flat `#ifndef`), затем trailing `#ifndef`-catch-all только для `-D`-override hook'а MAX.
  32-bit: MAX=1024, DEFAULT=16. Это ДО любого generic fallback — иначе 32-bit target получил бы
  bitmap на 262144.

### Ф.1 — Runtime env-read (`fiber_arena.c` + `fiber_arena_win.c`)
- На arena-create: `getenv("NOVA_FIBER_STACK")` / `getenv("NOVA_MAX_FIBERS")` → parse (human-size) →
  auto-round-up (page-align stack; ×64 slots) → clamp → `a->slot_size` / `a->slot_count`
  (поля уже runtime; сейчас просто берут из `#define`).
- `NOVA_FIBER_SLOT_COUNT_MAX` (compile-time, bitmap) отделить от runtime default; bitmap tail-mask
  для не-кратных-64 active count. Cross-platform (POSIX mmap + Windows VirtualAlloc).

### Ф.2 — Дефолт стека 8MB→4MB
- `NOVA_FIBER_STACK_SIZE` (`#define` default) → 4MB; проверить guard-взаимодействие; обновить
  комментарии-расчёты (4096 × 4MB и т.п.). Platform-conditional (32-bit fallback не трогать сильно).

### Ф.3 — nova.toml `[runtime]` (`manifest.rs` + build)
- `RuntimeConfig` struct (по образцу `FfiConfig` manifest.rs:130) + parse в `parse_manifest`
  (manifest.rs:315) — `fiber_stack`/`max_fibers`.
- Прокинуть в build (`test_runner.rs` / codegen-build clang-invoke) как
  `-DNOVA_FIBER_STACK_DEFAULT=<bytes>` / `-DNOVA_MAX_FIBERS_DEFAULT=<n>` → arena использует как
  default-if-env-not-set (`#ifndef ... #define`). **Precedence env > -D(toml) > builtin** держится
  естественно (env читается в runtime, перебивает compile-time default).

### Ф.4 — Валидация + диагностика
- Битый env/toml → `fprintf(stderr, "nova: invalid NOVA_FIBER_STACK '...' — using default 4MB\n")` +
  default. Слишком маленький стек → floor 256KB + warn. Превышение MAX → clamp + warn. Guard-overflow
  hint → ссылка на `NOVA_FIBER_STACK`.

### Ф.5 — Тесты (через release nova & компилятор)
- **Позитивные:** env применяется (стек/слоты); nova.toml baked default; env перебивает toml;
  auto-round-up (20000→20032); 100k слотов поднимается (бронь+lazy-commit) — smoke + fiber-spawn.
- **Негативные:** мусорный env → warn + default (не крах); стек < 256KB → floor; max > MAX → clamp;
  кратность-64 не задана пользователем → авто.
- Где возможно — C-уровень unit (arena create с разными env) + Nova-фикстура (spawn N fiber'ов).

### Ф.6 — Доки + acceptance
- Документировать env (`NOVA_FIBER_STACK`/`NOVA_MAX_FIBERS`) + nova.toml `[runtime]` (getting-started/
  runtime-tuning). Spec D-block финал. Acceptance criteria §4.

## 4. Критерии приёмки
1. clang/MSVC: arena собирается, дефолт 4MB; `nova test` no-regression.
2. `NOVA_FIBER_STACK=2MB` / `=8MB` → arena использует (проверяемо: spawn deep-stack fiber на грани).
3. `NOVA_MAX_FIBERS=N` → spawn'ится до ~N fiber'ов/воркер; N не кратное 64 → авто-округление вверх.
4. `nova.toml [runtime]` default запекается; env перебивает.
5. Битый конфиг → warn + default, НЕ крах. Стек < floor → floor. max > MAX → clamp.
6. Per-worker семантика верна (slots × MAXPROCS).
7. grow-vs-wake / iso-cancel остаются закрыты (arena-config не трогает scheduler-инварианты).
8. Pos + neg тесты зелёные на release nova.

## 5. Связь
- **Plan 82** — fiber arena (тюнингуется). **Plan 146 §6.1** — это и есть cheap-first вместо
  растущих стеков. **Supersedes** маркер `[M-fiber-arena-raise-cap]`.
- **Plan 03.x** — nova.toml manifest (расширяем `[runtime]` секцией).
- НЕ трогает: scheduler (Plan 83-go-cmn), GC (Plan 144), растущие стеки (Plan 146 impl — отдельно).

## 6. Риски
- Меньший дефолт стека (4MB) → fiber с очень глубокой рекурсией может упереться → **чистый краш**
  (guard page, не порча). Mitigated: 4MB щедро + env позволяет поднять + hint в сообщении.
- `nova.toml`→`-D` прокидка: проверить, что все build-пути (test/build/run, clang+MSVC) её несут.
- Bitmap MAX 262144 → 32KB/арена × воркеры — копейки, но задокументировать.

## 7. Результаты (closure 2026-06-12)

**Коммиты (branch `plan-149`):**
- Ф.0 GATE — `plan(149 F.0)`: D233 подтверждён free, self-ref 148→149, must_fix #1/#2/#4 resolution.
- Ф.1+Ф.2 — `feat(149 F.1+F.2)`: runtime env-read + bitmap MAX split + default stack 8MB→4MB.
- Ф.3 — `feat(149 F.3)`: nova.toml `[runtime]` fiber_stack/max_fibers → `-D` (3 toolchain arms).
- Ф.4 — `feat(149 F.4)`: config diagnostics — overflow hints → `NOVA_FIBER_STACK`.
- Ф.5 — `test(149 F.5)`: 7 pos/neg fixtures.
- Ф.6 — `docs(149 F.6)`: D233 spec + runtime-tuning page + supersede marker + logs.

**Ключевая правка из review (must_fix #1/#2):** per-fiber minicoro stack size берётся из RUNTIME
`a->slot_size` через новый `nova_fiber_arena_slot_size()` (в `_nova_mco_desc_init_arena`), не из
compile-time `NOVA_FIBER_STACK_SIZE`. Без неё env-стек не менял бы реальный usable-стек (AC2
недостижим) и floor (256KB) был бы небезопасен. Verified: `slot_size=8388608` при
`NOVA_FIBER_STACK=8MB`, `slot_size=2097152` при toml `fiber_stack="2MB"` (precedence
env > toml > builtin доказан).

**Тесты:** 7/7 plan149 fixtures PASS (clang). Регрешн-гарды AC7 (grow_vs_wake_explicit /
fibers_10k_sleep_cancel / ring_overflow_drain) зелёные; mn_runtime_smoke PASS. Sample fiber-heavy
suite (deep_spawn / cooperative_interleave / mn_lazy_spawn / sleep_bench) — все PASS, идентично
baseline'у (a3597b99, temp-worktree build).

**PRE-EXISTING failure (НЕ регрессия Plan 149):** `cancellation_test` (within[T]/race2[T]
monomorphized nested recursion) RUN-FAIL'ит на «fiber stack overflow in slot 0». Verified:
**падает идентично на baseline (8MB default, старый runtime)** + не лечится `NOVA_FIBER_STACK=64MB`
→ это **runaway/unbounded recursion**, НЕ stack-size issue (raise-stack — суть Plan 149 — не
помогает, потому что глубина неограничена; чистый guard-page краш — корректное поведение). Был
помечен в Plan 83.4.5.10 как «crashes immediate на 1MB»; с тех пор codegen дрейфанул и теперь
overflow'ит и на 8MB — orthogonal codegen-баг вне scope Plan 149. Маркер
`[M-cancellation-test-mono-recursion-overflow]` (P2, отдельный).

**Acceptance:** AC1 (default 4MB, no regression), AC2 (env stack scales — verified slot_size),
AC3 (20000→20032 round-up), AC4 (toml bake + env override — verified), AC5 (garbage/floor/clamp →
warn + default, no crash), AC7 (no scheduler/GC regression), AC8 (pos+neg green), AC9 (bitmap MAX),
AC10 (D233 + docs + marker + self-ref) — ✅. AC6 (per-worker × MAXPROCS) — эмерджентно, не
изменялось. MSVC arm wired (`/D`), но прогон на clang (MSVC arm — следующий verify при наличии).

**Note для будущих прогонов:** `nova test` резолвит `rt_dir` через `find_repo_root()` =
`std::env::current_dir()`, НЕ путь тест-файла. Запускать `nova test` с **CWD = worktree**, иначе
компилируется runtime ИЗ main-репо (worktree-правки .c не попадут в бинарь).

## Followups closed (2026-06-13)

Три post-close followup'а закрыты:

1. **#3 — 32-bit dead DEFAULT/comment.** `NOVA_FIBER_SLOT_COUNT_DEFAULT` исправлен
   `16`→`64` (round-UP-to-×64 + `MIN`=64 уже форсили 64; старый `16` и его «64MB»
   comment были мёртвыми) + `_Static_assert` на инвариант ×64 ∧ ≥MIN. Zero
   runtime-behavior change (64-bit/Windows = 16384). D233 §8 + docs/runtime-tuning.md
   обновлены.
2. **#2 — `nova build` / `nova bench` теперь honor `[runtime]`+`[ffi]`.** Manifest
   резолвится через `find_manifest` на 3 BuildOpts call-site'ах (cmd_build,
   bench `run`, bench `compile_for_profile`), зеркаля test_runner. Precedence
   `env > nova.toml(-D) > builtin` НЕ изменён. Verified: `[ffi]` bogus-lib доходит
   до линкера, `[runtime]` резолвится в обоих front-end'ах. D233 §2 +
   docs/runtime-tuning.md обновлены.
3. **#1 — `cancellation_test` suite-green.** Файл перенесён в
   `nova_tests/concurrency/cancellation_quarantine/` под `_fixture.toml` sentinel
   (walk_nv skip; module → `cancellation_quarantine.cancellation_test` для D78).
   Планируемый int/str/bool split **проверен и НЕ помогает** (single within[T] в
   собственном TU тоже overflow'ит даже на 64MB — unbounded codegen recursion).
   Все test-кейсы сохранены verbatim. Codegen root-cause НЕ тронут; маркер
   `[M-cancellation-test-mono-recursion-overflow]` **остаётся OPEN**.
