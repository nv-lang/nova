# К5 · «есть ли else» закодировано РАВЕНСТВОМ двух полей, и равенство пересчитывают трое

Трек novac · клетка `lower` × К5 · охота 2026-09-13.

## Цитата

`novac/src/lower/ir.nv:234-240`

```nova
export type IfTerm value {
    cond Cond /// what the branch tests -- see Cond
    then BlockId /// entered when `cond` holds
    els BlockId /// entered otherwise; equals `join` when there is no else
    join BlockId /// where both branches continue
    els_braced bool /// the else branch has its own `{ }` -- see the note below
}
```

То же на контексте, `novac/src/lower/ir.nv:354-358`:

```nova
export type IfCtx value {
    then BlockId /// entered when the condition holds
    els BlockId /// the else branch, or the join when there is none
    join BlockId /// where the branches continue
}
```

Кодирование живёт в одной строке — `novac/src/lower/ir.nv:599`

```nova
    ro els = if has_else { @reserve() } else { join }
```

## Почему это инвариант

Булев параметр `has_else` входит в дверь (`@begin_if`, `ir.nv:578`) и НЕ доезжает до
формы: он растворяется в равенстве `els == join`. Инвариант, который после этого
обязан соблюдаться: **`els` равен `join` тогда и только тогда, когда `else` не
написан**, а всякий читатель формы обязан знать эту договорённость, потому что типом
она не выражена — оба поля просто `BlockId`.

Представимость: `IfTerm { els: <любой третий блок>, join: X }` — совершенно законное
значение типа, означающее «есть else», хотя ветку никто не резервировал; и обратное —
«нет else», но `els` указывает не на join. Форма не отличит.

## Чем держится сегодня

Договорённостью, ПЕРЕСЧИТЫВАЕМОЙ в трёх местах руками — то самое «правило живёт в
вызывающих, а не в двери»:

1. `novac/src/lower/ir.nv:607-612` — построитель:

```nova
export fn FnBuilder mut @begin_else(c IfCtx) -> () {
    if c.els == c.join {
        ice("lower: begin_else on an if that was opened without an else branch")
    }
```

2. `novac/src/emit_c/emit_flow.nv:246-249` — печатник, отдельным сравнением:

```nova
                @print_chain(f, t.then, t.join, false, printed)
                if t.els == t.join {
                    // There was no `else` branch: the `els` block IS the join.
```

3. `novac/src/lower/ir.nv:631-634` — `@end_if`, третье сравнение того же семейства
   (`@cur_id != c.join`), решающее «ветка уже сомкнулась или нет».

Плюс восстановление того же факта на СТОРОНЕ ДЕРЕВА, независимо от формы:
`lowering.nv:338` (`ro has_else = kids.len() > 4`) и `lowering.nv:419`
(`ro has_else = kids.len() > 9`).

Итого: один факт, пять мест, где он выводится заново, и ни одного места, где он
записан. Стража нет; в `lower_test.nv` случай «an if without else: the else branch IS
the join» (строка 151) фиксирует ТЕКУЩЕЕ кодирование как ожидаемое, то есть закрепляет
договорённость, но не делает нарушение непредставимым.

Честно: сегодня равенство верно по построению — `@reserve()` (`ir.nv:491`) всегда даёт
свежий id, а `@open_if` — единственное место, где `IfTerm` конструируется (греп
`IfTerm {` по `novac/src` даёт одно попадание, `ir.nv:600`). Держится инвариант не
знанием на местах вызова, а тем, что конструктор один; на честном слове держится
ЧТЕНИЕ — каждый новый читатель формы обязан вспомнить договорённость сам, и трое уже
вспомнили её порознь.

## Чем снимается конструкцией

Ветка, которой может не быть, — это `Option`, а не совпадение двух чисел:

```nova
export type IfTerm value {
    cond Cond
    then BlockId
    els Option[BlockId] /// None -- else не написан, ветвь идёт прямо в join
    join BlockId
    els_braced bool
}
```

Тогда `@begin_else` разбирает `match c.els`, печатник — тоже, третьего вывода нет,
а состояние «есть else, и он совпал с join» становится непредставимым. Заодно это
снимает половину призрачного состояния, названного образцом охоты: при `els: None`
поле `els_braced` не читает никто (`emit_flow.nv:247` замыкает раньше), и объединение
двух полей в одно — `els Option[ElseShape]` — убирает обе бессмыслицы разом.
