# Отчёт облачной сессии, шаг 5: группа cast/numeric в sem/ после слияния sweep (ветка `p274-carina-cast`)

Вершина до шага — `4323347c`. Linux. Оракул и Карина пересобраны из слитого дерева перед замером
и перед стражами. Строк журнала времени нет (как раньше).

```
СЛИЯНИЕ: origin/p274-carina-sweep (54d377cb4) влит — коммит 1a89b6a1. Конфликт один:
  scripts/guards/novac-surface.baseline (обе хроники сохранены; итог builtins 61 — у ветки cast
  чистый ноль, у sweep 61 = main +1 и ASSERT_FN; страж на слитом дереве согласен). В sem/
  конфликтов не было — выбирать сторону не пришлось.
/
МЕРА 0.2: 63 (моя ветка до слияния) -> 82 после слияния sweep (D488: модуль sem типизируется
  впервые, его отказы вошли в меру) -> 78 после шага, ICE 0. double-build 134/153.
/
РАЗБОР CAST/NUMERIC в замере 82 (код · форма · файл:строка · причина):
  E_NOVAC_SUBSET · «this argument is not the newtype's representation» · sem/collect.nv:654
      `FnRow(fns.rows.len() - 1)` — КАСКАД: `fns` связан с `FnTable.new()`, а тот отвергнут как
      статическая перегрузка (зона mono-k2); конструктор newtype судил значение без типа.
  E_NOVAC_SUBSET · то же · sem/collect.nv:663 `fn_decl_rows[did] = FnRow(frow)` — `frow` уже
      FnRow: исходник Карины перевоборачивал newtype (D52: конструктор берёт представление).
  E_NOVAC_SUBSET · «only an `int` index is compiled yet» · sem/sem.nv:143
      `ctx.fn_decl_rows[id]` — индекс `[]FnRow` значением NodeId (newtype над int); у
      `Vec[T] @index(i int)` параметр int, newtype конвертируется только явно (D52, D405);
      строкой выше тот же файл пишет `raw_node(id)`.
  + следствие: E_NOVAC_SUBSET · «the type argument of this sum cannot be inferred» · sem/sem.nv:156
      `Some(row)` — row не типизирован из-за отказа строки 143.
  Других отказов группы cast/numeric в замере нет (остальное — shell str, статические перегрузки,
  голый None, D363, `?`, неоднозначность D84 и прочее вне моей зоны).
/
ЧТО СДЕЛАНО (по классам):
  КЛАСС 1 — каскад отравленного операнда в дверях cast и конструктора newtype (коммит c54ae65a).
      `@type_cast` и конструктор newtype больше не судят значение без типа (правило каскада,
      check/binds.nv). Конструктор при этом всё равно ЗАПИСЫВАЕТ конструирование: первая попытка
      «молча выйти» дала ICE решётки («a type in call position with no ...») — замерено, исправлено.
      Фикстуры cast_numeric/neg_9 и newtype/neg_2 — одна причина (неизвестный вызов в
      инициализаторе), один диагностик, пришпилено. Проба в обе стороны: без двух остановок —
      2 и 3 диагностика, с ними — по одному.
  КЛАСС 2 — исходник Карины опирался на снисходительность оракула (коммит caa201b3):
      sem.nv:143 -> `ctx.fn_decl_rows[raw_node(id)]`; collect.nv:663 -> `fn_decl_rows[did] = frow`.
      Чек-ер Карины не менялся: по спеке обе формы — ошибки.
  Мера: 82 -> 78 (−2 «not the newtype's representation», −1 «only an int index», −1 «type
      argument ... cannot be inferred»), ICE 0.
/
СТРАЖИ: `sh scripts/tools/novac-gate-guards.sh .` (21:11–21:25, на caa201b3): 98 запущено, 7 тяжёлых
  пропущено, 96 ok. Красные: check-novac-time-ledger (неглубокий облачный клон);
  check-novac-no-panic — СНЯТ ПРЕДЕЛОМ 240с раннера (вердикта нет), отдельно с пределом 580с:
  ok, 214 фикстур, 4 мин 07 с. Фикстур после слияния стало больше — раннерного предела не хватает
  на облачной машине; на твоей, вероятно, хватит.
/
МОДУЛЬНЫЕ ТЕСТЫ: novac/src/pipeline, check, sem, parse — PASS.
/
ОБОЛОЧКА: нет.
/
КОММИТЫ:
  1a89b6a1 Merge origin/p274-carina-sweep 54d377cb4 into p274-carina-cast: sem typed, the cast wave on top
  c54ae65a novac: a poisoned operand does not cascade into the cast and newtype-constructor doors
  caa201b3 novac(sem): two reads that leaned on the oracle's leniency -- an index by a newtype, a newtype re-wrapped
  (этот отчёт) docs: fifth report of the cloud session on p274-carina-cast
/
ВОПРОСЫ ИНТЕГРАТОРУ:
  1. Индекс не-`int` типа. Оракул принимает `xs[i]` с `i u8` и с `i` newtype-над-int (замер
     2026-10-01: `t.rows[u]` и `t.rows[Row(0)]` печатают 5), хотя `Vec[T] @index(i int)` и D405/D52
     такой неявной конверсии не дают. Текст Карины INT_INDEX_MSG говорит «the language allows other
     integer types here» (E_NOVAC_SUBSET) — по замеру оракула, не по спеке. Варианты: (а) это
     запрет языка -> перевести отказ в E_NOVAC_LANG и завести строку реестра на оракул;
     (б) язык разрешает целочисленный индекс любой ширины -> нужна норма в D-блоке, и Карина
     его откроет. Рекомендую (а): так говорит сигнатура std; решение — за владельцем/тобой.
  2. `Row(r)` при `r Row` (конструктор newtype от значения самого newtype): оракул принимает
     (тождество), D52 описывает конструктор только от представления. Запрет или тождество?
     Рекомендую запрет (так сейчас у Карины); если тождество — нужна строка в D52.
```
