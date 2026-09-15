# К5 · `Output* OutputEnd` — грамматика в плоском списке статей, записанная прозой

Трек novac · клетка `lower` × К5 · охота 2026-09-13.

## Цитата

`novac/src/lower/ir.nv:213-219`

```nova
export type Stmt enum
    | Assign(AssignStmt) // `local = value` -- through the placement door
    | Eval(Rvalue) // an expression in statement position, value dropped
    | Store(StoreStmt) // `place = value` / `place op= value`, place named by the tree
    | Decl(Local) // the local's declaration point (donor: rustc MIR `StorageLive`)
    | Output(OutputStmt) // one value handed to output, in this position of the sequence
    | OutputEnd // the newline that closes a `println` -- once, at the end
```

`novac/src/lower/ir.nv:526-529`

```nova
/// Records the newline that closes a `println`: once, after the last value.
export fn FnBuilder mut @output_end() -> () {
    @cur.push(Stmt.OutputEnd)
}
```

## Почему это инвариант

Слова «once, at the end» и «once, after the last value» — это ГРАММАТИКА:
один `println` = серия `Output` плюс ровно один закрывающий `OutputEnd`. Нарушения
каждое даёт валидный C и неверный вывод:

* забытый `output_end` — строки склеятся без перевода строки;
* лишний `output_end` — лишний `nova_print_newline();`;
* `OutputEnd` без предшествующих `Output` — пустая строка из ниоткуда.

Ни одно из трёх не диагностируется. `finish()` (`ir.nv:755-770`) проверяет ТОЛЬКО
терминаторы («statements were placed after the last sealed block», «a reserved block was
never sealed») и по статьям не ходит вовсе. Печатник печатает каждую статью независимо
и о парности не знает: `novac/src/emit_c/emit_flow.nv:381-389`

```nova
fn Emitter mut @print_stmt(s Stmt) -> () {
    match s {
        ...
        Output(o) => @emit_output(o)
        OutputEnd => @emit_output_end()
    }
}
```

## Чем держится сегодня

Одним циклом в одной функции — `@lower_println` (`novac/src/lower/lowering.nv:804-856`),
где `@ir.output_end()` стоит последней строкой после `for`. Это вся защита: пара «серия
и её закрытие» существует потому, что её половины стоят в одной функции, а не потому,
что форма их связывает.

Причём серия УЖЕ НЕ СПЛОШНАЯ, и это видно чтением. В том же цикле
`novac/src/lower/lowering.nv:836`

```nova
            match @lower_value_source(c, ty) {
```

`@lower_value_source` (`lowering.nv:864-888`) для `Coalesce`, `MatchExpr`, `IfExpr` /
`IfStmt` уходит в `@lower_coalesce` / `@lower_match` / `@lower_if_value`, а каждая из них
зовёт `@ir.begin_if` / `@ir.begin_switch`, то есть `@close` → `@seal`
(`ir.nv:569-572`, `ir.nv:550-565`), и `@seal` ЗАБИРАЕТ накопленное:

```nova
    @blocks[i] = Block { stmts: @cur, term }
    @cur = []Stmt.new()
```

Значит `Output`-статьи уже напечатанных аргументов уезжают в запечатанный блок, а
`OutputEnd` того же `println` ложится в блок-join — после ветвления. Порядок печати
обход сохраняет, поэтому C сегодня верен; но «серия и её закрытие» физически лежат в
РАЗНЫХ блоках графа, и всякий, кто впредь захочет проверить парность, обнаружит, что
проверять её негде.

Ни стража, ни теста: в `lower_test.nv` `OutputEnd` встречается пять раз (строки 62, 75,
91, 380, 429) и все пять — `OutputEnd => assert(false)`, то есть «сюда не должно
доходить» в тестах про другие формы. Случая про сам `println` в держателе нет.

## Чем снимается конструкцией

Сделать серию ОДНОЙ статьёй, которая несёт свои элементы:

```nova
    | Println([]OutputStmt) // весь println: значения по порядку, перевод строки в конце
```

Тогда «ровно один закрывающий» — не правило, а форма: закрытие печатает
`@emit_output_end()` после цикла внутри одной арки печатника, отдельного `OutputEnd`
не существует, потерять или удвоить его нельзя, и вопрос «в том же ли блоке лежит
закрытие» не встаёт — статья неделима.

Цена названа честно: сегодня аргумент, который ПОНИЖАЕТСЯ (`??`, `match`, `if`),
режет блок посреди серии, и при `Println([]OutputStmt)` это понижение обязано
целиком встать ПЕРЕД статьёй — то есть локалы аргументов готовятся заранее, а статья
несёт только операнды. Это и есть та же дисциплина, которую `OutputStmt` уже описывает
прозой («the lowering of an argument lands immediately before ITS OWN output statement»,
`ir.nv:200-202`), только выраженная формой.
