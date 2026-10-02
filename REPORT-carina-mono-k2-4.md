# REPORT-carina-mono-k2-4 — shell str, статические двойники, D84 «конкретное побеждает generic» (ветка p274-carina-mono-k2 поверх 1297b36ec)

**База меры: 55, ICE 0 на `1297b36ec`** (слияние `origin/main` 2488a47be в ветку, шаг 1 задания;
Linux, оракул пересобран после слияния, кэш смоука сброшен). **Итог: 55 → 33, ICE 0** на `e552389d5`.
Команда меры — из задания: `NOVAC_UNIT=1 NOVAC_SELF_PATH=novac/src novac check <все не-тестовые файлы novac/src>`.

## ИТОГ

| группа | было → стало | чья | путь | довод |
|---|---|---|---|---|
| несколько статических одного имени (sem/collect.nv) | 8 → 0 | моя | б (Карина) | статический двойник №1354: копия файла, поданная обратно, считалась вторым статическим |
| (каскады статических: `??` над их результатом, представление newtype) | 2 → 0 | — | — | ушли с причиной |
| shell str: `replace` ×5, `is_dir` ×1 | 6 → 0 | моя | б | необобщённое поданное тело, которого нет в оболочке, Карина делает сама — как обобщённое |
| `?? return` (D86) в `sem/handed_bodies.nv` | 2 → 0 | моя (мои строки) | б | переписано через `match` вместе с дверью `@instantiates` |
| D84 ничья `to_str` (main.nv ×4, pipeline.nv, sem/mangle.nv) | 6 → 0 | ничья → взял | б | D84 фильтр 4 п.1 «concrete побеждает generic» — у `dominates` этой оси не было |
| (вскрыто ничьей `to_str`) `??` над `Result` (main.nv:171, sem/mangle.nv:570) | 0 → 2 | cast (`??`) | — | честный отказ подмножества, раньше прятался за ничьей |

**Мера 0.2: 55 → 33, ICE 0.** Сверка списков (файл:строка:сообщение) до и после каждого шага:
новых сообщений нет, кроме двух `??` над `Result`, которые вскрыл выбор `[]u8 @to_str`.

### Чего мера НЕ видит — и сколько там (замер, не оценка)

`compile_with` типизирует тела экземпляров, только если единица прошла `check` без отказов
(pipeline.nv: `if diags.len() > 0 { return diags }`). В самосборке отказы есть почти в каждой
единице, поэтому отказы ВНУТРИ сделанных тел мера не показывает. Замерил отдельным бинарём, где
этот ранний выход снят (правка в scratchpad, в ветку не входит): **33 + 4 = 37**. Четыре скрытых:

- `[]u8 @to_str` (теперь выбирается по D84): `str.alloc_copy(...)` — «this type declares no static
  method of this name» и `Err(...)` — «this variant name is declared by more than one sum»;
- `Vec[T] @ptr() -> *T => @data` для `T = str` и `T = Node`: «cannot return value of type `*mut T`
  from a function declared `-> *T` (E7301)».

Ни один не новый класс моих правок: первые два — дефекты ниже (п. 3), третий — обобщённый
экземпляр, он был скрыт и до задания.

## ЧТО СДЕЛАНО — ПО КЛАССАМ

1. **Статический двойник №1354** (`0bb67d3f9`). `FnTable @file_declares_method` получил ВЛАДЕЛЬЦА,
   `seal_own` кладёт в цепочку и статические; поданный статический спрашивается по типу-владельцу,
   метод — по `no_ty()`, так что `T.make()` и `T @make()` не встречаются. `check/static_call.nv`:
   статический другого модуля ПРОГРАММЫ компилирует его единица-владелец (`in_program`), как
   свободные и методы.
2. **Необобщённое поданное тело** (`ce3449ad1`). `Ctx @instantiates` — одна дверь для обобщённых и
   необобщённых: тело без оболочки делается с пустой подстановкой; модуль программы — не здесь;
   `extern "nova"` — не из чего. `check/calls.nv`: «подано» — факт строки (`compiles_body` false),
   а не написание модуля: std подаётся и без строки `module`, и `handed_from != ""` пропускал
   вопрос к оболочке — `str @trim_ascii` звал `nova_fn_11is_ascii_ws`, которого нет (ошибка линковки).
   Попутно — «одна причина, одна диагностика» в сделанном теле: отказ внутри `unsafe { }` и чтение
   отравленного имени не дают второго E_UNSAFE_UNUSED; приведение отравленного операнда молчит
   (`check/unsafe.nv`, `check/casts.nv` — **casts.nv — файл группы cast**: три строки молчания перед
   разбором пары, конфликт при слиянии маловероятен, но предупреждаю).
3. **`unsafe { e }` — это `e` в любой позиции значения** (`1163c3c65`, D216: блок — контекст
   типизации). Тело `trim_ascii` пишет `unsafe { @ptr().read_at(i) } as int`. Одна дверь
   `sole_expr_of_unsafe` (sem, база поверхности 338 → 339 строкой хроники): гейт позиции пускает
   такой блок везде, эмиттер печатает его выражением; блок с инструкциями — по-прежнему только там,
   где его поднимает lowering.
4. **D84 фильтр 4 п.1** (`e552389d5`). В `dominates`, где режимы равны, строка без параметров типа
   бьёт обобщённую. Довод — спека, не оракул: `spec/decisions/10-overloading.md:83` («Concrete
   побеждает generic»), `:814-816` (D285 §2 п.1 — «единственная ось, которая остаётся»),
   `spec/conversions.ru.md:298-300` называет ровно пару `[]u8 @to_str` / bare-T blanket.

## ФИКСТУРЫ (байт-в-байт с оракулом — `novac-e1-smoke.sh`)

- `handed_plain_body/pos_1` — `trim_ascii`, `_start`, `_end`: сделанный метод, приватная
  `is_ascii_ws` std под именем Карины, `unsafe` операндом `as` внутри тела — **да**.
- `unsafe_value_operand/pos_1` — блок операндом приведения и сложения — **да**;
  `neg_1` — неизвестное имя внутри `unsafe`: один отказ (`NOVAC_EXPECT unknown name`).
- `handed_free_fn/neg_1 → pos_2` (`adler32_init`) — **да**; строка ушла из `novac/divergences.allow`,
  в `docs/plans/274.12-novac-divergences.md` раздел помечен ЗАКРЫТО (условие снятия, названное там
  же, выполнено). Попутно поправил число в шапке зеркала: там стояло «ЧЕТЫРЕ» с `option_ctor/neg_1,2`,
  которых в allow к 2026-10-02 уже не было; теперь «ОДНА» (`examples/basics/vec_cap.nv`).
- `std_method_body/neg_1 → neg_2` (`to_ascii_lower` сделан; внутри ОДИН отказ — `[]u8.new(cap: n)`,
  №1457) и новый `neg_1` на `str @hash` (`extern "nova"`, тела нет) — несёт каскадную пробу.
- D84: фикстуры нет — обобщённый метод в пользовательском файле вне подмножества («generic
  parameters are not compiled yet»); держит модульный тест.

**Проба в обе стороны** (условие выключено → красно; возвращено → зелено), по каждой правке:
статический двойник — тест red; `methods.nv` без `@instantiates` — 5 тестов pipeline red; `calls.nv`
— тесты W6 и шага 2a red; гейт `unsafe` без двери — тест red и `unsafe_value_operand/pos_1` даёт 2
отказа; каскад отказа в `unsafe` — тест red; отравленное в `unsafe` и в приведении —
`std_method_body/neg_2` даёт 2 и 3 диагностики; D84 — тест red.

## МОДУЛЬНЫЕ ТЕСТЫ (`--timeout 400`)

`check-novac-module-tests.sh`: модулей 10 (файлов 43), PASS 10, FAIL 0. Контракты, которые сдвинулись
(каждый с сохранённым контролем на теле без тела — `extern "nova"`): pipeline_test («E2 strings
surface», «receiver MODE», «`Self` in a declaration» — сделанное тело `@twin() -> Self => 0` теперь
отказано как `int` против `-> Cel`, что и доказывает чтение `Self`), handed_self_test («a REAL
overload»), handed_free_test (W6 и шаг 2a). Новые: `handed_plain_test.nv` (4 теста),
`overload_test` «concrete beats generic».

## СТРАЖИ

`novac-gate-guards.sh` (в облаке целиком не укладывается в 10 минут — прогнан тремя частями тем же
списком): все ok, кроме двух средовых — `check-novac-time-ledger` (неглубокий клон) и
`check-novac-oracle-fresh` (сравнивает `nova-cli/target/release/nova.exe` — в облаке это симлинк со
своим mtime 10-01; сам бинарь `nova` собран 03:05, исходники оракула — 02:50, `cargo build` —
«Finished … 0.06s»). `check-novac-surface` был красным на +1 экспорт sem — база поднята строкой
хроники. Тяжёлые: `check-novac-no-panic` ok (230 фикстур); `check-novac-differential` этап 1 ok
(112 фикстур, исходы совпали, в allow 0), этапы 2–3 не уложились в 10 минут — смоук новых и
перевёрнутых фикстур прогнан поштучно (выше).

## НОВЫЕ ДЕФЕКТЫ (реестр не трогал — строки ваши)

1. **P14: цепочка `-> @` законна, Карина её не берёт** (Карина). `consume sb = StringBuilder.new(cap: 8)`
   `sb.append("a").append("b")` → «this type has no such method in the declarations novac was handed»;
   оракул — PASS. По спеке законно: таблица D326 (`spec/decisions/02-types.md:3963-3977`) и амендмент
   D33 (`02-types.md:13577-13604`) — mut-самовид, вся-mut-цепочка законна. Это то, во что теперь
   упирается тело `str @replace` (сделанное, вместо отказа «оболочка не несёт»). Класс: результат
   `-> @`-метода — тот же получатель, а не временное; чинить флагом `-> @` в `FnDef`. Проба:
   ```nova
   fn f() -> str {
       consume sb = StringBuilder.new(cap: 8)
       sb.append("a").append("b")
       sb.into_str()
   }
   fn main() { println(f()) }
   ```
2. **ICE «method call reached type_of with no decision in the channel»** (Карина, существовал до
   задания — на `1297b36ec` тоже). Проба: `fn f(s str) -> int => StringBuilder.new(cap: s.byte_len() + 2 * 3).append_repeat(s, 2).append(s).into_str().byte_len()`
   + `fn main() { println(f("ab")) }`; оракул — PASS. Это тело `str @pad_start` дословно — вскрылось,
   когда тела стали делаться.
3. **Статический метод ПРИМИТИВА — ложный языковой вердикт** (Карина). `ro s = str.new()` →
   E_NOVAC_LANG «this type declares no static method of this name»; оракул — PASS (std:
   `export fn str.new() -> Self => ""`). `check/static_call.nv` пропускает владельца-примитив
   намеренно («A static method of a PRIMITIVE names no declaration»), а дверь вариантов
   (`check/variant_rules.nv:228`) называет это ошибкой программы. Минимум — честный отказ
   подмножества вместо языкового; полнота — статические примитивов. Именно сюда теперь упирается
   `[]u8 @to_str` (`str.alloc_copy`).
4. **`Err(...)` в теле `[]u8 @to_str`: «this variant name is declared by more than one sum»** (Карина,
   скрытый экземпляр, замер выше). Ожидание — возвращаемый `Result[str, Utf8Error]`; по D478 вариант
   выбирается по ожиданию. Пробой из пользовательского кода не воспроизводил — только в экземпляре.
5. **`Vec[T] @ptr() -> *T => @data` (поле `*mut T`)** — Карина отказывает E7301 в экземпляре; оракул
   собирает. Разрешено ли `*mut T → *T` без `as` — D216/D491 не читал до вердикта; вопрос, а не дефект.

## ОРАКУЛ

Корней в оракуле не найдено. `docs/plans/repro/` не трогал.

## РАЗВИЛКИ

Нет. D84 «concrete побеждает generic» — ответ спеки, не выбор (цитаты выше).

## АГЕНТЫ

Один spec-reader на **sonnet** (D84: concrete против blanket) — ответ сверен по файлу перед правкой.

## КОММИТЫ

```
e552389d5 novac: a concrete method beats the blanket where the modes tie (D84 filter 4 rule 1)
1163c3c65 novac: `unsafe { e }` with no statement is `e` in every value position
ce3449ad1 novac: a NON-generic handed body the shell lacks is made from its text
0bb67d3f9 novac: a STATIC method of the file handed back to it is one row, not two
```

## ОТКРЫТО (мера, 33) — по причинам и владельцам

| причина | шт | владелец | места |
|---|---|---|---|
| `?` (Try) не компилируется | 5 | cast (`?`) | resolve.nv:693, sem/defs.nv:231/246/264, sem/sem.nv:454 |
| число полезных значений образца ≠ варианту | 4 | k1 (payload count) | mono/mono.nv:302, :361 ×3 |
| вариант берёт другое число значений, чем дал вызов | 2 | k1 (payload count) | sem/callables.nv:42, :52 |
| `??` над `Result` | 2 | cast (`??`) | main.nv:171, sem/mangle.nv:570 |
| нет такого метода в поданных объявлениях (`store`, `clear`, `dist`) | 3 | ничья | main.nv:321, names/names.nv:95 (вслед за `HashMap[..]` names.nv:86), source/source.nv:49 (`*u8 @dist`, D216) |
| атрибут эффекта | 2 | ничья | diag/diag.nv:53, :61 |
| ветвь хвостового `if` без значения | 2 | ничья | lower/lower_match.nv:397, sem/sem.nv:492 |
| `if`/`else if`/`match` в позиции значения, ветви не согласны | 4 | ничья | check/typing.nv:249, sem/mangle.nv:478, check/static_call.nv:120, lex/lex.nv:293 |
| диспетчеризация оператора через протокол (`==`, `!`) | 2 | ничья | types/types.nv:152, main.nv:189 |
| прочие по одному: тип, применённый к аргументам; must-consume тип; индекс не-`int`; аргумент типа суммы не выводится; аргумент не представление newtype; `ALL` после точки (не сумма); P14 `kids` | 7 | ничья | names.nv:86, emit_c/shell.nv:96, sem/sem.nv:143, sem/sem.nv:156, sem/collect.nv:663, sem/collect.nv:233, parse/pattern.nv:73 |

Мои группы (shell str, статические) — **0**. D363/голый `None` (sweep) и №1573 (strarm) в мере не
встречаются.
