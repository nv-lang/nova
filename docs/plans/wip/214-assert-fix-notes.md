# Триаж красного nova-gate (заявлен как "214 assert regression") — 2026-07-20

## Итог: гипотеза задания (assert-callee мангл сбит #coerce finalize) — ОПРОВЕРГНУТА

Настоящий корень — ДРУГОЙ: устаревшая neg-фикстура `spec_tests/conformance/neg/
d55_bytes_lit_var_not_coerced_neg.nv` проверяла D55-подсекцию «str-литерал →
`[]u8`, ТОЛЬКО литерал», которую сам План 214 **явно ретрактировал** амендментом
D429 (`spec/decisions/02-types.md` §D429 «Ретракт», датировано 2026-07-18,
"Статус: спека нормативна... решение владельца"). Компилятор ведёт себя
СПЕК-ПРАВИЛЬНО; сломан был тест, не компилятор.

## Как узнали, что assert — красная селёдка

1. Узкий репро: `nova test-build spec_tests/conformance/app_effect_basic_t8_1.nv`
   (и любой другой файл ИЗ ЭТОЙ же папки — `spec_tests.conformance` folder-
   module = ОДНА CU на 993+ файлов, `nova test-build <file-в-папке>` всегда
   тянет ВСЮ папку, "мега-CU" неизбежна даже при запросе одного файла).
   CC-FAIL воспроизводится, но:
   - `spec_tests/conformance/app_effect_basic_t8_1.c:81251:9: error: use of
     undeclared identifier 'assert'`
   - РЕАЛЬНЫЙ источник bare `assert;` — НЕ app_effect_basic_t8_1.nv (там
     ровно один `assert(true)`, корректный call-form). Источник —
     `spec_tests/conformance/d157_d180_consume_pattern_rvalue_ok.nv:58`:
     ```
     Some(x) => {
         assert x == 4      // ← БЕЗ скобок, опечатка автора (единственное
     }                      //   такое место во ВСЁМ дереве, grep подтвердил)
     ```
     `parse_block` не требует разделителя между стейтментами (newline просто
     skip'ается) → парсится как ДВА элемента: `Stmt::Expr(Ident("assert"))` +
     trailing `x == 4`. `assert` объявлен в prelude
     (`std/src/prelude/runtime.nv:151/153`, `extern "nova" fn assert(...)`) —
     bare-ссылка тайпчекается (не ошибка резолва), codegen для bare-Ident
     печатает СЫРОЕ имя `assert;` (у intrinsic'а нет реального C-символа вне
     call-формы, см. `emit_call`'s `name == "assert"` intercept, emit_c.rs
     ~33607 — работает ТОЛЬКО для `Call`, не для bare `Ident`).
   - **Бисект ПРОВЕРЕН эмпирически, не на слово:** тот же файл
     `d157_d180_consume_pattern_rvalue_ok.nv` собран pre-214 бинарём (commit
     `3c331b2df`, отдельный worktree `../nova-pre214`) — **тот же самый
     CC-FAIL, та же строка, тот же bare `assert;`**. Диф файла между
     `3c331b2df` и `76ac8568a` (214-merge) — ПУСТОЙ (`git diff` — 0 строк).
     Т.е. этот баг СУЩЕСТВОВАЛ до 214 идентично; 214 его НЕ вносил и НЕ менял.
   - Имя `app_effect_basic_t8_1` в CC-FAIL-репорте — артефакт того, ЧТО
     test-раннер запросили первым/как representative-target для мега-CU, не
     имя реально сломанного теста.
2. **Ключевая находка — CI УЖЕ ЗНАЕТ про этот CC-FAIL и белит его:**
   `.github/workflows/nova-gate.yml:145`:
   ```
   known_red='^(spec_tests/conformance/app_effect_basic_t8_1|...)$'
   ```
   с комментарием "Известная Linux-краснота: M:N-runtime race... План 211".
   `CC-FAIL spec_tests/conformance/app_effect_basic_t8_1` ТОЧНО матчит этот
   regex → штатно фильтруется, gate засчитывается зелёным ИМЕННО для этой
   строки. (Отдельное наблюдение НЕ в объёме этой волны: комментарий в
   nova-gate.yml объясняет эту красноту M:N-race'ом — по факту это
   ДЕТЕРМИНИРОВАННЫЙ CC-FAIL от bare-assert опечатки, не race; сама опечатка
   в `d157_d180_consume_pattern_rvalue_ok.nv:58` — отдельный pre-existing
   баг вне этой волны, кандидат на будущий тикет, gate им НЕ блокируется.)
3. Прогнал РЕАЛЬНУЮ команду гейта локально (`nova test --positive
   --compile-error --timeout 300 --jobs 4 spec_tests/conformance`,
   `docs/plans/wip/214-gate-run.log`) — SUMMARY показал **ДВЕ** записи:
   ```
   CC-FAIL       spec_tests/conformance/app_effect_basic_t8_1   ← known_red, фильтруется
   NEG-NO-ERROR  spec_tests/conformance/neg/d55_bytes_lit_var_not_coerced_neg
                 # expected EXPECT_COMPILE_ERROR [E7301] but codegen succeeded
   ```
   Вторая НЕ матчит known_red-regex → она и есть настоящая причина красного
   gate (awk/grep-фильтр в nova-gate.yml оставляет её в `unexpected`, code!=0).

## Настоящий корень (подтверждён спекой, не догадка)

`spec_tests/conformance/neg/d55_bytes_lit_var_not_coerced_neg.nv` (до этого
коммита) требовал `EXPECT_COMPILE_ERROR [E7301]` для:
```nova
ro s = "not a literal"
ro b []u8 = s          // раньше: ошибка (D55 — только литерал)
```
Это была ЗАКРЫТАЯ до-214 доктрина D55 §"почему литерал, не любое str-значение".
План 214 ввёл `#coerce` (D429) и ЯВНО ретрактировал именно эту D55-подсекцию:

> `spec/decisions/02-types.md:15716` — «Подсекция D55 «Str-литерал → `[]u8`
> coercion» ... — RETRACTED → D429: литерал — частный случай str-значения,
> отдельного литерал-правила не остаётся; общее правило распространяется на
> литералы автоматически.»

Плюс R6/R9 (`spec/decisions/02-types.md` §D429) прямо включают `let`/`ro`/`mut`
с явной аннотацией в охват #coerce, и канон call-сайта — ГОЛОЕ значение (не
`.bytes()`). `#coerce fn str @bytes() -> ro []u8` объявлена в
`std/src/runtime/string/core.nv` (Plan 214 Ф.3) с док-комментарием, прямо
говорящим: «any str VALUE (not just a literal) now coerces». Т.е. компилятор
делает РОВНО то, что спека 214 требует; тест был не мигрирован в той же волне
(долг Ф.2/Ф.3 миграции корпуса, не пойман раньше — видимо потому, что
conformance мега-CU не гоняли перед мержем 214 целиком, см. владельческая
инструкция "мега-CU не re-run per this fix" в шапке `d55_bytes_lit_type_
directed.nv`).

## Фикс (тесты, НЕ компилятор — компилятор уже спек-корректен)

1. `spec_tests/conformance/neg/d55_bytes_lit_var_not_coerced_neg.nv` —
   **удалён** (проверял ретрактированное правило; единственный файл во всём
   дереве, ссылавшийся на этот E7301-кейс — grep подтвердил перед удалением).
2. `spec_tests/conformance/d55_bytes_lit_type_directed.nv` — тест «str-
   ПЕРЕМЕННАЯ всё ещё требует явный `.bytes()`» переписан в позитив: теперь
   утверждает, что голая str-переменная САМА коэрсится в `[]u8`-позиции
   (`ro b []u8 = s`, call-arg `d55_bytes_free_len(s)`), с комментарием,
   цитирующим D429 §Ретракт. Явный `.bytes()`-путь оставлен рядом (не
   обязателен, но и не запрещён — R9).
3. `app_effect_basic_t8_1.nv` — НЕ тронут (по заданию; и не нужно — не
   виновник).

## Верификация

- `nova test --positive --compile-error --timeout 300 --jobs 4
  spec_tests/conformance` (после фикса,
  `docs/plans/wip/214-gate-run-after-fix.log`): `PASS: 503 FAIL: 1` — ЕДИНСТВЕННЫЙ
  оставшийся FAIL = `CC-FAIL spec_tests/conformance/app_effect_basic_t8_1`
  (known_red). Локально прогнан ТОТ ЖЕ awk/grep-фильтр из nova-gate.yml —
  `unexpected=[]` → **gate был бы зелёным**.
- assert-фикстуры (d194_debug_assert_bare_no_parens_neg, n6_assert_historic_
  footgun, d13_no_prelude_panic_assert_intrinsic) — среди PASS: 503, ни разу
  не упомянуты в SUMMARY-фейлах — подтверждено PASS (они и не были под
  риском — assert call-form механика не трогалась).
- Флагман `examples/flagship/aggregator/src/main.nv --strict-effects` —
  built OK (только pre-existing warnings, 0 errors).

## Хэши / окружение

- Рабочая ветка: `p-fix-214-assert` (worktree `../nova-assertfix`, base `main`
  @ `249c8bdd8`).
- Эфемерный сравнительный worktree: `../nova-pre214` @ `3c331b2df` (удалён
  после сравнения).
- Модель: sonnet (Claude Sonnet 5).

## Рекомендация (вне объёма этой волны)

`spec_tests/conformance/d157_d180_consume_pattern_rvalue_ok.nv:58` содержит
`assert x == 4` (без скобок) — тайпчекается, но компилируется в невалидный C
(`assert;` bare-statement) ЛЮБЫМ компилятором (до и после 214), маскируется
ТОЛЬКО known_red-белым списком в nova-gate.yml (атрибуция там — "M:N-race",
по факту детерминированный CC-FAIL). Стоит отдельным тикетом: (а) починить
опечатку в фикстуре (`assert(x == 4)`), и/или (б) закрыть общий пробел —
bare (non-call) `Ident("assert")`-выражение вне `#debug`-контекста должно
быть compile-error по тому же духу, что и существующий
`d194_debug_assert_bare_no_parens_neg` (E_DEBUG_ATTR_TARGET), а не тихо
тайпчекаться и падать только на C-уровне.
