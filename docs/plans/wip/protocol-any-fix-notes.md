# [M-checker-protocol-typed-arg-any-bypass] чекпоинт

**Worktree:** `nova-protoany` (branch `p-fix-protocol-any-bypass`, из свежего main `323c8118f`).
**Модель:** sonnet.

## Баг

`resolved_cat_of_depth` (compiler-codegen/src/types/mod.rs) мапит ЛЮБОЙ
`TypeDeclKind::Protocol` expected-тип → `ResolvedType::Any`. `assignable_direct` на первой же
строке (`if matches!(exp_rt, ResolvedType::Any) { return Compat::Ok; }`) пропускает ЛЮБОЙ
аргумент для protocol-typed параметра БЕЗ структурной проверки. Прецедент: `.debug(sb)` где
`sb: StringBuilder` (только `Write`, не `Fmt`) компилировался без ошибки — тихая type confusion
на C-уровне (`Nova_StringBuilder*` вместо `Nova_FmtCtx*`, оба ptr@offset0).

Причина Protocol→Any: legacy `cat_of`-коллапс "protocol/effect/opaque permissive"
(комментарий строки ~17231 resolved_cat_of_depth), появился ДО структурной protocol-машинерии
(D42/D53/D72/D142) и никогда не пересматривался. `[T Bound]` generic-bound НЕ пострадал — тот
путь идёт через ОТДЕЛЬНЫЙ `BoundCtx::check_satisfaction`/`check_call_bounds`, никогда не
касается `resolved_cat_of`. Дыра — именно в PLAIN (не-generic) protocol-typed параметре
(`fn f(w Fmt)` / `fn f(x protocol {...})` — задокументированный "type value / existential"
сюрфейс, см. doc-comment `TypeDeclKind::Protocol`).

## Фикс

`assignable_direct` (types/mod.rs, ~L14064): на ветке `matches!(exp_rt, ResolvedType::Any)`
вызывает новый `self.protocol_mismatch_found(expr, expected, exp_gs, scope)` ПЕРЕД тем как
вернуть `Compat::Ok`. Если `expected` — Named-ссылка на `TypeDeclKind::Protocol` ИЛИ inline
`TypeRef::Protocol{..}` (D142 anon), и тип аргумента (`infer_expr_type`) резолвится в конкретный
non-generic non-primitive Named/Array — структурно проверяет, что аргумент реализует протокол:
метод по имени+арности присутствует (`method_overloads` = `sig.method_table` ∪ synth/auto-derive
overlay, U.2.3.3), ИЛИ у protocol-метода есть `default_body` (D183). Embed (`use X`, D145)
разворачивается рекурсивно (`protocol_missing_methods`, mirrors `BoundCtx`'s `flatten_dfs`).
Несоответствие → `Compat::Bad { found }` — переиспользует СУЩЕСТВУЮЩУЮ diagnostic-инфраструктуру
каждого call-site (`[E7301]` cannot-assign/cannot-pass, `[E_NO_MATCHING_OVERLOAD]` method-dispatch)
вместо нового кода ошибки — эти пути уже тестируются, новый код ошибки не заводился.

Новые private-методы `TypeCheckCtx` (после `assignable_direct`, до `mark_type_params`):
- `protocol_mismatch_found` — диспетчер (peel readonly/mut/uninit/ref, детект Named-vs-anon,
  best-effort skip: generic-param name, primitive, non-inferable arg type).
- `protocol_required_missing` — core: method_overloads-присутствие + default_body fallback.
- `protocol_missing_methods` — embed-flatten DFS для named protocol (seen-guard против циклов).

Что НЕ тронуто: `resolved_cat_of_depth` НЕ менялся (Protocol→Any коллапс остался — слишком
широкий blast radius для других consumers). `BoundCtx`/`check_satisfaction*` (generic-bound path)
НЕ тронуты (byte-identical) — separate struct, separate phase, уже корректны.

## Гейты

- Оба бинаря собраны В WORKTREE (`compiler-codegen` + `nova-cli`, release), env
  `NOVA_GC_LIB_DIR`/`NOVA_INCLUDE_DIR`/`NOVA_GC_INCLUDE_DIR` → главный репо vcpkg_installed.
  libuv submodule скопирован из главного репо (без .git).
- Baseline conformance (ДО фикса, чистый прогон без параллельных `nova test` — параллельный
  прогон гонится на shared target/ и даёт ложные CC-FAIL): 504 PASS / 0 FAIL / 14 SKIP.
- **False-positive #1 (пойман до коммита):** `pos_protocol_lit_three_caps.nv` — D142
  protocol-литерал (`protocol Writer4 { write(v) {...} }`) передаётся в `Writer4`-typed
  параметр; его "конкретный" тип (`infer_expr_type`) резолвится в ИМЯ САМОГО протокола
  (`Writer4`), а методы литерала захвачены в vtable НА МЕСТЕ КОНСТРУКЦИИ, не зарегистрированы
  в `method_table` под именем протокола → ложный missing. Фикс: `protocol_mismatch_found`
  пропускает проверку (permissive), если "конкретное" имя аргумента САМО оказывается
  `TypeDeclKind::Protocol`.
- **NEG-фикстуры (4, `spec_tests/conformance/neg/`):** `neg_protocol_param_missing_method`
  (простой missing-метод), `neg_protocol_param_embed_incomplete` (реплика реального
  StringBuilder/Fmt-прецедента — `use`-embed метод не реализован), `neg_protocol_param_anon_missing`
  (D142 inline anon-protocol как ПРЯМОЙ параметр, не generic-bound), `neg_protocol_param_wrong_arity`
  (тот же метод-имя, другая арность). Все PASS (E7301/E_NO_MATCHING_OVERLOAD "does not satisfy").
- **POS-паритет:** полный `spec_tests/conformance` (508 PASS / 0 FAIL / 14 SKIP — baseline
  504 + 4 новых neg). Byte-parity для ВСЕХ прошедших файлов гарантирована конструктивно —
  фикс живёт только в checker (`Compat`-решение), codegen его не читает; для файла без нового
  `Compat::Bad` codegen-путь буквально не меняется.
- `nova test std/src/collections std/src/checksums`: 16 PASS / 0 FAIL / 9 SKIP (чисто).
- `nova build examples/flagship/aggregator/src/main.nv --strict-effects`: собрался чисто
  (только pre-existing warnings, 0 errors).
- **Побочная находка (НЕ фикшена, задокументирована отдельным маркером
  `[M-protocol-box-callarg-vtable-incomplete]` в backlog-followups.md):** при попытке добавить
  ДВЕ доп. POS-фикстуры (embed-satisfying + 2-метод named-protocol, оба через ВЫЗОВ-АРГУМЕНТ
  boxing) обнаружился codegen-баг (`NovaBox_<Proto>`/`NovaVtable_<Proto>` несовместимость) —
  ПОДТВЕРЖДЁН пред-существующим на неисправленном `main`-бинаре (2 независимых repro), т.е. НЕ
  регрессия этой волны. Обе фикстуры удалены из дерева (не коммичены), баг залогирован.

## Статус

**ЗАКРЫТО.** D-амендмент — D53 (`spec/decisions/02-types.md`, секция "D53 amend
(2026-07-20, `[M-checker-protocol-typed-arg-any-bypass]`)"). Backlog-запись
`[M-checker-protocol-typed-arg-any-bypass]` помечена ✅ РЕШЕНО, новый маркер
`[M-protocol-box-callarg-vtable-incomplete]` заведён (P2, codegen, вне объёма этой волны).
