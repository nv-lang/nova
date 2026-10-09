# #54: полная самосборка после #52 — доказательства прогона

Промежуточный отчёт; задача не сдана. Самосборка и ступень 0.2 **не доказаны**.

## РЕПРО

Фактически выполнены два последовательных прогона под `crew_watch machine:true`:

1. `bash "$(cygpath -u "$LOCALAPPDATA")/Temp/opencode/ses_mv0lbkydosyhrllojp/build-oracle.sh"`
   — watch `1791528236128-e5b10l`, `cargo rc=0 duration_seconds=217`.
2. `bash "$(cygpath -u "$LOCALAPPDATA")/Temp/opencode/ses_mv0lbkydosyhrllojp/run-double-build.sh"`
   — watch `1791528562451-9vwb0y`, `double-build rc=1 duration_seconds=63`.

Скрипты команд сохранены рядом с этим отчётом. Свежий cargo release из пустого
приватного target; исходный `scripts/` целиком скопирован и проверен `diff -qr`
(rc=0, пустой вывод). `double-build.sh` и копия имеют SHA256
`15c20ace42515a7619d1449fe8b3086cc2af0021b21967042c1578990dfee102`.
Приёмочная цепочка вызвана как `bash -x <scratchpad>/scripts/tools/double-build.sh`;
алгоритм скрипта не изменён. `DOUBLE_BUILD_A`, `GC_DONT_GC`, `DOUBLE_BUILD_SABOTAGE`
явно unset. `DOUBLE_BUILD_TREE` указывает на измеряемое detached-дерево.

## ТОЧКА

Измеряемый полный HEAD: `02908e5ad35007d82a3b23eb27c319451563807d`.
Дерево: `D:/Sources/nv-lang/worktrees/nova-opencode-54-bootstrap-source`.
Перед cargo и перед double-build: `git status --porcelain` и diff к этому SHA пусты.
Это принятый #52, результат не распространяется на #53 или более новый main.

Первая стадия (`scripts/tools/double-build.sh:147`):

```text
bash <scratchpad>/scripts/tools/with-deadline.sh 900 /d/Sources/nv-lang/worktrees/nova-opencode-54-bootstrap-source/nova-cli/target/release/nova.exe build novac/src/main.nv -o <scratchpad>/target/double-build/A.exe
clang: error: unable to make temporary file: no such file or directory
```

Две строки штатного `target/double-build-verdict.txt` дословно:

```text
не собралась A: the oracle did not build novac/src/main.nv (log /c/Users/Евгений/AppData/Local/Temp/opencode/ses_mv0lbkydosyhrllojp/target/double-build/A.build.log)
02908e5ad (tree /d/Sources/nv-lang/worktrees/nova-opencode-54-bootstrap-source)
```

Классификация наблюдения: ошибка окружения при сборке A, **не** отказ A-check,
не ошибка emit/compile/link B или C, не недетерминизм линковщика и не B≠C.
Конкретная причина невозможности создать temporary file пока не установлена.
Трасса ограничивает длительность вызова A интервалом
`1791528564.832304`–`1791528627.175310` (62.343006 секунды).
Другие стадии не достигнуты; argv/PCH/cflags/link B/C не получены.

## Окружение и артефакты

Scratchpad (далее S):
`C:/Users/Евгений/AppData/Local/Temp/opencode/ses_mv0lbkydosyhrllojp`.

- `TMPDIR=/c/Users/Евгений/AppData/Local/Temp/opencode/ses_mv0lbkydosyhrllojp`.
- TEMP и TMP: Windows-форма того же пути, через `cygpath -w`.
- `CARGO_TARGET_DIR=S/cargo-target` (Windows-форма);
  `source/nova-cli/target` — junction на этот приватный target.
- Оракул: `source/nova-cli/target/release/nova.exe`, SHA256
  `83cf50e498034ac041ec7e63618d3521bd3c92eff1f677d5147db8655f78f1a8`.
- rustc `1.95.0 (59807616e 2026-04-14)`, host `x86_64-pc-windows-msvc`;
  cargo `1.95.0 (f2d3ce0bd 2026-03-21)`.
- clang `22.1.5`, LLVM commit `5ea218a153f4d2f815b8244eab3e4b4ba5e00e6c`,
  `C:/Program Files/LLVM/bin/clang.exe`, target `x86_64-pc-windows-msvc`.
- GC через штатную `novac_borrow_main_gc`:
  `D:/Sources/nv-lang/nova-opencode/target/gc-cache/gc.lib`, SHA256
  `ae979102310ecd9ee03f5c99a75b30681a784e9c49b0abeadae6360bdbfa7915`;
  include `D:/Sources/nv-lang/nova-opencode/target/gc-cache/include`, `gc.h` SHA256
  `b010f80e6770b063e087d5b105a35ee180e08aeadcc7b4d150d24f950a9a82b2`.

Полные логи: `S/cargo-build.log`, `S/environment.txt`, `S/cargo-result.txt`,
`S/oracle.sha256`, `S/double-build-environment.txt`, `S/double-build.log`,
`S/double-build.trace.log`, `S/double-build-result.txt`,
`S/target/double-build/A.build.log`, `S/target/double-build-verdict.txt`.
Логи harness:
`D:/Sources/.opencode-data/opencode/crew-harness/watches/1791528236128-e5b10l.out`,
`D:/Sources/.opencode-data/opencode/crew-harness/watches/1791528562451-9vwb0y.out`.

Дополнительные факты первого прогона:

- Оракул штатно материализовал libuv submodule на
  `1cfa32ff59c076ffb6ed735bbc8c18361558661f` и собрал libuv.lib (37 файлов).
- Собрал runtime archive (14 файлов) в
  `source/target/rt-archive-cache/b9ddddff1d491c9c/libnova_rt.lib`.
  Это побочный выход в detached-дереве вне scratchpad: про необходимость
  перенаправить и этот target интегратору сообщено до следующего запуска.
- При submodule вызове native child не сохранил trace fd:
  `sh: BASH_XTRACEFD: 9: invalid value for trace file descriptor`.
  Это отдельное предупреждение измерительной обёртки, не причина clang-ошибки,
  которая пока не локализована.

## ВЕТКА/КОММИТ и подготовительный блокер

Основная ветка задачи `t54-karina-0-2-svezhaya-polnaya-samosborka-a`:
HEAD `a8cebecd1dd526818f6724c6acd7eda2922a66c8`,
MERGE_HEAD `02908e5ad35007d82a3b23eb27c319451563807d`, конфликтов нет.
Попытка ff-only отвергнута из-за расхождения истории. Обычный merge остановлен
`check-merge-discipline`: сохранённый verdict называл
`20be1409d490bb092d99658a1626cd0dd0c4a9ec`, а вливался `02908e5...`.
История не переписана, hook не обойдён, незавершённый merge оставлен как есть.

Разрешение интегратора 09:41 уточнило бриф: измерять точный принятый SHA в отдельном
detached source; включение базы в историю ветки для этого измерения не требуется.
До разрешения merge отчёт хранить в scratchpad, коммит отчёта согласовать позже.
Первоначальный отказ создания worktree упомянут интегратором; исходного вывода
этого отказа у исполнителя нет, причина не установлена. Фактически основное
дерево существует, зарегистрировано и использовалось без пересоздания.

## ФИКС / ФИКСТУРЫ / САБОТАЖ / std/src

ФИКС: отсутствует; задача измерения, исходники языка/компилятора/std и скрипты
приёмки не изменялись. Кодовая фикстура и проверка класса фикса неприменимы.
Наблюдаемое поведение проверяет штатный double-build; получен его rc и verdict.

САБОТАЖ: **не выполнен** — C не достигнута, C.c и executable отсутствуют.
Два сырых IDENTICAL и красные DIFFER с восстановлением не получены.
Подмена синтетических файлов не выдаётся за саботаж реальных B/C.

std/src: ДО/ПОСЛЕ — не применимо: нет правок компилятора или std.
Spec/D-блок и новый инвариант — не применимо: язык и проверки не менялись.
Реестр: диагностика среды ещё не локализована; вопрос интегратору `qmv0lwo55x8bm`,
решение о дальнейшей пробе и записи новой находки ожидается.
Gate/CI/LANDED: не выполнены, задача не сдана; локальные гейты не запускались.

## ЧТО НЕ СДЕЛАНО

A executable, A-check, prepare argv/PCH, B, C, relink B, сравнения и саботаж не
достигнуты. Полный argv внутреннего clang на остановившейся A оракул не напечатал.
Дальнейшие прогоны или изменение окружения требуют ответа интегратора.
Нет task-коммита/CI кандидата; ступень 0.2 и выпуск не объявляются.

## Продолжение после разрешения интегратора 09:51

Разрешены минимальные clang-пробы путей и повтор полной цепочки с доказанной
рабочей формой временного каталога, сохранением первого прогона; разрешён перенос
собственных артефактов source/target в scratchpad.

Минимальная программа `int main(void) { return 0; }` скомпилирована и слинкована
native clang в 12/12 пробах (rc=0, stderr пуст, каждый executable вернул 0):
унаследованные пути / явно Windows / штатный short-path, по два повтора;
ещё шесть проб с `LC_ALL=C` и штатным `with-deadline.sh`.
Для всех трёх переменных проверены существование и запись в каталог.
Результаты и полные argv: `S/env-probe/results.json`,
`S/env-probe-deadline/results.json`; исходник, exe и выводы сохранены в этих
каталогах, обёртки `S/env-probe.py`, `S/env-probe.sh`.
Watch `1791528787691-j9x44q` (4s) и `1791528842696-bprib0` (3s), оба rc=0.

MSYS преобразует исходный TMPDIR в Win-форму уже при вызове native Python:
унаследованная и явно Windows формы в пробах равны. Поэтому эти успехи не
доказывают причину первого отказа A; Unicode сам по себе также не доказан причиной.
Short-path того же каталога: `C:\Users\B7E3~1\AppData\Local\Temp\opencode\SES_MV~4`.

Перед повтором source/target проверен: обычная директория, не reparse point,
содержит только `.nova-cache`, `libuv-cache`, `rt-archive-cache` первого прогона.
Инвентарь сохранён в `S/source-target-inventory.json`; содержимое перенесено в
`S/source-target`, source/target теперь junction туда. Ничего не удалялось.
После переноса git status source пуст; libuv SHA
`1cfa32ff59c076ffb6ed735bbc8c18361558661f`, GC/libatomic_ops не инициализированы.

Первый W перемещён в `S/attempt-1/double-build`; его verdict, stdout, trace,
environment, result и исходная обёртка скопированы в `S/attempt-1/`.
Ссылки на первый прогон выше теперь читаются с этим префиксом для double-build.

Повтор №2: watch `1791528929720-842apj`, machine:true, тот же SHA и оракул,
TMPDIR=`C:/Users/B7E3~1/AppData/Local/Temp/opencode/SES_MV~4`, TEMP/TMP — short
Windows-форма. Экспорт BASH_XTRACEFD убран; `bash -x` пишет трассу в общий лог.
Результат второго прогона ожидается. Это ещё не приёмка.
