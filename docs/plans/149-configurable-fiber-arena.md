<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 149 — Configurable fiber arena (stack size + max fibers): env + nova.toml

> **Создан:** 2026-06-12 (из discussion про растущие стеки — [Plan 146 §6.1](146-growable-fiber-stacks.md)).
> **Статус:** 📋 PLANNED.
> **Приоритет:** P2 — cheap, contained, повышает потолок масштаба + DX (тюнинг под нагрузку).
> **Оценка:** ~1-1.5 dev-day (runtime env-read + manifest-parsing + codegen-D + тесты).
> **Родитель:** [Plan 82](82-windows-fiber-arena.md) (fiber arena), [Plan 146 §6.1](146-growable-fiber-stacks.md)
> (cheap-first вместо растущих стеков). **Supersedes** `[M-fiber-arena-raise-cap]`.
> **Worktree:** `nova-p148` (создать при старте).

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

### Ф.0 — GATE: финализация дизайна + spec
- Утвердить имена env/toml, дефолты, диапазоны, precedence, auto-round-up rule.
- Решить: лоуэрить дефолт стека 8MB→4MB (да) + дефолтный max (16384, без сюрприза).
- **Spec D-block** (next free project-wide — подтвердить в Ф.0, во избежание коллизии): контракт
  configurable arena (env + toml + precedence + auto-round + bounds). Q при необходимости.

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
