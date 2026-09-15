<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# F2 (К7) — механизм вывода M3a перехватывает `println` ПО ГОЛОМУ ИМЕНИ и только в позиции оператора

**Трек:** novac · **Клетка:** `emit_c` × К7 · **Найдено охотником 2026-09-16.**

## Свойство

Волна M3a сделала `println` НЕ обычным вызовом: он становится
последовательностью `Stmt.Output` в пониженной форме. Механизм заведён в одной
половине — в позиции ОПЕРАТОРА (`@lower_eval`), и опознаёт он цель **сравнением
ТЕКСТА вызываемого со строкой `"println"`**, не спрашивая канал чекера. Во всех
остальных позициях тот же вызов идёт старой дорогой — через `@out.callee_of`,
то есть через РАЗРЕШЁННЫЙ каналом вызываемый.

Пока имени `println` нет в таблице объявлений, обе дороги случайно совпадают, и
подмена невидима. Стоит пользователю объявить `fn println`, как дороги
расходятся: **одна и та же запись `println(x)` в одной программе означает разное
в зависимости от позиции.**

## Два адреса, отвечающие на один вопрос по-разному

Вопрос: **какой вызываемый стоит за `println(x)`?**

* `novac/src/sem/node_questions.nv:109-112` (`is_println_call`) — отвечает
  «встроенный вывод», и отвечает ПО ТЕКСТУ:

  ```
  export fn is_println_call(e Node) -> bool {
      if e.kind_of() != NodeKind.Call { return false }
      call_callee_text(branch_children(e)) == PRINT_FN
  }
  ```

  Потребитель — `novac/src/lower/lowering.nv:781-787` (`@lower_eval`).

* `novac/src/check/calls.nv:745-756` (`@type_free_call`) — отвечает «то, что
  нашла таблица объявлений», и записывает ответ в канал:
  `ro found = @ctx.defs.def_of(cname)`; если не нашла — отказ
  `outside the subset: this name is not a callable novac knows in an expression
  (println is a statement here, not a value)`.

Именно этот отказ и держит механизм M3a целым в обычной программе. Но он
срабатывает ТОЛЬКО когда `def_of` пуст. Объявленный пользователем `fn println`
наполняет `def_of` — отказ молчит, значение-позиция открывается, и второй дороги
больше ничто не закрывает.

## Что происходит

`docs/.../f2-println-name-vs-channel/user-declared/m.nv`:

```nova
fn println(n int) -> int { n + 100 }

fn main() {
    ro k = println(7)
    println(k)
}
```

* `novac check` — **rc=0** (принял),
* `novac emit` — **rc=0**, и выпущенный C показывает обе дороги рядом:

```c
static nova_int novac_fn_userdeclared_m_println__nova_int__to_nova_int(nova_int n) {
...
    nova_int k = novac_fn_userdeclared_m_println__nova_int__to_nova_int(((nova_int)7LL));
    nova_print_int(k);
```

Строка `k = ...` зовёт ФУНКЦИЮ ПОЛЬЗОВАТЕЛЯ (канал), следующая строка — ВСТРОЕННЫЙ
вывод (голое имя). Один и тот же `println`, две разные сущности, соседние строки.

Вариант `nested-one-line/` сжимает то же в одно выражение `println(println(7))`:

```c
    nova_print_int(novac_fn_nestedoneline_m_println__nova_int__to_nova_int(((nova_int)7LL)));
```

* оракул — **rc=1**: он разрешает `println` во встроенный вывод ВСЕГДА, поэтому
  видит `ro k = ()` и отказывает `[E7301] cannot pass `()` as argument `n` of type `int``.

## Что должно было бы и по какому правилу

Классификация дифф-раннера, дословно
(`scripts/tools/novac-diff-corpus.sh`, шапка):

> `DANGER  (novac ПРИНЯЛ, оракул отверг) — класс К7; красный вне allow;`

То есть направление расхождения тут — худшее из названных самим механизмом, и
имя класса в шапке раннера — ровно К7. В `novac/divergences.allow` этой формы нет
(проверено), носителя в корпусе нет (`grep -rn "fn println(" examples/ novac/fixtures/ novac/src/` даёт ноль).

Правило «кто решает, какой вызываемый» записано в карте:
`docs/dev/novac-architecture.md:1804` — корень «подбор перегрузки» принадлежит
`resolve`, дом двери `FnIndex.lookup`, и у `emit_c`/`mono` ребра к нему нет.
Опознание вызываемого по ТЕКСТУ имени в `sem`+`lower` — второй ответ на тот же
корень, и именно он побеждает в позиции оператора.

## Воспроизведение

```sh
# из корня дерева
sh docs/dev/hunts/novac/probes/2026-09-16-emitc-k7/f2-println-name-vs-channel/user-declared/cmd.sh
sh docs/dev/hunts/novac/probes/2026-09-16-emitc-k7/f2-println-name-vs-channel/nested-one-line/cmd.sh
```

## Оговорка про носителя

Носитель тут искусственный — пользователь, назвавший свою функцию `println`.
Ценность строки не в нём, а в СВОЙСТВЕ: перехват формы по голому тексту имени
мимо канала. Починка, которая запретит объявлять `println` (носитель), класс не
закроет: тот же приём опознания по тексту останется, и следующее имя, попавшее и
во встроенные, и в таблицу объявлений, повторит расхождение.
