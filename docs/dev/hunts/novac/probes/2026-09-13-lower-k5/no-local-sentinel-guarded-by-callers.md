# К5 · `no_local()` не отбивается ни одной дверью — правило живёт в двух вызывающих

Трек novac · клетка `lower` × К5 · охота 2026-09-13.

## Цитата

`novac/src/lower/ir.nv:102-104`

```nova
/// The local that is not one: the statement-position match hands it to arms
/// that assign nothing (the same shape as `no_ty()` and `no_node()`).
export fn no_local() -> Local => Local(-1)
```

Двери, ЗАПИСЫВАЮЩИЕ статью тела, берут `Local` и не проверяют его ни разу:

`novac/src/lower/ir.nv:502-504`

```nova
export fn FnBuilder mut @place(e Node, dest Local) -> () {
    @cur.push(Stmt.Assign(AssignStmt { dest, src: Rvalue.Expr(e) }))
}
```

Тем же образцом: `@take_payload` (`ir.nv:509`), `@take_elem` (`ir.nv:516`),
`@output` (`ir.nv:522`), `@read` (`ir.nv:532`), `@take_field` (`ir.nv:661`),
`@copy` (`ir.nv:667`). Семь дверей, ноль проверок.

Проверяют — ровно две соседние двери, и они проверяют ЧУЖОЙ вопрос (чтение, не запись):

`novac/src/lower/ir.nv:471-477`

```nova
export fn FnBuilder @decl_of(l Local) -> LocalDecl {
    ro i = raw_local(l)
    if i < 0 || i >= @locals.len() {
        ice("lower: asking the declaration of a local the body never created")
    }
```

(то же в `@declare_zeroed`, `ir.nv:459-463`).

## Почему это инвариант

`no_local()` — это `Local(-1)`, и `Local` — это `type Local int`: страж-значение и
настоящий индекс неразличимы типом. Инвариант: **`no_local()` не смеет доехать ни до
одной записывающей двери**, иначе в тело попадает `Stmt.Assign { dest: Local(-1) }` —
статья, которую построитель примет молча, `finish()` пропустит (он смотрит только
терминаторы, `ir.nv:755-768`), а упадёт она у ПЕЧАТНИКА, в `@emit_assign` →
`@lo.ir.decl_of(a.dest)` (`novac/src/emit_c/emit_place.nv:118`) с текстом «asking the
declaration of a local the body never created» — сообщением о ЧТЕНИИ, без единого следа
того, какая дверь эту статью записала.

## Чем держится сегодня

Дисциплиной на местах вызова, переписанной ДВАЖДЫ дословно — тем же приёмом, что и
образец с `braced`:

`novac/src/lower/lowering.nv:595`

```nova
    if dest != no_local() {
```

`novac/src/lower/lowering.nv:745`

```nova
    if dest != no_local() {
```

Первая держит `@ir.declare_zeroed(dest)` (`lowering.nv:604`), вторая —
`@lower_place(arm_e, dest)` (`lowering.nv:747`). Обе — один и тот же тест, написанный
руками в двух функциях. Снять любую из них — и `no_local()` уедет в дверь.

Третьей защиты нет: `@lower_place` (`lowering.nv:764-773`) начинается с
`@ir.decl_of(dest).ty`, то есть ICE прилетит из ЧИТАЮЩЕЙ двери, а не из той, куда
значение кладут.

Ни стража, ни теста: `novac/src/lower/lower_test.nv` (23 случая, прочитан целиком по
списку `test "…"`) ни одного `no_local()` не содержит — греп по дереву даёт 7
попаданий, все в `ir.nv` и `lowering.nv`.

## Чем снимается конструкцией

`no_local()` существует ради ОДНОГО вопроса — «есть ли у этого match результат»: он
рождается в `lowering.nv:66` и `lowering.nv:209` и умирает в двух тестах выше. Это
`Option`, а не `-1`:

```nova
fn Lowerer mut @lower_match(m Node, dest Option[Local]) -> ()
fn Lowerer mut @lower_arm_body(arm_e Node, dest Option[Local]) -> ()
```

Тогда «позиция-выражение» и «позиция-статья» — два разных значения, оба теста
превращаются в `match dest`, а `Local` перестаёт иметь недопустимое значение вовсе,
и `no_local()` уходит из экспорта. Недопустимое состояние становится непредставимым
там, где сегодня оно представимо и просеивается вниманием.
