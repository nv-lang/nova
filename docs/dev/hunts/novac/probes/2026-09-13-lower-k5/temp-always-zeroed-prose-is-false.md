# К5 · «каждый `Temp` зануляется, и мест всего три» — инвариант прозой, уже ложный

Трек novac · клетка `lower` × К5 · охота 2026-09-13.

## Цитата

`novac/src/lower/ir.nv:53-67` (докстрока `DeclAt`), два соседних утверждения:

```nova
/// This is ORTHOGONAL to `LocalKind`, and separating them is what makes the fourth
/// combination legal -- a `Temp` declared at its first assignment, which is exactly the
/// `match` scrutinee. The indent follows THIS field rather than the kind: four spaces for a
/// declaration, eight for a bare assignment inside a branch. That correlation is total
/// today -- every existing `Temp` is zero-initialised (all three `temp` sites either call
/// `declare_zeroed` themselves or hand the local to a door that does) -- which is why this
/// separation changes no printing at all.
```

## Почему это инвариант

Из этой прозы ВЫВЕДЕНО поведение печатника. `@emit_assign_head`
(`novac/src/emit_c/emit_place.nv:194-208`) выбирает ОТСТУП строки по полю `decl_at`:

```nova
            match d.decl_at {
                FirstAssign => @body.append("    ${c_type(@ctx, d.ty)} ${d.name} = ")
                Zeroed => @body.append("        ${d.name} = ")
            }
```

То есть `decl_at` отвечает на ДВА вопроса сразу: «где напечатано объявление» и «на
какой глубине стоит присваивание» (4 пробела против 8). Второй вопрос — про вложенность
блока, а поле про неё ничего не знает. Утверждение «That correlation is total today»
и есть тот инвариант, которым держится второй ответ, а критерий волны — побайтно
совпадающий C.

## Чем держится сегодня

Ничем, и утверждение уже НЕВЕРНО — счёт грепом по дереву, а не по памяти.

Мест `@ir.temp(` в рабочем коде **девять**, а не три:
`novac/src/lower/ir.nv:810`; `novac/src/lower/lowering.nv:97, 133, 137, 430, 523, 527,
593, 869`.

`@ir.declare_zeroed(` — **три**: `lowering.nv:98` (для 97), `lowering.nv:138` (для 137),
`lowering.nv:604` (для локала, созданного на 869 и переданного в `@lower_match`).

Значит **шесть** `Temp`-локалов сегодня живут с `DeclAt.FirstAssign`:

| место | локал | что это |
|---|---|---|
| `ir.nv:810` | `l` в `@bind_pair_temp` | скрутини деструктуризации |
| `lowering.nv:133` | `lo` в `@lower_coalesce` | Option левой части `??` |
| `lowering.nv:430` | `lo` в `@lower_if_let` | Option у `if let` |
| `lowering.nv:523` | `lv` в `@lower_for_each` | коллекция обхода |
| `lowering.nv:527` | `li` в `@lower_for_each` | индекс обхода |
| `lowering.nv:593` | `ls` в `@lower_match` | скрутини match |

Стража нет: `check-novac-arch-invariants.py` считает счётчики ИНВАРИАНТОВ в разделах
`docs/dev/novac-architecture.md`, а раздел `lower` (строки 2176-2178) объявляет
**Счётчик: 1**, и этот один — про дверь размещения, не про `DeclAt`. Теста нет:
`lower_test.nv` проверяет роль локала («each local carries its role», строка 128) и не
проверяет `decl_at` ни в одном из 23 случаев.

Держится, стало быть, ровно тем, что автор каждой из девяти строк помнит про соотношение
— и на шести из них соотношение уже другое, чем описано.

## Чем снимается конструкцией

Разделить два вопроса так же, как в 2026-09-09 разделили `LocalKind` и `DeclAt` (та же
докстрока, `ir.nv:42-47`, описывает ровно этот приём): отступ — не следствие поля
`decl_at`, а свойство МЕСТА печати. Печатник ходит по блокам (`@print_chain`,
`emit_flow.nv:168`) и глубину знает сам — он её уже ведёт в аргументе `head_continues`.
Отступ берётся из состояния обхода, поле `decl_at` отвечает только на свой вопрос
(«объявлен ли локал отдельной статьёй»), и утверждение про «total correlation» перестаёт
быть нужным — а с ним и возможность соврать.

Меньший шаг, если менять печать нельзя: сделать утверждение машинным — `finish()` знает
все локалы и все статьи, и проверка «ни один `Decl` не относится к локалу с
`FirstAssign`» уже стоит у печатника (`emit_place.nv:91`), но обратной — «`Temp`
+ `FirstAssign` законен, и вот его список» — нет нигде.

## Отдельно: это ещё и противоречие внутри одной докстроки

`ir.nv:58-60` говорит «the fourth combination [is] legal -- a `Temp` declared at its
first assignment, which is exactly the `match` scrutinee», а `ir.nv:61-64` тремя
строками ниже — «every existing `Temp` is zero-initialised». Скрутини match
(`lowering.nv:593`) — `Temp`, и он НЕ зануляется. Оба места приведены дословно выше;
выбор между ними — не охотника.
