---
source_rev: 07df7d2c9
source_date: 2026-08-02
---

<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Быстрый старт

[English](quickstart.md) | **Русский**

Эта страница доведёт вас от скачанного zip-архива до работающей программы на
Nova за несколько минут, а затем до чуть более крупного примера, показывающего
две вещи, которые отличают Nova: эффекты в сигнатурах функций и
структурированную конкурентность без `async`/`await`.

Nova — компилируемый язык: `nova build` превращает `.nv`-файл в C, а затем в
нативный бинарник через системный C-компилятор. Интерпретатора нет (`nova run`
намеренно не поддерживается) — `nova build` и `nova test` это две команды,
которыми вы будете пользоваться.

## Установка (Windows x64)

1. Скачайте `nova-v0.1.0-windows-x64.zip` со
   [страницы GitHub Releases](https://github.com/nv-lang/nova/releases) и
   распакуйте куда угодно, например в `C:\nova`.
2. В сессии PowerShell, из папки, куда вы распаковали архив, выполните
   **dot-source** скрипта настройки (ведущее `. ` важно — без него переменные
   окружения исчезнут при выходе скрипта):

   ```powershell
   . .\setup-env.ps1
   ```

   Скрипт устанавливает пять переменных окружения (`NOVA_STD_PATH`,
   `NOVA_CG_INCLUDE`, `NOVA_RT_DIR`, `NOVA_GC_LIB_DIR`, `NOVA_GC_INCLUDE_DIR`),
   которые говорят `nova.exe`, где искать стандартную библиотеку и C-рантайм,
   когда он запускается не изнутри монорепозитория Nova, а также добавляет
   папку в `PATH` для текущей сессии. Чтобы закрепить это навсегда, добавьте
   папку в ваш `PATH` и задайте те же пять переменных через
   *Параметры → Переменные среды* (или `setx`).

3. Проверьте, что всё сработало:

   ```powershell
   nova --version
   # nova 0.1.0
   ```

4. Вам также понадобится C-компилятор на машине — Nova компилирует в C, а не
   напрямую в машинный код. MSVC (Visual Studio Build Tools) определяется
   автоматически через `vcvars64.bat`; Clang или GCC тоже работают через
   `--toolchain`.

### Установка (Linux)

Готового Linux-архива для v0.1.0 пока нет — собирайте из исходников. Полный
рецепт (пакеты Debian/Ubuntu, Rust-тулчейн, `git submodule update`, сборка,
smoke-тест) — в [docs/guide/linux-build.md](linux-build.md); наверху той
страницы есть пятиминутный `TL;DR`.

## Привет, Nova

Каждому проекту Nova нужен свой `nova.toml`, чтобы компилятор знал корень
пакета. Создайте папку с двумя файлами:

`nova.toml`:

```toml
[package]
name = "hello"
version = "0.1.0"
```

`hello.nv`:

```nova
module hello

fn main() {
    println("Hello, Nova!")
}
```

Имя модуля (`hello`) должно совпадать с именем пакета для `.nv`-файла, лежащего
прямо в корне проекта — это конвенция Nova, а не опечатка.

Соберите и запустите:

```powershell
nova build hello.nv
.\hello.exe
# Hello, Nova!
```

`nova build` компилирует `hello.nv` до `hello.exe` за один шаг (Nova → C →
нативный бинарник). Есть также `nova check` (только type-check, без
C-компилятора) и `nova test` (запускает файловые блоки `test { ... }`).

## Пример побольше: эффекты + конкурентность

Однострочник выше не показывает, что делает Nova интересной. Вот это
показывает — это настоящий файл `examples/mini_aggregator.nv` из репозитория
Nova, ~30 строк, без сети, без UI:

```nova
module nova_examples.mini_aggregator

import std.time.duration

const BUDGET_MS int = 120   // total budget for the whole run, ms

// One "source": waits its own latency (simulated network call) and
// reports on a channel. A spawn that misses the shared deadline is
// genuinely cancelled — not left running in the background with its
// result thrown away.
fn probe(latency_ms int, deadline Monotonic) Time -> str {
    ro { tx, rx } = Channel[bool].new(1)
    // A `with Fail[T]` handler runs IN THE FIBER of the failing operation,
    // not the installing scope's fiber (see spec/decisions/06-concurrency.md
    // D441) — a bare `mut` flag captured there is a data race under M:N.
    // `AtomicBool` is the synchronized alternative.
    mut timed_out = AtomicBool.new(false)
    with Fail[TimeoutError] = |_e| { timed_out.store(true) } {
        supervised(deadline: deadline) {
            spawn {
                Time.sleep(latency_ms)
                ro _ = tx.try_send(true)
            }
        }
    }
    if timed_out.load() {
        "cancelled"
    } else {
        match rx.try_recv() {
            Some(_) => "done"
            None    => "cancelled"
        }
    }
}

// Fan-out: all sources start AT ONCE, results collected into []str.
fn fan_out(latencies []int, deadline Monotonic) Time -> []str {
    ro outcomes = parallel for i int in 0..latencies.len() {
        probe(latencies[i], deadline)
    }
    outcomes
}

fn main() Time {
    ro latencies []int = [20, 40, 60, 80, 300, 800]   // ms; last two miss the budget
    ro t0 Monotonic = Monotonic.now()
    ro deadline Monotonic = t0 + BUDGET_MS.to_millis()
    ro outcomes = fan_out(latencies, deadline)
    mut done = 0
    mut cancelled = 0
    for i int in 0..outcomes.len() {
        if outcomes[i] == "done" { done = done + 1 } else { cancelled = cancelled + 1 }
    }
    ro now Monotonic = Monotonic.now()
    ro wall = now.elapsed_since(t0)
    println("done=${done} cancelled=${cancelled} wall=${wall.millis()}ms")
}
```

Соберите и запустите из клона репозитория Nova (файл лежит в `examples/`,
рядом с `nova.toml`, который уже объявляет рабочее пространство):

```powershell
cd examples
nova build mini_aggregator.nv -o mini_agg
.\mini_agg
# done=3 cancelled=3 wall=155ms
```

(Примечание: с явным `-o name` выходной файл называется ровно `name` — `.exe`
не добавляется, даже несмотря на то, что это обычный PE-бинарник на Windows.
Без `-o` команда `nova build hello.nv` называет выход `hello.exe` по имени
входного файла.)

Точное распределение `done`/`cancelled` и время `wall` зависят от тайминга —
важно структурное: шесть источников стартуют вместе, два самых медленных
(300ms, 800ms) не успевают завершиться, потому что превышают общий бюджет
120ms, и они реально отменяются (`supervised(deadline:)`), а не остаются
работать после отбрасывания их результата. Обратите внимание на то, чего здесь
**нет**: ни `async fn`, ни `.await`, ни `Future<T>` в типе возврата — `Time` в
сигнатуре функции это единственный маркер того, что код трогает часы, а
`spawn`/`parallel for`/`supervised` дают структурированную конкурентность без
отдельного «async»-диалекта языка.

## Куда дальше

- [spec/overview.md](../../spec/overview.md) — основные идеи, что у кого
  заимствовано, обзор тулинга.
- [examples/flagship/aggregator](../../examples/flagship/aggregator) —
  полноразмерная версия примера выше: настоящий HTTP-сервер (через пакет
  `http`), веб-UI с визуализацией waterfall и та же сигнатура эффектов
  `Net Time Emit`, проверяемая компилятором (`--strict-effects`). Поставляется
  со своим Dockerfile.
- [spec/decisions/](../../spec/decisions/) — журнал проектных решений
  (D-номера), авторитетный источник по синтаксису и семантике Nova — каждая
  языковая возможность восходит к решению здесь.
- [docs/dev/test-conventions.md](../dev/test-conventions.md) — как работает
  `nova test`, маркеры `EXPECT_*`, флаги CLI.
- [docs/guide/linux-build.md](linux-build.md) — сборка из исходников на
  Linux/WSL2.
