<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# F1 (К7) — `??` понижается, только когда он ЦЕЛОЕ значение; вложенный — не понижается никем

**Трек:** novac · **Клетка:** `emit_c` × К7 · **Найдено охотником 2026-09-16.**

## Свойство (а не конструкция)

**`??` — форма, которая ПОНИЖАЕТ** (кладёт Some-тест с двумя ветвями). Понижать её
обязана фаза понижения, до обхода печати. Сегодня это делает `@lower_value_source`,
и он вызывается ровно там, где `??` — ЦЕЛОЕ значение позиции (целый аргумент
`println`, целый инициализатор, целый хвост). Если `??` стоит ВНУТРИ большего
выражения, до `@lower_value_source` он не доходит никогда: его встречает печатный
хойст `@hoist_array_lits`, который зовёт понижение в УЖЕ ЗАПЕЧАТАННЫЙ строитель.

Свойство — «форма понижает, а позиция не целая». Синтаксисов, в которых оно
выразимо, найдено **восемь** (ниже), и они дают **две разные** диагностики, так что
греп по одному тексту отказа класс не очертит.

## Два адреса, отвечающие на один вопрос по-разному

Вопрос: **кто понижает `??` и в какой момент?**

* `novac/src/lower/lowering.nv:873-881` (`@lower_value_source`, арм `NodeKind.Coalesce`)
  — отвечает «понижение, ДО обхода печати», и говорит о себе дословно:

  > ``` ??` LOWERS -- it lays down a Some-test with two branches -- so it cannot be
  > printed inline, and under the walk of step 3b a printer that lowered would push
  > into a FINISHED builder. That is not a guess: the door refused with
  > `lower: a block sealed twice (it already returns)` on both corpus carriers of the
  > form `println(x ?? y)` (measured 2026-09-09). Answering here puts `??` on the ONE
  > door for every value position at once -- the class, not the two carriers. ```

* `novac/src/emit_c/emit_c.nv:483-486` (`@hoist_array_lits`, арм `NodeKind.Coalesce`)
  — отвечает «ПЕЧАТЬ, в момент печати»:

  ```
  if kind == NodeKind.Coalesce {
      @lit_tmp[raw_node(e.id_of())] = @lo.ir.decl_of(@lo.lower_coalesce(e)).name
      return
  }
  ```

Второе место — ровно тот путь, который первое объявляет закрытым. Фраза
«the class, not the two carriers» опровергается замером: закрыты именно
два носителя корпуса, класс открыт в восьми синтаксисах.

Та же мысль уже записана самим планом, и это делает находку не догадкой, а
незакрытым остатком: `docs/plans/274.8-novac-mir.md:3478-3483` —
**«Урок формы: класс, закрытый списком МЕСТ вместо свойства („форма понижает“),
закрыт наполовину.»** Список мест с тех пор вырос на одно место, свойством он не стал.

## Что происходит и что должно бы

`novac check` — ЗЕЛЁНЫЙ (rc=0), оракул — ЗЕЛЁНЫЙ (rc=0), `novac emit` — ICE (rc=2).
Программа законна по обоим эталонам, компилятор падает внутренней ошибкой.

Должно бы: либо форма понижается (тогда C печатается), либо отказ держит ЧЕКЕР со
сроком — как это уже сделано для соседней формы того же класса: вложенный
if-значение отвечает `outside the subset: an `if` in value position is not compiled
yet` (проба `p_ifvalue`, см. README). Правило — шапка `printer_of` и конвенции novac
П6: **граница держится чекером, никогда не ice эмиттера** (та же формулировка стоит
в `novac/fixtures/println_literals/pos_1.nv`: «a boundary is held by the checker,
never by an emitter's ice»).

## Восемь носителей — по каталогу на каждый

| каталог | синтаксис | ICE |
|---|---|---|
| `c1-binary/` | операнд `+` | `lower: a block sealed twice (it already returns)` |
| `c2-if-cond/` | условие `if` | ``emit_c: a `??` the hoist never built (coalesce wave)`` |
| `c3-array-elem/` | элемент литерала массива | `lower: a block sealed twice (it already returns)` |
| `c4-interp-slot/` | слот интерполяции | ``emit_c: a `??` the hoist never built (coalesce wave)`` |
| `c5-return/` | операнд `return` | `lower: a block sealed twice (it already returns)` |
| `c6-tail/` | хвост блока | `lower: a block sealed twice (it already returns)` |
| `c7-callarg/` | аргумент вызова | `lower: a block sealed twice (it already returns)` |
| `c8-bind/` | инициализатор связывания | `lower: a block sealed twice (it already returns)` |
| `control-toplevel/` | **КОНТРОЛЬ**: `??` — целый аргумент | зелёный, rc=0 |

Вторая форма (`the hoist never built`, `emit_c/emit_expr.nv:437`) появляется там, где
хойст до узла не доходит вовсе, а печать выражения на него натыкается. Это тот же
класс с другим симптомом: греп по «sealed twice» нашёл бы шесть носителей из восьми.

## Воспроизведение

```sh
# из корня дерева
sh docs/dev/hunts/novac/probes/2026-09-16-emitc-k7/f1-nested-coalesce-not-lowered/c1-binary/cmd.sh
```

и так для каждого каталога; `control-toplevel/cmd.sh` обязан остаться зелёным —
он и есть обратная половина пробы: снимает подозрение «`??` сломан вообще».

## Чем это было невидимо

Все носители `??` в корпусе — ЦЕЛЫЕ аргументы `println`
(`examples/basics/coalesce.nv:10-11`, `examples/basics/option_ctor.nv:32-34`,
`novac/fixtures/option_ctor/pos_1.nv`). Вложенного `??` в корпусе нет ни одного,
поэтому `check-novac-differential.sh` зелёный. Отказа нет — значит
`check-novac-subset-debt-dated.py` предмета не видит (он судит ТЕКСТЫ отказов).
Код возврата 2 — «честный отказ двери с сообщением» по определению 274.3/F3
(`scripts/guards/lib/novac.sh`), поэтому не PANIC, и дифф-раннер такой файл
красным не считает даже если положить его в корпус без смоука.
