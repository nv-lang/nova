# Отчёт облачной сессии, шаг 10: №1645 и №1646 в оракуле, номера 1644–1646, предварительная сверка оболочки (ветка `p274-carina-cast`)

Вершина до шага — `b9497f73`. Linux. `origin/main` (08ff689c) партию шага 9 ещё не несёт
(`0c622275` не предок main) — поэтому слияние и сверка оболочки после синка НЕ сделаны, см. п. 4.
Оракул пересобирался после каждой правки. Строк журнала времени нет.

```
1. НОМЕРА (коммит b8f0d551): строки шага 9 получили №1644, №1645, №1646.
/
2. №1645 — ОРАКУЛ: PAYLOAD ВАРИАНТА В ЛЮБОЙ ПОЗИЦИИ (коммит c92d3a9e).
   Корень: дверь 1 №1517 (`variant_ctor_compat`) сверяет payload только через `assignable_direct`,
   то есть только там, где позиция ЖДЁТ сумму конструктора. Без ожидания (`ro s = Sv.Int(x)`,
   аргумент обобщённого параметра, payload другого конструктора) payload не судился вовсе.
   Починка: дверь 4 `check_variant_ctor_payload` (types/variant_ctor.rs), вызов рядом с дверью
   арности в `f1_expr_inner`. Поле, не называющее параметр типа суммы, одинаково в каждом
   экземпляре — значит, его можно судить без позиции. Суждение идёт тем же `assignable`:
     другой тип ............ E7301 «cannot pass value of type `int` as the payload of `Sv.Txt` of type `str`»
     другое число .......... E_IMPLICIT_NARROWING (D491; `int` vs `i64` — D129)
     литерал не влезает .... E_LIT_OUT_OF_RANGE (D489)
   Дверь 1 такие поля оставляет двери 4 (одна причина — один отчёт); поля с параметром судит, как прежде.
   Фикстуры: neg/p1645_variant_payload_{any_position,narrowing,literal}_neg (пины по строкам),
     standalone/p1645_variant_payload_ok. Проба в обе стороны: дверь выключена — три neg-фикстуры
     дают 0 диагностик; включена — PASS.
   Ложных срабатываний нет: `nova check` std / examples / novac / spec_tests — дверь срабатывает только
     на p1517 и p1645 negative (две клетки p1517 с необобщённым полем теперь называет дверь 4: тот же
     код, та же строка).
/
3. №1646 — ОРАКУЛ: ЛИТЕРАЛ-РУКАВ ВНЕ ДИАПАЗОНА (коммит 95726add).
   `check_numeric_arms_agree`: если одна сторона — литерал, а другая — типизированный рукав, то
   сообщается вердикт литерала: E_LIT_OUT_OF_RANGE (`300 > u8.MAX (255)`, `-1 < u32.MIN (0)`) или
   E_LIT_INEXACT (целый литерал рядом с f32), с нотой о рукаве, чей тип он берёт. Два
   ТИПИЗИРОВАННЫХ рукава разных типов по-прежнему дают E_MATCH_ARM_WIDTH_MISMATCH. Дверь возврата
   больше не повторяет тот же текст на том же месте (у возвращаемого `match` было два одинаковых).
   Фикстуры: neg/p1646_literal_arm_out_of_range_neg (оба порядка, `match` и `if`),
     neg/p1646_literal_arm_inexact_neg.
   ПЕРЕВЕДЕНЫ НА НОРМУ (тест авторитетен, но тут его код противоречит примеру спеки — D489,
     02-types.md:20740: `None => -1` при `Option[u32]` — E_LIT_OUT_OF_RANGE):
     neg/p1608_match_typed_arm_vs_sentinel_neg, neg/n_match_arm_width_negative_literal_uint
     (теперь с пином; заодно `nova:allow W_MANUAL_COALESCE`, как у соседа — находка линта
     была и до правки). Храповик nova:expect 554 -> 553, с записью в летописи.
   Проба в обе стороны: ветка литерала выключена — обе p1646 и sentinel дают NEG-WRONG-MSG;
     включена — PASS. (n_match_arm_width_negative_literal_uint зелёная и без ветки: это позиция
     возврата, там литерал называет дверь возврата.)
/
ПРИЁМКА (как у помощника):
  крейт: compiler-codegen `cargo test --release --lib` (RUST_MIN_STACK как в CI) — 1294/0;
         nova-cli `cargo test --release` — всё зелёное;
  фильтры (`nova test spec_tests --full --filter ...`): p164x 6/0, p1517 4/0, p1608 16/0,
         n_match_arm 3/0, p1595 1/0, variant 33/0, sentinel 2/0, p959 7/0, p1611 14/0, p1593 7/0;
  флагманы: все шесть целей scripts/guards/flagship-targets.txt собираются с --strict-effects (rc=0);
  стражи: nova-expect-ratchet, expect-markers, expect-marker-colon, test-fixture-coverage,
         diag-fixture-coverage, checker-entrypoints, driver-channel-parity, diag-paths,
         slice-role-one-question — ok; registry-entry-shape красный только из-за строки №TBD (п. 5).
/
4. ОБОЛОЧКА (shell-freshness) — ПРЕДВАРИТЕЛЬНО, ДО СИНКА.
   Партии в main нет, слияния нет, поэтому сверку провёл по коду, на оболочке ветки
   p274-carina-strarm (ed986f87):
     оракул на main (codegen/emit_c/value_abi.rs, `recv_by_copy`): `ro @` value-записи передаётся
       КОПИЕЙ при размере <= 24 байт; указателем — если больше, при `mut @`, `consume @`, fluent
       `-> @` и для закреплённых типов (#no_copy, consume, #zero_on_move, #share);
     Карина (emit_c/emit_expr.nv:252): `by_ptr = in_program(row) && row_recv_by_pointer(row)` —
       для строк ОБОЛОЧКИ вопрос не задаётся, и ресивер ВСЕГДА передаётся значением.
   Итог: для Duration и других малых `ro @` совпадает (оболочка strarm ждёт значение — Карина
     передаёт значение). НЕ совпадает для того, что в оболочке strarm осталось указателем: итераторы
     (`mut @ next`: VecIter, MapIter, FilterIter, SplitIter, CharsIter ...), Path (14 методов),
     OpenOptions (fluent, 12), IoError, EnvVar. Это видно уже сейчас: смоук Карины на
     `Path.posix("a/b.txt").to_str()` — clang «passing 'NovaValue_Path' to parameter of
     incompatible type 'NovaValue_Path *'».
     (Там же отдельная беда: статик `Path.posix` Карина печатает необъявленным именем
     `novac_fn_std_fs_posix__nova_str__to_Path`.)
   Предлагаемая правка — СЛЕДУЮЩИЙ ШАГ ПОСЛЕ СИНКА: на месте вызова спрашивать
     `row_recv_by_pointer` для любой строки, не только своей программы, и научить
     `value_recv_by_pointer` тем же исключениям, что у оракула (consume, fluent `-> @`, закреплённые
     типы). Без синка не делаю: на нынешней оболочке ветки (все ресиверы по указателю) правка
     сломала бы малые записи.
/
5. НАЙДЕНО ПОПУТНО (строка №TBD, 🟠 К2; положительная половина №1645): `ro e = Tag(3, "t")` при
   `type Tg[T] enum Tag(u16, T) | Nn` — `check` ok, C падает («initializing 'void *' with ...
   'nova_str'»). `T` экземпляра теряется, когда рядом голый литерал в поле без параметра типа.
   С выключенной дверью 4 — то же самое, то есть дефект старый. Положительная фикстура записывает тип.
/
КОММИТЫ:
  b8f0d551 docs: registry numbers 1644-1646 for step 9 rows
  c92d3a9e types: a variant payload is judged in every position, not only where the sum is expected (1645)
  95726add types: an out-of-range literal arm is E_LIT_OUT_OF_RANGE, not an arm width mismatch (1646)
  7699af63 docs: registry rows 1645 and 1646 fixed on the branch; ... uncompilable C
  (этот отчёт) docs: tenth report of the cloud session on p274-carina-cast
/
ВОПРОСЫ ИНТЕГРАТОРУ:
  1. Партия шага 9 ещё не в origin/main. Как только выйдет — вливаю, пересобираю, гоняю
     shell-freshness / double-build и делаю правку п. 4. Подтверди, что она моя.
  2. Строка №TBD (кодоген, обобщённая сумма с литералом) — нужен номер; чья она?
```
