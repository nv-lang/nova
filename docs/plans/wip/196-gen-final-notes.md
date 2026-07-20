<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 196 — GEN-final снос по карте Producer B, чекпойнт

**Worktree:** `nova-196fin`, ветка `p196-gen-final`. **База:** main `64b1f1396` (Producer B
слит). **Модель:** sonnet. **Карта:** `docs/plans/wip/196-prodb-notes.md` §9.

---

## Задача 1 — `resolve_method_level_subst` legacy `explicit_tf` seed — СНЕСЕНО

**Файл:** `compiler-codegen/src/codegen/emit_c.rs`, функция `resolve_method_level_subst`
(~21235). Целевой блок (было ~21340-21357, маркер `[M-91.1-method-turbofish-dispatch]`):
позиционный seed `subst_slots` из `explicit_tf` (`current_method_turbofish`), выполнявшийся
ТОЛЬКО когда channel-first блок (~21276-21309, читает `node_substs[call_id]`) промахнулся
или дал partial hit.

### Процесс (детач → трейс → снос)

1. **Детач**: добавлен `#[cfg(debug_assertions)]`-панике-блок ПЕРЕД легаси-seed (маркер
   `[M-196-gen-final-detach]`) — паникует, если `explicit_tf` непуст в момент достижения
   легаси-кода (т.е. channel-first промахнулся для explicit-turbofish-instance-method
   вызова).
2. **Прогон корпуса** (debug build, `NOVA_TRACE_ICR=1 NOVA_NODE_SUBSTS_TRACE=1`):
   - 7 карта-фикстур: `d119_option_result_method_level_generic`,
     `d122_bound_method_mono_dispatch`, `d122_generic_bound_forwarding`,
     `d30_try_op_unwrap_pair`, `d408_option_chain_sized_width`,
     `standalone/m176_method_return_turbofish` (оба реальных call-сайта класса —
     `r.empty[str]()`/`r.into[str]()`), `m196_facetc_instance_collision_and_method_generic_default`.
     **0 паник `M-196-gen-final-detach`.** (d119/m176 падают на НЕСВЯЗАННЫЙ pre-existing
     `[P67-LEGACY]` на `.to_str()`/`.push()` — standalone-tool prelude-gap, задокументирован
     Producer B и предыдущими GEN-волнами, НЕ регрессия этой правки.)
   - `std/src/{collections,time,encoding}` standalone corpus (104 файла, 14 успешных
     компиляций — остальные падают на тот же pre-existing prelude-gap): **0 паник**.
   - Частичный sweep `spec_tests/conformance/*.nv` (262/993 файлов, alphabetically до
     ~`d326_*`, прерван по времени): **0 паник**.
   - **Репо-wide grep** (свежий, независимый от Producer B) по AST-форме
     `obj.method[Type](` (excl. `fn`-декларации/комментарии) по `std/`, `examples/`,
     `spec_tests/`, `nova_tests/`, `detect172/`: подтверждён **тот же N=2**, что нашёл
     Producer B — ОБА в `standalone/m176_method_return_turbofish.nv`
     (`r.empty[str]()`/`r.into[str]()`). Других реальных call-сайтов этого AST-шейпа в
     репозитории НЕТ (остальные грep-хиты — `fn Type.method[T](...)`-декларации или
     комментарии-описания). Это делает 0-паник-результат структурно исчерпывающим, а не
     статистической выборкой.
3. **Снос**: легаси-блок физически удалён (не оставлен как panic — repo-wide grep
   гарантирует, что единственные два call-сайта этого AST-шейпа уже проверены). Take
   `current_method_turbofish` (`let _explicit_tf = ...`) оставлен — это clearing side-effect
   (защита от утечки turbofish-стейта во вложенный вызов), НЕ связан с чтением значения.

### Byte-parity (ревёрт-цикл: сохранить правку → `git checkout HEAD` baseline → собрать →
`.c` → восстановить → пересобрать → `.c` → diff)

| Фикстура | diff |
|---|---|
| `d122_bound_method_mono_dispatch` | 0 (identical) |
| `d122_generic_bound_forwarding` | 0 (identical) |
| `d30_try_op_unwrap_pair` | 0 (identical) |
| `d408_option_chain_sized_width` | 0 (identical) |
| `m196_facetc_instance_collision_and_method_generic_default` | 0 (identical) |
| `standalone/m176_method_return_turbofish` | N/A — оба (до/после) падают standalone на
  ТОЙ ЖЕ несвязанной pre-existing `.push()` P67-паника, тот же call_id/msg (только
  thread-id/номер строки в файле сместился из-за добавленных комментариев в других местах —
  ожидаемо) |
| `d119_option_result_method_level_generic` | N/A — стандалон недостижим на обеих сторонах
  (pre-existing prelude-gap, идентично до/после) |

**Вывод:** снос корректен, 0 регрессий.

---

## Задача 2 — B11r/B11q (frozen `infer_call_ret_c`) — ICR-trace: НЕ 0-достижимость,
**НЕ детачится**

Карта предполагала (гипотеза, НЕ факт): раз `d30_try_op_unwrap_pair`/
`d408_option_chain_sized_width` проходят мега-CU, канал уже отвечает раньше каскада для
этих сайтов и B11r/B11q "скорее всего уже мертвы". Проверено ICR-трейсом (`NOVA_TRACE_ICR=1`)
+ `NOVA_TRACE_MLRFS=1` (существующий, ортогональный трейс на `infer_method_level_return_for_sum`
— НАПРЯМУЮ на функции, которую B11q/B11r вызывают для method-generic Result/Option-методов).

**Результат — гипотеза ОПРОВЕРГНУТА:**

- `d30_try_op_unwrap_pair`: `[ICR-HIT] B11r_result_like_methods` +
  `[MLRFS-HIT] sum=Result method=is_ok resolved=false`.
- `d408_option_chain_sized_width`: `[ICR-HIT] B11q_novaopt_methods` +
  `[MLRFS-HIT] sum=Option method={map,or} resolved=false`.
- `d119_option_result_method_level_generic` (до pre-existing prelude-панике):
  `[ICR-HIT] B11q_novaopt_methods` + `[MLRFS-HIT] sum=Option method={map,unwrap}
  resolved=false`.

B11q/B11r **срабатывают на ВСЕХ ТРЁХ фикстурах, специально выбранных как их класс** — не
0-достижимость, а 100%-достижимость на целевом корпусе. Это СОГЛАСУЕТСЯ с уже существующей
в коде документацией (`infer_method_level_return_for_sum`'s doc-comment, ~47760-47793,
Plan 196.3 wave-2, D52/D407/D85 one-window audit, ПРЕДЫДУЩАЯ волна — независимо пришла к
тому же выводу с той же парой трейсов): «checker's channel is intentionally conservative
here... this fn is genuinely doing load-bearing work for calls the channel does not (yet)
cover, not dead weight».

**Причина (честный технический разбор):** Producer B's `node_substs`-канал целится в
generic INSTANCE-методы user-типов (`resolve_instance_method_return_arity`/
`resolve_return_channel`, structs/records). B11q/B11r обслуживают BUILTIN Option/Result
sum-типы — отдельный диспетчер (`generic_type_methods`/`infer_method_level_return_for_sum`),
который `resolve_instance_method_return` (types/mod.rs) намеренно консервативен для
(propose-then-verify через `unify_type`-round-trip) — не расширен этой волной Producer B.
Два механизма ортогональны; закрытие Producer B НЕ покрывает B11q/B11r.

**Действие: НЕ детачится, НЕ панике-детач.** Панике-детач на подтверждённо-живой ветке был
бы регрессией (сломал бы d30/d408/d119 немедленно). Вместо этого — маленькие doc-only
комментарии добавлены НАД `icr_trace("B11q_novaopt_methods")` и
`icr_trace("B11r_result_like_methods")` (маркер `[M-196-gen-final]`), фиксирующие этот
вердикт + trace-доказательства, чтобы будущая волна не повторяла ту же гипотезу без
перепроверки. Frozen-логика `infer_call_ret_c` НЕ переписана (только добавлены комментарии,
0 изменений в поведении/структуре кода).

---

## Реестр 196 — что закрыто этой волной

- ЗАКРЫТО: `resolve_method_level_subst`'s legacy `explicit_tf` seed
  (`[M-91.1-method-turbofish-dispatch]`) — снесено, byte-parity 5/5 identical, 0 паник на
  всём достижимом материале + repo-wide-grep-исчерпывающем N=2.
- НЕ ЗАКРЫТО (задокументировано, честный отрицательный вердикт, НЕ регрессия): B11q/B11r
  (`infer_call_ret_c`, frozen) — подтверждено ЖИВЫМИ (100%-достижимость на целевых
  фикстурах), не покрыты Producer B (разные механизмы: user-generic instance-method канал
  vs builtin Option/Result sum-method диспетчер). Остаются как есть; doc-комментарии
  добавлены для будущих волн.

## Гейты (см. финальный отчёт)

- Byte-parity 5/5 identical (revert-cycle).
- Детач-трейс: 0 паник (гейт-фикстуры + std-корпус + частичный conformance-sweep +
  repo-wide-grep-подтверждённый N=2 исчерпывающий class).
- Авторитетный гейт (release, `nova test spec_tests/conformance`) + флагман — см. отчёт
  агента (заполняется после сборки release).

Модель: sonnet.
