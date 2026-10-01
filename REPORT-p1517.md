# REPORT p1517 — конструктор варианта против ожидаемого инстанса суммы (облачная сессия, 2026-10-01)

Ветка `p1517-variant-ctor-check` от `origin/integrate` `fc6277ba9`; к моменту сдачи `integrate` не сдвинулся (слияние не потребовалось).

**ВОСПРОИЗВЕДЕНИЕ** (на `fc6277ba9`, Linux, clang):
`ro x = None` — `check` PASS, сборка, печатает `1`;
`fn bad() -> Option[str] => Some(1)` + `bad() ?? "x"` — `check` PASS, clang: «returning 'NovaOpt_nova_int' from a function with incompatible result type 'NovaOpt_nova_str'»;
`Some(1, 2)` (с аннотацией и без) — `check` PASS, clang: «too many arguments to function call, expected single argument 'v', have 2 arguments».
Сетка 20 клеток: до фикса отказывала ОДНА (аргумент `Some(1)` при `Option[str]`, `E_ARG_ELEM_TYPE_MISMATCH`).

**КОРЕНЬ:** у конструктора варианта не было двери. `assignable_direct` судил вызов `Some(1)` по выведенному `Option[int]` против `Option[str]` по категории без аргументов типа (принимал), payload не сверялся с полем варианта инстанса; число значений payload не считал никто; дверь возврата (`check_return_compat`) сообщает `Bad` только для примитивных возвратов (№959), так что и пойманное на возврате глушилось; биндинг без типа с `None` получал тип от кодогена.

**ФИКС:** `compiler-codegen/src/types/variant_ctor.rs` (новый, рядом с `record_lit_schema.rs`) — три двери:
- `variant_ctor_compat` — payload против полей инстанса через полный `assignable` (D55, адаптация литералов сохранены), вызывается из `assignable_direct` (`types/mod.rs`, перед `match &expr.kind`) — все позиции с дверью; отказ — `E7301` самой позиции;
- `check_variant_ctor_arity` — из арма `Call` в `f1_expr_inner`, все позиции: `E_VARIANT_CTOR_ARITY` (новый код: существующие `E_TUPLE_CONSTRUCT_ARITY_MISMATCH` — про именованные кортежи с default-полями, `E_FN_VALUE_CALL_ARITY` — про fn-значения; у вызова функции кода нет вовсе);
- `check_untyped_variant_ctor` — из ветки неаннотированного `let`: `E_VARIANT_CTOR_UNTYPED`, если конструктор generic-суммы оставляет параметр без источника (payload его не упоминает, default нет);
- `is_ctor_of_expected_sum` — дверь возврата пропускает `Bad` конструктора своей суммы (вердикт точный, не вывод).
Не судится намеренно (названо в доке модуля): имя, затенённое локалом/функцией/типом или общее двум суммам (№962); сумма, ИМЯ которой объявлено в двух файлах единицы — поиск №705 выбирает не ту (замер: `std/src/net/error.nv`, `import std.io.{ErrorKind}` разрешался в `ErrorKind` пакета `nova-http` с `Other(str)` → ложный отказ на `ErrorKind.Other(0)`; вылечено пропуском).
Спека (развилка не наступила): D55 «`let x = value` (без аннотации) — **выводится тип значения**», D88 — параметр фиксируют аргументы или default (`first[]([])` — ERROR); вывода из последующего использования спека не даёт → п.3 сделан.

**КЛЕТКИ** (до → после; все «после» — в `check`):
- `Some`/`Ok`/`Err` × `let` с аннотацией / аргумент / возврат `=>` / `return` / хвост `if` / хвост `match` / элемент `[]Option[str]` / поле записи × тип — PASS (аргумент `Some`: `E_ARG_ELEM_TYPE_MISMATCH`) → `E7301`;
- пользовательская сумма голым (`Circle("x")`) и квалифицированным (`Shape.Sq(1, "y")`) конструктором × тип — PASS → `E7301`;
- generic-сумма `-> P[str] => Two(1, 2)` × тип — PASS → `E7301`;
- `Some(1, 2)` (без типа и с аннотацией), `Some()`, `None(1)`, `Shape.Sq(1)`, `Sq(1, 2, 3)`, `Dot(1)` × арность — PASS → `E_VARIANT_CTOR_ARITY`;
- `ro x = None`, `mut b = None` (+ позднее присваивание), `ro e = Err("x")`, `Option.None`, пользовательский `Zero` × без типа — PASS → `E_VARIANT_CTOR_UNTYPED`.
Позитивы (вывод из payload, `None` при аннотации/возврате/параметре, generic-тело `Some(xs[0])` против `Option[T]`, вложенный `Some(Some(4))`, `Some(200)` в `Option[u8]`) — зелёные и печатают ожидаемое.

**ЦЕНА** (дифф ошибок `check` база↔фикс, тем же способом по всем наборам): std 1, novac 1, conformance 1, examples 0, contracts 0, флагманы 0 (6/6 собрались); soundness/strict_effects/p270 — 0. Все три — законные находки класса:
- `std/src/collections/vec_lazy/core.nv:650` `mut result = None` → `mut result Option[T] = None`;
- `novac/src/check/match_arms.nv:178` `mut first_t = None` → `mut first_t = no_arm_type()` (`fn no_arm_type() -> Option[TyId] => None`): аннотация локала вне подмножества Карины (E2-b3, +1 диагностика самопроверки единицы `check`), функция — нет; самопроверка единицы `novac/src/check` (`NOVAC_UNIT=1`) совпала с базой построчно;
- `neg/plan103_1_ordering_construct_with_payload_neg` (`MemOrdering.Acquire(42)`): ждала `EXPECT_CC_ERROR` без подстроки — переведена на `E_VARIANT_CTOR_ARITY`, база `expect-cc-error` 8 → 7.
Отказ включён: **да**.

**КАРИНА:** строк `novac/divergences.allow` 8 → 6 (в группах «undeclared_type/vec_cap/option_ctor/handed_free_fn» — 4 → 3; в бриф «4 → 2» не сошлось — в файле восемь строк, сняты ровно две `option_ctor`). Исход: `neg_1` — novac отверг / оракул отверг (база: принял); `neg_2` — novac отверг / оракул отверг (база: принял); `pos_1` — оба приняли. Раздел 274.12 помечен «УШЛО 2026-10-01 (№1517, ветка `p1517-variant-ctor-check`)». novac собирается оракулом с фиксом (`nova build novac/src/main.nv`, ok).

**ФИКСТУРЫ:** `spec_tests/conformance/neg/p1517_variant_ctor_payload_type_neg.nv` (14 построчных `nova:expect E7301`), `p1517_variant_ctor_arity_neg.nv` (6 × `E_VARIANT_CTOR_ARITY`), `p1517_variant_ctor_untyped_neg.nv` (5 × `E_VARIANT_CTOR_UNTYPED`), `standalone/p1517_variant_ctor_ok.nv`. `--filter p1517`: 4/4. Проба в обе стороны на одном дереве фикстур: бинарь без проверки (сборка `fc6277ba9`) — arity/untyped `NEG-NO-ERROR`, payload_type `NEG-WRONG-MSG` (срабатывает только старый `E_ARG_ELEM_TYPE_MISMATCH`), plan103 — `NEG-NO-ERROR`, позитив PASS; бинарь с проверкой — все PASS. Выключателя в коде нет (проба через базовый бинарь).

**СОСЕДИ:** `--filter` p1448 4/0, p1260 1/0, d55 4/0, option 11/0, result 13/0, sum 135/1 (упавшая — `soundness/assume_trust_introduced_warn`, `E_D78_MODULE_PATH_MISMATCH`, так же красна на базе; в фильтр попала подстрокой «as**sum**e»), variant 24/0, enum 3/0, match 46/0; `nova test std/src/collections std/src/text std/src/io` — 18/0; `check-std-test-baseline` — ok, «НОВЫЙ ОТКАЗ» нет (печатает «почищено»: `encoding/serde/decode_errors_test`, `net/addr` — база не тронута); стражи реестра (rows-intact, single-verdict, routes, entry-shape), `arch-ratchet`, `check-nova-expect-ratchet`, `check-expect-cc-error-ratchet`, `check-ecode-fixture-debt`, `check-diag-fixture-coverage` — ok.

**КРЕЙТ:** compiler-codegen `--lib` 1284 прошло / 0 упало.

**ХРАПОВИК:** emit_c строк 67690 (не тронут; потолок 68857).

**LINT:** `nova lint --deny spec_tests` — 6 находок, ВСЕ в чужой `standalone/p1498_c_ident_bypass.nv` (`W_NON_COMPOUND_ASSIGN`, строки 82–84, 101, 145, 152), они на `integrate` до этой ветки; в файлах ветки 0. Не правил: фикстура проверяет чтение экранированных имён, и замена `ctx = ctx + …` на `+=` может увести её с проверяемого пути — решать её автору.

**КОММИТЫ:** `de706f546` (дверь + фикстуры + цена + реестр №1517 + allow/274.12), `463d2141b` (перевод plan103-фикстуры с Си на чекер), последний — этот отчёт.

**ОТКРЫТО** (найдено по ходу, вне класса; всё красно и на базе):
1. Переменная `Option[int]` в `Option[str]`: `ro y = Some(1); ro x Option[str] = y` — `check` PASS (сравнение аргументов типа есть только у двери аргумента, `E_ARG_ELEM_TYPE_MISMATCH`). Сосед класса (позиции без сверки аргументов типа — родня №906/№959), конструктор тут не участвует.
2. D55 в `Option`: `fn w() -> Option[int] => 9` — `check` PASS, clang «returning 'nova_int' … 'NovaOpt_nova_int'»; `t("auto")` при `t(o Option[str])` — `check` отказывает `E7301`. Спека (D55, «`ro opt Option[str] = "alice"` // Some("alice")») обещает обе формы.
3. `fn fl() -> Option[f64] => Some(1)` — `check` верно принимает (литерал без точки в позиции `f64`, амендмент D55 2026-09-04), кодоген строит `NovaOpt_nova_int`, clang падает. Кодоген, не чекер.
4. `ro x = []` (пустой массив без типа) — `check` PASS; та же форма «нет источника типа», что голое `None`, другой конструктор.
5. Строка №262 (перепись детей «кольца permissive») мною не правилась: №1517 закрывает три следа W4 — интегратору решить, вносить ли в перепись.

**ВОПРОСЫ ИНТЕГРАТОРУ:**
1. Новые коды `E_VARIANT_CTOR_ARITY` и `E_VARIANT_CTOR_UNTYPED` — языкового поведения не меняют (обе формы и раньше не были законными: первая падала в C, вторая противоречит D55/D88), поэтому D-блок не заводил. Варианты: (а) так и оставить; (б) записать оба кода строкой в D406/D55. Рекомендация: (б) короткой строкой в D55 «Позиции…» про голое `None` — правило там уже сформулировано («выводится тип значения»), не хватает называния отказа.
2. Пункты ОТКРЫТО 1–4 — завести ли строками (номера ваши). Рекомендация: 1 и 2 — да (К2: 1 — тихий пропуск в `check`, 2 — CC-FAIL на форме, которую спека называет каноничной); 3 — к №1517 не относится, но та же дверь «чекер решил, кодоген не прочитал» (№262); 4 — по усмотрению.
3. Номера 1505–1516 внесены в `gaps=` (в этом дереве строк нет) — при слиянии сократить на те, что приедут.
