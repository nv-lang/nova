# План 214 Ф.1-Ф.3b — рабочие заметки (sonnet, worktree nova-214, ветка p214-coerce)

Базовая ревизия: 6411de6f3 (main). Карта — `docs/plans/214-coerce-attribute.md`
(ревью-7, финал), спека — D429 `spec/decisions/02-types.md:15547`.

## Статус: Ф.1 механизм реализован, внутренний смоук в процессе (итеративная доводка)

### Сделано (коммиты в этой ветке, хронологически)

1. `#coerce` атрибут: AST-поле `FnDecl.coerce_attr`, парсер (`parse_coerce_attr`,
   контекстный ident по образцу `#cancel_safe`/`#blocking`) + `pre_coerce`
   pre-export парс (ОБЯЗАТЕЛЕН — авторы пишут `#coerce` ДО `export`, конвенция
   `#realtime`). R15: `#coerce` на protocol/effect-требовании — parse-time
   `E_COERCE_ON_PROTOCOL` (protocol-методы это `EffectMethod`, не `FnDecl`).
2. `CoercePairEntry` + `collect_coerce_pairs(module, lookup)` — sig-скан +
   ПОЛНЫЙ набор валидаций (NOT_UNARY/RECEIVER_FORM_DEFERRED R1,
   NOT_ZERO_COST R2 [mut-receiver/view-без-ro/finalize-с-ro],
   DUPLICATE_PAIR R3+R11 через `single_wrap_candidates`, GENERIC_UNSUPPORTED
   R14, EFFECTFUL R12). Дедуп по `Span` (module.items ∩ peer_files —
   найдено эмпирически, self-collision).
3. Accept-путь: `assignable()` — coerce-fallback ПОСЛЕ single-wrap fallback
   (`coerce_expr_input_name` — литералы напрямую, иначе `infer_expr_type`).
4. Rewrite: `try_coerce_leaf` в `MapLitAnnotator` (try_wrap_leaf-семья),
   вызывается из `walk_expr` сразу после `try_wrap_leaf`. Leaf-only (литерал/
   Ident-с-известным-var_type) — СИММЕТРИЧНО ограничению try_wrap_leaf.
   `simple_expr_type` расширен арм'ом `Type.new(...)` (D372 canonical ctor) —
   иначе `let sb = StringBuilder.new()` (без явной аннотации, ТИПИЧНАЯ форма)
   не даёт var_types запись → try_coerce_leaf молчит на самом частом кейсе.
5. std: `#coerce` на `str @bytes()`, `StringBuilder consume @into_str()`,
   `WriteBuffer consume @into_bytes()`. Тронут external_registry.rs (include_str
   snapshot-trap).
6. **ConsumeRegistry D133-интеграция (найдено эмпирически, НЕ было в карте
   плана явно):** finalize-lane bare-Ident в call-arg свободной функции
   (`log(sb)` где `sb StringBuilder`, callee `log(v str)`) БЕЗ этого фикса
   давал ложный `D133-not-consumed` — consume-чекер работает на ДО-rewrite
   дереве и не знал, что позиция получит implicit `.into_str()`. Добавлены
   `ConsumeRegistry.fn_param_output_keys` (fname → per-position
   `coerce_type_key`) + `coerce_finalize_output_keys` (I → set O, только
   finalize) + кредит `mark_consumed` в `consume_walk_expr`'s free-fn-call
   ветке. **Scope: ТОЛЬКО free-fn call-arg.** Method-call-arg (`obj.method(sb)`)
   и record-literal-field НЕ покрыты — не встретились в 3 seed-парах,
   задокументированный gap для Ф.4/будущей волны, если найдётся кейс.
7. **Return-position rewrite gap (найдено эмпирически):** `MapLitAnnotator`
   ВООБЩЕ не прокидывал expected-тип в `Stmt::Return` (было `walk_expr(v,
   None)` безусловно) — `return sb` в `-> str` функции компилировался в сырой
   `return sb;` (CC-FAIL, `Nova_StringBuilder*` vs `nova_str`). Добавлено поле
   `current_fn_return_ty`, `Stmt::Return` прокидывает его (ЛЮБАЯ глубина
   вложенности — `return X` всегда целится в функцию, однозначно). Отдельный
   `walk_fn_body_block` (только ДЛЯ ВЕРХНЕГО блока тела функции) прокидывает
   его же в trailing-выражение (неявный return без keyword). **Scope:
   вложенный tail-position (`if`/`match` как последнее выражение функции) НЕ
   протянут** — вне охвата этой волны, задокументированный gap (аналог
   существующего "return не идёт через assignable" пробела).

### Choke-points (для справки, актуальны)

- Accept: `assignable`/`assignable_direct` (types/mod.rs, ~13897/13939).
- Rewrite: `MapLitAnnotator::walk_expr`/`try_wrap_leaf`/`try_coerce_leaf`
  (~33700+), `walk_fn_body_block` (новый).
- Consume/D133: `ConsumeRegistry::build` + `consume_walk_expr` free-fn-arm
  (~30450+).
- Ф.2 костыль: `emit_c.rs::synthesize_bytes_lit_call_args` (call-arg pre-pass,
  keyed по `method_overloads` (recv_type, method_name) — УЖЕ type-directed,
  НЕ name-gated — единственный оставшийся хардкод — пара (str,[]u8) и StrLit-
  only условие). **РЕШЕНИЕ (эмпирически подтверждённое):** см. ниже —
  вероятна НЕ прямая отмена, а обобщение, т.к. `resolve_call_params`/
  `unique_method_param_types` (MapLitAnnotator's call-arg expected-type
  propagation) НЕ резолвит ИМЯ метода, зарегистрированное на ≥2 разных типах
  (напр. `write` — WriteBuffer/StringBuilder/TcpStream/Stdout/... все имеют
  `@write`) — именно ТАКОЙ protocol-erased/overloaded-receiver сценарий
  (`f.write("[")`, `f Fmt`) — исходный целевой кейс костыля. Прямой снос БЕЗ
  замены регрессирует `d55_literal_coercion.nv`. Проверка — следующий шаг
  (byte-parity гейт).

## Внутренний смоук (docs/plans/wip/214-scratch/scratch.nv, standalone module)

**ЗАКРЫТ ✅.** После return-position фикса + R7-диагностики: `scratch.nv`
(3 seed-пары x 4 позиции, view+finalize) — PASS; `neg_use_after.nv`
(use-after-consume neg) — корректно падает с R7-текстом ("потреблён неявной
#coerce-финализацией `into_str()` в вызове … (D429 R7); для чтения без
потребления используйте явный view-метод"). Итерации чинились РЕАЛЬНЫМИ
компиляторными багами (не гипотезами) — полная хронология выше в п.1-7.

## Ф.2 — РЕШЕНИЕ (эмпирически подтверждено экспериментом)

`emit_c.rs::synthesize_bytes_lit_call_args` **СОХРАНЁН**, НЕ снесён (отклонение
от буквального текста плана "снос ... + call-site", обосновано эмпирически —
см. развёрнутый doc-comment на функции, добавленный этим же слиянием).
Эксперимент: временно застабил `bytes_lit_wrapped` в `None`, пересобрал,
прогнал `scratch.nv` — тест "call-arg: str-литерал -> []u8 через
protocol-erased Fmt.write" (зеркалит `d55_literal_coercion.nv`'s
`f.write("[")`) сломался: CC-FAIL `passing 'const nova_str' to parameter of
incompatible type 'Nova_Vec____nova_byte *'`. Корень: AST-rewrite
(`MapLitCtx::resolve_call_params` → `unique_method_param_types`) резолвит
call-arg expected-тип ТОЛЬКО для ГЛОБАЛЬНО-уникального имени метода — `write`
объявлен на WriteBuffer/StringBuilder/TcpStream/Stdout/Stderr/BytesWriter/
File/FmtCtx (protocol `Write`, D374/D422) → НЕ уникален → rewrite молчит на
ИМЕННО той форме (protocol-erased/overloaded receiver), для которой этот
codegen pre-pass изначально писался (Plan 208 Ф.3). Мех-м УЖЕ type-directed
(не name-gated — диспатчит по `method_overloads`, никогда не сравнивает имя
метода) — остаточный §3-хардкод УЖЕ гораздо уже, чем в тексте плана: одна
пара (str,[]u8) через структурный C-тип предикат `is_bytes_slice_c_ty`
(тот же паттерн, что `emit_expr_with_target_type`). Полное обобщение на
БУДУЩИЕ произвольные #coerce-пары на этом уровне (мост Nova-canonical-key ->
C-тип-предикат) — легитимный follow-up, вне этой волны (сегодня НЕТ второй
пары, которой бы этот путь понадобился — обе finalize-пары это Ident-значения,
не литералы, покрыты try_coerce_leaf + ConsumeRegistry-кредитом).
Эксперимент откачен (функция восстановлена байт-в-байт + расширенный doc).

## Открытые вопросы / TODO перед переходом к Ф.3-миграции/Ф.3b

- [ ] Byte-parity гейт: `d55_literal_coercion.nv`/`d374_write_sink_decouple.nv`
  — .c ДО (main, костыль-версия старой волны) vs ПОСЛЕ (эта ветка, #coerce
  + сохранённый обобщённый костыль) — байт-в-байт.
- [ ] Ф.3 миграция std-сайтов (после Ф.3b линта).
- [ ] Ф.3b линт W_COERCE_EXPLICIT_REDUNDANT.
- [ ] Целевые гейты (checksums/vec/lint --deny/флагман/standalone-CU).

Чекпоинт-коммиты — после каждого логического шага (см. git log ветки).
