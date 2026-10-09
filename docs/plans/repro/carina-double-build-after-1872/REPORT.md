<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# #54 — свежая самосборка Carina после №1872

**Результат: построена и запущена настоящая B; цепочка остановилась на компиляции
C, `clang -c` выдал 83 ошибки. C executable нет. B≡C и ступень 0.2 не доказаны.**
Находка зарегистрирована как №1875; задача измерения не содержит ремонта.

## РЕПРО: база, источник и команда

Измерен точный принятый #52: **`02908e5ad35007d82a3b23eb27c319451563807d`**.
Исходники — собственное detached-дерево
`D:/Sources/nv-lang/worktrees/nova-opencode-54-bootstrap-source` (далее TREE).
До/после прогона tracked delta к этому SHA отсутствует:
[source-status.txt](evidence/source-status.txt). Результат не переносится на #53
или будущий main. На момент финальной проверки origin/main всё ещё `02908e5...`.

Scratchpad S:
`C:/Users/Евгений/AppData/Local/Temp/opencode/ses_mv0lbkydosyhrllojp`.
W = `S/target/double-build`. Cargo target = `S/cargo-target`;
`TREE/nova-cli/target` — junction туда. `TREE/target` после первого прогона
перенесён в `S/source-target` с сохранением содержимого и заменён junction.
Исходники detached-дерева не правились. Штатная сборка инициализировала libuv
на `1cfa32ff59c076ffb6ed735bbc8c18361558661f`; источники подмодуля не менялись.

`scripts/` точно скопирован в S, идентичность всего каталога проверена `diff -qr`
перед обоими прогонами. SHA256 исходного и скопированного `double-build.sh`:
`15c20ace42515a7619d1449fe8b3086cc2af0021b21967042c1578990dfee102`.
Копия сохраняет относительные зависимости, ROOT и W оказываются в scratchpad;
`DOUBLE_BUILD_TREE` указывает на TREE. Штатный `--prepare` вызывается из TREE.

Все сборки последовательны через `crew_watch machine:true`:

| запуск | watch | rc / время |
|---|---|---|
| свежий release Rust-оракул | `1791528236128-e5b10l` | 0 / 217 s |
| штатная цепочка, исходные TEMP-пути | `1791528562451-9vwb0y` | 1 / 63 s, окружение A |
| минимальные clang-пробы | `1791528787691-j9x44q` | 0 / 4 s |
| пробы с LC_ALL=C и deadline | `1791528842696-bprib0` | 0 / 3 s |
| штатная цепочка, short TEMP-пути | `1791528929720-842apj` | 1 / 83 s, clang C |

Команды watch:

```bash
bash "$(cygpath -u "$LOCALAPPDATA")/Temp/opencode/ses_mv0lbkydosyhrllojp/build-oracle.sh"
bash "$(cygpath -u "$LOCALAPPDATA")/Temp/opencode/ses_mv0lbkydosyhrllojp/run-double-build.sh"
```

Полные обёртки: [build-oracle.sh](evidence/build-oracle.sh),
[run-double-build.sh](evidence/run-double-build.sh). Первая выполняет
`cargo build --release --manifest-path "$TREE/nova-cli/Cargo.toml"` в свежий
приватный target; вторая — неизменённый `bash -x "$S/scripts/tools/double-build.sh"`.
`DOUBLE_BUILD_A`, `GC_DONT_GC`, `DOUBLE_BUILD_SABOTAGE` unset.
`NOVAC_SELF_PATH=novac/src` выставлен штатным скриптом при emit B и C.

## Окружение

- Rust `1.95.0 (59807616e 2026-04-14)`, cargo `1.95.0 (f2d3ce0bd 2026-03-21)`,
  host `x86_64-pc-windows-msvc`; [environment.txt](evidence/environment.txt).
- Свежий оракул `TREE/nova-cli/target/release/nova.exe`, SHA256
  `83cf50e498034ac041ec7e63618d3521bd3c92eff1f677d5147db8655f78f1a8`.
- Clang `22.1.5`, LLVM `5ea218a153f4d2f815b8244eab3e4b4ba5e00e6c`,
  `C:/Program Files/LLVM/bin/clang.exe`, target `x86_64-pc-windows-msvc`.
- GC — штатная `novac_borrow_main_gc`: library
  `D:/Sources/nv-lang/nova-opencode/target/gc-cache/gc.lib`, SHA256
  `ae979102310ecd9ee03f5c99a75b30681a784e9c49b0abeadae6360bdbfa7915`;
  headers `D:/Sources/nv-lang/nova-opencode/target/gc-cache/include`, gc.h SHA256
  `b010f80e6770b063e087d5b105a35ee180e08aeadcc7b4d150d24f950a9a82b2`.
- Во втором прогоне TMPDIR=`C:/Users/B7E3~1/AppData/Local/Temp/opencode/SES_MV~4`,
  TEMP/TMP=`C:\Users\B7E3~1\AppData\Local\Temp\opencode\SES_MV~4` — штатный
  short-path **того же S**, проверены существование и запись.
  [Полные значения](evidence/double-build-environment.txt).

Первый запуск остановился до A-check с
`clang: error: unable to make temporary file: no such file or directory`.
Его W и все доказательства сохранены в `S/attempt-1/`; краткие копии:
[A.build.log](evidence/attempt-1-A.build.log),
[обе строки verdict](evidence/attempt-1-verdict.txt).
Минимальные native clang-пробы дали 12/12 успешных compile/link/run, rc=0 и пустой
stderr: исходная, Windows и short формы × два повтора, затем то же с LC_ALL=C и
штатным deadline. [JSON с argv и native env](evidence/env-probes.json).
MSYS уже преобразует исходный TMPDIR в Windows-форму при native вызове.
**Конкретная причина первого отказа не доказана; Unicode причиной не объявлен.**
Во втором прогоне также убран экспорт BASH_XTRACEFD (native child терял fd),
а runtime/libuv cache сохранился; это не контролируемый A/B одной переменной.

## ТОЧКА: стадии второго прогона

| стадия | результат | время |
|---|---|---|
| nova.exe → A.exe | построена, 4 387 328 bytes | 8 s, штатная строка |
| A check собственных исходников | rc=0, accepted **214/214**, без fallback | 16.54 s |
| prepare argv/PCH | пройден | около 9.42 s |
| A emit → B.c | rc=0, 13 249 423 bytes | 26 s, штатная строка |
| clang -c B | пройден, B.o 4 272 320 bytes | около 2.66 s |
| link B | пройден, executable 3 876 352 bytes | около 0.31 s |
| B emit → C.c | rc=0, 13 210 676 bytes | 15 s, штатная строка |
| clang -c C | отказ, **83 errors**, нет C.o | около 2.33 s |
| link C / relink B / сравнения | не достигнуты | — |

Дробные времена — интервалы timestamp трассы до и после команды (включают
минимальный overhead оболочки), не отдельный benchmark. Общий rc=1, 83 s.
[Дословные stage-строки](evidence/stage-output.txt).

Две строки штатного verdict:

```text
не собралась C (clang -c failed: 83 errors; /c/Users/Евгений/AppData/Local/Temp/opencode/ses_mv0lbkydosyhrllojp/target/double-build/C.cc.log)
02908e5ad (tree /d/Sources/nv-lang/worktrees/nova-opencode-54-bootstrap-source)
```

Это **ошибка C-компиляции эмиссии настоящей B**, а не ошибка линковки B,
не отказ A-check и не результат штатного сравнения B≠C.
Скрипт остановился в `scripts/tools/double-build.sh:285–290`.

### Точные clang/PCH и link argv

Сохранены без изменений:
[cflags](evidence/cflags-1791528608-e11.argv),
[link](evidence/link-1791528608-e11.argv).
PCH: `W/cache/prelude-1791528608-e11.pch`, 18 074 724 bytes, SHA256
`37bd1905928bd456c1b743514d1ca8f934586d4a3a5c37ead469e4cb1e806425`.
Обе компиляции B/C используют один PCH и один argv. Штатный compile C:

```bash
S="$(cygpath -u "$LOCALAPPDATA")/Temp/opencode/ses_mv0lbkydosyhrllojp"
W="$S/target/double-build"
PCH="$W/cache/prelude-1791528608-e11.pch"
REAL_CLANG='C:/Program Files/LLVM/bin/clang.exe'
CFLAGS="$W/cache/cflags-1791528608-e11.argv"
LINKCMD="$W/cache/link-1791528608-e11.argv"
eval "\"$REAL_CLANG\" $(tr '\n' ' ' < "$CFLAGS") -ferror-limit=0 -include-pch \"$PCH\" -c \"$W/C.body.c\" -o \"$W/C.o\""
# Штатная успешно выполненная линковка B:
eval "\"$REAL_CLANG\" $(tr '\n' ' ' < "$LINKCMD") -o \"$W/B/novac.exe\" \"$W/B.o\""
```

Компиляционные флаги: `--target=x86_64-pc-windows-msvc -O0 -Wno-everything
-DNOVA_GC_BOEHM -DGC_THREADS -DNOVA_MAX_EFFECT_STORAGES=11 -DNOVA_USE_LIBUV=1`,
include TREE/compiler-codegen, libuv/include и GC include. Полный link argv несёт
runtime/libuv archives, системные библиотеки, `-fuse-ld=lld -Wl,-Brepro`.
Последний флаг добавлен самим неизменённым double-build; его детерминизм этим
прогоном **не проверен**, стадия повторной линковки не достигнута.

### Наблюдения №1875 и границы диагноза

Полный [C.cc.log](evidence/C.cc.log),
[сырая гистограмма точных диагнозов](evidence/raw-diagnostics.json),
[группировка формы сообщений](evidence/error-classes.json).
Группировка: 43 несовместимых присваивания, 31 несовместимая инициализация,
9 undeclared identifier. Это **три формы сообщений, не доказанные три корня**;
83 ошибки не объявляются одним исправленным/локализованным классом.

Первые носители (строки именно C.body.c):

```text
130586:63: assigning to 'Nova_novac_lex_Token *' ... from incompatible type 'NovaValue_BuiltinType'
131268:21: initializing 'NovaValue_TyRow' ... with an expression of incompatible type 'Nova_novac_lex_Token *'
131276:63: assigning to 'Nova_novac_lex_Token *' ... from incompatible type 'NovaValue_TyRow'
```

В C.body.c разные Vec-инстансы получают `struct Nova_Vec____` с разными полями
`data`; BuiltinType push использует это общее имя вместо специализированного.
В соответствующем B.body.c имя специализировано. Срезы с номерами строк для
всех найденных деклараций и первого потребителя сохранены в
[carrier-excerpts.txt](evidence/carrier-excerpts.txt).
Это наблюдаемая потеря различимости C-представлений между поколениями;
точка исходной мискомпиляции, связь undeclared identifier с этой потерей и
влияние прочих факторов требуют отдельного расследования. Гипотеза не выдана
за локализованный корень. Приоритет №1875 — К1 для лестницы Carina, громкий
отказ самосборки; поставляемый Rust-компилятор этим замером не признан сломанным.
Фикс одного BuiltinType-потребителя не будет приёмкой класса.

## ФИКС / ФИКСТУРЫ / САБОТАЖ / std/src

- **ФИКС:** отсутствует, только измерение и реестр; код и инструменты не менялись.
- **ФИКСТУРЫ:** отдельная кодовая фикстура неприменима; наблюдение проверяет
  штатный double-build на полном novac/src. B действительно вызвана для emit C,
  а не только существует как файл.
- **САБОТАЖ:** критерий **не выполнен**: нет executable C. Ни два сырых IDENTICAL,
  ни пара DIFFER→IDENTICAL на текстовой и executable scratch-копиях не получены.
  C.c существует, но подмена одного текстового сравнения не выдана за всю приёмку.
- **std/src:** ДО/ПОСЛЕ — не применимо: нет правок компилятора/std.
- **class:** ремонта нет. Просмотрены полная гистограмма, разные Vec-поля/потребители
  и B/C-эмиссия; это передача симптомов, не закрытие класса.
- **spec/invariant:** не применимо — язык и инварианты проверок не менялись.
- **registry:** №1875 выделен интегратором и добавлен в 221.1 тем же docs-коммитом.
  Тесты не ослаблены/удалены, новых E_*/W_* нет.
- **gate/ci:** локальные гейты не запускались. CI чистого кандидата, проверка
  `check-push-proven-by-ci.py` и строка `LANDED task=#54 main=…` — работа приёмщика;
  до неё эти шаги не заявлены выполненными.

## Критерии задачи дословно и статус

1. «Свежий release Rust-оракул из измеряемого дерева, свой target, записаны полный SHA, путь, GC/header/library и clang/PCH/cflags/link argv.» — выполнено, ссылки выше.
2. «Штатный scripts/tools/double-build.sh, без DOUBLE_BUILD_A и без GC_DONT_GC для приёмочного прогона; NOVAC_SELF_PATH=novac/src и вся цепочка. Все cargo build и double-build через crew_watch machine:true, без параллельного тяжёлого прогона.» — штатный алгоритм запущен, остановился на C compile; обойти остановку не пытались.
3. «Успех только при настоящих B и C executable, штатной проверке детерминизма повторной линковки B и двух сырых побайтных IDENTICAL (B.c/C.c и B/C); приложить rc и обе строки double-build-verdict, все стадии/времена. Нормализация/удаление различий не разрешены.» — условия успеха не достигнуты; rc/verdict/стадии приведены, нормализации нет.
4. «Доказать красноту проверки сравнения: на scratch-копиях изменить байт C-текста и executable, штатный --compare должен дать DIFFER/ненулевой rc и после восстановления IDENTICAL. Если цепочка не дошла до C, явно назвать этот критерий невыполненным; не объявлять задачу успешной самосборкой.» — не выполнен, нет executable C; успешная самосборка не объявляется.
5. «При остановке — полный точный вывод/команда/окружение, отличить ошибку среды, A-check, emit/compile/link B, C, недетерминизм linker или B≠C; новая находка в 221.1 №TBD, номер у интегратора. НЕ чинить её в этой задаче.» — выполнено, итоговая остановка C compile, №1875 выдан интегратором. Первый env-отказ сохранён отдельно; его корень не доказан.
6. «Полный отчёт и компактные доказательства в docs/plans/repro/carina-double-build-after-1872/REPORT.md с адресами полных логов; бинарные артефакты не коммитить, гигантские C не добавлять без необходимости. Ступень 0.2 не объявлять без CI того же измеренного commit и остальных её условий.» — отчёт и компактные текстовые доказательства здесь; бинарники/PCH/полные C остаются в S. Ступень 0.2 не объявлена.

Приёмка глазами, потому что разграничение доказанного симптома и неизвестного
корня, а также отсутствие заявления релиза требуют чтения отчёта; rc, SHA,
байтовые размеры и диагностические строки проверяемы по приложенным файлам.

Точечные проверки docs/реестра (локальные gate не запускались):

```text
check-registry-entry-shape ok: роста неоформленных записей нет (343 <= 343, номера сверены поимённо), разобрано строк 1774 >= 1142
check-registry-single-verdict ok: у каждой строки реестра ровно один действующий вердикт, статус и маршрут; вердиктов без статуса 19 при базе 19 (номера сверены поимённо), разобрано строк 1774 >= 1142
check-registry-routes ok: открытых блокеров тега 108 (база 109), K1 без маршрута 0 (база 0), без оговорки 0 (база 0)
```

Все три rc=0; `git diff --check` — rc=0, пустой вывод.

## ВЕТКА/КОММИТ и доставка

Ветка `t54-karina-0-2-svezhaya-polnaya-samosborka-a` первоначально имела HEAD
`a8cebecd1dd526818f6724c6acd7eda2922a66c8` (унаследованный chore).
Подготовительный merge принятой базы остановил `check-merge-discipline`:
старый verdict был на `20be1409d490bb092d99658a1626cd0dd0c4a9ec`.
Hook не обходился, локальный gate и подделка verdict не применялись.
Интегратор в 09:41 разрешил detached-измерение без включения базы в историю.
В 09:58 разрешил отменить только этот подготовительный merge после снимка:
в `S/merge-snapshot/` сохранены status, HEAD/MERGE_HEAD, index и binary diff;
проверено отсутствие unstaged/untracked/conflicted файлов, затем `merge --abort`.
После abort дерево чистое; далее добавлены только этот отчёт, evidence и №1875.
Task-коммит — docs-only; SHA указан в письме сдачи, чтобы не самоссылаться в коммите.
Английский message, DCO, именованный index и `--only`, без Co-Authored-By.

Приёмщик строит чистый кандидат от актуального origin/main и проверяет отсутствие
code delta. Унаследованные chore/handoff изменения не являются результатом #54.
Если main продвинется, доказательство остаётся на exact `02908e5...`; нового
утверждения о новом main без отдельного повторного измерения нет.

## Полные логи и ЧТО НЕ СДЕЛАНО

В S: `cargo-build.log`, `double-build.log` (включая timestamp trace),
`double-build-environment.txt`, `double-build-result.txt`, `target/double-build/*`,
`attempt-1/*`, `env-probe*/results.json`, `merge-snapshot/*`.
Полные watch-логи:
`D:/Sources/.opencode-data/opencode/crew-harness/watches/<watch-id>.out`,
где все пять watch-id перечислены в таблице выше.
[Размеры и SHA256 всех поколений](evidence/artifacts.json) связывают сохранённые
бинарники/C/PCH с измерением; отсутствие C.o и executable C записано явно.
Полные C и бинарники не коммитятся. Компактные логи с командами и диагнозами —
в `evidence/`; история первого отказа — [attempt-1-history.md](evidence/attempt-1-history.md).

Не выполнены: C executable, повторная линковка B, оба штатных сравнения,
саботаж сравнения, CI кандидата и остальные условия ступени 0.2.
Не исправлялись №1875 и неизвестная причина первого env-отказа.
Не выпускались tag/bootstrap-бинарь; локальные gate/mega-CU/full nova test не запускались.
