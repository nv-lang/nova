# Проба к №TBD — `Path @equals` мимо операторной диспетчеризации

Заведено исследовательским окном 2026-09-18 по вопросу владельца «верно ли назван метод `equal`,
почему не `equals`». Ответ на вопрос: канон — `@equal` (D363 `03-syntax.md:3106`, D46 `:3020`,
`spec/syntax.md:429`, `std/src/prelude/protocols.nv:83`); `Path` — единственный в дереве, кто пишет
`@equals`, и это носитель дефекта.

## Замер 2026-09-18 (оракул)

```sh
cp docs/plans/repro/TBD-path-equals/path_equals.nv.txt <dir>/pe.nv
nova-cli/target/release/nova.exe build <dir>/pe.nv -o <dir>/pe.exe
<dir>/pe.exe
```

| выражение | стиль | факт | ожидание |
|---|---|---|---|
| `a.equals(b)` | оба Posix | `true` | `true` |
| `a == b` | оба Posix | `true` | `true` |
| `a.equals(c)` | Posix vs Windows | **`true`** | `false` (doc: «same bytes AND same style»; спека `04-effects.md:7224`: «byte+style exact») |
| `a == c` | Posix vs Windows | `false` | `false` |

Две двери к одной операции расходятся, и неверна именно названная: `@equals`
(`std/src/fs/path.nv:139`) сравнивает только `@bytes`, `style` не смотрит; `==` не зовёт её вовсе
(диспетчеризация идёт на `@equal`) и падает на структурное равенство value-записи, где `style`
участвует.

Существующий тест `std/src/fs/d323_path_ops_test.nv:89-90` расхождение не ловит: обе пробы на
ОДНОМ стиле.
