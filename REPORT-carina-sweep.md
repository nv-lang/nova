# REPORT-carina-sweep — подметание меры 0.2 (ветка p274-carina-sweep)

База: `origin/p274-carina-cloud` = `8405e5fcd`. Замер интегратора по группам был на `b8d501817`;
на `8405e5fcd` мой замер: **145, ICE 0** — сходится с заданием, и счёт по моим группам тоже
сошёлся (кроме «bare `None`»: 6, а не 5–6; «unknown name»: 4, как сказано).

Всё — Linux (облако), `libgc-dev` поставлен из apt, оракул собран `cargo build --release`.
Строк журнала времени не добавлял: потолок долей за день достигнут (по заданию).
Модели агентов: один агент `spec-reader` на sonnet (вопросы про приватный тип в
публичной сигнатуре и про приёмник методов value-записи).

## ГРУППЫ

Путь: (а) правится исходник, (б) дописана форма Карины, (в) оставлено.

| группа | было → стало | путь | довод | места |
|---|---|---|---|---|
| bare `None` … (E2-b3) | 6 → 0 | б | ожидание объявленного возврата течёт в РУКАВА хвостового `match`, как уже текло в ветви хвостового `if` (option_rules сама это обещала); голова и паттерн/guard ожидания не получают | check/typing.nv (два входа хвостового match), check/match_arms.nv (`@type_match`, `@type_arm`) |
| binding's initializer refers to the name it binds (D347 R3) | 2 → 0 | б (дефект Карины) | это НЕ самочтение: `ro name = fns.rows[i].name` читает запись, а `name` после точки — поле. Детектор считал всякий идентификатор. Теперь пропускает имя после `.`/`@` и метку перед `:` | check/binds.nv `is_member_label` |
| unknown name (было 4) | 4 → 0 | б | следствие предыдущей строки: связывание `name` не объявлялось, четыре чтения падали | — |
| голова `for` — range, коллекция «с дженериками» | 2 → 0 | б | `@captures` (SelfField) и `[]str.of(..)` (ArrayLit) — векторные значения, которые все три слоя уже типизируют/копируют/обходят; закрыт был только список видов | sem/node_questions.nv `is_collection_head` |
| E7301 `StringBuilder` → `-> str` | 3 → 0 | б | D429 R2 называет позицию возврата (вкл. last-expr) коэрсибельной, R9 — голое значение каноном; исходник канонический, путь (а) запрещён правилом 4 | check/return_rules.nv `@coerce_return`; sem/channel.nv таблица `coerce_at`; emit_c/emit_place.nv обёртка; sem/coerce.nv `coerce_shell_symbol` |
| `assert` is not a callable novac knows | 3 → 0 (types.nv 2 + pipeline.nv 1; pipeline.nv НЕ правился) | б | прелюдия объявляет `assert` двумя арностями как `extern "nova"`: тела нет нигде, это интринсик (D81) | check/assert_call.nv (новый), check/calls.nv, emit_c/emit_assert.nv (новый), emit_c/emit_expr.nv, builtins.nv `ASSERT_FN` |
| this call omits `op` | 1 → 0 (текст) | б (дефект Карины) | отказ говорил правилом связывания МЕТОДА ДРУГОГО ТИПА (`FnBuilder @store(target, op, src)`) про `exit_code.store(..)` на `AtomicInt`. Теперь строка чужого конкретного приёмника не кандидат, причина названа верно: «this type has no such method». Счёт не меняется (1 → 1 другим текстом); сама причина — handed-методы AtomicInt недоступны — зона интеропа/k2 | check/methods.nv `@receiver_is_foreign` |
| record constructor is compiled only as … | 2 → 0 | б | именованный конструктор — хвост ветви `if`-значения; `@lower_if_value` кладёт его той же дверью `@lower_place`. Анонимный `{ .. }` в ветви остаётся отказом (без ожидания — ICE в emit, замерено). **ВНИМАНИЕ: см. «Мера» — этот коммит демаскирует модуль `lower/` (+146)** | check/check.nv (гейт конструктора, `value_if` для `IfStmt`-хвоста) |
| `offset_in_file` / `file_of_offset` + unknown field | 4 → 4 | в | спека МОЛЧИТ (см. вопросы): `UnitFile` не экспортирован, а экспортные `Unit.files` и две функции его упоминают. Карина по #1491 вообще не регистрирует приватные типы чужого модуля (одна таблица имён по голому имени), оракул принимает. Правка — либо спека, либо типы по модулю в Карине (большое: идентичность приватного типа по пути модуля); `export` в pipeline.nv — вне моей зоны и запрещён правилом 4, пока спека не сказала | main.nv:123,129–131 |
| method on a `value` record needs receiver by POINTER | 3 → 3 | в (развилка) | спека противоречит себе о приёмнике НЕ-mut метода value-записи — цитаты в вопросах | sem/channel.nv |
| effect attribute (`#impl(Serialize)`) | 2 → 2 | в | синтез `Serialize` — генерация сериализатора (обобщения/протокол), большое; сама Карина нарочно его отказывает | diag/diag.nv |
| `type X consume` (B13) | 1 → 1 | в | обязательства потребления нигде не проверяются — принять декларацию = обещать гарантию; большое (274.7 B13) | emit_c/shell.nv |
| `if` in value position | 1 → 1 | в | `if` как голова `match`; открыть позицию можно, но голова — `Option[FnRow]`, и отказ сразу станет «match on an applied sum» (зона appsum) — выигрыша 0 | check/typing.nv:249 |
| `match` is compiled only as … | 1 → 1 | в | `ids = match @deduce_from_args(..)` — `match` справа от `=` над `Option[[]TyId]`: та же история, следующий отказ — applied sum | check/static_call.nv:120 |
| branches of a tail `if` must agree | 1 → 1 | в (чужая зона) | `if k < n { b[k] } else { 0 }` в `-> u8`: голый литерал ветви должен взять тип возврата — правка в `@type_branch_value`, check/tail_rules.nv — зона p274-carina-k1 | lex/lex.nv:293 |

## ФИКСТУРЫ

Все pos — байт-в-байт с оракулом (`scripts/tools/novac-e1-smoke.sh`, stdout и код возврата):

- `tail_match_none/pos_1` да; neg_1 (голова `match None` без ожидания) — проба в обе стороны да;
- `self_init_member/pos_1` да; neg_1 (настоящее самочтение) — да;
- `for_head_value/pos_1` да; neg_1 (`for x in 5`) — да;
- `method_foreign_recv/pos_1` да; neg_1 (`a.settle(1)` на AVal) — да (старый бинарь: «omits `op`», новый: «no such method»);
- `coerce_return/pos_1` да; neg_1 (`ro`-параметр через `consume`-приёмник, у оракула E_READONLY_COERCE) — да;
- `assert_stmt/pos_1` да; neg_1 (три аргумента) — да; плюс проба проваленного `assert(1 > 2, "boom")`: exit 101 и строка паники совпали байт-в-байт;
- `ctor_if_tail/pos_1` да; neg_1 (анонимный конструктор в ветви; без гейта — ICE) — да.

Проба в обе стороны: да, по каждой правке (числа — в сообщениях коммитов).

## САМОСБОРКА ОРАКУЛОМ

Да: `nova build novac/src/main.nv` проходит после каждого коммита (каждая проба собиралась так).

## МЕРА 0.2

- `8405e5fcd`: **145, ICE 0**.
- после всех коммитов, КРОМЕ последнего (`ctor_if_tail`): **129, ICE 0** (−16).
- после всех коммитов: **273, ICE 0** — рост не регресс. Два отказа конструктора в
  lower_match.nv были ПОСЛЕДНИМИ отказами фазы обхода в модуле `lower/`, а юнит с отказом обхода
  не типизируется вовсе. Сняв их, Карина впервые типизирует `lower/` — и за ними стояли 146
  диагностик. Последний коммит отделим: интегратор решает, двигать ли меру с ним.
- double-build (предусловие, принято файлов): база 122/142; без последнего коммита 127/144;
  со всеми 121/144 (файлов стало 144: два новых — check/assert_call.nv, emit_c/emit_assert.nv).

Ожидание задания было −25…−30; вышло −16 без демаскировки: 4 группы — вне зоны или спорны
по спеке, ещё 2 упираются в applied sum.

## ГРУППЫ (топ-15 после, со всеми коммитами, 273)

```
 89 parameter `X` needs a mutable place (`X`, or a `X` binding) -- this argument is not one (P14)
 47 outside the subset: this type has no such method in the declarations novac was handed
 27 the operands are different numeric types -- no implicit widening (D405 ...)
 26 outside the subset: a `X` on an applied sum (`X`) is read but not compiled yet ...
 24 outside the subset: a cast `X` is not compiled yet (E2-b, with the numeric family)
 24 outside the subset: a `X` arm on a string or char literal is read but not compiled yet ...
  4 the pattern's payload count disagrees with the variant's declaration ... (D59)
  4 `X` on a sum needs the type to implement `X` (D363) ...
  3 outside the subset: a method on a `X` record needs the receiver taken by POINTER ...
  3 outside the subset: `X` is not a callable novac knows in an expression ... (file_of_offset/offset_in_file)
  3 a bare `X` here has no type to take ... (None как АРГУМЕНТ вызова, другая форма)
  2 outside the subset: this argument's type is not the one this function declares here
  2 outside the subset: the shell novac links into carries no `X` for Vec[TyId] ...
  2 outside the subset: an effect attribute is read but not compiled yet
  1 unknown field: this type has no field with this name
```

Без последнего коммита (129) верх такой: 27 numeric types, 24 cast, 24 string arm,
11 «this type has no such method», 10 applied sum, 4 D59, 3 P14, 3 value-record, 3 shell `index(Range)`,
3 file_of_offset/offset_in_file, 2 effect attribute, 2 record constructor, далее одиночки.

## СТРАЖИ

`sh scripts/tools/novac-gate-guards.sh .` на `cd116f0`: 98 прогнано, 91 ok, 7 красных, 7 тяжёлых пропущено
раннером. Разбор красных:

- `check-novac-branch-complete` (4 неполных ветвления — мои) → починено `e3445d5`, ok;
- `check-novac-ctx-tables` (таблица канала `coerce_at` не объявлена в 274 §10.3б) → строка в плане, `e3445d5`, ok;
- `check-novac-lowering-one-door` (арм `NodeKind.Lit` в emit_assert.nv, 20 при базе 19) → `212de7c`, ok;
- `check-novac-surface` (builtins +1, sem +3) → база поднята со строками хроники, `212de7c`, ok;
- `check-novac-commit-no-simplification` — известный красный раннера (Errno 21: «Is a directory: '.'»);
- `check-novac-local-only-work` — ветка не была на origin; уходит пушем;
- `check-novac-time-ledger` — неглубокий клон облака («нужен fetch-depth 0»). Строк леджера я по заданию
  не добавлял: на полном клоне он, вероятно, покраснеет на коммитах этой ветки — решает интегратор.

После правок четыре починенных стража перепрогнаны точечно — ok; `fixture-expect` ok (47 пришпиленных).
Повторный полный прогон раннера после `212de7c` не делал.

## МОДУЛЬНЫЕ ТЕСТЫ

`nova test <каталог>` (как `check-novac-module-tests.sh`): novac/src/check — PASS 1 / FAIL 0;
novac/src/pipeline — PASS 1 / FAIL 0; novac/src/sem — PASS 1 / FAIL 0; novac/src/resolve — PASS 1 / FAIL 0
(каждый каталог — один юнит со всеми его `*_test.nv`); после последних правок перепрогнаны — то же.

## ДЕФЕКТЫ ОРАКУЛА/КАРИНЫ

1. **Карина, P14 по полю приёмника (класс, ~89 мест после демаскировки).** `arg_class`
   (check/type_of.nv) отвечает `ArgRo` на любой `FieldAccess` и `ArgOwnedTemp` на `SelfField`,
   поэтому `@ir.temp(t)` внутри `fn Lowerer mut @...` (поле `mut`-приёмника, метод `FnBuilder mut @temp`)
   отказывается как «parameter `@` needs a mutable place». Оракул это принимает. Проба: весь
   модуль `novac/src/lower/` после коммита ctor_if_tail. Это крупнейшая группа меры после
   демаскировки — первым кандидатом в следующую волну.
2. **Оракул, permissive (D102 п.2, правка 2026-09-04).** Обязательный параметр, переданный по
   имени, принят: `fn pick(name int, size int) -> int`, `ro p = pick(name: 5, size: 6)` —
   `nova check`/`build` проходят и печатают 56; Карина отказывает («has no default value, so it is
   passed positionally, not by name (D102)») — Карина права.
3. **Карина, permissive (D30, E_TYPE_NAME_TOO_SHORT).** `type A { n int }` — Карина принимает,
   оракул отказывает «type name `A` is a single character».
4. **Карина, принято → C не собирается.** `consume sb = StringBuilder.new()` (дефолт
   `cap = INITIAL_CAPACITY`): `novac check` чисто, `emit` печатает
   `Nova_StringBuilder_static_new(INITIAL_CAPACITY)`, clang: «use of undeclared identifier
   'INITIAL_CAPACITY'» — дефолт-выражение handed-функции с приватной константой std. Проба:
   `fn f() -> str { consume sb = StringBuilder.new() sb.append("x") sb }` + `println(f())`.
5. **Карина, манглинг `consume`-методов шелла.** `c_shell_method` (sem/mangle.nv) знает только
   `Nova_<T>_method_<name>`, а шелл пишет `consume`-приёмник как `Nova_<T>_consume_<name>`
   (`Nova_StringBuilder_consume_into_str`): явный `sb.into_str()` отказывается «the shell carries no
   `into_str` for StringBuilder». Для коэрции возврата написание взято отдельно
   (`coerce_shell_symbol`, помечено INTEROP); саму дверь манглинга не трогал — зона k1.
6. **Карина, отказ после демаскировки.** `None` как АРГУМЕНТ вызова (`@lower_if(st, None)`,
   параметр `Option[..]`) — отказ «bare `None`»: ожидание параметра в аргумент не течёт. Не моя
   группа (она появилась только после ctor_if_tail).

## КОММИТЫ

```
7620ffc novac: a bare `None` in an arm of a tail match takes the declared return
ef0c75e novac: a member name after `.`/`@` or a label before `:` is not a self-read
5cd1eb9 novac: a receiver field and an array literal are `for` collection heads
3c386be novac: a method of ANOTHER type no longer speaks for a call it was not asked
995388d novac: a `#coerce` pair carries a value to the declared return (D429 R2, R9)
16ff297 novac: name the silent exit of `@type_arm` after its pattern refusal
02c3871 novac: the prelude's `assert(cond[, msg])` is typed and emitted (D81)
cd116f0 novac: a named record constructor is a legal `if`-value branch tail   <- отделимый
e3445d5 novac: Carina's guards on this sweep -- complete branches, the new channel table declared
212de7c novac: the sweep's surface raised by name, the assert message read without a kind arm
```

Если интегратор откладывает `cd116f0`, то `e3445d5` и `212de7c` от него не зависят: они правят
файлы других коммитов и базу поверхности.

Замечание к `cd116f0`: строка `Spec: D405` выбрана неудачно (D405 — о конверсиях, о позициях
конструктора не говорит); честнее было `Spec: none — ...`. История не переписывалась.

## ОТКРЫТО

- группы (в) из таблицы; P14 по полям — главный следующий кандидат;
- applied sum закрывает собой ещё 2 моих одиночки (if-голова match, match справа от `=`).

## ВОПРОСЫ ИНТЕГРАТОРУ

1. **Двигать ли меру коммитом `cd116f0`** (129 → 273 при ICE 0). Варианты: (а) слить — мера честно
   показывает спрятанный модуль `lower/`; (б) отложить до волны P14. Рекомендую (а): спрятанное
   расстояние всё равно наше, а 89 из 146 — один класс с одной дверью (`arg_class`).
2. **Приватный тип в публичной сигнатуре/поле** (`UnitFile` в `export type Unit` и в `export fn
   file_of_offset`). Спека молчит (spec-reader: по `spec/` и `docs/dev/` норм нет; D47 —
   07-modules.md:947–948 «без `export` = приватная для модуля»; поля export-типа публичны —
   07-modules.md:1011; единственное соседнее правило — предупреждение про `export`-метод на
   неэкспортном типе, 07-modules.md:1053–1065). Варианты: (а) спека разрешает (как оракул) — тогда
   Карине нужны типы, идентичные по модулю, а не по голому имени (#1491 ввёл фильтр как раз из-за
   коллизии); (б) спека запрещает private-in-public (как E0446 в Rust) — тогда `export type UnitFile`
   в pipeline.nv (путь а) и дефект оракула. Рекомендую (б): дешевле, однозначнее, и соответствует
   D47 «приватный тип — модуля»; номер D-блока/поправки — ваш.
3. **Приёмник не-mut метода value-записи.** Противоречие спеки (02-types.md):
   :14473–14474 «Method receiver `@`: pointer на stack-slot (`NovaValue_X*`) — мутации видны
   caller'у» (без различия mut/ro) против :3390 и :3422–3423 «value categories — by-value normally,
   но `mut @` receiver требует pointer»; для named tuple :5197 прямо: ro — по значению, `mut @` —
   указатель. Оракул пишет `NovaValue_X*` всегда. Решить, какая трактовка нормативна для `ro`-метода;
   от этого зависит, хватит ли Карине передачи по значению для трёх методов sem/channel.nv.
