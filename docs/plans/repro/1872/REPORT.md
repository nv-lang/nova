# #52 — отчёт исполнителя и доработка приёмки

## Критерии приёмки (дословно)

1) Замер ДО и ПОСЛЕ дословно: команда самосборки и строка итога clang (число ошибок), плюс разбивка по сообщениям (sort | uniq -c по тексту ошибки). После фикса «no member named 'ctx'» = 0.
2) Минимальная проба класса в docs/plans/repro/1872/ (.nv + выданный C до фикса), фикстура-регрессия в spec_tests/conformance (или e1-smoke Карины), которая краснеет при откате фикса и зеленеет с ним — оба вывода дословно.
3) Поиск класса: где ещё novac эмитирует то же обращение (все пути эмиссии, не один носитель), перечислено в отчёте.
4) Стражи Карины, если тронут novac/src/check: check-novac-diag-schema и check-novac-no-cascade напрямую; check-novac-deferral-address.py и check-hunter-debt.sh на вершине до сдачи.
5) Шаблон оболочки, если затронут, — перегенерирован и check-novac-shell-freshness ok; novac-emission.baseline — подъём/спуск только с объяснением. Вердикт гейта — CI.

## РЕПРО и замер самосборки

Подтверждён повторным запуском при доработке. ДО-бинарь сохранён прежним
исполнителем из `3f4b68a8b`, фиксированный — из исходников
`047f5c80269171d1d56deb38034eaf2c30ec55c3`. Приёмщик независимо подтвердил
эмиссию `_novac_self->nv_ctx` и паннинг через `nv_ctx` фиксированным бинарём.
Измеряется **компиляция C в объектный файл**: линковка и запуск стадии B не входят
в этот замер. Фиксированный A ранее построен командой:

```sh
nova-cli/target/release/nova.exe build novac/src/main.nv -o novac/target/novac.exe
```

Полная воспроизводимая команда повторного замера в Git Bash (из корня дерева):

```sh
ROOT="$PWD"
S="$(cygpath -m "$LOCALAPPDATA")/Temp/opencode/ses_mv09dgcgazjabk2u11"
CACHE="$ROOT/target/t52-ctx/cache"
# Сохранённый кэш подготовлен прежним исполнителем этой командой:
NOVAC_BIN="$ROOT/novac/target/novac.exe" NOVAC_SMOKE_CACHE="$CACHE" \
  sh scripts/tools/novac-e1-smoke.sh --prepare
# Для нового кэша задайте NOVAC_SMOKE_CACHE в scratchpad сессии.
sh docs/plans/repro/1872/measure.sh "$ROOT/target/t52-ctx/novac-before.exe" "$CACHE" "$S/before"
sh docs/plans/repro/1872/measure.sh "$ROOT/novac/target/novac.exe" "$CACHE" "$S/after"
```

`measure.sh` печатает исполняемые команды, проверяет единственность пары
cflags/PCH, снимает только первое включение prelude перед применением PCH.
Ниже его полная дверь эмиссии и clang (переменные определены выше;
`SIDE=before` / `after`, бинарь соответствует стороне):

```sh
NOVAC_SELF_PATH=novac/src "$NOVAC" emit novac/src/main.nv > "$S/$SIDE/self.c" 2> "$S/$SIDE/emit.log"
sed '0,/^#include "nova_rt\/nova_rt.h"$/{//d}' "$S/$SIDE/self.c" > "$S/$SIDE/body.c"
"C:/Program Files/LLVM/bin/clang.exe" \
  --target=x86_64-pc-windows-msvc -O0 -Wno-everything \
  -DNOVA_GC_BOEHM -DGC_THREADS -DNOVA_MAX_EFFECT_STORAGES=10 \
  -I "$ROOT/compiler-codegen" -DNOVA_USE_LIBUV=1 \
  -I "$ROOT/compiler-codegen/nova_rt/libuv/include" \
  -I D:/Sources/nv-lang/nova-opencode/target/gc-cache/include \
  -ferror-limit=0 \
  -include-pch "$CACHE/prelude-1791500697-e10.pch" \
  -c "$S/$SIDE/body.c" -o "$S/$SIDE/self.o" > "$S/$SIDE/clang.log" 2>&1
grep ' error: ' "$S/$SIDE/clang.log" | sed 's/^.*error: //' | sort | uniq -c
```

PCH — результат `novac-e1-smoke.sh --prepare`, файл
`prelude-1791500697-e10.h` содержит ровно `#include "nova_rt/nova_rt.h"`.
Он построен теми же cflags из `cflags-1791500697-e10.argv`:

```sh
REAL_CLANG='C:/Program Files/LLVM/bin/clang.exe'
CFLAGS="$CACHE/cflags-1791500697-e10.argv"
PCH="$CACHE/prelude-1791500697-e10.pch"
eval "\"$REAL_CLANG\" $(tr '\n' ' ' < "$CFLAGS") -x c-header \"$CACHE/prelude-1791500697-e10.h\" -o \"$PCH\""
```

Все cflags перечислены в полной clang-команде выше, скрытых флагов нет.
Для подготовки/смоуков в этом окружении использованы те же настройки, что
у прежнего исполнителя:

```sh
export NOVA_GC_LIB_DIR=D:/Sources/nv-lang/nova-opencode/target/gc-cache
export NOVA_GC_INCLUDE_DIR=D:/Sources/nv-lang/nova-opencode/target/gc-cache/include
export TMPDIR="$S"
```

ДО — дословный результат повторного замера, без свёртки имён и без top-N:

```text
CLANG_RC=1
CLANG_ERRORS_TOTAL=1104
RAW_SORT_UNIQ_BEGIN
    774 no member named 'ctx' in 'struct Nova_Checker'
    273 no member named 'ctx' in 'struct Nova_Emitter'
     51 no member named 'ctx' in 'struct Nova_Lowerer'
      6 use of undeclared identifier 'ctx'
RAW_SORT_UNIQ_END
1104 errors generated.
```

ПОСЛЕ — дословно (clang при успехе не печатает `0 errors generated.`, поэтому
ноль — счётчик `grep -c 'error:'`, вместе с реальным кодом возврата):

```text
CLANG_RC=0
CLANG_ERRORS_TOTAL=0
RAW_SORT_UNIQ_BEGIN
RAW_SORT_UNIQ_END
```

| Класс | ДО | ПОСЛЕ | Пример |
|---|---:|---:|---|
| SelfField: сырое имя при экранированном объявлении поля | 1098 | 0 | `no member named 'ctx' in 'struct Nova_Checker'` |
| Паннинг: сырое чтение экранированного локала | 6 | 0 | `use of undeclared identifier 'ctx'` |
| Остаточные классы clang | — | 0 | отсутствуют |

Новые остаточные классы не обнаружены; строк №TBD для них нет.
Минимальная проба: `self_field_ctx.nv`, ДО-эмиссия:
`self_field_ctx.before.c.txt` (23136 строк). Репро-команда `cmd.sh` исправлена:
четыре подъёма от `docs/plans/repro/1872/` до корня. Запущена из scratchpad,
вне репозитория, результат дословно:

```text
novac-e1-smoke ok: /d/Sources/nv-lang/worktrees/nova-opencode-52-karina-samosborka-klass-no-member-named/docs/plans/repro/1872/self_field_ctx.nv — поведение идентично оракулу (stdout байт-в-байт, exit 0; оракул собран)
```

Дополнительно сам `cmd.sh` запущен из корня дерева:

```sh
sh docs/plans/repro/1872/cmd.sh
echo CMD_ROOT_RC=$?
```

```text
novac-e1-smoke ok: /d/Sources/nv-lang/worktrees/nova-opencode-52-karina-samosborka-klass-no-member-named/docs/plans/repro/1872/self_field_ctx.nv — поведение идентично оракулу (stdout байт-в-байт, exit 0; оракул из кэша)
CMD_ROOT_RC=0
```

## ТОЧКА, ФИКС и поиск класса

Три пути одной асимметрии объявление/использование проведены через существующий
`c_ident`, а не через специальную замену `ctx`:

1. `novac/src/emit_c/emit_expr.nv:464`: `SelfField` (`@ctx`). Объявления полей
   уже экранируются в `emit_decls.nv:66`, `emit_instance_structs.nv:61`.
2. `novac/src/emit_c/emit_c.nv:840`: значение паннинга `Box { ctx }`.
   Объявления локалов/параметров уже экранируются (`ir.nv:583`, `emit_c.nv:542`).
3. `novac/src/emit_c/emit_handler.nv:122`: имя op-параметра хендлер-литерала.
   Чтение имени в теле уже экранируется. Носителя в самосборке нет; `pos_3`
   специально называет параметр `ctx` и вызывает эту операцию.

Приёмка глазами, потому что полнота путей эмиссии не доказывается одним
носителем: прежний исполнитель и приёмщик просмотрели Name (`emit_expr:433`),
обычный FieldAccess (`:529`), объявления полей, параметры, оба плеча паннинга
(`emit_c:826/840`), `emit_place:241/575`, захваты `emit_handler`/`emit_spawn`,
`@named` в `ir:583`. Эти пути уже используют дверь. `c_ident` охватывает
`ctx`/`schedlink`, C-ключевые слова, макросы и зарезервированные префиксы.

Сырые имена с **обеих** сторон, согласованные между собой, не принадлежат
найденной асимметрии: op-vtable (`emit_handler:242`) и **variant payload**
(`emit_decls:106/129`, `emit_place:292`). Их прежнее общее название
«vtable-члены» исправлено также в README и строке №1872 реестра.

## ФИКСТУРЫ и САБОТАЖ

`novac/fixtures/carina_c_names/pos_2.nv`: вызов `b.bump()` меняет `@ctx.n`
с 41 на 42; вывод метода и контрольное чтение `b.ctx.n` дают `42\n42\n`.
Паннинг создаёт реально используемый объект. `pos_3.nv` вызывает
`ResourceTrace.on_resource_enter("hi")`, тело op печатает параметр `ctx`.
Смоук сравнивает stdout с оракулом байт-в-байт и требует exit 0.
Соседняя `pos_1` зелёная в исходном прогоне и независимо у приёмщика.

Команды повторного саботажа и восстановления (для `f=pos_2`, затем `pos_3`):

```sh
export NOVAC_SMOKE_CACHE="$CACHE"
NOVAC_BIN="$ROOT/target/t52-ctx/novac-before.exe" \
  sh scripts/tools/novac-e1-smoke.sh "novac/fixtures/carina_c_names/$f.nv"
NOVAC_BIN="$ROOT/novac/target/novac.exe" \
  sh scripts/tools/novac-e1-smoke.sh "novac/fixtures/carina_c_names/$f.nv"
```

ДО-бинарь — все три исправления отсутствуют; оба смоука красные. Дословные
строки повторного запуска:

```text
novac-e1-smoke: FAIL — clang -c упал: C:/Users/Евгений/AppData/Local/Temp/opencode/ses_mv09dgcgazjabk2u11/novac-smoke.254562/body.c:18186:20: error: no member named 'ctx' in 'struct Nova_Box'
 18186 |     ((_novac_self->ctx)->n) = nova_int_checked_add(((_novac_self->ctx)->n), ((nova_int)1LL));
       |       ~~~~~~~~~~~  ^
C:/Users/Евгений/AppData/Local/Temp/opencode/ses_mv09dgcgazjabk2u11/novac-smoke.254562/body.c:18186:67: error: no member named 'ctx' in 'struct Nova_Box'
novac-e1-smoke: FAIL — clang -c упал: C:/Users/Евгений/AppData/Local/Temp/opencode/ses_mv09dgcgazjabk2u11/novac-smoke.254664/body.c:18197:20: error: use of undeclared identifier 'nv_ctx'
 18197 |     nova_print_str(nv_ctx);
       |                    ^~~~~~
1 error generated.
```

Фиксированный бинарь — обе зелёные, дословно:

```text
novac-e1-smoke ok: novac/fixtures/carina_c_names/pos_2.nv — поведение идентично оракулу (stdout байт-в-байт, exit 0; оракул из кэша)
novac-e1-smoke ok: novac/fixtures/carina_c_names/pos_3.nv — поведение идентично оракулу (stdout байт-в-байт, exit 0; оракул из кэша)
GREEN_RC=0
```

Перезапуск сервера прервал оболочку перед зелёным `pos_3`; этот последний шаг
выполнен отдельно после возобновления. Красные и остальные зелёные логи
сохранились; приёмщик также ранее независимо получил red rc=1 / green rc=0.

## std/src

Пропущенный исходным исполнителем контроль восстановлен при доработке:
два запуска `nova-cli/target/release/nova.exe check std/src`.
Это **не исторические логи** до внесения фикса. Для ДО использованы тождественные
входы проверки: `git diff 3f4b68a8b HEAD -- compiler-codegen nova-cli std`
пуст; дополнительная проверка включает конфиг команды:
`git diff 3f4b68a8b HEAD -- compiler-codegen nova-cli std nova.toml nova.lock.toml`
также пуста. Rust-оракул не читает исправленные `.nv` исходники эмиттера novac.
Оба запуска выполнены подряд в одной оболочке, из одного cwd и с тем же
окружением, без пересборки или замены Rust-бинаря между ними. SHA-256
`nova-cli/target/release/nova.exe`:
`ecd7fbff4caa6b1ae3b0acb76d7496eb17c416a1ff4759eab992434141d0fe39`.
Физический откат этих трёх файлов для `nova check std/src` не требуется.
Обе итоговые строки (сняты только управляющие ANSI-коды цвета):

```text
ДО:
PASS: 158  FAIL: 26  WARN: 67
STD_BEFORE_RC=1
ПОСЛЕ:
PASS: 158  FAIL: 26  WARN: 67
STD_AFTER_RC=1
```

Контроль не зелёный, но одинаковый: 26 отказов существующего Rust-чекера std.
Эти отказы не являются остаточными ошибками clang самосборки.

## Остальные шаги приёмки

- **registry:** №1872 в `docs/plans/221.1-bug-sweep.md` входит в тот же набор
  изменений; номер назначен интегратором. Существующие тесты не ослаблены и
  не удалены. Новых `E_*`/`W_*` нет.
- **spec:** не применимо — язык не меняется, исправляется согласованность
  печати C-имён с уже действующей дверью `c_ident`.
- **invariant:** новый инвариант/страж не вводится. Восстановлен существующий:
  объявление и использование имени должны иметь одинаковый C spelling;
  без этого clang отвергает допустимую программу.
- **check:** `novac/src/check` не изменён, условные проверки diag-schema и
  no-cascade не применимы.
- **shell/baseline:** шаблон и `novac-emission.baseline` не менялись.
  Исходный исполнитель получил дословно:

```text
check-novac-emission-size ok: файлов 4, объём эмиссии совпадает с базой
check-novac-shell-freshness ok: шаблон == эмиссия оракула по probe (23107 строк, 1350045 байт)
```

Обязательные стражи повторно запущены при доработке:

```sh
python scripts/guards/check-novac-deferral-address.py .
sh scripts/guards/check-hunter-debt.sh .
git diff --check
```

```text
check-novac-deferral-address ok: файлов .nv 214, отсылок ответственности 112, из них без адреса 34 (база 34) — храповик вниз, цель 0
check-hunter-debt ok: novac: долг 718/1500 (отчёт); guards: долг 1135/3000 (отчёт); spec: долг 513/2000 (отчёт); часы — из git, не из рук, и не воскрешаются
```

## ВЕТКА/КОММИТ и ЧТО НЕ СДЕЛАНО

Ветка `t52-karina-samosborka-klass-no-member-named`.
Фикс: `047f5c80269171d1d56deb38034eaf2c30ec55c3`.
Доработка доказательств — следующий коммит этой ветки, содержащий этот отчёт.
Коммиты на английском, с Signed-off-by, без Co-Authored-By; добавление по
именам, коммит с `--only -- <пути>`.

**gate/ci:** ожидают приёмщика: CI на свежем `integrate/t52`, проверка
`check-push-proven-by-ci.py`, затем `LANDED task=#52 main=…` от `land-task.sh`
под замком. Исполнитель локальные gate.sh/gate-novac.sh, мега-CU и полный
nova test не запускал. Этот отчёт не заявляет зелёный CI или факт вливания.
Rust-оракул, std/src/os и задачи плана 294 не изменялись.
