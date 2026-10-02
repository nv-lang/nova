# Отчёт облачной сессии, шаг 6: индекс и E_NEWTYPE_CTOR_SELF, затем крупнейшие свободные группы (ветка `p274-carina-cast`)

Вершина до шага — `613625e6`, после него — `7de02a03` (Linux). Оракул и Карина пересобирались из
слитого дерева перед каждым замером и перед стражами. Строк журнала времени нет.

```
СЛИЯНИЕ: origin/integ/merge-train-1001 (e2f4fc96) влит — коммит c585fd5c. Конфликты:
  novac/src/emit_c/emit_expr.nv — сторона поезда (дверь body_ours / c_overload_symbol);
  novac/src/check/methods.nv — импорт поезда + `import ../sem.{row_recv_by_pointer}` (нужен ветке);
  novac/src/pipeline/subset_pattern_test.nv — сторона поезда;
  scripts/guards/novac-surface.baseline — версия поезда + pipeline 14 и sem 338 со строкой хроники
  (числа — замер стража на слитом дереве).
/
МЕРА 0.2: 73 (сразу после слияния) -> 60 -> 51 -> 51, ICE 0. double-build: 135/163 -> 140/163
  (своё исходное принимается; S5 по-прежнему не достигнут).
/
1. ИНДЕКС НЕ-`int` — ЗАПРЕТ ЯЗЫКА (коммит 77e1578f).
   INT_INDEX_MSG: E_NOVAC_SUBSET -> E_NOVAC_LANG во всех трёх дверях (индекс, граница среза,
   `read_at` указателя); текст называет D238, D405, D52 и `as int`. Литерал-индекс (D489) сюда не
   доходит. Фикстуры index_key/pos_1 (байт в байт с оракулом), index_key/neg_1 (пришпилен).
   Тест check/index_key_test.nv (новый файл: check_test.nv упёрся в предел 1000 строк) держит
   КОД. Проба в обе стороны: дверь среза снова на @report -> тест красный; вернул -> зелёный.
   Проба для реестра: docs/plans/repro/TBD-index-key-type/ — две клетки (`u8` и newtype над int по
   полю-Vec), вывод оракула (печатает 20 на обеих) и Карины, README с нормой. Строка реестра — твоя.
   subset-debt.baseline опущен со строкой хроники: refusals 141->140, undated 58->57, by door 72->70.
/
2. E_NEWTYPE_CTOR_SELF (коммит 72eda8ae). `W(e)` при `e W` — дверь конструктора отвергает через
   @reject (E_NOVAC_LANG) с кодом E_NEWTYPE_CTOR_SELF в тексте. Фикстура newtype/neg_3; тест в
   pipeline/cast_test, КОНТРОЛЬ — конструктор от представления компилируется. Проба в обе
   стороны пройдена. Оракул (№1596) не трогал.
/
3. ГРУППА CAST/NUMERIC — после слияния поезда в мере ПУСТА. Взял крупнейшие свободные группы:
   а) D363 (коммит 3e95f545): `#impl(Equal)` на PatShape (sem/pattern.nv) и ParamMode
      (sem/binding.nv) — Карина сравнивает их `==`, амендмент D363 2026-09-30 требует согласия
      суммы. Мера 73 -> 60.
   б) голый `None` (коммит d9056901): ожидание (`@value_expect`) теперь доходит до хвостового
      `if`/`if let` блочного тела, не сбрасывается после `return` внутри руки (сохраняется и
      восстанавливается), и поле записи берёт ожидание из объявленного типа поля. Фикстуры
      none_expect/pos_1 (байт в байт), neg_1; тест pipeline/option_test, проба в обе стороны.
      Ушёл и каскад collect.nv:638 «variant name declared by more than one sum». Мера 60 -> 51.
   в) неоднозначность D84 (коммит 7de02a03): в `dominates` резолвера добавлена ось ресивера
      (D285 §2 п.1): при равных модах конкретный ресивер бьёт blanket `fn[T] T @m`. Тест
      pipeline/overload_test, КОНТРОЛЬ — два blanket-а остаются неоднозначностью; проба в обе
      стороны. 6 отказов «fits more than one method» (`bytes.to_str()` в main/pipeline/mangle)
      стали честными «the shell carries no `to_str` for Vec[u8]» — это зона шелла (mono-k2). Мера
      51 -> 51.
/
ЧТО ОСТАЛОСЬ В МЕРЕ 51 (по текстам):
  12 шелл не несёт метода/функции — зона mono-k2;
   8 статические перегрузки — зона mono-k2;
   5 `?` (Try) — горизонт E2 (resolve.nv, sem/defs.nv x3, sem/sem.nv);
   4 «the pattern's payload count disagrees» — mono/mono.nv:302 и :358 (`CalleeFn(row)`,
     `CalleeVariant(_)` при скрутини `cr.target CalleeTarget`); у объявления счёт верный (1),
     mono.nv импортирует CalleeRef, но не CalleeTarget — вероятно, объявления, переданные модулю,
     а не правило паттерна. Не моя зона, наблюдение;
   2 «different number of payload values» — sem/callables.nv:42/52 `Some((first, cnt))`: тип
     возврата `Option[(VariantRow, int)]`, кортежный тип за горизонтом E2-b (проба: `Some((a, b))`
     оракул принимает, Карина даёт этот же текст — каскад непрочитанного кортежного типа, а
     не арность);
   2 `?? return false` — sem/handed_bodies.nv:151/158 (см. вопрос 1);
   остальное — по одному (tail-if, effect attribute, prefix/binary operator и т. п.).
/
СТРАЖИ: `sh scripts/tools/novac-gate-guards.sh .` (00:23–00:34, на 7de02a03, Карина
  пересобрана): 98 запущено, 7 тяжёлых пропущено, 97 ok. Красный: check-novac-time-ledger
  (неглубокий облачный клон). no-panic на этот раз уложился в предел раннера.
/
МОДУЛЬНЫЕ ТЕСТЫ: novac/src/pipeline, check, resolve — PASS.
/
КОММИТЫ:
  c585fd5c Merge origin/integ/merge-train-1001 e2f4fc96 into p274-carina-cast
  77e1578f novac: an index key other than `int` is a language error -- E_NOVAC_LANG
  72eda8ae novac: a newtype's constructor given the newtype itself is E_NEWTYPE_CTOR_SELF
  3e95f545 novac: `#impl(Equal)` on PatShape and ParamMode
  d9056901 novac: a bare `None` takes its position's Option in three more places
  7de02a03 novac: a method on a concrete receiver beats a blanket one on a bare type parameter
  (этот отчёт) docs: sixth report of the cloud session on p274-carina-cast
/
ВОПРОСЫ ИНТЕГРАТОРУ:
  1. `X ?? return R`. Текст Карины COALESCE_RETURN_MSG (check/exprs.nv:418) говорит «the form is
     retracted (D86)» — это устарело: АМЕНДМЕНТ D86 от 2026-09-01 (spec/decisions/04-effects.md)
     сузил ретракцию — форма ЗАКОННА, когда возврат объемлющей функции не Option/Result и нет
     generic-параметра. Оба сайта в мере (handed_bodies.nv, `-> bool`) законны. Сейчас отказ идёт
     под E_NOVAC_SUBSET, но текст лжёт о языке. Предлагаю: текст сразу поправить («not compiled
     yet»), а саму форму делать вместе с `?` — у обоих один механизм (ранний возврат из выражения
     через hoist-temp в lower_coalesce), вместе это 7 отказов меры. Беру ли я горизонт E2
     (`?` + `?? return`), или он чей-то?
  2. mono.nv:302/358 «payload count disagrees» — если зона свободна, разберу; иначе передай
     владельцу mono.
```
