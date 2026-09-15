<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Охота 2026-09-16 — `novac` × модуль `emit_c` × класс К7 (полуготовый механизм)

Отчёт охотника; **ничего здесь не закрыто и не заведено в реестр** — строки
реестра заводит окно, с меткой `НАЙДЕНО ОХОТНИКОМ 2026-09-16 (novac)`.

Бинари на момент замера: `novac/target/novac.exe` (сборка 2026-09-15 18:15),
оракул `nova-cli/target/release/nova.exe`. Все команды — **из корня дерева**.

Файлов в `novac/src/emit_c/` всего **12** (11 `.nv` + `shell.tpl.c`).
Читал СОДЕРЖАТЕЛЬНО — **6**: `emit_c.nv`, `emit_place.nv`, `emit_flow.nv`,
`emit_expr.nv`, `emit_match.nv`, `shell.nv`. Только грепом — **5**: `emit_decls.nv`,
`emit_interp.nv`, `emit_requires.nv`, `emit_destructure.nv`, `emit_option.nv`.
`shell.tpl.c` (327 КБ, C-шаблон оболочки) — грепом.

## Находки

| проба | предмет | имена для грепа |
|---|---|---|
| [`f1-nested-coalesce-not-lowered.md`](f1-nested-coalesce-not-lowered.md) | `??` понижается только как ЦЕЛОЕ значение; вложенный не понижается никем — 8 синтаксисов, 2 разных ICE, корпус пуст | `lower_value_source`, `hoist_array_lits`, `lower_coalesce`, `NodeKind.Coalesce`, `a block sealed twice`, `the hoist never built` |
| [`f2-println-name-vs-channel.md`](f2-println-name-vs-channel.md) | перехват `println` по ГОЛОМУ ТЕКСТУ имени и только в позиции оператора; в позиции значения тот же вызов идёт через канал — две сущности в одной программе | `is_println_call`, `PRINT_FN`, `call_callee_text`, `lower_eval`, `type_free_call`, `def_of` |
| [`f3-printer-gate-literal-half.md`](f3-printer-gate-literal-half.md) | ворота `@has_printer` стоят на ветви выражения и отсутствуют на ветви литерала того же цикла; `[INV-PROPERTY]` утверждает обратное | `has_printer`, `is_printable_carrier`, `printer_of`, `INV-PROPERTY`, `judge_interp_lit` |
| [`f4-bridge-has-no-deadline.md`](f4-bridge-has-no-deadline.md) | самоистечение временного есть для РЁБЕР §3 и нет для ФОРМЫ: мост M1 срока не несёт, ширину его никто не считает; строка карты «только пониженная форма» неверна | `Operand.Tree`, `Rvalue.Expr`, `M1 bridge`, `until:`, `check-novac-temp-edges`, `check-novac-lowering-one-door`, `check-novac-edge-payload` |

## Каталоги проб

Каждая `.nv`-проба лежит в СВОЁМ каталоге с собственным `cmd.sh`: `novac check`
тянет каталог как модуль, и две пробы рядом мерили бы свою сумму.

```
f1-nested-coalesce-not-lowered/
    c1-binary/           c2-if-cond/      c3-array-elem/   c4-interp-slot/
    c5-return/           c6-tail/         c7-callarg/      c8-bind/
    control-toplevel/            КОНТРОЛЬ: тот же `??` целым аргументом -> зелёный
    control-ifvalue-refused/     ВТОРОЙ КОНТРОЛЬ: сестринская форма, отказ ДЕРЖИТ ЧЕКЕР
f2-println-name-vs-channel/
    user-declared/       nested-one-line/
f3-printer-gate-literal-half/
    lit-char/            expr-char/       КОНТРОЛЬ: тот же тип через вторую ветвь
```

## Сводный замер (воспроизводится одной командой ниже)

| проба | `novac check` | `novac emit` | оракул `check` |
|---|---|---|---|
| `f1/c1-binary` | 0 | **2 (ICE)** | 0 |
| `f1/c2-if-cond` | 0 | **2 (ICE)** | 0 |
| `f1/c3-array-elem` | 0 | **2 (ICE)** | 0 |
| `f1/c4-interp-slot` | 0 | **2 (ICE)** | 0 |
| `f1/c5-return` | 0 | **2 (ICE)** | 0 |
| `f1/c6-tail` | 0 | **2 (ICE)** | 0 |
| `f1/c7-callarg` | 0 | **2 (ICE)** | 0 |
| `f1/c8-bind` | 0 | **2 (ICE)** | 0 |
| `f1/control-toplevel` | 0 | 0 | 0 |
| `f1/control-ifvalue-refused` | 1 (чистый отказ) | — | 0 |
| `f2/user-declared` | 0 | 0 | **1 (оракул отверг)** |
| `f2/nested-one-line` | 0 | 0 | **1 (оракул отверг)** |
| `f3/lit-char` | 0 | **2 (ICE)** | 0 |
| `f3/expr-char` | 1 (чистый отказ) | — | 0 |

Прогнать всё разом, из корня дерева:

```sh
B=docs/dev/hunts/novac/probes/2026-09-16-emitc-k7
for d in $(find "$B" -name m.nv | sort); do
    p=$(dirname "$d")
    NC=$(./novac/target/novac.exe check "$d" >/dev/null 2>&1; echo $?)
    EM=$(./novac/target/novac.exe emit  "$d" >/dev/null 2>&1; echo $?)
    OR=$(./nova-cli/target/release/nova.exe check "$d" >/dev/null 2>&1; echo $?)
    printf '%-62s check=%s emit=%s oracle=%s\n' "${p#$B/}" "$NC" "$EM" "$OR"
done
```

## Чего эти пробы НЕ доказывают

* Ни одна из них не вердикт: «ICE на восьми формах» — это про ПОИСК, а не про
  готовность `emit_c`. Клетка обойдена не вся, что обошёл — названо в отчёте.
* Ни один каталог не заводит строку реестра и не двигает базу храповика.
* `f3` носителем не нов: `char` описан в шапке
  `novac/fixtures/println_literals/pos_1.nv`. Нов КЛАСС — оговорка стоит первой
  строкой самого файла находки.
