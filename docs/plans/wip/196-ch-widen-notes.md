<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 196 — Зона CH дожимка: чекпойнт (sonnet, worktree `nova-196chw`, ветка `p196-ch-widen`)

**Родитель:** [196-campaign-map.md](196-campaign-map.md) §«Зона CH — types/mod.rs, канал (ФУНДАМЕНТ)».
**Задание:** (а) починить debug-only SHADOW-mismatch ICE (капстоун §4.4, флаг для Zone CH) — продюсер
пишет ЛОЖНОЕ значение в `node_substs`, это P1 для канала; (б) поднять покрытие продюсеров так, чтобы
Q10 fallback-счётчики трендовали к 0 и 5-10 ядровых веток капстоуна (§3.4, 33 «Core») стали снимаемыми.
**База:** main `34a3f5f50` (после merge p-fix-char-tostr).

---

## Итог одной строкой

SHADOW ICE (`node_substs[ExprId][T] lowers to Nova_K*, legacy gave nova_str`) — корень найден и
починен: **3 продюсера** (`resolve_return_channel` / `resolve_generic_static_return` /
`resolve_method_return_with_closure_args`) материализовали bare-`Named{name}`-листья БЕЗ проверки,
что `name` — не unsubst. generic-параметр ОКРУЖАЮЩЕГО (ещё генерик) тела, а не конкретный тип — новый
общий гейт `rt_is_closed` (registry-based, `self.types.contains_key`) закрывает класс во ВСЕХ трёх.
Побочно (в ходе верификации): нашёл + починил ЕЩЁ ОДИН P1-регресс в ТОМ ЖЕ файле
(`check_instance_overload`), блокировавший ВЕСЬ conformance mega-CU — `char.eq`/`char.lt` E7320
(SIG-COMPLETE-ветка не консультировала `primitive_instance_method_known`, третий канал резолва,
пропущенный вчерашним `[M-char-blanket-shadowed-by-sig-complete]`-фиксом).

---

## 1. SHADOW-ICE — корень + фикс

**Репро (подтверждено байт-в-байт с капстоун-заметками):** `nova test std/src/collections/lru_test.nv`
(debug-бинарь) → `assertion left==right failed: [M-196.5-node-substs] SHADOW mismatch:
node_substs[ExprId(3461)][T] lowers to Some("Nova_K*"), legacy pairs gave "nova_str"`.

**Трассировка (временная инструментация, снята после диагноза):**
- `NOVA_NODE_SUBSTS_TRACE=1` → `producer=B-carrier call_id=ExprId(3461) method=len`.
- Добавленный временный print в `infer_method_call_channel_type` (`recv_ty`) показал: receiver для
  ЭТОГО `.len()`-вызова — `Array(Named{"K"})` (т.е. `[]K`), НЕ `LinkedList[T]` (первая гипотеза —
  self-recursive `LinkedList[T].len()`'s `Cons(_, t) => 1 + t.len()` — оказалась НЕВЕРНОЙ; `t` там даже
  не бинднится в `scope`, `Cons` имеет 2 payload-поля, `match_arm_bindings`'а guard `patterns.len()==1`
  её не покрывает вовсе — красная селёдка).
- Настоящий call-site: `std/src/collections/lru.nv:86` — `if @order.len() >= @capacity` внутри `Lru[K,V]`
  (K/V — ЕЩЁ АБСТРАКТНЫЕ generic-параметры ОКРУЖАЮЩЕГО метода на момент CHECK, canonicalized
  `@order []K` → `ResolvedType::Named{"Vec", [Named{"K"}]}`, D239). `.len()` резолвится на `Vec[T].len()`
  (D239-алиас), `T` — Vec-ОВСКИЙ carrier, чужой для K/V.

**Корень (`resolve_return_channel`, `types/mod.rs` ~10033):** solver юнифицирует
`recv_pattern=Vec[Var(T)]` (T — ТОЛЬКО этой функции ("Vec.len()"'s own) var) против
`concrete_recv=Vec[Named("K")]`. `ty_from_resolved_vars` (~9957) не находит "K" среди СВОИХ `vars`
(которые знают только carrier-имена ЭТОЙ FnDecl, Vec's "T") → падает в `Ty::Named{name:"K",args:[]}`
(НЕ `Ty::Var`, НЕ `Ty::Concrete` — просто структурный "named"-лист) — solver трактует это как ОБЫЧНЫЙ
конкретный тип-лист и биндит `Var(T) := Named("K")`. `as_concrete_leaf` репортит это как "полностью
резолвлено" (нет свободных vars внутри) → `ordered=[("T", Named("K"))]` проходит whole-map
completeness-гейт (`ordered.len()==decl_order.len()`) — гейт проверяет ПОЛНОТУ набора имён, но НЕ
«действительно ли каждое значение конкретно» → канал материализует ЛОЖЬ (T=K), пока легаси (codegen,
POST-mono, у которого уже ЕСТЬ `current_type_subst` c K→str для ЭТОЙ конкретной монообразации) честно
даёт T=str. **Общий root cause:** Nova's `TypeRef` не имеет отдельного carrier'а «это generic-параметр»
(см. существующий коммент ~7982 «`self.types.get("T")` finds nothing; "T" is not a registered type») —
`ResolvedType::from_type_ref` лоуэрит bare unsubst. generic-параметр и genuine 0-arg конкретный named
type ОДИНАКОВО (`Named{name,args:[]}`), и solver'у (anti-d119 constraint core, `constraint_solver.rs`)
негде это различить, если вызывающая функция не знает про ОКРУЖАЮЩИЙ generic-scope (`resolve_return_
channel`/`resolve_generic_static_return`/`resolve_method_return_with_closure_args` не берут `gs` —
в отличие от Producer A/D, которые ВЫЗЫВАЮТСЯ прямо из `f1_expr` и УЖЕ гейтят `typeref_mentions_any(t,
gs)` там, где gs доступен).

**Фикс — новый метод `rt_is_closed(&self, rt: &ResolvedType) -> bool`** (рядом с `rt_respells_names`,
~9957): рекурсивно (Tuple/Array/FixedArray/Readonly/TypedPtr/Func) проверяет, что НИ ОДИН bare
`Named{name,args:[]}`-лист не «residual» — `TypeParam(_)` residual безусловно; `Named` — residual,
если `!self.types.contains_key(name)` (не зарегистрированный тип → почти наверняка leaked generic-имя,
т.к. ВСЕ настоящие Nova-типы, вкл. std Vec/HashMap, регистрируются в `self.types`). Применён как
ДОПОЛНИТЕЛЬНЫЙ member whole-map completeness-гейта (той же дисциплины: unclosed → канал остаётся
НЕЗАПИСАННЫМ, откат на legacy) в **трёх** продюсерах, все они имели ТОЧНО ту же уязвимость (turbofish/
receiver-структура call-сайта может содержать окружающий, ещё не резолвленный generic):
1. `resolve_return_channel` (~10033, Producer B/B-method-residual) — оба return-точки (`rt` и `ordered`).
2. `resolve_generic_static_return` (~15062, Producer C, turbofish-based static return) — `type_args`
   с call-сайта может ТОЖЕ нести окружающий abstract-generic (`Vec[K].new()` внутри generic-тела).
3. `resolve_method_return_with_closure_args` (~15951, Producer B-closure-arg) — тот же класс через
   receiver `subst`.

**Верификация:**
- Точечный репро (`lru_test.nv`) — ICE ушёл, PASS.
- `nova test --full std/src/collections` (debug) — PASS 14/0/6skip (было бы crash без фикса).
- `nova test --full std/src/collections std/src/time --skip overflow_safe_test std/src/encoding`
  (debug И release) — PASS 41/0/14skip, δ0.
- `NOVA_NODE_SUBSTS_TRACE=1` на этом же корпусе: 51273 hits (все 6 продюсеров представлены:
  A/B-carrier/B-closure-arg/B-method-residual/C-static-turbofish/D-freefn-turbofish) — гейт НЕ
  схлопнул покрытие (rejection затрагивает только «named-leaf не в registry» класс, редкий).
- Флагман `examples/flagship/aggregator/src/main.nv --strict-effects` (release) — собирается чисто
  (только pre-existing warnings — unused imports/W_PARAM_TYPE_POS_MUT, не мои).
- Регресс-пины недавнего merge (`char_test`/`sync_test`/`string_builder_test`) — PASS 3/0, δ0.

**Файлы:** `compiler-codegen/src/types/mod.rs` только (per заданию).

---

## 2. Побочная находка + фикс: `char.eq`/`char.lt` E7320 (блокировал ВЕСЬ conformance mega-CU)

При попытке прогнать d-фикстуры/standalone из капстоун-корпуса обнаружено: `spec_tests/conformance/`
(top-level файлы — ОДИН co-equal модуль/CU, включая ЛЮБОЙ файл, даже стандартный `d119_*`/`d16_*`) —
**красный** на базе этого worktree из-за `d109_primitive_builtin_methods.nv` (`char.eq(a)`/`char.lt(c)`
→ `[E7320] no field or method`). Подтверждено в изоляции (файл падает сам по себе, вне зависимости от
combo с другими файлами) — **pre-existing, НЕ моя регрессия** (я не трогал primitive-dispatch/
check_instance_overload до этой находки).

**Корень:** прямое продолжение ВЧЕРАШНЕГО мержа `378fca648` (`[M-char-blanket-shadowed-by-sig-
complete]`, стоп-волна P1) — тот фикс добавил `!self.prefix_generic_method_exists(rt, method_name)`
в SIG-COMPLETE-ветку `check_instance_overload` (`types/mod.rs` ~10762), закрыв ложный E7320 на
`char.to_str()` (резолвится bare-T blanket'ом). НО ранний primitive-gate (~10652-10658) консультирует
**ТРИ** канала перед E_UNKNOWN_METHOD (`method_overloads` / `primitive_instance_method_known` —
D109 компилятор-интринсик eq/lt/le/gt/ge/hash, эмитится НАПРЯМУЮ `prim_builtin_method` в emit_c.rs,
НИКОГДА не декларируется в `.nv` / `prefix_generic_method_exists`), а вчерашний фикс добавил в
SIG-COMPLETE-ветку только ВТОРОЙ и ТРЕТИЙ канал — `primitive_instance_method_known` пропущен. `char`
стал sig-complete (тот же триггер — `char @to_stringbuilder()`), и это ТЕПЕРЬ затеняет
`char.eq`/`.lt`/`.le`/`.gt`/`.ge` (чисто компилятор-интринсик, ни в одном .nv-канале) — та же болезнь,
другой орган.

**Фикс** (`check_instance_overload`, ~10762): добавлена ТРЕТЬЯ проверка
`&& !crate::codegen::emit_c::CEmitter::primitive_instance_method_known(type_name, method_name)` —
симметрично с ранним гейтом.

**Верификация:** `d109_primitive_builtin_methods.nv` — E7320 на eq/lt ушёл (после фикса на этом же
файле всплывает ДРУГОЙ, тоже pre-existing и тоже НЕ мой баг — `v3_user_generic_newtype_ok.nv`'s
`[E_UNSAFE_CALL_REQUIRES_WRAP]` на `p115_int_to_ptr` — устаревшая фикстура, не обновлённая после
enforcement `E_UNSAFE_CALL_REQUIRES_WRAP`/D424 M4; conformance mega-CU остаётся красным по ЭТОЙ
отдельной причине — НЕ трогал, вне периметра волны, "полный conformance НЕ гонять"). Изолированные
корпуса (collections/time/encoding/standalone, все выше) — δ0, char/sync/string_builder-пины зелены.

**Файлы:** `compiler-codegen/src/types/mod.rs` (тот же файл, другая функция).

---

## 3. Что НЕ сделано в эту сессию (для следующей волны)

- **Q10 fallback-счётчики (784/193, 842/~22) НЕ переизмерены на официальном корпусе** —
  `spec_tests/conformance` (где эти числа мерялись в `196.5-stage-c2-notes.md`) недоступен для
  чистого прогона (красный по ДВУМ независимым pre-existing причинам, обе НЕ мои — см. §2 выше +
  `v3_user_generic_newtype_ok.nv`). Измерено на collections+time+encoding вместо: 51273 hits, δ0
  vs baseline (без прямого before/after diff — не было чистого прогона ДО фикса на этом наборе,
  проверено только что фикс не КОЛЛАПСИРУЕТ покрытие).
- **Новое покрытие Call-`ExprId` (Q5/Q2, sum/Result-return) НЕ добавлено** — эта сессия ушла в
  root-cause SHADOW ICE (флагированное P1 задания) + его расползание на 2 сестринских продюсера +
  побочный conformance-блокер. И carte-de-widening (Q10 Source 2+/2f/3 форм — arg-based/closure-
  return-bound/structural-bound без turbofish) уже АКТИВНО покрыта существующими продюсерами
  (Producer A `f1_check_call` ~11150-11430, Producer B-closure-arg) — при беглой сверке (см. код)
  не нашёл ОЧЕВИДНОГО, безопасного НЕДОСТАЮЩЕГО класса без полного corpus-гейта (та же ловушка
  §4.1 карты — D239 93/2 precedent, риск false-negative на моей узкой выборке).
- **33 «Core» ветки капстоуна (§3.4) — НЕ проверялись напрямую** (это emit_c.rs/frozen-territory,
  вне `types/mod.rs`-only периметра этой сессии; "читать можно, снос — capstone-агент"). Мой фикс
  (rt_is_closed) — КОРРЕКТНОСТЬ (не даёт каналу лгать), НЕ НОВОЕ покрытие → сам по себе едва ли
  напрямую разблокирует эти 33 ветки (они требуют РАСШИРЕНИЯ канала на новые формы, не просто
  «канал перестал врать в редком крайнем случае»). Рекомендация следующей волне: сверить,
  не изменилось ли что-то в icr_trace-хитах B01/B05/etc после этого фикса (channel теперь ЧАЩЕ
  бэйлит в редком K-leak-классе — если какая-то frozen-ветка ЖИЛА именно из-за ЛОЖНОГО
  материализованного значения, теоретически могла БЫ измениться, но крайне маловероятно —
  K-leak-класс узкий и не пересекается с известными B0x-паттернами).

## 4. Коммиты (ветка `p196-ch-widen`, worktree `nova-196chw`)

1. `fix(types): [M-196.5-node-substs] rt_is_closed — SHADOW-ICE root fix (3 producers)` —
   §1: `resolve_return_channel` / `resolve_generic_static_return` /
   `resolve_method_return_with_closure_args`.
2. `fix(types): [M-char-blanket-shadowed-by-sig-complete] follow-up — primitive_instance_method_known
   третий канал в SIG-COMPLETE` — §2.
3. `docs(196): Zone CH-widen — SHADOW-ICE root+fix, char.eq/lt follow-up, notes` — этот файл.

**В main НЕ мёржено. Push запрещён по заданию.**
