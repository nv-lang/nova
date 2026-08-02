# Плата 185 — два новых стилевых линта (W_NON_COMPOUND_ASSIGN,
# W_WHILE_COUNTER_FOR_RANGE) — ЗАВЕРШЕНО

Ветка: `p185-style-lints` (worktree `d:/Sources/nv-lang/nova-lintstyle`).
Модель: sonnet.

> **Итог (закрыто):** оба линта реализованы, зарегистрированы в `CONV_RULES`,
> 20 юнит-тестов зелёные (7 compound-assign + 9 while-counter + 4 regression/
> sanity на два бага, найденных этой же волной — см. ниже). Волна
> разкраснения — 178 файлов std/spec_tests/examples, временный fixer
> применён и удалён (коммит `9cdaff296`). Регресс полного nova test по
> затронутым std-модулям вскрыл 2 живых бага (Mul/Div-компаунд на
> value-record `Duration`; потеря явной type-аннотации счётчика на
> `for`-конверсии) — оба исправлены в правильном месте (сужен
> `conv_binop_to_compound` до +=/-=; сообщение линта несёт тип счётчика,
> 3 сайта в tz.nv вручную докручены `i32`-аннотацией), коммит `714a5c817`.
> Финал: `nova lint --rule W_NON_COMPOUND_ASSIGN,W_WHILE_COUNTER_FOR_RANGE
> std spec_tests examples` = 0; полный regress затронутых std-модулей —
> 53 PASS/0 FAIL; flagship aggregator `--strict-effects` — собран зелёным.
> Мега-CU spec_tests/conformance НЕ гонялся (по заданию — гейт у
> интегратора); вместо него — точечный аудит обоих найденных паттернов
> бага по ВСЕМ 178 файлам волны (см. ниже) — чисто.
>
> Ниже — рабочие заметки chekpoint'а времён сетевого обрыва (сохранены как
> история сессии, не переписывались задним числом).

## Сделано (закоммичено/готово к коммиту)

1. **compiler-codegen/src/lints.rs**:
   - `conv_place_key(e) -> Option<String>` — общий хелпер «простое место»
     (ident / `@field` / цепочка полей), используется обоими линтами.
   - `conv_is_stringish` вынесен из локальной замыкания `conv_str_concat_loop`
     в file-scope (переиспользуется W_NON_COMPOUND_ASSIGN для дедупа).
   - **W_NON_COMPOUND_ASSIGN**: `x = x OP e` → предложить `x OP= e`.
     Поддержанные компаунд-операторы Nova — ТОЛЬКО `+=`/`-=`/`*=`/`/=`
     (`AssignOp` enum: `Assign/Add/Sub/Mul/Div`, НЕТ `Mod`/битовых; парсер
     лексирует только 4 compound-токена — подтверждено grep'ом лексера/
     парсера + комментарием в emit_c.rs:~27730 «НЕТ `%=` в языке»).
     LHS ограничен «простым местом» (ident/`@field`/цепочка полей,
     `conv_place_key`) — Index-места (`x[i] = x[i]+e`) НАМЕРЕННО исключены:
     компаунд по индексу в кодогене идёт ДРУГИМ путём (emit_c.rs
     Stmt::Assign — bounds-checked/struct-value/fixed-array write-ветки
     гейтятся `if *op == AssignOp::Assign` буквально; `+=` на Index падает
     в generic `emit_expr(target)` fallback — легальность/корректность для
     нескалярных элементов НЕ подтверждена → консервативно молчим).
     Дедуп с W_STR_CONCAT_LOOP: тот же сайт (`in_loop && Add && стрингиш`)
     — молчит.
   - **W_WHILE_COUNTER_FOR_RANGE**: `mut i = start; while i < end {
     ...; i += 1 }` → `for i in start..end { ... }` (`<=` → `..=`, Nova
     имеет inclusive range, `03-syntax.md:1989`). Критерии (все ОБЯЗАНЫ
     выполниться, иначе молчим): let непосредственно перед while (тот же
     блок, стата-соседи ИЛИ while — trailing блока); cond строго `i < END`/
     `i <= END`; тело БЕЗ trailing-expr; инкремент `i+=1`/`i=i+1` —
     ПОСЛЕДНИЙ statement тела; `i` больше нигде не присваивается в теле
     (any depth, over-conservative — реассайн тенью в nested loop тоже
     молчит); END — «простое место» (`conv_place_key`) ИЛИ int-литерал
     (`conv_end_int_literal`) — голый Call/Index как END молчит (риск:
     переоценка каждую итерацию `while` vs один раз в `for`); END-место не
     мутируется в теле; НЕТ `continue` где-либо в теле (any depth,
     over-conservative); `i` не используется ПОСЛЕ while в остатке блока;
     `while` без `invariants`/`decreases` (SMT-контракты потерялись бы).
     Проверено на реальном std-кейсе (string_builder.nv `@pad_in_place`
     c/b/pos): c-loop и b-loop подпадают, `pos` — НЕ подпадает (структурно,
     между `mut pos=0` и любым while всегда есть другой `mut`-let).
   - Оба правила зарегистрированы в `CONV_RULES`.
   - 16 юнит-тестов добавлены в `mod tests` (7 compound-assign pos/neg + 9
     while-counter pos/neg, включая continue-в-теле/i-после-цикла/
     reassign-в-теле/END-мутируется/END-это-call/инкремент-не-последний/
     nested-pos-case-2-хита). Все прошли (`cargo test lints::tests` — 51/51
     зелёные, включая старые).
   - **ВРЕМЕННО** (удалить перед финальным коммитом): `pub fn
     find_non_compound_assign_edits` / `pub struct NonCompoundAssignEdit` /
     `pub fn find_while_counter_edits` / `pub struct WhileCounterEdit` +
     приватные `collect_while_counter_edits_*` walker'ы — плюмбинг для
     one-shot codemod-фиксера (переиспользуют ТУ ЖЕ детект-логику, что и
     линты — не дублируют). Помечены `// TEMPORARY` блок-комментарием в
     конце файла.

2. **nova-cli/Cargo.toml**: временный `[[bin]] name = "fix_p185_style"`
   (по прецеденту migrate_plan60/65) — удалить перед финальным коммитом.

3. **nova-cli/src/bin/fix_p185_style.rs** (НЕ коммитить в финал / удалить
   перед финальным коммитом): one-shot codemod, два режима:
   - Phase 1 (while-counter, СНАЧАЛА): итеративные раунды innermost-first
     (nested candidates типа c/b перекрываются по байт-диапазону —
     ре-парс + выбор непересекающихся МИНИМАЛЬНЫХ по размеру кандидатов
     каждый раунд, до фикспоинта). `WHILE_COUNTER_SKIP` — 26 conformance-
     файлов Plan123/LICM/IPA-семьи (тестируют ИМЕННО while-специфичное
     поведение оптимизатора loop-invariant-code-motion/field-cache — их
     docstring'и явно это подтверждают, см. список констант в файле) +
     `perf_contract_hot_loop_slow.nv` (перф-изоляция контракт-оверхеда,
     смена формы цикла исказила бы, что именно измеряется) — эти 27
     НЕ авто-конвертируются, получат `nova:allow` вручную с причиной.
   - Phase 2 (compound-assign, ПОСЛЕ): один проход, span-precise (slice
     оригинального текста по target/right span — без риска
     переформатирования).
   - Safety: после трансформации файл ре-парсится; если парс упал — файл
     НЕ пишется (сообщение в stderr), оригинал остаётся нетронутым.
   - Собран (`cargo build --release --bin fix_p185_style`) — ЧИСТО, без
     ошибок. **ЕЩЁ НЕ ЗАПУСКАЛСЯ** (ни dry-run, ни --apply) — сеть
     оборвалась ровно перед запуском.

## Метрики находок (полный прогон `nova lint --rule
W_NON_COMPOUND_ASSIGN,W_WHILE_COUNTER_FOR_RANGE std spec_tests examples`,
СНЯТЫ ДО фикса, актуальны на момент чекпоинта):
- W_NON_COMPOUND_ASSIGN: 326 находок.
- W_WHILE_COUNTER_FOR_RANGE: 101 находка, из них 29 в spec_tests/conformance
  (27 из них → skip-лист выше, 2 безопасны для авто-фикса:
  `gc_forced_collect.nv`, `p176repro_generic_wrapper_valuerecord_err.nv`,
  `p176repro_result_valuerecord_err.nv` — ПОДОЖДИТЕ это 3, не 2, см. код
  файла fix_p185_style.rs — WHILE_COUNTER_SKIP содержит РОВНО 26 записей,
  значит из 29 conformance-файлов 3 остаются для авто-фикса).
  Файловое распределение (топ): std/src/sort.nv=15,
  std/src/runtime/string_builder.nv=8 (карьер-кейс из задания),
  std/src/text/diff.nv=4, остальные 1-3 на файл.

## СЛЕДУЮЩИЙ ШАГ (после восстановления сети)

1. Запустить `fix_p185_style` DRY-RUN сначала (без `--apply`), просмотреть
   diff-план (файл + число изменений на файл), затем `--apply`.
2. touch `compiler-codegen/src/external_registry.rs` (string_builder.nv —
   include_str!-снимок ОБЯЗАТЕЛЬНО touch после его правки — иначе
   фантомные симптомы incremental-сборки).
3. Для 26 skip-листа conformance-файлов — добавить `// nova:allow
   W_WHILE_COUNTER_FOR_RANGE -- <причина: LICM/field-cache-тест на
   while-специфичном поведении оптимизатора / перф-изоляция>` вручную
   ПЕРЕД `mut i = ...`-строкой каждого (D428 синтаксис — строго строкой
   ПЕРЕД сайтом находки, т.е. перед `while`-строкой, т.к. `w.diag.span`
   у while-counter указывает на САМ while, не на предшествующий let).
   ПРОВЕРИТЬ, на какой строке реально указывает span (мог быть let ИЛИ
   while) — конвенция nova:allow гасит по `line+1==finding_line`, значит
   комментарий должен стоять НЕПОСРЕДСТВЕННО перед строкой, на которую
   указывает диагностика (line/col из `nova lint` вывода — уже видны в
   scratchpad/lint_full.txt).
4. Пересобрать release, прогнать `nova lint --rule
   W_NON_COMPOUND_ASSIGN,W_WHILE_COUNTER_FOR_RANGE std spec_tests examples`
   — должно быть 0.
5. Прогнать `nova lint std spec_tests examples` ПОЛНОСТЬЮ (все правила) —
   0 (новые лишние находки от рефакторинга быть не должно, но проверить).
6. Убрать временный fixer: удалить `nova-cli/src/bin/fix_p185_style.rs`,
   `[[bin]]`-запись в Cargo.toml, `pub fn find_*_edits`/`pub struct
   *Edit`/`collect_while_counter_edits_*` из lints.rs (весь блок под
   `// TEMPORARY` в конце файла).
7. Таргетные `nova test` (std/src/runtime — string_builder рядом лежащие
   тесты) — зелёные как до.
8. Build флагмана (`examples/flagship/aggregator`) `--strict-effects` —
   зелёный.
9. Доки: docs/dev/nv-coding-style.md — новая §29 (компаунд-присваивание,
   рядом с §26-28 форматом) + трейлер «Проверка (185): W_WHILE_COUNTER_FOR_
   RANGE» в СУЩЕСТВУЮЩЕЙ §10 (уже описывает этот канон словами владельца,
   линт — машинная проверка того же). docs/plans/185-nova-lint.md — новая
   строка/абзац фазы. Спек-амендмент НЕ нужен (линты не меняют язык).
10. Финальный коммит(ы) — мелкими шагами, git add по именам, грep
    конфликт-маркеров одной командой с commit.

## Открытые вопросы / риски, которые НЕ забыть на приёмке

- Итеративный innermost-first алгоритм в fix_p185_style ещё НЕ
  верифицирован на реальном corpus'е (только сборка чистая) — ПЕРВЫЙ
  прогон обязательно dry-run + ручная проверка diff хотя бы на
  string_builder.nv (карьер-кейс, 8 находок, включая вложенный c/b).
- `splice_body_without_last` (удаление последнего statement тела при
  сборке for-range) — трим-эвристика (trailing whitespace + один `;`)
  проверена вручную на 3 формах (однострочный `;`-разделённый,
  многострочный, полностью пустой после удаления) но НЕ на реальном
  corpus'е — смотреть за странными двойными пробелами/пустыми строками в
  diff.
- 3 conformance-файла НЕ в skip-листе (gc_forced_collect.nv,
  p176repro_generic_wrapper_valuerecord_err.nv,
  p176repro_result_valuerecord_err.nv) — перепроверить diff на них перед
  --apply (это единственные conformance-файлы, которые реально
  переписываются автоматически).
