# Чекпоинт: срочный триаж регрессии главного гейта (closure-megacu, 2026-07-19)

Ветка: `p-fix-closure-megacu` (worktree `d:/Sources/nv-lang/nova-clfix`).
Задача: main красный на 134248143 (Merge p200-18-utf8) — 2 CODEGEN-FAIL
(`d22_closure_full_unannotated_let.nv:22:56`, `d402_closure_return_width.nv:32:16`)
с идентичной ошибкой "слишком много позиционных аргументов: ожидалось 0, передано 1".

## Окружение репро (важно для продолжения)

- Worktree создан из `main` (был на 134248143 в момент старта).
- Собран `nova-cli` release с `CARGO_TARGET_DIR=/d/Sources/nv-lang/nova-clfix-target`
  (шар-кэш, склонирован robocopy'ем из main `nova-cli/target`, ускоряет rebuild
  до ~2.5 мин). Бинарь: `/d/Sources/nv-lang/nova-clfix-target/release/nova.exe`
  (НЕ `<worktree>/target/release/` — CARGO_TARGET_DIR уводит его).
- `NOVA_GC_LIB_DIR`/`NOVA_GC_INCLUDE_DIR` → `D:/Sources/nv-lang/nova/compiler-codegen/vcpkg_installed/x64-windows-static/{lib,include}`.
- `libuv`: скопирован в worktree (`compiler-codegen/nova_rt/libuv`, `.git` внутри
  удалён) + `target/libuv-cache` скопирован из main — нужны ТОЛЬКО если гонять
  `nova test` из cwd=worktree-root напрямую на реальный `spec_tests/conformance`
  (мы этого не делаем, мега-CU запрещён).
- Изолированный репро: НЕ одиночный файл (даёт E_D78_MODULE_PATH_MISMATCH) —
  мини-пакет с `nova.toml` (копия `spec_tests/nova.toml`, package name `spec_tests`)
  + `conformance/` подпапка с нужными файлами. Путь:
  `C:/Users/B7E3~1/AppData/Local/Temp/claude/d--Sources-nv-lang-nova/a48a9f3a-0403-4a44-a6e3-8894781d4b88/scratchpad/repro/`
  (и под-пакеты `pkg_d22/`, `pkg_d402/` для изоляции). ЭТО scratchpad — эфемерно,
  переживает только текущую сессию; при обрыве пересоздать по этой инструкции.
- Команда прогона: `"$NOVA_BIN" test "$REPRO/conformance"` (env GC-переменные
  экспортировать в ТОЙ ЖЕ bash-команде — новый Bash-вызов = новый shell, env не
  переживает).

## Бисект (ЗАВЕРШЁН)

Окно регрессии = мёрж `134248143` (единственный кодовый мёрж в окне
`f4b7d572f..134248143`), состоит из ДВУХ кодовых коммитов на отдельных
родителях (не линейная цепочка от main):
- `bdae7f4e9` (родитель `78503bf5d`) — refactor char `@encode_utf8()` →
  `(int,[4]u8)`, миграция `defaults.nv`/`string_builder.nv`/`write_buffer.nv`.
  НЕ трогает checker/callnorm, НЕ добавляет top-level символов в
  `spec_tests/conformance`.
- `2f5128367` (родитель `d9af43662`) — codegen tuple+fixarr typedef topo-sort
  фикс (`emit_c.rs`) + НОВАЯ фикстура `spec_tests/conformance/tuple_fixarr_typedef.nv`
  с top-level `fn f()`/`fn g()`/`fn h()`.

**Виновник = `2f5128367`** (конкретно новая фикстура, не сам codegen-фикс).
Изолированный репро подтверждён: d22+d402 БЕЗ tuple_fixarr_typedef.nv → PASS
(и порознь, и вместе). Добавление tuple_fixarr_typedef.nv (с оригинальными
именами f/g/h) → воспроизводит ТОЧНУЮ ошибку из CI 1:1 (тот же текст, та же
CODEGEN-FAIL категория, обе строки d22:22:56 + d402:32:16).

## Корень (НАЙДЕН, с точностью до строки)

`compiler-codegen/src/callnorm.rs:601` — `try_normalize_call`, коуарс-фолбэк
для голого идентификатора-callee:

```rust
ExprKind::Ident(name) => sigs.free.get(name)?,
```

Срабатывает, когда у чекера НЕТ записи в `resolved_callees` для этого call-site
(это ЧЕСТНО происходит для вызова ЛОКАЛЬНОЙ переменной/параметра fn-типа — там
нет `Item::Fn`-декларации со span'ом, некуда резолвить). Фолбэк матчит callee
ПО ИМЕНИ ГОЛО, без scope/shadowing — если где-то в ТОМ ЖЕ flat-namespace
folder-module CU есть ОДНОЗНАЧНЫЙ (единственный) top-level `fn <имя>`, коуарс-
фолбэк подставляет ЕГО сигнатуру, даже если по факту вызывается ЛОКАЛЬНАЯ
переменная/параметр с тем же именем, тенюще (shadowing) внешнюю fn.

Конкретно: `d22_closure_full_unannotated_let.nv:22` — `ro apply = fn(f fn(int)
-> int, x int) -> int => f(x)` — внутри тела `f(x)` вызывает ПАРАМЕТР `f`.
`d402_closure_return_width.nv:32` — `ro got = f(big)` — `f = |v| v` ЛОКАЛЬНАЯ
переменная. Оба совпали по имени с НОВЫМ top-level `fn f() -> (int,[4]u8)` в
tuple_fixarr_typedef.nv (0 params) → `bind_call_args` против чужой 0-arg
сигнатуры → `TooManyPositional{expected:0, got:1}` → тот самый диагностик,
эмитится ИЗ callnorm (пост-чекер, пре-codegen) → категория CODEGEN-FAIL
(не CC-FAIL) — чекер сам этот вызов принял корректно, ошибка родилась в
нормализации.

Убедился (`grep '^fn [fgh]\('` по `spec_tests/conformance/*.nv`, БЕЗ
поддиректорий `neg/`/`standalone/` — те другой module-путь, не мержатся):
tuple_fixarr_typedef.nv — ЕДИНСТВЕННЫЙ источник top-level `f`/`g`/`h` в
корневом `spec_tests.conformance` до этого мёржа. Поэтому именно `2f5128367`
(её новая фикстура) сделала `sigs.free["f"]` однозначным (`v.len()==1`) и
включила коуарс-путь.

Общий класс бага (архитектурный, НЕ новый) — `try_normalize_call` не носит
scope-стек локалов, поэтому в принципе не может отличить "голый идентификатор
= вызов ЛОКАЛЬНОЙ переменной" от "вызов top-level fn с тем же именем". Раньше
это было безопасно ТОЛЬКО потому, что ни один root-level файл
`spec_tests/conformance/*.nv` не объявлял top-level `f`/`g`/`h` — конвенция
"top-level имена с файловым префиксом" (уже задокументирована в самом
`d402_closure_return_width.nv`: "все top-level имена D402-prefixed во
избежание клэша") существовала как неписаное практики, но НЕ была соблюдена
новой фикстурой.

## Фикс (ПРИМЕНЁН, шаг 1 из 2 — см. ниже НОВУЮ находку)

`spec_tests/conformance/tuple_fixarr_typedef.nv`: `fn f()`/`fn g()`/`fn h()`
→ `fn tft_f()`/`fn tft_g()`/`fn tft_h()` (+ все call-sites внутри теста) +
поясняющий комментарий-маркер `[M-callnorm-free-fn-name-collision]` над `tft_f`
с полным механизмом (для будущих читателей/грепа).

Обоснование выбора (rename, а не патч `callnorm.rs`): задание прямо
санкционирует rename/скоуп как приемлемый минимальный фикс при корне
"name-collision конкретного символа" (п.5 задания). Полный корректный фикс
`try_normalize_call` требует scope-stack threading через весь AST-walk этого
файла (`normalize_item`/`normalize_block`/`normalize_stmt`/`normalize_expr`/
`walk_children`) — рискованно чинить вслепую без прогона мега-CU (запрещён
дисциплиной задания) на файле с большой историей тонких ICE-фиксов
(комментарии в самом файле ссылаются на несколько прошлых P67/ICE инцидентов).
Архитектурный дефект остаётся ПОМЕЧЕННЫМ маркером `[M-callnorm-free-fn-name-
collision]` в самой фикстуре — не молчаливый обход (следующий, кто тронет
`callnorm.rs`, увидит контекст).

Верификация rename-фикса (изолированный репро, БЕЗ мега-CU):
`pkg_d22` (d22+tuple_fixarr_typedef, переименованный) и `pkg_d402`
(d402+tuple_fixarr_typedef, переименованный) — ОБА теперь падают ИНАЧЕ:

```
nova: internal error at compiler-codegen/src/codegen/emit_c.rs:44196: [P67] nova_int collapse
```

Это ДРУГОЙ баг (ICE, не диагностика). collision-фикс сработал (арность-ошибка
ушла), но НОВАЯ проблема всплыла ПОСЛЕ него — раньше compile обрывался на
CODEGEN-FAIL до того, как код доходил до этой точки. Требует ОТДЕЛЬНОГО
расследования — НЕ известно пока (а) относится ли к regression-окну
(bdae7f4e9/2f5128367) или это ортогональный pre-existing дефект, ранее
замаскированный collision-багом; (б) какой именно файл/конструкция триггерит
(`current_array_elem_hint` в контексте emit_c.rs:44196 — похоже на array-
literal codegen, возможно `tft_g()`'s `[(1,2),(3,4)]` fixarr-of-tuple literal,
а НЕ сам d22/d402 closure-код). СЛЕДУЮЩИЙ ШАГ: изолировать
tuple_fixarr_typedef.nv В ОДИНОЧКУ (без d22/d402) — если ICE воспроизводится
и там, это чужой баг НЕ связанный с closure-collision, и out of scope для
ЭТОЙ волны (но тогда встаёт вопрос — ЭТОТ файл и с оригинальными именами
f/g/h тоже бы ICE'нул, просто МЫ не дошли до этой точки в тех прогонах,
потому что раньше ошибка была РАНЬШЕ в пайплайне (callnorm ДО codegen)).

## Второй баг (НАЙДЕН + ЗАФИКСИРОВАН) — [P67] nova_int collapse ICE

Изоляция: `tuple_fixarr_typedef.nv` В ОДИНОЧКУ (без d22/d402) ТОЖЕ ICE'ил —
не связан с closure-коллизией, чисто codegen-баг, ранее МАСКИРОВАННЫЙ тем, что
callnorm-коллизия обрывала компиляцию РАНЬШЕ (до codegen). Дальнейшая изоляция
по функциям (`tft_f`/`tft_g`/`tft_h` порознь) → виновник ТОЛЬКО `tft_h`:

```
fn tft_h() -> (int, [2](int, int)) {
    (9, [(1, 2), (3, 4)])   // ← implicit trailing return
}
```

`return (9, [...])` (ЯВНЫЙ `return`) — РАБОТАЕТ. Implicit trailing (без
`return`) — ICE. Корень: `emit_c.rs` эмитит trailing/arrow-body return через
УЗКИЙ whitelist-гейт (`ret.starts_with("NovaOpt_") || ret.starts_with(
"_NovaFixArr_") || is_typed_integer(ret) || is_bytes_slice_c_ty(ret)`) —
только ЭТИ случаи роутятся через `emit_expr_with_target_type` (type-directed
coercion); всё остальное идёт через НЕТИПИЗИРОВАННЫЙ `emit_expr`. Тип `(int,
[2](int,int))` мангленный как `_NovaTuple_...` НЕ входил ни в один из
вариантов whitelist'а → нетипизированный `emit_expr` на литерал `[(1,2),
(3,4)]` внутри tuple-литерала теряет element-type hint → падает в legacy
array-literal path (`current_array_elem_hint` не установлен) → panic.
`Stmt::Return` (явный `return`) использует ДРУГОЙ, ШИРОКИЙ гейт (`ret_ty !=
"nova_int" && ret_ty != "nova_unit"` — emit_c.rs, `Stmt::Return` арм,
~line 27967 до правки) → уже безусловно типизированный, багу не подвержен.

Этот whitelist-гейт ДУБЛИРОВАН 4 РАЗА в файле (типичный паттерн этого
кодогена — тот же класс правки уже задевал все 4 копии в недавнем D55-амендменте
str-literal→[]u8, коммит 9dac463be, судя по идентичному комментарию "mirrors
the sibling gate above"):
- `compiler-codegen/src/codegen/emit_c.rs` (метод-body / generic-mono-body
  trailing, 2 копии, были ~22705 и ~23677 до правки)
- то же (top-level `FnBody::Expr` arrow-body `=> expr`, была ~24706/24732)
- `emit_block_stmts` (top-level `FnBody::Block` implicit trailing —
  ИМЕННО этот сайт стрельнул на `tft_h`, была ~26410/26442)

**Фикс:** во ВСЕХ 4 местах добавлено `|| ret.starts_with("_NovaTuple_")` (имя
переменной — `ret`/`ret_c`/`ret_ty` по месту) к тому же whitelist. Безопасность:
`emit_expr_with_target_type`'s `TupleLit`-ветка (уже существующая, используется
явным `return`/`?? (0,0)`-коалесингом) либо (а) `expr.kind == TupleLit` И
`parse_mono_tuple_elements(target)` успешно декодирует arity-match → типизированная
коэрсия по элементам (то, что нужно); либо (б) любое несовпадение → падает на
`return self.emit_expr(expr)` — БАЙТ-В-БАЙТ то же самое, что вызывающий код делал
бы и без этой ветки. Т.е. для любого return-выражения, которое НЕ является
буквальным tuple-литералом (например tuple, возвращаемый ИЗ ВЫЗОВА функции),
поведение НЕ меняется вообще.

**Верификация:**
- `solo_h` (только `tft_h`) — PASS (было ICE).
- Полный изолированный репро (`d22` + `d402` + `tuple_fixarr_typedef` с
  rename) — PASS (было CODEGEN-FAIL).
- Расширенная (но НЕ мега-CU) sanity-партия — 45 файлов
  `spec_tests/conformance/*.nv` с tuple-возвратами/consume/defer/protocol-
  контентом, собранная как ОДИН CU через временный пакет ВНУТРИ воркти
  (`nova-clfix/spec_tests_sanity_tmp/` — создан, прогнан, УДАЛЁН, не закоммичен;
  нужен был внутри репо, а не в scratchpad, чтобы `std/` резолвился корректно
  через `find_repo_root`) — PASS: 1 FAIL: 0 (весь merged CU зелёный).

## Итог

ОБА бага (callnorm name-collision + emit_c.rs `_NovaTuple_` gate) зафиксированы
точечно, без отката. Полный мега-CU НЕ гонялся (дисциплина задания) — увеpенность
из: (1) точной причинно-следственной трассировки по коду с чтением конкретных
строк, НЕ гадания; (2) изолированного репро 1:1 воспроизводящего ОБЕ исходные
CI-ошибки и подтверждающего их устранение; (3) 45-файлового sanity-прогона
тематически близкого корпуса.

## Хэши

- main / worktree HEAD на старте волны: `134248143c023ad3464184a8b78bdb4e11ca93c`
- bdae7f4e9238a495fbaacef6b8bf8635a4b30a91 (encode_utf8 refactor — НЕ виновник)
- 2f51283675089257db39db839a0cb1f79c7af091 (tuple+fixarr topo-sort + фикстура —
  фикстура была виновником коллизии; сам topo-sort фикс в emit_c.rs — НЕ трогали,
  корректен; ОБНАРУЖЕН отдельный pre-existing gap в ДРУГОЙ части emit_c.rs,
  экспонированный только после снятия коллизии)
- Модель: sonnet.
