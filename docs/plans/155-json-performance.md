<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 155 — `std/encoding/json` production-grade performance rewrite

> **Создан:** 2026-06-14. **Статус:** 📋 PLANNED, P1. **Эстимат:** ~2–3 dev-day.
> **Model:** Opus + Thinking ON (perf-critical + benchmark-driven).
> **Владеет:** D270 (+ Q-json-perf, Q-json-order). **Родитель:** Plan 91 (std MVP) /
> Plan 18 (stdlib roadmap). **Зависит от:** 152.1 (`as_bytes` O(1) byte-lens),
> 153 (Vec API), 131 (RawMem/Vec storage).
> **Принцип (этос D135):** «оптимально» = **измеренное**, а не заявленное —
> каждый шаг подтверждается бенчмарком до/после, иначе это гадание.

---

## 0. Проблема

Текущий `std/encoding/json.nv` (~1026 строк) корректен и проходит тесты, но
**не оптимален** — несколько архитектурных перф-проблем:

### P1 — O(n²) лексер (главный) 🔴
```nova
fn Lexer @peek() -> Option[char] => @input.as_chars().nth(@pos)   // json.nv:240
```
`as_chars().nth(@pos)` **декодирует строку с НАЧАЛА каждый вызов** (`nth` = O(@pos),
chars.nv:104). Лексер зовёт `peek`/`advance` в цикле с растущим `@pos` →
**O(n²)** на весь документ. Плюс `@pos` — **codepoint-индекс**, не байтовый: на
не-ASCII всё съезжает (и `nth` ещё дороже). Это ровно тот footgun, от которого
предупреждает док `CharsIter` (chars.nv:19-22).

### P2 — codepoint-итерация вместо байтов
JSON-структура (`{`, `}`, `[`, `]`, `:`, `,`, цифры, ключевые слова) — **ASCII**.
Декодить каждый байт в codepoint избыточно: сканер должен работать на
`@input.as_bytes()` (O(1)-вью, 152.1) с байтовым курсором. Не-ASCII встречается
только внутри string-значений (там нужен UTF-8-aware путь, но и он байт-ориентирован).

### P3 — аллокации
- Per-token аллокации (TokenWithPos и т.п.) в горячем цикле.
- `HashMap`/`Vec` строятся без `with_capacity` → рост + rehash на каждом объекте/массиве.
- Промежуточные строки при парсе ключей/значений вместо zero-copy слайсов исходника
  (для escape-free строк ключ можно держать как срез — но R-UTF8/lifetime: ключ
  должен пережить документ; решается owned-копией только при необходимости).

### P4 — number parsing
Парс чисел через codepoint-итерацию + возможные промежуточные строки вместо
прямого байтового `strtod`-подобного прохода.

### P5 — сериализация
Encode должен идти в **один** pre-sized `StringBuilder`/byte-буфер с байтовым
escaping (таблица escape-нужности на 256 байт), без промежуточных строк на узел.

**Вывод.** Корректность есть; нужен **byte-level single-pass** ре-дизайн с
измеримым выигрышем. Без бенчмарков «оптимально» — пустое слово (D135).

---

## 1. Принцип ре-дизайна

> **Byte-level single-pass.** Лексер/парсер работают на `@input.as_bytes()` с
> **байтовым** курсором (O(1) доступ, без codepoint-декода для структуры).
> Рекурсивный спуск напрямую по курсору (без отдельного token-потока с
> аллокациями). Буферы (Vec/HashMap/StringBuilder) — **pre-sized**. Строки —
> zero-copy срез исходника, когда нет escape'ов; owned-копия только при escape
> или когда нужно пережить документ. UTF-8-валидность (R-UTF8) сохраняется.

Прецедент: Rust `serde_json` (byte slices + `SliceRead`), Go `encoding/json`
(byte scanner), simdjson (byte-level, хотя SIMD вне scope V1).

---

## 2. Декомпозиция (фазы)

- **155.0 — Бенчмарк-харнесс + профиль baseline (СНАЧАЛА).** `std/bench`-бенчи на
  представительных payload'ах: small (~1 KB), large (~1 MB массив объектов), deep
  (вложенность), wide (много ключей), string-heavy (escape/unicode), number-heavy.
  Замерить throughput (MB/s) + alloc count (`gc.alloc_count`) ТЕКУЩЕЙ реализации.
  Это baseline, против которого доказываем выигрыш. **Без этого фазы 1+ нельзя
  принять** (нечего сравнивать).
- **155.1 — Byte-level scanner (убить O(n²)).** Заменить `peek`/`advance` на
  байтовый курсор над `as_bytes()` (`pos int` = байтовый offset; `cur()`/`bump()`
  O(1)). Структурные токены — прямое сравнение байтов. **Главный выигрыш** (O(n²)→O(n)).
- **155.2 — Recursive-descent parser по курсору.** Парс value/object/array/
  literal напрямую (без промежуточного token-Vec); pre-sized `Vec[JsonValue]` /
  `HashMap[str,JsonValue]` (оценка ёмкости эвристикой по остатку ввода).
- **155.3 — String + number на байтах.** String: быстрый сканер до `"`/`\`/
  control; escape-free срез — zero-copy (или одна owned-копия); `\uXXXX` →
  UTF-8 (surrogate-пары, 152.6 `from_utf16`-логика); R-UTF8 на выходе. Number:
  байтовый проход + один parse в f64.
- **155.4 — Serializer (encode) по одному буферу.** Один pre-sized byte-builder
  (`_buffer`/StringBuilder); 256-байтовая escape-таблица; без промежуточных строк
  на узел; pretty-print опц.
- **155.5 — Валидация + бенчмарк-доказательство.** Полный `json` тест-сьют +
  JSONTestSuite-conformance (y_/n_/i_ кейсы) + бенчмарки 155.0 ПОСЛЕ → таблица
  до/после (throughput ×, alloc'и ↓). Закрытие: spec D270 + simplifications + backlog.

---

## 3. Spec / D / Q

- **D270 (NEW)** — JSON impl-модель: byte-level single-pass scanner/parser,
  zero-copy string policy, pre-sizing, serializer-буфер. Что гарантируется
  (корректность, R-UTF8, throughput-класс), что нет (SIMD V1).
- **Q-json-perf (NEW)** — целевые перф-классы (throughput MB/s порядок,
  alloc/документ) + методология бенчей; что считаем «оптимальным» для bootstrap
  (не обязан биться с simdjson, но обязан быть O(n) + без лишних аллокаций).
- **Q-json-order (NEW)** — сохранять ли порядок ключей объекта? Сейчас `HashMap`
  (порядок не гарантируется). Решить: оставить HashMap (быстро) или
  insertion-order (нужна order-preserving структура — дороже). Прецедент: Go map
  (no order), serde (feature-gated). По умолчанию — НЕ гарантируем (документировать).

---

## 4. Тесты (позитивные + негативные)

- **Позитивные:** существующий `json` сьют (roundtrip, типы, вложенность) + new
  byte-level кейсы (unicode-escape, surrogate-пары, большие числа, глубокая
  вложенность, escape-heavy строки). Прогон через **релизные** nova + компилятор.
- **Негативные:** malformed JSON → `Result::Err`/`Fail` (незакрытая скобка,
  trailing comma, lone surrogate, невалидный escape, число-мусор, control char в
  строке без escape, обрезанный ввод). JSONTestSuite `n_*` (must-reject).
- **Conformance:** JSONTestSuite (Nicolas Seriot) — `y_*` (accept), `n_*`
  (reject), `i_*` (либо). Аналог UAX-conformance для строк: внешние эталоны →
  встроенная фикстура (генератор, как `nova-codegen unicode --emit-conformance`).
- **Бенчмарки:** 155.0 harness — до/после, в `nova_tests`/`bench`.

---

## 5. Критерии приёмки (production-grade, без упрощений)

- **G0 (ОБЯЗАТЕЛЬНО: без упрощений).** Ре-дизайн полный (byte-level scanner +
  parser + serializer), не косметика. Алгоритм O(n) (доказано отсутствие
  O(n²)-путей: нет `as_chars().nth(pos)`-в-цикле).
- **G1 (корректность).** Полный `json` сьют + JSONTestSuite (`y_*` accept, `n_*`
  reject) зелёные; roundtrip-инвариант `parse(serialize(v)) == v` на корпусе.
- **G2 (перф измерен).** Бенчмарк до/после: O(n²)→O(n) подтверждён (large payload
  не деградирует квадратично); throughput ↑ и alloc'и/документ ↓ — **с числами**
  в плане/simplifications (иначе «оптимально» не засчитано).
- **G3 (R-UTF8).** Выход parse — всегда валидный UTF-8 (escape/surrogate
  обработаны); string-значения корректны на не-ASCII.
- **G4 (0 регрессий).** Полный `nova test` без новых FAIL; потребители json
  (если есть) не сломаны.
- **G5 (spec/docs).** D270 + Q-json-perf + Q-json-order закрыты; `docs/` гайд по
  json-API (если меняется поверхность); simplifications + backlog обновлены.

---

## 6. Для исполнителя

- Worktree `nova-p155` (или текущий, если последовательно).
- D-блок **D270** зарезервирован (следующий свободный после D269). Другие
  агенты — с D271.
- **Профиль-first:** не оптимизировать вслепую — сначала 155.0 (baseline-числа),
  потом таргетные фиксы по хотспотам. Каждая фаза — коммит-чекпойнт.
- Конвенции репо: `git add` точечно; DCO sign-off; коммит-сабжекты англ.,
  тела/логи рус.; обновлять project-creation.txt + simplifications.md +
  nova-private/discussion-log.md после крупной задачи. Синтаксис Nova — только из
  `spec/`+`examples/`.
- **Осторожно с byte-level + R-UTF8:** срез исходника как ключ/значение должен
  пережить документ (GC держит буфер через `str`-поле) — проверить lifetime;
  при escape — owned-копия обязательна.
