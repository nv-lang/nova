# К5 · стек открытых match ничей: `end_switch(c)` снимает вершину, не спросив, чья она

Трек novac · клетка `lower` × К5 · охота 2026-09-13.

## Цитата

Стек — два параллельных вектора без владельца:

`novac/src/lower/ir.nv:402-403`

```nova
    open_arms []SwitchArm /// arms of the open matches, innermost match last (M2b-1d)
    arm_start []int /// where each open match's arms begin in `open_arms`, innermost last
```

Кадр кладётся безымянным — `novac/src/lower/ir.nv:687`

```nova
    @arm_start.push(@open_arms.len())
```

и снимается по вершине, кто бы ни пришёл — `novac/src/lower/ir.nv:713-736`

```nova
export fn FnBuilder mut @end_switch(c SwitchCtx) -> () {
    if @arm_start.len() == 0 {
        ice("lower: end_switch without an open match")
    }
    ro start = @arm_start[@arm_start.len() - 1]
    ro _ = @arm_start.pop()
    ...
    @seal(c.head, Terminator.Switch(SwitchTerm { scrutinee: c.scr, arms, join: c.join,
                                                 uniq: c.uniq }))
```

## Почему это инвариант

`SwitchCtx` (`ir.nv:370-375`) — обычное значение: четыре поля, ни одно не связывает его
с построителем и с кадром стека. Инвариант: **`end_switch(c)` можно звать только с
контекстом САМОГО ВНУТРЕННЕГО открытого match**. Иначе строка 717 берёт `start` чужого
кадра, цикл 720-729 собирает ЧУЖИЕ армы, и `@seal(c.head, …)` запечатывает внешний
заголовок армами внутреннего. Внутренний заголовок остаётся `Unreached` и всплывёт
позже в `finish()` (`ir.nv:761`) сообщением «a reserved block was never sealed» — то
есть про ДРУГОЕ место и без намёка на настоящую причину.

Тот же разрыв у `@end_arm` (`ir.nv:707-709`): он не проверяет ничего вовсе.

## Чем держится сегодня

Двумя вещами, и обе — не форма.

**Первая: рекурсия одного вызывающего.** `@end_switch` зовётся ровно из
`novac/src/lower/lowering.nv:615`, внутри `@lower_match`, чей `@ir.begin_switch(ls)`
стоит на строке 610 той же функции. Вложенные match вкладываются правильно потому, что
вкладываются вызовы `@lower_match`, а не потому, что построитель это требует.

**Вторая: проверка у СОСЕДНЕЙ двери — и только у неё.** `@begin_arm` умеет ловить
рассогласование, и это показывает, что вопрос был замечен:

`novac/src/lower/ir.nv:694-700`

```nova
export fn FnBuilder mut @begin_arm(c SwitchCtx, pat Pattern, guard Option[Node]) -> () {
    if @cur_id != c.head {
        ice("lower: begin_arm while the previous arm of the match is still open")
    }
    if @arm_start.len() == 0 {
        ice("lower: begin_arm outside of a match")
    }
```

То есть энфорс знает КАНОНИЧЕСКИЙ путь (открытие арма) и не знает закрытия: у
`@end_arm` и `@end_switch` тест «этот ли `c` владеет вершиной» отсутствует, хотя всё
нужное у них на руках. Ни стража, ни теста: в `lower_test.nv` нет ни одного случая с
двумя открытыми match разом (23 случая, ни один не вкладывает match в арм match).

## Чем снимается конструкцией

Кадр обязан нести имя владельца — а владелец у него уже есть, это `head`:

```nova
    arm_start []int      /// где начинаются армы кадра
    arm_owner []BlockId  /// чей это кадр -- заголовок его match
```

и в `@end_switch` / `@end_arm` первой строкой:

```nova
    if @arm_owner[@arm_owner.len() - 1] != c.head {
        ice("lower: end_switch with the context of a match that is not the innermost open one")
    }
```

Это делает нарушение ГРОМКИМ в точке нарушения, а не через две фазы в `finish()`.
Сильнее и дороже: не отдавать `SwitchCtx` наружу вовсе, а дать построителю форму
«открытый match» с временем жизни — но в подмножестве, которым novac компилирует сам
себя, этого нет, поэтому названный выше кадр-с-владельцем — ровно та конструкция,
которая здесь выразима.
