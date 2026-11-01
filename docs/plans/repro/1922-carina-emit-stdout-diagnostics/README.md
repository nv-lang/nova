# Улики №1922 — драйвер терял stdout-диагностику pinned Carina B1 emit (задача 127)

Дата: 2026-10-30. Основа: main `aaf59cb76` (свежий origin/main на момент работы).

## РЕПРО (носитель: `examples/getting_started.nv`)

Прямой запрос к pinned-сиду (переменные как у драйвера):

```
set NOVA_STD_PATH=<repo>\std\src
set NOVAC_SELF_PATH=<repo>\novac\src
novac.exe emit <repo>\examples\getting_started.nv > seed-stdout.bin 2> seed-stderr.bin
```

Результат: `rc=1`, `stdout 1052 байта` (JSONL-диагностика `E_NOVAC_SUBSET`),
`stderr 0 байт` — файлы `seed-stdout-getting_started.txt`,
`seed-stderr-getting_started.bin` (пустой).

Штатный драйвер (собран `cargo build --release` в `nova-driver/`):

```
nova-driver\target\release\nova.exe build examples\getting_started.nv
```

* `driver-before-main.txt` — состояние main ДО задачи 127: задача 120 уже
  закрыла половину класса (stderr пуст → берётся stdout), поэтому на этом
  носителе (stdout-only) сообщение уже несёт JSON-диагностику.
* Оставшаяся дыра класса (её чинит 127): при НЕПУСТОМ stderr stdout-диагностика
  сида отбрасывалась — см. юнит-тесты `emit_failure_detail_*` в
  `nova-driver/src/carina_build.rs` (5 штук: stdout-only / stderr-only /
  смешанный без дублирования / идентичные потоки / оба пустые).

## САБОТАЖ (доказательство в обе стороны)

Хелпер `emit_failure_detail` временно сведён к `stderr-only` (исходный дефект):

* юнит: `test result: FAILED. 16 passed; 2 failed` —
  падают `emit_failure_detail_shows_stdout_jsonl_when_stderr_is_blank` и
  `emit_failure_detail_keeps_both_streams_without_duplication`;
* интеграция: `sabotage-red.txt` — `nova: Carina B1 emit failed (exit 1): `
  с ПУСТЫМ хвостом (точная подпись №1922 из замера задачи 119).

Возврат фикса: `unit-tests-green.txt` — `test result: ok. 18 passed; 0 failed`;
`driver-after-fix.txt` — `nova: Carina B1 emit failed (exit 1): {"id":"d0","code":"E_NOVAC_SUBSET",...}`.

## Что НЕ являлось носителем

`nova-cli/src/carina_build.rs` несёт текстually ту же stderr-only форму, но это
мёртвый код: модуль объявлен с `#[allow(dead_code)]` («superseded by
cmd_build_oracle (task 092)»), вызывающих в `nova-cli` нет (grep
`carina_build::` — только `nova-driver/src/main.rs:112`).
