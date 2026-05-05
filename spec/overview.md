# Nova — обзор

## Центральная идея

**Всё нечистое — эффект. Handler — функция первого класса.** Из этой
одной абстракции (алгебраические эффекты в стиле Koka/Effekt, но
доведённые до прикладного состояния) следует всё остальное в языке.

См. [revolutionary.md](revolutionary.md) для развёртки.

## Killer use-case

**AI-first программирование.** Когда LLM пишет 50–80% кода, языку нужны:
- видимость побочных действий в сигнатуре (эффекты)
- compile-time гарантии вместо runtime-проверок (контракты, capabilities)
- локальность контекста (одна функция понятна без чтения 10 файлов)
- ошибки компилятора как обучающий сигнал для LLM
- стабильность синтаксиса (LLM учится на старых данных)

Все существующие языки спроектированы до AI-эпохи. Nova — первый
язык, явно оптимизированный под пару «LLM пишет, человек ревьюит».

## Поддерживающие решения

1. **Один язык — три режима компиляции:** AOT (как Go/Rust), JIT (как
   .NET), интерпретатор (как Python). Один и тот же исходник.
2. **Память: managed по умолчанию (современный concurrent GC),
   regions opt-in для real-time.** Программист пишет код без префиксов
   памяти — циклы освобождаются автоматически, паузы <1ms. Escape
   analysis оставляет на стеке всё, что не утекает (без GC overhead).
   Для real-time зон (звук, торговля, embedded) — блок `realtime nogc { }`
   ([D64](decisions/04-effects.md#d64)), внутри `region { }` для arena-
   allocations. См.
   [decisions/05-memory.md#d6](decisions/05-memory.md#d6).
3. **Структурная типизация + вывод типов везде.**
4. **Protocols + data вместо классов.** Никакого наследования. Структурные
   контракты через `protocol` (см. [decisions/01-philosophy.md#d1](decisions/01-philosophy.md#d1), [decisions/02-types.md#d42](decisions/02-types.md#d42)).
5. **Контракты в сигнатуре.** `requires`/`ensures`/`invariant` —
   опциональны, но проверяются статически где можно.

## Что заимствует у кого

| Фича | Источник |
|------|----------|
| Алгебраические эффекты + handler'ы | Koka, Effekt, Eff |
| Скорость компиляции, простой синтаксис | Go |
| Производительность, traits, мономорфизация | Rust |
| Concurrent GC, простота памяти для backend | Go, Java ZGC |
| Pattern matching, ADT, sum-types | OCaml/Rust |
| REPL + AOT в одном | Common Lisp / Julia |
| Регионы памяти | Zig, Odin |
| Structured concurrency, supervision | Erlang/OTP, Swift |
| Запуск скрипта как `nova file.nv` | Python |
| Контракты, refinement-types | Eiffel, Dafny, F* |
| Capability security | E, Pony |
| Time-travel debugging | rr, Hypothesis |

## Tooling из коробки

- `nova run file.nv` — интерпретатор для скриптов
- `nova build` — статический бинарь
- `nova fmt`, `nova lint`, `nova test`, `nova bench`, `nova doc`
- `nova check --fragment '...'` — типечекинг одной функции без проекта
- `nova run --record trace.nrec` / `nova replay trace.nrec` — time-travel
- LSP — часть компилятора
- Пакетный менеджер — content-addressed (как Deno + Nix)
- Hot reload в dev-режиме
- Структурированные ошибки с готовыми патчами для LLM

## Что выкинуто из обычных языков

- **Заголовочные файлы, namespaces, modules-vs-packages** — один файл = модуль
- **Null** — только `Option[T]`
- **Исключения как невидимое control flow** — только эффект `Fail[E]`
- **`async`/`await` ключевые слова** — suspension это ambient runtime
  ([D62](decisions/04-effects.md#d62)), эффекты в типах: `Net`, `Io`, `Db`
- **Перегрузка операторов на произвольные типы**
- **Макросы как препроцессор** — только typed comptime (как Zig)
- **Глобальное изменяемое состояние** — `mut` поля/параметры
  (локально) или специализированные state-эффекты (Counter, Cache)
- **DI через рефлексию** — зависимости в эффектах или параметрах
- **Mock-библиотеки** — handler'ы из языка
- **Скрытые импорты** — каждый идентификатор виден откуда

## Зарезервированные identifier'ы

Помимо grammar-keyword'ов (`fn`, `type`, `effect`, `handler`, `let`,
`if`, `match`, `return`, ... — около 38 слов), Nova имеет
**identifier'ы с зарезервированной семантикой**. Они парсятся как
обычные имена, но компилятор знает их специальное значение в
определённых контекстах.

| Identifier | Категория | Где валиден | См. |
|---|---|---|---|
| `Self` | referential type | в любом type-контексте — refers к receiver-типу метода / типу удовлетворяющему protocol'у | [D66](decisions/02-types.md#d66) |
| `any` | top-type | везде; runtime type-tag для downcast'а | [D54](decisions/03-syntax.md#d54) |
| `Never` | bottom-type | return type не-возвращающих функций (`throw`, `panic`, `loop`) | [D26](decisions/08-runtime.md#d26) |
| `Option[T]`, `Some`, `None` | sum-тип в prelude | везде | [D26](decisions/08-runtime.md#d26) |
| `Result[T, E]`, `Ok`, `Err` | sum-тип в prelude | везде | [D26](decisions/08-runtime.md#d26) |
| `Error` | record-тип в prelude | для `throw err` | [D26](decisions/08-runtime.md#d26) |
| `RuntimeError` | sum-тип в prelude | bottom-уровневые runtime-ошибки | [D26](decisions/08-runtime.md#d26) |
| `Handler[E]` | first-class тип handler'а эффекта `E` | везде | [D61](decisions/04-effects.md#d61) |
| `Fail[E]`, `Fail` | стандартный эффект | в effect-row сигнатуры | [D25](decisions/04-effects.md#d25), [D65](decisions/04-effects.md#d65) |
| `Io`, `Net`, `Db`, `Fs`, `Time`, `Random`, `Log`, `Trace`, `Ask[T]`, `Alloc[R]`, `Detach`, `Blocking` | стандартные эффекты | в effect-row сигнатуры | [D2 (REVISED)](decisions/04-effects.md#d2), [D50](decisions/06-concurrency.md#d50) |
| `int`, `i8`-`i64`, `u8`-`u64`, `f32`, `f64`, `str`, `bool`, `byte` | примитивные типы | везде | [D44](decisions/03-syntax.md#d44), [D27](decisions/03-syntax.md#d27) |

Эти identifier'ы можно **переопределить локально** (например, тип
`Net` пользовательской библиотеки), но это — анти-паттерн. Линтер
выдаст warning.

## Главные trade-offs

1. **Algebraic effects сложны в реализации** — это передовой край PL,
   Koka работает 10+ лет и всё ещё академический.
2. **Понимание эффектов — порог входа** — решается **только** качеством
   сообщений компилятора. Если они академически точны и человечески
   непонятны — язык мёртв.
3. **Performance эффектов** требует агрессивной оптимизации (статический
   handler-резолюшн, инлайнинг).
4. **Ставка на AI-кодинг** как доминирующий тренд — статистически вероятна,
   но не гарантирована.
5. **9 из 10 таких проектов проваливаются.** Это нормальный риск
   революционной попытки. Альтернатива — гарантированный «ещё один Nim».
