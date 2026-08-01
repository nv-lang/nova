# PROGRESS — p-op-channel (окно 196, операторный канал)

Worktree: `d:/Sources/nv-lang/nova-pop`, branch `p-op-channel`, база main `82cbfdb70`.
Модель: sonnet. (Корневой `PROGRESS.md` — НЕ мой, содержимое чужой закрытой задачи
(перевод `///` на английский), закоммичен в main, не трогаю.)

## Baseline (подтверждено байт-в-байт)
- `nova check std/src` → `PASS: 147  FAIL: 26  WARN: 60` — совпадает с каноном брифа.
- arch-ratchet: `lines=64286 infer=349` (scripts/guards/arch-ratchet.baseline) — совпадает
  с каноном брифа.

## Разведка (до кода)
- `compiler-codegen/src/codegen/operator_dispatch.rs` — уже прошла волна `p-operator-unify`
  (2026-08-01, main-коммиты `6906dbd68`/`d7f689cdf`, ratchet −128): ОДНА таблица
  BINOP_TABLE/UNOP_TABLE + ОДИН резолвер `resolve_binop_dispatch`, но резолв ПО-ПРЕЖНЕМУ
  в codegen (читает `method_overloads`/`self_method_decls` САМ) — НЕ через
  `resolved_callees`. Это opunify (DRY codegen-side), не channel-migration (checker-side).
  Мой бриф — следующий шаг ПОСЛЕ opunify: codegen должен ЧИТАТЬ канал, не резолвить.

## Карта (Explore-агент, файл:строка)

1+4 (13 операторов через чекер-канал + value-record):
  - `resolved_callees: HashMap<ExprId, Span>` (types/mod.rs:3654, emit_c.rs:1091) — канал уже
    существует и населяется для ОБЫЧНЫХ call-узлов; для Binary/Unary НЕ населяется вообще
    (0 хитов в types/mod.rs на Binary/Unary-ветках, f1_expr арм ~9511-9707).
  - Чекер УЖЕ имеет `method_overloads(type_name, method) -> Option<&Vec<&FnDecl>>`
    (types/mod.rs:4489) — ТОТ ЖЕ реестр, что codegen использует. Резолв на чекер-стороне
    технически достижим без новой инфраструктуры (в отличие от 196.2 class-C — там реестра
    не было вовсе).
  - `ExprKind::Binary{op,left,right}`/`Unary{op,operand}` (ast/mod.rs ~2567-2575) — несут
    `ExprId` через обёртку `Expr` (2365-2380).
  - Compound-assign — `Stmt::Assign{target,op,value,span}` (ast/mod.rs 2141-2146), БЕЗ
    `ExprId`. Codegen сегодня синтезирует `Expr::new(Binary,...)` с `ExprId::UNSET` и
    повторно диспетчит (emit_c.rs ~32186-32202). План: ключевать резолв компаунд-ассайна
    на `target.id` (lvalue-Ident/Path — не пересекается с операторными резолвами на других
    узлах).
  - Value-record (`NovaValue_X`) и named-tuple (`NovaTuple_X`) — ОТДЕЛЬНЫЕ ручные ветки в
    emit_c.rs (34191-34370 бинарные value-record / 34143-34190 tuple), НЕ используют
    `resolve_binop_dispatch` вовсе — расширение канала на них ОТДЕЛЬНАЯ, более рискованная
    работа (3-я ABI-форма).
  - Читающая сторона канала (образец для operator_dispatch): emit_c.rs 30411-30413 /
    37727 / 43429 — `resolved_callees.get(&e.id)` → matched `MethodSig` по `fn_span`.
  - `resolve_binop_dispatch` (сегодня): operator_dispatch.rs:130-205; вызов из
    emit_c.rs Binary-арма (33915-34701, `Nova_T*`-путь начинается 34387, вызов
    34448-34451); Unary-арм 34702+, вызов `unop_method_name` на 34757.

2. Compound-assign — через тот же канал (`target.id`), убрать synth-Binary hack для
   overloaded-типов (emit_c.rs ~32162-32202).

3. Сравнения (==/!=/</<=/>/>=) — `Nova_T*`-путь = ручная `@equal`/`@compare`
   protocol-цепочка (emit_c.rs 34473+), НЕ table-driven. Мигрировать отдельно от
   BINOP_TABLE (другая логика — protocol lookup, не method_overloads).

5. D46 `@not` retraction: `!` сужается до `bool`, `@not` уходит из UNOP_TABLE/D46-таблицы
   (spec/decisions/03-syntax.md:2860-2876 mapping table), спек-амендмент, 2
   фикстуры-носителя сносятся (`spec_tests/conformance/d46_operator_overload_at_methods.nv`,
   `spec_tests/conformance/m128_neg_not_operator_only_pos.nv`), негативная фикстура
   `!x` на не-bool = ошибка.

6. №247 `[M-named-tuple-compare-operators-no-dispatch]`: NovaTuple без Lt/Le/Gt/Ge
   (emit_c.rs 34149-34157, `_ => ""` съедает ordering, гейт 34157 пропускает блок).
   Текст — docs/plans/backlog-followups.md:4216.

7. №248 `[M-named-tuple-cu-recv-method-misresolution]`: подтверждено — КОРЕНЬ
   НЕ операторный диспетч (обычный `.div_rem()` method-call, класс №129
   single-key-registry last-wins). НЕ трогаю (амендмент брифа: «если нет — задокументировать
   диагноз, не трогая» — уже задокументирован в backlog-followups.md:4217).

## Окружение (важно для след. волн)

`NOVA_GC_LIB_DIR`/`NOVA_GC_INCLUDE_DIR`, ранее в памяти как `nova_rt/build`/`nova_rt` —
ОШИБКА (не проверено, дало uv_loop_t/gc.h CC-FAIL на ЛЮБОМ позитивном C-компилируемом
тесте; негативные EXPECT_COMPILE_ERROR фикстуры не задеты — они не доходят до
C-компиляции). ПРАВИЛЬНО: `NOVA_GC_LIB_DIR=<main-repo>/compiler-codegen/vcpkg_installed/
x64-windows-static/lib` (INCLUDE_DIR НЕ задавать — авто-выводится как `lib/../include`,
где реально лежит `gc.h`). Плюс `--toolchain clang` (MSVC архив-билд молча падает на
`gc.h`/`winsock2.h`, откатывается на inline-компиляцию, которая БЕЗ этого фикса мешает
worktree/main-repo пути). Плюс libuv-submodule скопирован в worktree (`cp -r
<main>/compiler-codegen/nova_rt/libuv` → `<worktree>/compiler-codegen/nova_rt/libuv`,
`rm -rf .git` внутри).

## Почему п.1-4 (core channel-миграция) НЕ выполнены этим окном — честный разбор

Прочитан `types/mod.rs`'s `ExprKind::Binary` арм (9511-9707) и `Unary` арм (9708-9741)
целиком. Вывод: **резолва оператор-overload'ов в чекере сегодня НЕТ вообще** — арм
только материализует РЕЗУЛЬТИРУЮЩИЙ ТИП (`resolved_types_buf`), никогда не determине,
КАКОЙ `@plus`/`@minus`/... FnDecl будет вызван (0 хитов `method_overloads`/`@plus` в
этом арме, подтверждено грепом). Это структурно РАЗНАЯ работа от "резолв возврата"
(196.2/196.3), которую чекер уже делает для обычных call-узлов — здесь нужно строить
НОВУЮ логику с нуля, а не переносить существующую.

Дважды документированная в самом `f1_expr`-арме история (`audit POISON 6875`,
`D263 fix`) показывает: эта функция — один из САМЫХ хрупких участков чекера, с
несколькими прошлыми регрессиями от, казалось бы, безобидных правок result-type
inference РЯДОМ с тем местом, куда легло бы новое дополнение. Добавление
`resolved_callees`-записи для операторных узлов потребовало бы:
  - для Homogeneous (`+ * / % & | ^`) — резолва НЕТ (см. brief/operator_dispatch.rs
    комментарий: "checker already guarantees this holds" — верно ТОЛЬКО в смысле "имя
    метода тривиально", не в смысле "чекер что-то резолвит"), т.е. канал тут был бы
    ПУСТОЙ ФОРМАЛЬНОСТЬЮ — не даёт emit_c.rs сокращения (ratchet не считает
    operator_dispatch.rs, только emit_c.rs).
  - для Heterogeneous (`- << >>`) и сравнений (`== != < <= > >=`) — резолв РЕАЛЕН
    (multiple-overload matching по типу RHS / `@equal`-vs-`@compare`-synthesis
    protocol-chain) — это ЕДИНСТВЕННОЕ место, где миграция дала бы genuine пользу
    (устранение codegen-side поиска + codegen-side `NoMatchingOverload`-текстовой
    ошибки взамен честной чекер-диагностики). Но: для GenericMono-ресиверов чекеру
    ВСЁ РАВНО понадобится codegen-side mono-регистрация (`register_mono_method_instance`,
    C-манглинг) — canonical rustc-модель ("mono = отдельная ФАЗА") требует
    инфраструктуры, которой у нас нет (см. план 196's собственный "Ф.S1b class-C"
    урок — B07-спайк занял отдельную волну именно на этом классе сложности).
  - Compound-assign (п.2) структурно требует либо нового `id: ExprId` на
    `Stmt::Assign`, либо ключевания на `target.id` — оба варианта МЕНЯЮТ AST/checker
    инвариант, который нигде в этом окне не был протестирован end-to-end.
  - Value-record/named-tuple ABI (п.4, кроме уже закрытого №247-среза) — ТРЕТЬЯ
    отдельная кодовая форма с собственной ручной диспетч-логикой (emit_c.rs
    34143-34370), не связанная с `resolve_binop_dispatch`.

**Решение этого окна:** не делать рискованную хирургию в `types/mod.rs`'s самом
хрупком месте без выделенного спайка (по прецеденту 196.2 B07 — "спайк-на-авторитет
ОБЯЗАТЕЛЕН для НОВОЙ class-C несущей способности", §7.14 конвенции). Вместо
полу-рабочей/недоверенной миграции — довести до полного, протестированного закрытия
то, что БЕЗОПАСНО и ценно само по себе (D46 retraction, №247), честно задокументировать
карту для следующей волны (этот файл), и НЕ заявлять п.1-4 сделанными.

**Рекомендация для следующей волны (если владелец продолжит):**
1. Отдельный спайк (аналог B07): построить в чекере резолв "какой FnDecl отвечает на
   `@minus`/`@shl`/`@shr` для КОНКРЕТНОГО (не generic) `Nova_T*`-ресивера" — самый
   узкий, наименее рискованный срез (Heterogeneous shape, non-mono), с byte-parity
   гейтом на КАЖДОМ шаге, ПРЕЖДЕ чем трогать сравнения/generic-mono/value-record/
   compound-assign.
2. Сравнения (`@equal`/`@compare` protocol-chain) — отдельный под-шаг ПОСЛЕ (1)
   доказан: другая логика (protocol synthesis, не `method_overloads`-lookup).
3. GenericMono/value-record/named-tuple/compound-assign — только после (1)+(2)
   доказаны на живом corpus (conformance + std), т.к. каждый — своя ABI-форма.

## Статус по пунктам

| # | Пункт | Статус |
|---|---|---|
| 1 | 13 операторов через чекер-канал (Nova_T* pointer-path) | НЕ СДЕЛАНО — карта готова, риск для этого окна признан слишком высоким без спайка (см. выше) |
| 2 | Compound-assign через канал | НЕ СДЕЛАНО — требует AST-инвариант (ExprId на Stmt::Assign либо target.id-ключ), не проверено |
| 3 | Сравнения через канал | НЕ СДЕЛАНО — отдельная protocol-chain логика, следующий шаг после (1) |
| 4 | Value-record/named-tuple путь | ЧАСТИЧНО: №247 (Lt/Le/Gt/Ge на NovaTuple) ЗАКРЫТ; остальная ABI-миграция НЕ СДЕЛАНА |
| 5 | D46 retraction `@not`/`!` | ✅ ГОТОВО — коммит `d40f82ad1` |
| 6 | №247 NovaTuple Lt/Le/Gt/Ge | ✅ ГОТОВО — коммит `ea20540b3` |
| 7 | №248 диагноз | ✅ ГОТОВО (диагноз подтверждён, не трогаю — root cause НЕ операторный) |

## Коммиты (чекпоинты)
- `d40f82ad1` — feat(196/D46): retract @not — `!` narrowed to strict bool
- `ea20540b3` — fix(196/№247): NovaTuple ordering operators dispatch through @compare

## Финальные гейты (2026-08-02)
- `nova check std/src` → `PASS: 147 FAIL: 26 WARN: 60` — байт-в-байт, дважды подтверждено
  (до и после обеих правок).
- arch-ratchet: `lines=64311 infer=348` (baseline поднят 64286→64311 с обоснованием,
  та же волна, тот же коммит `ea20540b3`; infer УПАЛ 349→348 — не поднимался).
- Стандэлон-фикстуры операторов: 7/7 D46/234 негативных + 7/7 позитивных
  (D46/D215/D363/opunify-семья + новая №247) — δ0, все PASS.
- `cargo test operator_dispatch` — 6/6 PASS.
- Мега-CU `spec_tests/conformance` — НЕ гонялся (канон брифа: авторитетный гейт у
  интегратора).
