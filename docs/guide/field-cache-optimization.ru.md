---
source_rev: 27d5dd055
source_date: 2026-08-02
---

// SPDX-License-Identifier: MIT OR Apache-2.0
# Оптимизация field-cache — руководство пользователя

[English](field-cache-optimization.md) | **Русский**

> Зонтичный план 123 (активны V1-V5). Обновлено 2026-06-02.

## Что она делает

Компилятор Nova автоматически кэширует чтения `@field` и вызовы
`@<pure_method>()` в телах методов, устраняя избыточные разыменования
указателей `self->X` в генерируемом `.c`-выводе. Методы на горячем пути
(ReadBuffer, StringBuilder, итераторы HashMap) обычно получают снижение числа
разыменований указателей на 15-30% в debug-сборках под `-O0`.

Оптимизация **прозрачна** — гарантируется семантическая эквивалентность.
Вы можете отключить её в любой момент с помощью переменных окружения (см.
«Аварийные люки» ниже).

## Что кэшируется

Совместно работают четыре уровня:

### D217 V1 — прямой кэш полей (Plan 123.1)

Для ro-полей, читаемых 2+ раза → кэшируются в начале тела метода:

```nova
fn Point @sum_squared() -> int {
    @x * @x + @y * @y      // Before: 4 pointer derefs
}
// After D217 V1:
//   ro _at_x = @x; ro _at_y = @y; _at_x * _at_x + _at_y * _at_y
```

### D218 V2 — LICM-вынос из цикла (Plan 123.2)

Инвариантные чтения полей внутри циклов → выносятся непосредственно перед
циклом:

```nova
fn Buf @sum_n(n int) -> int {
    mut total = 0
    for i in 0..n {
        total = total + @data[i] + @size   // @size invariant
    }
    total
}
// D218 hoists @size immediately before for-loop.
```

### D219 V3 — кэш чистых вызовов (Plan 123.3)

Вызовы `@<pure_method>()` 2+ раза → кэшируются:

```nova
#pure
fn Vec3 @magnitude_sq() -> int => @x * @x + @y * @y + @z * @z

fn Vec3 @double() -> int {
    @magnitude_sq() + @magnitude_sq()   // Cached single call.
}
```

### D217 V4 — кэш цепочек (Plan 123.4)

Вложенные доступы `@a.b.c` 2+ раза → кэшируются:

```nova
fn Outer @check() -> int {
    @inner.cfg.limit + @inner.cfg.limit + @inner.cfg.limit
    // D217 V4 caches @inner.cfg.limit once.
}
```

## Как посмотреть решения кэша

Флаг CLI `--explain-cache` у `nova check`:

```sh
nova check src/buffer.nv --explain-cache
```

Пример вывода:

```
=== src/buffer.nv ===
  fn ReadBuffer @try_read_u32_le — 4 cache(s):
    D217 field cache: data, pos
    D219 pure-call cache: len
    D217 V4 chain cache: @header.signature

field-cache total: 1 method(s) affected, 4 cache(s) inserted
```

## Аварийные люки

Отключить всё кэширование:

```sh
NOVA_FIELD_CACHE=0 nova build
```

Отключить отдельные уровни:

```sh
NOVA_FIELD_CACHE_LICM=0    # disable D218 LICM
NOVA_FIELD_CACHE_PURE=0    # disable D219 pure-call
NOVA_FIELD_CACHE_CHAIN=0   # disable D217 V4 chain
```

Настроить пороги:

```sh
NOVA_FIELD_CACHE_THRESHOLD=3        # default 2 (D217 V1)
NOVA_FIELD_CACHE_LICM_THRESHOLD=3   # default 2 (D218)
NOVA_FIELD_CACHE_PURE_THRESHOLD=3   # default 2 (D219)
NOVA_FIELD_CACHE_CHAIN_THRESHOLD=3  # default 2 (D217 V4)
```

Потолок кэшей на функцию (бюджет кадра стека):

```sh
NOVA_FIELD_CACHE_MAX=12   # default 8 — total across all 4 layers
```

## Ожидаемая производительность

- Сборки `-O0`: снижение числа разыменований указателей на горячих путях на
  15-30%.
- Сборки `-O2`: меньший выигрыш (C-компилятор уже делает NoAlias-based CSE).
  Всё же измеримый из-за детерминированной эмиссии Nova.
- Кросс-платформенность: идентичный вывод AST на Windows MSVC / Linux clang /
  macOS clang.
- Влияние на кадр стека: ≤ 8 кэш-локальных на функцию × 8 байт ≈ 64 байта.

## Семантическая эквивалентность

Все 4 уровня — это **чистые преобразования AST→AST**. Отключение любого
уровня (или всех) даёт идентичное наблюдаемое поведение:

- stdout / stderr идентичны.
- Паника возбуждается в тех же условиях.
- Эффекты файловой системы / сети идентичны.
- Поведение GC идентично.

Проверено дифференциальным тестированием (все фикстуры nova_tests/plan123_*
проходят PASS идентично при включённом и отключённом кэше).

## Ссылки на spec

- D217 (Plan 123.1) — базовый кэш полей + расширение цепочек V4.
- D218 (Plan 123.2) — семантика LICM.
- D219 (Plan 123.3) — кэш чистых вызовов.
- D24 (Plan 33.1+33.2) — инфраструктура чистоты `#pure`.

## Followup'ы и будущие версии

- V5 (Plan 123.5, эта) — LSP code-lens (отложено) + флаг CLI.
- V6 (Plan 123.6) — телеметрия + production-раскатка + полный набор флагов CLI.
- V7 (Plan 123.7) — межпроцедурный анализ (IPA) для точной инвалидации.
