# Перепись дверей newtype — где различие `FnRow` ≠ `TyId` держится, а где нет

Заведено исследовательским окном (owner-research) 2026-09-04 по наблюдению интегратора: четыре
находки за сутки (№821, №893, №894, №904) — одной формы «одно правило, несколько дверей, на одной
из них оно не применяется». Проверена клетка целиком для одного правила — «newtype типизированно
отличен от представления и от других newtype» (D52, амендмент 2026-09-04). Перечислены все двери,
через которые значение newtype встречает ожидаемый тип; каждая — своя проба; для контроля те же
двери прогнаны с `int`/`str`, чтобы отделить «newtype стирается» от «чекер не смотрит вовсе».

Исследование целиком: `docs/dev/research/2026-09-04-newtype-operators.md` §7.

## Как запускать

```sh
cp docs/plans/repro/906-newtype-doors/<файл>.nv.txt docs/plans/repro/906-newtype-doors/<файл>.nv
nova-cli/target/release/nova.exe check docs/plans/repro/906-newtype-doors/<файл>.nv
nova-cli/target/release/nova.exe build docs/plans/repro/906-newtype-doors/<файл>.nv -o <куда-нибудь>.exe
```

Все пробы прогнаны 2026-09-04 одним бинарём `nova-cli/target/release/nova.exe`; те же файлы
повторно прогнаны из `examples/` (внутри workspace с `nova.toml`) — результаты `check` совпали
до строки, так что «permissive вне workspace» здесь не при чём.

## Замер

| дверь | проба | `nova check` | `nova build` → stdout | класс |
|---|---|---|---|---|
| объявление с типом, `ro y FnRow = t` (`t TyId`) | `decl_newtype` | `E7301` | — | **держит** |
| объявление с типом, `ro x int = s` (`s str`) — контроль | `ctrl_decl_var` | `E7301` | — | держит |
| объявление с типом, `ro x int = f` (`f f64`) — контроль | `ctrl_decl_float` | ok | — | числовая ширина permissive by design (D54) |
| аргумент вызова `f(t)`, `HashMap[FnRow,_].insert(t, …)` | `hashmap_key` (и `904/call`) | `E7301` | — | **держит** |
| литерал коллекции `ro v []FnRow = [a, t]` | `collection_lit` | `E7301 []TyId → []FnRow` | — | держит |
| `FnRow == 1`, `match a { 1 => … }` — литерал | `eq_lit`, `match_lit` | ok | — | норма (Plan 200: литерал адаптируется) |
| унарный `-a` | `unary` | ok | built → `-1` | норма: тип сохранён |
| бинарные `== < +` между `FnRow` и `TyId` / `int`-переменной | `904/mixed` | ok | built → исполняется | **А** — №904 |
| составное `a += t` | `compound` | ok | built → `2` | **А** — носитель №904 |
| `a + N`, `const N int` (типизированная константа, не литерал) | `const_int` | ok | built → `6` | **А** — носитель №904 |
| диапазон `a..t` со смешанными концами | `range_mixed` | ok | built → `0 1 2` | **А** — носитель №904 |
| `match a { ONE => … }`, `const ONE TyId` | `match_const` | ok | **CC-FAIL** `member reference type 'nova_int' is not a pointer` | **А** — носитель №904, ловит только Си |
| встроенный `[]str` индексируется `FnRow` без `as int` | `slice_index` | ok | built → `x` | **А** — носитель №904: сахар `[]` принимает newtype как `int` |
| `m[t]` на `HashMap[FnRow, str]` с ключом `TyId` | `hashmap_sugar` | ok | — | №894 (сахар `[]` не проверяет ключ) |
| переприсваивание `x = t` (`mut x FnRow`) | `assign`, `reassign_newtype_typed` | ok | built → `2` / `1` | **Б** — молча |
| переприсваивание `n = t` (`mut n int`, `t TyId`) | `assign_int` | ok | built → `2` | **Б** — молча |
| переприсваивание `x = s` (`mut x int`, `s str`) — контроль | `reassign_typed`, `ctrl_assign_var`, `ctrl_assign_lit` | ok | — (CC-FAIL) | **Б** — не newtype-специфично: чекер не проверяет вовсе |
| `Option[FnRow] = Some(t)` | `option` | ok | built → `true` | **Б** — молча |
| `Option[int] = Some(s)` — контроль | `some_typed`, `ctrl_option_var` | ok | — (CC-FAIL) | **Б** |
| поле записи `Rec { r: t }` при `r FnRow` | `field` | ok | built → `1` | **Б** — молча |
| поле записи `Rec { r: s }` при `r int` — контроль | `field_typed`, `ctrl_field_var` | ok | — (CC-FAIL) | **Б** |
| ветви `if c { a } else { t }` | `if_branches` | ok | built → `1` | **Б** — молча |
| ветви `if c { n } else { s }` — контроль | `ctrl_if_var` | ok | — (CC-FAIL) | **Б** |
| унификация `same[T](a, t)` | `generic_unify` | ok | built → `true` | **Б** — молча |
| унификация `same[T](n, s)` — контроль | `ctrl_generic_var` | ok | — (CC-FAIL) | **Б** |
| возврат `TyId` из `fn -> FnRow` | `ret` | `E_READONLY_COERCE` | — | дверь не достигнута: другое правило (параметр `ro`) сработало раньше |
| интерполяция `"${a}"` | `interp` | `E_INTERP_NO_DISPLAY` | — | не про различие: newtype не наследует `Display` |

## Два класса, не один

**А — сахар стирает newtype (№904 и носители).** Операторы, составное присваивание, типизированная
константа в операнде, диапазон, const-паттерн `match`, индекс встроенного среза. Вызов и
объявление с типом различие видят; эти двери — нет. Чекер знает правило и не применяет его на
части дверей. Норма — амендмент к D52 от 2026-09-04.

**Б — чекер не сверяет типы в этих позициях ни для кого.** Переприсваивание, `Some(x)` в
`Option[T]`, поле литерала записи, ветви `if`, унификация generic-параметра: `str` в `int` проходит
чекер так же, как `TyId` в `FnRow`. Это известное свойство оракула — «кольцо permissive» №262
(чекер permissive by design, «codegen выберет»), и корпус на него опирается: 11 негативных фикстур
ждут ошибку **от Си**, маркером `EXPECT_CC_ERROR` (в т.ч. `neg/d128_char_distinct_from_int.nv` —
различие `char`/`int` ловит только Си). Пока представления разные, Си — последняя линия и она
срабатывает (CC-FAIL). **Для newtype представление то же, и Си не срабатывает никогда**: все
Б-двери с newtype собраны и исполнены молча. Эскалация известного класса из К2 (CC-FAIL вместо
диагностики) в К1 (тихо) — ровно тем, что делает newtype newtype'ом.

Контроли, которые держат (`decl_newtype`, `hashmap_key`, `collection_lit`), показывают, что дверь
объявления и дверь вызова у чекера одна и рабочая; починка Б — провести остальные позиции через
неё же, не завести вторую сверку.
