<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# №1323 — тип handler-литерала записывался как `E[E]`, а не `Effect[E]`

**Проба:** [probe.nv.txt](probe.nv.txt) — обработчик, привязанный `ro h = effect Random {...}`,
отдан дальше хвостом и `return` из `-> Effect[Random]` и аргументом в `h Effect[Random]`.

Запуск из корня (novac ищет std относительно рабочего каталога):

```sh
cp docs/plans/repro/1323-handler-literal-type-head/probe.nv.txt /d/Temp/p1323.nv
novac/target/novac.exe check /d/Temp/p1323.nv
nova-cli/target/release/nova.exe build /d/Temp/p1323.nv -o /d/Temp/p1323.exe && /d/Temp/p1323.exe
```

| компилятор | до `f9a69c3bf` | после |
|---|---|---|
| оракул | `check` PASS, `build` собирает, печатает `1` | то же |
| novac `check` | три «cannot return value of type `Random[Random]` from a function declared `-> Effect[Random]` (E7301)» и «this argument's type is not the one this function declares here» | rc=0, отказов нет |

**Место:** `novac/src/check/handler.nv`, `@type_handler_resolved` — применение строилось как
`@ctx.tys.app(edecl, [E])`, конструктором было объявление САМОГО эффекта. Комментарий над
строкой обещал «строку засева Effect», код интернировал другой терм. Слот `with` ошибки не
видел: его правило принимает любой вид-применение (`TkApp`).

**Проба в обе стороны:** откат `handler.nv` краснит тест
`pipeline/with_test.nv` «a handler bound by `ro` is handed on as `Effect[Fs]`» на
`d.len() == 0`; с правкой зелёный.

**Носители в `novac/src`:** ноль — handler-литерал там один (`main.nv:286`), он стоит в слоте
`with` и дальше не передаётся. Ступень 0.2 это не держало.
