# №1877 — явные эффекты точек входа OS-примеров

Задача #57, 2026-10-10. Исходная база:
`6a3275d0add28a9fe207fac9464baa3380f97f8d`.
Перед сдачей база обновлена fast-forward до
`ec4e6a23531100177f0e2a258ce44478d5595dd9` (только документы).

## РЕПРО

GitHub `nova-gate` run [38039017937](https://github.com/nv-lang/nova/actions/runs/38039017937),
job `rest`: full, 1231 с, один итоговый отказ A-E2. Его реальные носители:

- `examples/os/agent_runner.nv:75/89` — `args`, `run_agent`;
- `examples/os/pty_session.nv:60/69` — `args`, `run`.

`case4/5/6/9/10/11` и `tmo_one/other/tmo_vanished` в том же логе —
синтетические отрицательные случаи классификатора, итог дословно:
`САМОТЕСТ ЗЕЛЁН: 11/11`. Jobs mega/conformance зелёные. Это не новые
носители №848/1038; сводка самотеста не отдельная причина отказа.
API артефактов прогона вернул пустой список; доступен полный job log.

Свежий частный release-оракул собран из исходной базы за 3 минуты.
`check-novac-oracle-fresh` подтвердил свежесть всех Rust-исходников.
Команды воспроизведения (выходные бинарники — в scratchpad сессии):

```sh
nova build examples/os/agent_runner.nv --strict-effects -o "$SCRATCH/agent_runner.exe"
nova build examples/os/pty_session.nv --strict-effects -o "$SCRATCH/pty_session.exe"
```

До правки:

```text
before agent_runner build exit=1 seconds=12.59
before pty_session build exit=1 seconds=18.59
```

У каждой программы `E_UNDECLARED_TRANSITIVE_EFFECT`: `args` требует `Os`,
`run_agent`/`run` требует `Proc` и `Io`. После первой правки строгая проверка
прошла, но линковка остановилась из-за отсутствия Boehm GC в новом worktree.
После штатного получения закреплённых gc/libatomic_ops сборка продолжена
с обычным GC; режим без GC не использовался.

## ТОЧКА И ФИКС

В обеих точках входа `fn main()` заменено на `fn main() Os Proc Io -> ()`.
Тела функций, обработчики, тайм-бюджет и сообщения сохранены.

Норма: D62 — транзитивные обязательства в strict-режиме; D492:
«Рядом с `Os` появляется новый plumbing-эффект `Proc`»,
«рабочий обработчик `real_proc()` — `#default_handler`»
(`spec/decisions/04-effects.md:8735–8743`). Дефолтный runtime-обработчик
не отменяет статической декларации. `args` имеет эффект `Os`,
`run_agent` и `run` — `Proc Io`.

Охват класса: поиск `Proc`, `Command.start`/`.start(` и `start_pty` по всем
`examples/**/*.nv` нашёл эти два файла. Существующий A-E2 уже ловит отказ
обоих носителей; нового инварианта и нового стража не требуется.

## ФИКСТУРЫ И ИСПОЛНЕНИЕ

Проверяются сами изменённые примеры: это исправление их деклараций,
а не поведения компилятора. Дублирование тел в conformance не проверяло бы
исправленные файлы. Негативная проба — снятие деклараций в тех же файлах.

Положительная проверка на Windows:

```text
after-ready agent_runner build exit=0 seconds=22.72
after-ready agent_runner usage exit=0 stdout='usage: agent_runner <program> [args...]\n' assertion=True
after-ready agent_runner process exit=0 seconds=0.11 assertion=True
task: demo
agent exited with code 0
after-ready pty_session build exit=0 seconds=10.01
after-ready pty_session usage exit=0 stdout='usage: pty_session <program> [args...]\n' assertion=True
after-ready pty_session process exit=0 seconds=0.13 assertion=True
hello
program exited with code 0
```

У `agent_runner` дочерний процесс — настоящий `System32/sort.exe`,
у `pty_session` — настоящий `System32/cmd.exe` через ConPTY.
Usage сравнивается целиком, процессы — код 0 и указанные подстроки
результата (PTY выводит также терминальные управляющие последовательности).

## САБОТАЖ

Обе effect-row сняты; строгая сборка снова отвергла обе программы:

```text
sabotage agent_runner build exit=1 seconds=11.33
sabotage pty_session build exit=1 seconds=6.52
```

У каждой вновь три `E_UNDECLARED_TRANSITIVE_EFFECT` для `Os`, `Proc`, `Io`.
После пробы обе сигнатуры восстановлены. После снятия согласованного барьера
перезапуска OpenCode выполнен отдельный зелёный повтор, общий watch — 44 с:

```text
restored agent_runner build exit=0 seconds=17.40
restored agent_runner usage exit=0 stdout='usage: agent_runner <program> [args...]\n' assertion=True
restored agent_runner process exit=0 seconds=0.09 assertion=True
task: demo
agent exited with code 0
restored pty_session build exit=0 seconds=25.06
restored pty_session usage exit=0 stdout='usage: pty_session <program> [args...]\n' assertion=True
restored pty_session process exit=0 seconds=0.12 assertion=True
hello
program exited with code 0
```

Обе восстановленные сборки использовали C-кэш именно исправленного исходника;
отрицательная проба между ними вновь отказала на строгой проверке типов.
Исходники Rust после создания частного оракула не менялись.

## std/src, НОРМА И ПРИЁМКА

### Доработка оформления записи по CI

Кандидат `187654a28970613c018d2f064041cc47515afc0e` отклонён CI:
`registry-shape` не распознал оговорку «одного зелёного примера недостаточно».
В №1877 добавлена явная формула «фикс носителя приёмкой не считается»;
анализ класса и носителей сохранён. База этого стража не менялась.
Точечный `bash scripts/guards/check-registry-entry-shape.sh .`:

```text
check-registry-entry-shape: записей без полного оформления 344 (база 343)
check-registry-entry-shape: ВЫРОСЛО — 344 > базы 343
check-registry-entry-shape: FAIL
```

После исправления:

```text
check-registry-entry-shape: записей без полного оформления 343 (база 343)
check-registry-entry-shape ok: роста неоформленных записей нет (343 <= 343, номера сверены поимённо), разобрано строк 1787 >= 1142
```

Код примеров в доработке не менялся; для нового SHA обязательны новый
полный набор required CI и manual full, прежний кандидат не засчитан.

### Оставшиеся действия приёмки

- `std-check`: не применимо — Rust-компилятор и std не изменены.
- Язык не меняется; новый D-блок не требуется.
- Реестр: №1877. Временная дыра №1876 разрешена интегратором, потому что
  номер выделен #55; при появлении его строки резерв обязательно снять.
- Приёмка глазами: поведение сохранено, поскольку изменены только две
  декларации; дополнительно измерены наблюдаемый вывод и коды завершения.
- Полный гейт и mega-CU локально не запускались. Обычный CI и manual full
  на точном кандидате, финальный combined SHA с #55, посадка и cleanup
  остаются обязательными действиями приёмки, а не следуют из этих проб.

Сырые локальные логи: scratchpad сессии `ses_mv2f16xcda8z5rm89f`,
`ci-failed.log`, `oracle-build.log`, `before.log`, `after.log`,
`after-ready.log`, `sabotage.log`, `restored.log` и пофайловые журналы. Воспроизводящие команды и
содержательные строки приведены выше, чтобы результат не зависел
от сохранности временного рабочего дерева.
