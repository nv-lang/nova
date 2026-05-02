[English](README.md) | **Русский**

# Nova

Гипотетический язык программирования с **одной центральной абстракцией**
(алгебраические эффекты + handler'ы) и **одним killer use-case**
(AI-first программирование с верифицируемым кодом от LLM).

Дизайн-документ, не реализация. Накапливается в ходе обсуждения.

## Главный тезис

> **Nova — это язык, в котором LLM может писать код, который человек
> может доверять, потому что эффекты делают всё видимым, контракты
> делают всё проверяемым, а handler'ы делают всё тестируемым.**

## Содержание

- [spec/overview.md](spec/overview.md) — главные идеи, что заимствует у кого, tooling
- [spec/revolutionary.md](spec/revolutionary.md) — **революционные возможности**:
  effects + handlers, AI-first дизайн, контракты, time-travel debugging
- [spec/syntax.md](spec/syntax.md) — примеры синтаксиса
- [spec/effects.md](spec/effects.md) — система эффектов (базовое введение)
- [spec/open-questions.md](spec/open-questions.md) — нерешённые вопросы
- [decisions.md](decisions.md) — журнал дизайн-решений с эволюцией

## Из чего следует всё остальное

Одна идея: **всё нечистое — эффект, любой эффект перехватывается
handler'ом**. Отсюда автоматически:

- Тесты без моков (handler-подмена)
- Транзакции, undo/redo, snapshot (handler `db`/`mut`)
- Capability security (запрет эффектов в скоупе)
- Time-travel debugging (запись handler-вызовов)
- Detерминированный repro (handler `time`+`random` с фиксацией)
- Supervision как в Erlang (structured `par` + handler перезапуска)
- LLM-безопасный код (побочные действия видны в типе)

## Память: managed по умолчанию, regions opt-in

**Программист пишет, GC работает.** Никаких префиксов памяти в обычном
коде. Циклы освобождаются автоматически. Современный concurrent GC даёт
паузы <1ms.

Для real-time зон (звук, торговля, embedded) — явный `region { ... }`
блок с эффектом `Realtime`, GC внутри выключен:

```nova
fn process_audio(samples []f32) Realtime -> []f32 =>
    region {
        let buf = []f32.with_capacity(1024)
        // ... обработка, гарантированно нет GC pauses
        buf.to_owned()
    }
```

Для perf-критичного кода компилятор использует **escape analysis** —
не утекающие значения остаются на стеке без аллокаций. Программист не
пишет ничего особого. См. [decisions.md D6](decisions.md).

## Статус

Концептуальный набросок. Главная цель документа — фиксировать
дизайн-решения и причины за ними.

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
