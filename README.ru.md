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
- [compiler-codegen/](compiler-codegen/) — компилятор Nova (Rust): парсер, type-checker, treewalk-интерпретатор, C-backend codegen

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
        samples.map(|x| x * gain)      // без GC, без suspension
    }
```

Для perf-критичного кода компилятор использует **escape analysis** —
не утекающие значения остаются на стеке без аллокаций. Программист не
пишет ничего особого. См. [spec/decisions/05-memory.md#d6](spec/decisions/05-memory.md#d6).

## Статус

Активная разработка. Спецификация стабильна по ключевым областям (эффекты,
handlers, синтаксис, память, конкуренция). Один компилятор:

- **compiler-codegen** — Rust-реализация с парсером, type-checker'ом,
  treewalk-интерпретатором и C-backend codegen'ом. Компилирует Nova в C
  через нативный runtime (эффекты, файберы, GC); используется как для
  интерактивных запусков (`run`, `test`), так и для нативной компиляции
  (`compile`).

## Сборка из исходников

Pipeline двухступенчатый: `nova-codegen` создаёт `.c`, нативный
C-компилятор линкует его с runtime'ом (`nova_rt/`). Wrapper-скрипты
делают это одной командой:

```powershell
# Windows (требуется MSVC Build Tools)
cd compiler-codegen
cargo build
.\build_c.ps1 path\to\hello.nv -Run
```

```sh
# Linux / Mac (требуется gcc или clang)
cd compiler-codegen
cargo build
./build_c.sh path/to/hello.nv --run
```

Без wrapper'а:

```sh
cd compiler-codegen
cargo run -- compile path/to/hello.nv          # Nova → C
gcc path/to/hello.c nova_rt/alloc.c nova_rt/effects.c nova_rt/fibers.c \
    -I. -o hello                                # C → бинарь
./hello
```

Есть также режимы без codegen'а: `cargo run -- run file.nv` (treewalk-
интерпретатор), `cargo run -- check file.nv` (только type-check),
`cargo run -- test file.nv` (запуск `test "..."` блоков через интерпретатор).

Полный guide, опции, известные ограничения:
[compiler-codegen/README.md](compiler-codegen/README.md).

## Поддержка редакторов

Plugin'ы подсветки синтаксиса для нескольких редакторов лежат в
[editors/](editors/). Все — TextMate grammar / handcrafted, без
семантического анализа (LSP пока не реализован).

| Редактор | Подкаталог | Заметки |
|---|---|---|
| VSCode / Cursor / VSCodium | [`editors/vscode/`](editors/vscode/) | TextMate grammar |
| Sublime Text / TextMate | [`editors/sublime/`](editors/sublime/) | переиспользует `.tmLanguage.json` от VSCode |
| Vim / Neovim | [`editors/vim/`](editors/vim/) | handcrafted `syntax/nova.vim` |
| Emacs | [`editors/emacs/`](editors/emacs/) | major-mode `nova-mode.el` |

Полный обзор, команды установки для каждого редактора и roadmap
(LSP, tree-sitter, JetBrains): [editors/README.md](editors/README.md).

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
