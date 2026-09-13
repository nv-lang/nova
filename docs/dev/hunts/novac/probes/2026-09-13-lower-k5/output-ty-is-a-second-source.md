# К5 · `OutputStmt.ty` — второй источник факта, который локал уже несёт

Трек novac · клетка `lower` × К5 · охота 2026-09-13.

## Цитата

`novac/src/lower/ir.nv:206-209`

```nova
export type OutputStmt value {
    value Operand /// what is printed -- a local, or a tree expression while the bridge stands
    ty TyId /// its DECLARED type; the printer takes the carrier from it (a newtype prints as what it holds)
}
```

`novac/src/lower/ir.nv:522-524`

```nova
export fn FnBuilder mut @output(value Operand, ty TyId) -> () {
    @cur.push(Stmt.Output(OutputStmt { value, ty }))
}
```

## Почему это инвариант

`Operand` — сумма двух форм (`ir.nv:114`): `Tree(Node)` и `Place(Local)`.

* Для `Tree(e)` тип НЕОБХОДИМ отдельным полем: у узла дерева тип живёт в канале, и
  печатник до канала не ходит.
* Для `Place(l)` тип УЖЕ известен: `@decl_of(l).ty` — и печатник читает эту самую
  запись строкой ниже, чтобы взять имя.

`novac/src/emit_c/emit_place.nv:52-58`

```nova
    @body.append("    ")
    @body.append(@printer_of(carrier_of(@ctx, o.ty)))
    @body.append("(")
    match o.value {
        Place(l) => @body.append(@lo.ir.decl_of(l).name)
        Tree(e) => @emit_expr(e)
    }
```

Одна строка берёт ПЕЧАТНИК из `o.ty`, следующая берёт ИМЯ из `decl_of(l)`. Инвариант:
**для `Place(l)` поле `ty` обязано равняться `decl_of(l).ty`**. Если они разойдутся,
вокруг правильного имени встанет печатник другого носителя — `nova_print_int(s)` вместо
`nova_print_str(s)`. Это не падение и не диагностика: это молча другой вывод, и ловится
он только корпусным диффом C и только на том файле, который такую пару несёт.

## Чем держится сегодня

Одним местом вызова и порядком двух строк в нём:

`novac/src/lower/lowering.nv:834-839`

```nova
        } else if is_expr_kind(ck) {
            ro ty = @out.type_of(c.id_of())
            match @lower_value_source(c, ty) {
                Some(l) => @ir.output(Operand.Place(l), ty)
                None => @ir.output(Operand.Tree(c), ty)
            }
```

`l` пришёл из `@lower_value_source(c, ty)`, а тот создаёт локал как `@ir.temp(ty)`
(`lowering.nv:869`) либо возвращает локал `@lower_coalesce` / `@lower_if_value`,
созданный из `res_t` того же узла. То есть равенство держится тем, что автор передал
в дверь ТУ ЖЕ переменную `ty`, которой минуту назад создал локал. Никакой проверки:
ни `@output` (`ir.nv:522`), ни `@emit_output` (`emit_place.nv:44`), ни `finish()`
(`ir.nv:755`) не сравнивают `o.ty` с `decl_of(l).ty`. Теста тоже нет — в
`lower_test.nv` пять упоминаний `OutputEnd`, и ни одно не касается типа операнда.

Заметим и обратную сторону правила проекта «как можно меньше»: для `Place` это поле
ЛИШНЕЕ — оно не несёт ни одного факта, которого бы не было в локале.

## Чем снимается конструкцией

Тип перестаёт быть отдельным полем там, где он выводим:

```nova
export type Operand enum
    | Place(Local)          // тип берётся из decl_of
    | Tree(Node, TyId)      // у узла типа нет — он приезжает рядом
```

и `OutputStmt` схлопывается в один `Operand`. Тогда `@emit_output` спрашивает тип
одним способом для обеих форм, и «разойтись» нечему: у `Place` второго источника
просто нет. Меньший шаг с тем же эффектом — оставить поле, но в `@output` для формы
`Place` не БРАТЬ `ty` у вызывающего, а читать `@decl_of(l).ty`, отчего подпись двери
разбивается на две: `@output_local(l Local)` и `@output_tree(e Node, ty TyId)`.
