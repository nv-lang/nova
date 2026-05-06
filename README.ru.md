[English](README.md) | **Русский**

# Nova

Язык программирования с **одной центральной абстракцией**
(алгебраические эффекты + handler'ы) и **одним killer use-case**
(AI-first программирование с верифицируемым кодом от LLM).

## Главный тезис

> **Nova — это язык, в котором LLM может писать код, который человек
> может доверять, потому что эффекты делают всё видимым, контракты
> делают всё проверяемым, а handler'ы делают всё тестируемым.**

## Содержание

- [spec/overview.md](spec/overview.md) — главные идеи, что заимствует у кого, tooling
- [spec/revolutionary.md](spec/revolutionary.md) — **флагманские возможности**:
  effects + handlers, AI-first дизайн, контракты, time-travel debugging
- [spec/syntax.md](spec/syntax.md) — примеры синтаксиса
- [spec/effects.md](spec/effects.md) — система эффектов (базовое введение)
- [spec/open-questions.md](spec/open-questions.md) — нерешённые вопросы
- [spec/decisions/](spec/decisions/) — журнал дизайн-решений с эволюцией
- [compiler-bootstrap/](compiler-bootstrap/) — treewalk-интерпретатор (Rust)
- [compiler-codegen/](compiler-codegen/) — C-бэкенд компилятор (Rust → C → нативный бинарь)

## Из чего следует всё остальное

Одна идея: **всё нечистое — эффект, любой эффект перехватывается
handler'ом**. Отсюда автоматически:

- Тесты без моков (handler-подмена)
- Транзакции, undo/redo, snapshot (handler `Db`)
- Capability security (`forbid X { ... }` запрещает эффект в скоупе)
- Time-travel debugging (запись handler-вызовов)
- Детерминированный repro (handler'ы `Time`+`Random` с фиксацией)
- Supervision как в Erlang (`supervised { spawn ... }` + restart strategy)
- LLM-безопасный код (побочные действия видны в типе)

## Память: managed по умолчанию, real-time opt-in

**Программист пишет, GC работает.** Никаких префиксов памяти в обычном
коде. Циклы освобождаются автоматически. Современный concurrent GC даёт
паузы <1ms.

Для real-time зон (звук, торговля, embedded) — блок `realtime { ... }`.
Внутри него компилятор гарантирует отсутствие приостановок и GC-пауз;
нарушение — compile-time error:

```nova
fn map_audio(samples []f32, gain f32) -> []f32 =>
    realtime {
        samples.map((x) => x * gain)   // без GC, без suspension
    }
```

Для perf-критичного кода компилятор использует **escape analysis** —
не утекающие значения остаются на стеке без аллокаций. Программист не
пишет ничего особого. См. [spec/decisions/05-memory.md#d6](spec/decisions/05-memory.md#d6).

## Статус

Активная разработка. Спецификация стабильна по ключевым областям (эффекты,
handlers, синтаксис, память, конкуренция). Существуют два компилятора:

- **compiler-bootstrap** — treewalk-интерпретатор, запускает все spec-тесты
- **compiler-codegen** — компилирует Nova в C через нативный runtime (эффекты,
  файберы, GC); используется для проверки реализации против спецификации

## Лицензия

Nova распространяется на условиях одной из двух лицензий по выбору
пользователя:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT License ([LICENSE-MIT](LICENSE-MIT))

`SPDX-License-Identifier: MIT OR Apache-2.0`

Документация и спецификация языка распространяются под
[CC-BY-4.0](https://creativecommons.org/licenses/by/4.0/).

### Контрибуции

Любой вклад, намеренно отправленный для включения в проект, по умолчанию
лицензируется как `MIT OR Apache-2.0`, без каких-либо дополнительных
условий — в соответствии с разделом 5 Apache License 2.0.

Подробности — в [CONTRIBUTING.md](CONTRIBUTING.md). Коротко: коммиты должны
быть подписаны DCO (`git commit -s`), это проверяется CI.
