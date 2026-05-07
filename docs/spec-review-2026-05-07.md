# Spec review (2026-05-07) — структурные вопросы и предложения

Документ — результат перечитывания spec/ после раундов 1-3 работы над
bootstrap-компилятором (60/60 tests-nova passing). Адресован другому
агенту/собеседнику для обсуждения дальнейшей работы над spec и
compiler-codegen (bootstrap-компилятор Nova на Rust).

Контекст:
- Spec зрелая, ~21k строк, центральная линия (D10 "всё эффект" + AI-first)
  выдержана.
- Bootstrap покрывает основное: типы, методы, эффекты с handler'ами,
  fibers, GC, школа B str (codepoints), bitwise ops, anon-record D55.
- Зияющие пробелы в bootstrap: pattern alternation `|`, composite
  tuple-patterns, for-in iter type-inference, embed (D39), generic
  syntax в части позиций, contracts (D24), effect inference D28.

Ниже — пять структурных вопросов из spec и конкретные предложения по
каждому. Это не bug list — это **дизайн-уточнения**, которые лучше
зафиксировать до v1.0.

---

## 2. Effects vs protocols — граница тонкая

### Проблема
D62 правило 4 sniff-test ("нужна with-substitution? нужен continuation-
capture?") — для зрелого программиста ОК, но для AI/новичка две разные
семантические сущности с близким синтаксисом → guessing.

В реальном коде `Logger` может быть и `effect` (нужны test-handlers),
и `protocol` (просто структурный contract на `log(msg)`). Spec говорит
"выбор программиста", но не даёт устойчивого критерия.

### Варианты

- **(A) Decision matrix в D62 с примерами.** Таблица 10–15 канонических
  случаев из реального кода:
  - `Logger` → effect (нужен mock в тестах)
  - `Hashable`, `Comparable` → protocol (структурный)
  - `Db`, `Net`, `Fs`, `Time`, `Random` → effect
  - `Iter[T]`, `Display`, `Eq` → protocol
  - `Authn`, `Authz` → effect (capability)
  - `Cache[K,V]` → effect (mockable)

- **(B) Compile-error с подсказкой при misuse.** Если programmer пишет
  `fn f[T Db](x T)` (Db в protocol-position), компилятор должен говорить
  не просто "Db is effect", а "use `fn f(x T) Db -> ...` (effect-position)
  instead — Db needs `with`-substitution".

- **(C) Линтер/AI-hint.** `protocol` со сложной business-семантикой
  (DB-like, IO-like) — warning "consider `effect` if you'll mock this
  in tests".

### Рекомендация
**(A)** — добавить decision matrix в D62. Дёшево, AI-friendly, не меняет
язык. (B) тоже полезно, как часть quality-of-error work.

---

## 3. `Fail` vs `Fail[E]` гибрид — two-ways-to-do-one-thing

### Проблема
D65 разрешает оба, но "public API → typed, scripts → any" — convention,
не enforcement. Bootstrap suite mixed. Программист может случайно сделать
`export fn parse(s str) Fail -> int`, что для public API анти-паттерн
(caller не знает что catch'ить).

### Варианты

- **(A) Усилить линтер, не язык.** `export fn` с `Fail` без параметра —
  **error**, не warning (suppressable через `@allow_any_fail` или
  per-project настройку). На уровне language design не меняем; на
  уровне tooling делаем строже. Сейчас в spec это convention — стоит
  сделать enforced default с opt-out.

- **(B) Формальный desugar `Fail` ≡ `Fail[any]` в bootstrap.** Сейчас
  bootstrap двух их не сравнивает напрямую. Должно быть:
  - lexically `Fail` → token same as `Fail[any]`
  - effect-row-equality видит их одинаково

- **(C) Auto-narrow `Fail[any]` → `Fail[E]` в private.** Если в теле
  private fn точно один `throw expr` где `type-of(expr) = E`, инференс
  выводит `Fail[E]`, не `Fail[any]`. Это **D28 inference**, мы её не
  закончили.

### Рекомендация
**(A) + (C).** Первое — лёгкое в bootstrap (один lint pass). Второе —
часть **D28 effect inference**, которая нужна независимо для AI-first.

---

## 4. `Detach`/`Blocking` эффекты vs `Async` ambient — асимметрия

### Проблема
Граница "видимый side-effect для caller'а" субъективна. `Time.sleep()`
блокирует fiber → почему `Time` эффект, а suspension вообще не эффект?
`Detach` = "переживает caller'а" → почему это не другой type кошмаром?

Это философский вопрос, не bootstrap-задача. Но видеть его стоит.

### Варианты

- **(A) Уточнить D62 критерий через "resource-capability".**
  Формулировка: "**Эффект описывает resource-capability — нечто, что
  можно подменить handler'ом в скоупе. Suspension — не resource, а
  runtime mechanic, общая для всех асинхронных операций.**"
  - `Time` resource = "источник времени" (можно подменить fixed clock'ом)
  - `Detach` resource = "глобальный supervisor" (можно подменить sync-handler'ом)
  - `Blocking` resource = "OS-thread pool" (можно подменить mock'ом)
  - `Async` resource = "fiber scheduler" — **не подменяется** в обычном
    коде, runtime-инфраструктура.

- **(B) Признать что граница прагматичная.** Spec может это явно сказать
  в D62 разделе "Что отвергнуто" — тогда пользователи не ищут глубокую
  логику.

### Рекомендация
**(A)** — добавить в D62 формулировку через "resource-capability". Это
закрывает 80% вопросов читателя. Bootstrap: ничего не меняется, просто
документация.

---

## 5. `Mem` эффект — два класса эффектов

### Проблема
В spec'е есть `Db`/`Net` (semantic effects, влияют на программу) и
`Mem`/`Trace` (instrumental effects, observability). Они смешаны в одном
списке prelude (D26). Программист не отличает.

### Варианты

- **(A) Декларативно разделить в spec.** В D26 prelude таблице — две
  колонки или категория:
  - **Semantic effects:** `Fail`, `Io`, `Net`, `Db`, `Fs`, `Time`,
    `Random`, `Log`, `Ask`, `Detach`, `Blocking` — влияют на семантику
    программы.
  - **Instrumental effects:** `Mem`, `Trace` — observability/profiling,
    не должны влиять на семантику.

- **(B) Линтер.** Instrumental эффекты не должны входить в `export fn`
  сигнатуру (implementation detail). Программа использующая `Mem` —
  это тест/profiler, не business-logic.

- **(C) Тип-маркер `instrumental`.** `type Mem instrumental effect { }` —
  компилятор знает, что эффект не должен влиять на семантику. Лишний
  механизм, не рекомендую.

### Рекомендация
**(A)** — просто документация и категоризация. Низкий приоритет.

---

## 6. `spawn` без synchronization — race conditions ⚠️

### Проблема (важная)
Spec D50 говорит: "результаты через mut-захваты или channels". Но
**channels не специфицированы как D-decision!** `Mutex` тоже нет в
prelude.

Spec их **упоминает** в `select { msg <- channel_a => ... }` примерах
(D14, D50), но формальной D-decision нет.

D71 bootstrap-runtime — single-threaded cooperative, race conditions
технически отсутствуют (нет preempt'а). Но в **production-runtime**
(D14, future) preemption будет, и
```nova
let mut a = 0
spawn { a = compute_a() }
```
— undefined behavior без synchronization.

**Это реальный пробел в spec/stdlib.**

### Варианты

- **(A) D-channels — formal decision с минимальным API.**
  ```nova
  type Channel[T] { ... }
  fn Channel[T].new(capacity int) -> Channel[T]
  fn Channel[T] @send(v T) -> ()              // blocking when full
  fn Channel[T] @recv() -> Option[T]          // None when closed
  fn Channel[T] @close() -> ()
  fn Channel[T] @is_closed() -> bool
  ```
  Channels уже **подразумеваются** в `select { msg <- ch => ... }`
  примерах — нужно их формально декларировать. Поведение closed-state,
  bounded vs unbounded, single vs multi-producer — всё в decision.

- **(B) Запретить разделяемое `mut` между fiber'ами на уровне типов.**
  Captured `mut`-binding в `spawn { ... }` — compile error, если binding
  outlives spawn-scope. Это требует анализа escapes, нетривиально, но
  закрывает класс багов.

- **(C) `parallel { ... }` block с typed tuple-result.** Альтернатива
  `mut`-захватам:
  ```nova
  let (a, b) = parallel {
      compute_a(),  // → a (тип A)
      compute_b()   // → b (тип B)
  }
  ```
  Программист **не пишет** mut-захват, компилятор сам собирает результаты
  в tuple. Закрывает гетерогенный fan-out без race-prone pattern.

### Рекомендация

**Spec, краткосрочно:**
- Добавить **D-channels** с минимальным API. Channels уже подразумеваются
  в spec — формализовать. Это **высокий приоритет**, потому что без
  channels программисту нельзя писать корректный concurrent код в
  language semantics, только в bootstrap (где нет preemption).

**Spec, долгосрочно:**
- Проработать `parallel { ... }` блок с typed tuple-result (C). Это
  убирает race-prone `mut`-захват pattern из рекомендаций D50.

**Bootstrap:**
- До channels — добавить **lint rule** "captured mut binding in spawn
  scope" → warning. Не блокировать, но видеть.

---

## Сводная таблица приоритетов

| # | Что | Spec/Bootstrap | Сложность | Приоритет |
|---|---|---|---|---|
| 2A | Decision matrix effect/protocol в D62 | spec | низкая | **высокий** |
| 3A | Линтер: `export` + `Fail` без E → error | bootstrap | низкая | средний |
| 3C | D28 inference для `Fail[E]` в private | bootstrap | средняя | **высокий** (AI-first core) |
| 4A | Уточнить "resource-capability" в D62 | spec | низкая | средний |
| 5A | Разделить semantic vs instrumental в prelude | spec | низкая | низкий |
| 6A | D-channels с минимальным API | spec | средняя | **высокий** (gap в spec) |
| 6C | `parallel { tuple-result }` блок | spec | высокая | средний (v2) |
| 6 lint | Captured mut в spawn → warning | bootstrap | низкая | средний |

---

## Если выбирать что делать сейчас

1. **D28 effect inference** в bootstrap (3C). Это разблокирует stdlib,
   потому что много функций сейчас падают на необъявленных эффектах.
   AI-first core feature, давно в spec'е, в bootstrap не реализована.

2. **D-channels spec** (6A). Закрывает gap в spec, который потом нужен
   для concurrency examples и production-runtime.

3. **Decision matrix effect/protocol** (2A). Дёшево, AI-friendly,
   AI-генерация кода сразу качественнее.

---

## Что не входит в этот документ

- **Парсер/codegen хвост для stdlib** (pattern alternation `|`,
  composite tuple-patterns, for-in iter type-inference и т.д.) — это
  **bootstrap engineering**, отслеживается в `examples/stdlib/STATUS.md`.
- **D24 contracts** (requires/ensures) — отдельный фронтир, в bootstrap
  пока не приоритет.
- **D78 package tooling** — целиком вне bootstrap, отдельная задача.
- **Effect-type vs Handler[E] strict разделение в type checker** —
  bootstrap не строгий; production compiler должен быть.
- **Rank-2 polymorphism** (D61 generic в effect-методах) — отложен в
  spec, в bootstrap erased через `any`.
