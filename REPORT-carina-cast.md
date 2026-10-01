# Отчёт облачной сессии: числовые приведения `expr as T` в Карине (ветка `p274-carina-cast`)

База: `origin/p274-carina-cloud` = `8405e5fcd`. `origin/main` не вливался. Linux, оракул
`nova-cli` собран `cargo build --release`, Карина — `nova build novac/src/main.nv`.
Строк журнала времени не добавлял: потолок долей за день достигнут (по брифу).

```
D54 ПАРЫ:
  iN/uN/int/uint -> iN/uN/int/uint (сужение, расширение, смена знака) -> законна; младшие биты
      цели (амендмент 2026-09-04 п.1: одна операция) -> реализовано, C-приведение;
      ИСКЛЮЧЕНИЕ int -> uint -> придержано (CAST_MSG): D54 и оракул расходятся, см. ниже;
  f32/f64 -> любое целое -> законна; насыщение, NaN -> 0, +-inf -> границы, отрицательное в
      беззнаковое -> 0 -> реализовано, `nova_<f>_to_<ряд>` из nova_rt/cast.h (int -> i64,
      uint -> u64 — как у оракула);
  целое -> f32/f64 -> законна; ближайшее IEEE -> реализовано, C-приведение;
  f64 <-> f32 -> законна; IEEE-округление -> реализовано, C-приведение;
  тождество числового ряда (n as int при n int) -> законна -> реализовано, операнд без приведения;
  char-ЛИТЕРАЛ -> любое целое -> законна (исключение D54 «'A' as int, 'A' as u8»); кодовая точка,
      младшие биты для узкой цели ('Ā' as u8 == 0) -> реализовано;
  int-ЛИТЕРАЛ -> char -> законна в U+0..U+10FFFF вне U+D800..U+DFFF, проверка статическая;
      вне диапазона — E_NOVAC_LANG с названным литералом -> реализовано (neg_5, neg_6);
  целая ПЕРЕМЕННАЯ (iN/uN/int) -> char -> запрещена (таблица D54) -> E_NOVAC_LANG (neg_1);
      внутри unsafe { } -> законна (исключение D54), считается операцией блока, как у оракула
      -> реализовано;
  char-ПЕРЕМЕННАЯ -> u8 -> запрещена (таблица D54) -> E_NOVAC_LANG (neg_4);
      внутри unsafe { } -> придержано: D54 разрешает, оракул даёт E_UNSAFE_UNUSED;
  char-ПЕРЕМЕННАЯ -> прочие целые (u16..u64, int, i8..i64) -> D54 молчит/противоречит
      себе -> придержано (CAST_MSG), вопрос 2;
  число -> bool -> запрещена (таблица D54) -> E_NOVAC_LANG (neg_2);
  str -> X и X -> str -> запрещена (D54 + амендмент 2026-08-01) -> E_NOVAC_LANG (neg_3);
  any -> T -> запрещена (D54 «Запрещено») -> E_NOVAC_LANG;
  T -> any -> вне числового семейства (упаковка D53) -> CAST_MSG, не трогал;
  bool -> число -> D54 правила не даёт, обзор spec/conversions.md даёт true=1 -> придержано
      (CAST_MSG), вопрос 3 (neg_7 пришпиливает именно придержание);
  char <-> f32/f64, char -> char, bool -> bool, str -> str, () / never -> правила в D54 нет
      («Произвольные типы без явного правила — ошибка компиляции») -> E_NOVAC_LANG;
  newtype -> представление -> законна -> было до волны, не трогал;
  значение -> newtype (42 as UserId), sum -> int (дискриминант) -> законны по D54, но вне
      числового семейства -> CAST_MSG, не трогал (текст CAST_MSG их называет).
/
РАСХОЖДЕНИЯ D54 <-> ОРАКУЛ:
  1. int as uint. D54, амендмент 2026-09-04 п.1: «Исключений по имени типа нет: `int as uint` —
     та же операция (D130 Q2 снят тем же днём)»; spec/conversions.md: «`(-1 as int) as uint ==
     2^64−1`». Оракул: `ro n = 65; println((0 - n) as uint)` печатает `0` (emit_c.rs, арм
     ExprKind::As: «Plan 70.5 Q2: int → uint saturation (neg → 0)», `nova_int_to_uint`), тогда как
     `(0 - n) as u64` печатает 18446744073709551551. Пара придержана.
  2. uN/iN-переменная as char. D54: «`int as char`, `iN/uN as char` | формы нет». Оракул: `ro b u8
     = 66; println(b as char)` печатает `B` (его список запретов называет только int/i32/i64/u32/u64).
     Карина отказывает по D54 (neg_1).
  3. bool as число. D54 правила не даёт; оракул: `println(true as u8)` -> `1`. Придержано (вопрос 3).
  4. Пары без правила, которые оракул принимает: `'A' as f64` -> 65, `c as f64` -> 113,
     `f as char` (f = 2.5) -> управляющий символ, `true as bool`, `s as str`, `'A' as char`.
     Карина отказывает по общему правилу D54.
  5. char-переменная as u8 внутри unsafe { }: D54 разрешает («внутри unsafe { } запрещённые
     as-cast'ы для переменных разрешены»); оракул: `unsafe { c as u8 }` -> [E_UNSAFE_UNUSED].
     Придержано.
  6. Где живёт запрет. У оракула таблица запретов (`check_as_cast_allowed`) стоит в ЭМИТТЕРЕ:
     `nova check` принимает neg_2..neg_6, отказ приходит только на `nova build` как
     «codegen error». Карина отказывает в чекере.
/
ФИКСТУРЫ:
  cast_numeric/pos_1 (47 клеток: 300 as u8, -1 as u32/u64/i8, 131071 as i16, 70000 as u16,
      u16 65535 as i16, i8 -1 as u64, 3.9/-3.9 as int, 1e20/-1e20 as int, 70000.5 as i16,
      -1.0 as u16, 300.0 as u8, NaN as int, +-inf, f32-цепочки, 'A'/'é'/'Ā'/'€'/'\n' в целые,
      65/0x41/233 as char): байт-в-байт с оракулом ДА (novac-e1-smoke ok);
      вывод сверен и с D54 по каждой клетке (например -1e20 as int = -9223372036854775808,
      -1.0 as u16 = 0, NaN = 0);
  neg_1 (`u8 as char`), neg_2 (`int as bool`), neg_3 (`str as int`), neg_4 (`char as u8`),
      neg_5 (литерал 0x110000), neg_6 (суррогат 55296) — E_NOVAC_LANG, пара/литерал названы,
      пришпилены NOVAC_EXPECT; neg_7 (`bool as u8`) — пришпилено ПРИДЕРЖАНИЕ (E_NOVAC_SUBSET);
      check-novac-fixture-expect ok (47/47), check-novac-no-cascade ok (88 фикстур, по одному);
  проба в обе стороны: ДА — на одном дереве `prim_cast_verdict` для числового семейства
      возвращал CvHeld: pos_1 -> 39 отказов CAST_MSG, смоук FAIL; cast_test — 4 из 6 тестов
      красные; правка возвращена — смоук ok, тесты PASS.
/
МЕРА 0.2: 8405e5fcd 145, ICE 0 -> после 121, ICE 0 (группа cast 24 -> 0);
  double-build 122/142 на базе -> 125/145 после (+3 — новые файлы волны, приняты; lex.nv по-прежнему
  отвергнут, см. остаток numeric);
  остаток numeric: 27 из 27 — НЕ cast. Все 27 — чтения ТИПИЗИРОВАННЫХ модульных констант с
  литеральным инициализатором: `const B_LF u8 = 0x0A` (lex.nv 16: B_TAB/B_CR/B_LF/INTERP_*_B/
  B_UTF8_TAIL_LO), `UNIT_SLASH`/`UNIT_BACKSLASH`/`UNIT_LF` (pipeline/pipeline.nv 9), `SLASH_BYTE`/
  `BACKSLASH_PATH_BYTE` (main.nv 2). `('a' as u8)` в сравнениях после волны — тип `u8`, и ни одного
  отказа на них нет; замер интегратора приписал группу cast'у ошибочно. Причина — дефект Карины,
  описан ниже (ОТКРЫТО, дефект А).
/
ГРУППЫ (топ-10 после):
  27 the operands are different numeric types (D405)
  24 outside the subset: a `match` arm on a string or char literal ... (E2-b, with the string family)
  12 outside the subset: a `match` on an applied sum ... (E2-b2, with generics)
   6 outside the subset: this type has no such method in the declarations novac was handed
   6 outside the subset: `X` is not a callable novac knows in an expression ... (E2-b3)
   6 a bare `None` here has no type to take ... (E2-b3)
   4 unknown name: nothing with this name is bound at this point
   4 the pattern ... match every payload field (D59)
   3 parameter `X` needs a mutable place (P14)
   3 outside the subset: a method on a `value` record needs the receiver taken by POINTER
  (до: та же десятка плюс «24 outside the subset: a cast `expr as T` is not compiled yet» на 2-м месте)
/
СТРАЖИ: `sh scripts/tools/novac-gate-guards.sh .` (Linux, 13:48–13:56): 98 запущено, 7 тяжёлых
  пропущено раннером; после последней правки 96 ok, красных 2:
  check-novac-commit-no-simplification -> известный красный раннера (Errno 21: '.' — каталог);
  check-novac-time-ledger -> облачный клон неглубокий («нужен fetch-depth 0»), даты коммитов
      не восстановить; строки журнала времени по брифу не добавлял (потолок долей за день) —
      на полной истории страж, вероятно, покраснеет на трёх коммитах в novac/** без строки
      журнала, это решение интегратора.
  По дороге было красным и починено: no-name-hardcode, module-donor (донором стоял оракул),
  line-length — коммит 902f814; surface (builtins 59 -> 61, два экспорта имён рядов
  int/uint) — база поднята коммитом 00b2392 со строкой хроники и причиной;
  local-only-work — ушёл с пушем ветки. Базы подмножества и наполнителей не трогал
  (subset-debt-dated: 141 <= 141, отказов не прибавилось — CAST_MSG переписан, не размножен).
/
МОДУЛЬНЫЕ ТЕСТЫ: novac/src/pipeline — прошло (PASS, включая новый cast_test: 6 тестов, у
  каждого КОНТРОЛЬ); novac/src/check — прошло (PASS); novac/src/emit_c — тестов в каталоге нет.
/
ОБОЛОЧКА: нет (shell.tpl.c и таблица интеропа не тронуты: cast.h уже входит в nova_rt.h).
/
КОММИТЫ:
  260095f novac: compile the numeric cast family of D54 -- every legal pair, the forbidden ones named
  e3e07fe novac: hold `bool as <number>` -- D54 has no rule for it, its overview page gives true=1
  902f814 novac: the cast wave passes Carina's guards -- no name literals, no oracle as donor, short lines
  00b2392 guards: raise builtins' surface base 59 -> 61 for the two address-sized row names
  (этот отчёт) docs: report of the cloud session on p274-carina-cast
  Файлы: check/cast_pairs.nv (новый: таблица D54, `prim_cast_verdict`), check/casts.nv (дверь
  передаёт пару примитивов), check/messages.nv (CAST_MSG переписан — называет, что придержано;
  тексты запретов), builtins/builtins.nv (INT_TYPE_NAME/UINT_TYPE_NAME), emit_c/emit_cast.nv (новый: эмиссия cast, вынесена из emit_expr.nv, который
  стоял на 987 строках из 1000), pipeline/cast_test.nv, fixtures/cast_numeric/*,
  docs/plans/274.7-subset-to-spec.md (строка у записи об `as`-конверсиях).
/
ОТКРЫТО:
  Дефект А (Карина, ярус «неверный отказ», 27 диагностик меры 0.2). Объявленный тип модульной
  константы теряется: `@const_type_of` (check/exprs.nv) берёт тип ТОЛЬКО с инициализатора, и
  `const B u8 = 0x0A` читается как `int`. Проба (оракул: check ok, вывод `true`/`false`; Карина:
  E_NOVAC_LANG «the operands are different numeric types» на `b == B`, и НЕТ отказа на `b == C`):
      module q
      const B u8 = 0x0A
      const C u8 = ('x' as u8)
      fn f(b u8) -> bool => b == B
      fn g(b u8) -> bool => b == C
      fn main() {
          println(f(10 as u8))
          println(g(10 as u8))
      }
  Комментарий у `@const_type_of` («the subset's constants declare none») устарел: собственный
  исходник Карины пишет тип у 27 читаемых констант. Класс: «записанный тип отброшен в пользу
  выведенного». Правка вероятна в одном месте (взять записанный тип, когда он есть, и судить
  литерал по нему), но в этой волне НЕ делалась — бриф: дефект описать, номер даёт интегратор.
  Ожидаемый выигрыш — до −27 и, вероятно, приём lex.nv в double-build.

  Дефект Б (оракул, класс O-1c — чекер пропускает то, что отвергает эмиттер, плюс пропуски самого
  эмиттера): см. расхождения 2, 4, 6. Проба: `ro b = 66 as u8; println(b as char)` — `nova build`
  печатает `B`; `ro n = 1; println(n as bool)` — `nova check` PASS, `nova build` — codegen error.

  Наблюдение В (оба компилятора, приоритет префиксного минуса и `as`). Карина: `-x as T` —
  `(-x) as T` (минус берётся в primary, а литерал `-1.0` — один токен-литерал); оракул —
  `-(x as T)`. Проба `ro f = 2.5; println(-f as u8); println(-1.0 as u16)`: Карина 0 и 0, оракул
  -2 и -1 — у оракула ещё и унарный минус на u8/u16 прошёл, хотя conversions.md: «Unary minus is
  defined for signed types only ... compile error». В фикстуре отрицательные операнды взяты в
  скобки, поэтому она от этого не зависит. Спека приоритет `as` относительно унарного минуса, по
  моему поиску, не фиксирует (conversions.md пишет `(-1.0) as u32` со скобками, но и
  `0.0 / 0.0 as i16 // 0`, что читается как `(0.0/0.0) as i16`, то есть `as` слабее `/` — а оба
  компилятора делают `as` сильнее бинарных операторов).
/
ВОПРОСЫ ИНТЕГРАТОРУ:
  1. `int as uint`: младшие биты (D54 + conversions.md) или насыщение (оракул)? Рекомендация —
     младшие биты: обе страницы спеки согласны, а комментарий оракула ссылается на снятый D130 Q2;
     чинить оракул (274.10), пару в Карине открыть одной строкой (`prim_cast_verdict`).
  2. char-ПЕРЕМЕННАЯ в целое, кроме u8: D54 запрещает только `char as u8`, а исключение для
     литералов называет и `'A' as int`, будто переменная форма запрещена вся. Варианты: (а) законна
     для любой цели, младшие биты (как оракул); (б) законна только для целей, вмещающих любую кодовую
     точку (21 бит: i32/u32/int/uint/i64/u64), а i8/u8/i16/u16 — запрещены, как u8; (в) запрещена вся.
     Рекомендация — (б): она продолжает логику запрета `char as u8` («может не влезть») и не теряет
     данных; нужен амендмент D54 (номер/форма — за интегратором).
  3. `bool as число`: D54 правила не даёт (= ошибка), conversions.md «Bool ↔ everything» даёт
     `true=1`, оракул компилирует. Рекомендация — амендмент D54 по обзору (законна, 1/0): форма
     однозначная и тотальная; иначе править обзор и оракул.
  4. char-переменная в u8 внутри `unsafe { }`: D54 разрешает, оракул считает блок пустым
     (E_UNSAFE_UNUSED). Рекомендация — оракул считает такой cast операцией unsafe (как уже считает
     `n as char`), Карина откроет пару.
  5. Приоритет `-x as T` (наблюдение В): нужна норма; рекомендация — записать в D54/синтаксис, что
     `as` сильнее префиксного минуса или слабее, и привести второй компилятор. Это вопрос парсера, не
     этой волны.
  6. Дефект А — номер реестра и кому чинить (Карина, одна дверь `@const_type_of`).
```
