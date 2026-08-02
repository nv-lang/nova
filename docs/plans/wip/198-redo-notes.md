<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 198 Ф.2 REDO — карта батчей и чекпойнт (worktree nova-198, ветка triage-198)

**Цель:** довести REDO до D307-канона: все мигрированные flat-файлы `spec_tests/conformance/*.nv` — пиры ОДНОГО folder-module `module spec_tests.conformance`; конфликты имён решены `priv(file)` (D307) либо ordinal-rename (типы с методами — ограничение D307 §3, см. docs/dev/test-conventions.md §«Когда folder-module невозможен»); файлы с процессными EXPECT-маркерами — standalone (канонное исключение конвенции).

**База:** HEAD `4394fec95` (шаг 1/N — revert module-деклараций сделан). Компилятор НЕ трогаем. Тесты НЕ ослабляем: правки = только module-строка, перемещение файла, `priv(file)`-префикс, механический word-boundary rename, `fn main` → `test`-блок. Содержимое assert'ов/логики НЕ меняется.

## Окружение и команды

```sh
cd /d/Sources/nv-lang/nova-198
export NOVA_GC_LIB_DIR=/d/Sources/nv-lang/nova/compiler-codegen/vcpkg_installed/x64-windows-static/lib
export NOVA_GC_INCLUDE_DIR=/d/Sources/nv-lang/nova/compiler-codegen/vcpkg_installed/x64-windows-static/include
export NOVA_INCLUDE_DIR=/d/Sources/nv-lang/nova/compiler-codegen/vcpkg_installed/x64-windows-static/include
# бинарь уже собран: nova-cli/target/release/nova.exe (соответствует HEAD)
# гейт батча (типчек всего CU, ~2 мин):
./nova-cli/target/release/nova.exe check spec_tests/conformance 2>&1 | tail -3
# точечная проверка имени после фикса кластера:
./nova-cli/target/release/nova.exe check spec_tests/conformance 2>&1 | grep "<имя-кластера>"
```

Правила: git add ТОЛЬКО по именам файлов; коммит после каждого батча; греп конфликт-маркеров одной командой с коммитом; БЕЗ Co-Authored-By; в main НЕ мёржить; nova_tests/ НЕ трогать.

**Гейт батча:** число строк `error: duplicate` в `nova check spec_tests/conformance` уменьшилось ровно на вклад батча, НОВЫХ видов ошибок нет. Каскадные E7310/E7320/E7330/D133-* на файлах батча должны исчезнуть вместе с dup-фиксом (они вторичны — «не тот тип победил»).

## Статус батчей (чекпойнт — обновлять ПОСЛЕ каждого батча)

| Батч | Объём | Статус |
|---|---|---|
| M — маркерные → standalone/ | 58+1 файлов | ✅ 72d0f30bd+8b7f69fe5 (57/59 PASS, 2 stable-red документированы) |
| T — fn main → test-блок | 14 файлов | ✅ (dup `main` 0, duplicate 437→378) |
| D1 — dedup | 10 кластеров / 32 файлов | ✅ e1aa79ac7 |
| D2 — dedup | 14 кластеров / 17 файлов | ✅ ed45a5d04 |
| D3 — dedup | 9 кластеров / 22 файлов | ✅ 481eb845b |
| D4 — dedup | 8 кластеров / 22 файлов | ✅ ce5bda4d0 |
| D5 — dedup | 9 кластеров / 21 файлов | ✅ b9b0ee425 |
| D6 — dedup | 13 кластеров / 22 файлов | ✅ 8b06d9f93 |
| D7 — dedup | 15 кластеров / 22 файлов | ❎ НЕ ТРЕБУЕТСЯ (см. журнал) |
| D8 — dedup | 13 кластеров / 22 файлов | ❎ НЕ ТРЕБУЕТСЯ (см. журнал) |
| D9 — dedup | 18 кластеров / 22 файлов | ❎ НЕ ТРЕБУЕТСЯ (см. журнал) |
| D10 — dedup | 13 кластеров / 22 файлов | ❎ НЕ ТРЕБУЕТСЯ (см. журнал) |
| D11 — dedup | 11 кластеров / 22 файлов | ❎ НЕ ТРЕБУЕТСЯ (см. журнал) |
| D12 — dedup | 11 кластеров / 22 файлов | ❎ НЕ ТРЕБУЕТСЯ (см. журнал) |
| FIN — интеграция | — | ✅ 2026-07-13: conformance+soundness полный прогон PASS 463 / FAIL 14 / SKIP 9; 9 TIMEOUT (neg/) = environmental, точечный ре-ран 9/9 PASS → эффективно **472 PASS / 5 объяснённых FAIL**: merged-CU (блокер №8), view_descriptor_stack + t4_sqlite (known-red), 2 soundness/deferred (Ф.3 статус-кво) |

## Батч M — файлы с процессными EXPECT-маркерами → `standalone/`

Каждому файлу: `git mv spec_tests/conformance/<F>.nv spec_tests/conformance/standalone/<F>.nv`, затем в файле заменить строку `module spec_tests.conformance` на `module standalone.<F>` (stem файла). Больше НИЧЕГО не менять. Обоснование: маркер относится к целому TU (runner читает маркеры только из entry-файла CU) — в merged CU маркеры чужих пиров мертвы; конвенция прямо оставляет такие тесты standalone.

| Файл | Маркеры |
|---|---|
| `channel_zero_capacity_panic.nv` | EXPECT_RUNTIME_PANIC |
| `defer_panic_mainflow.nv` | EXPECT_RUNTIME_PANIC, EXPECT_STDOUT |
| `defer_throw_single.nv` | EXPECT_RUNTIME_PANIC, EXPECT_STDOUT |
| `e2e_collate.nv` | EXPECT_STDOUT |
| `e2e_general_category.nv` | EXPECT_STDOUT |
| `e2e_normalize.nv` | EXPECT_STDOUT |
| `e2e_words.nv` | EXPECT_STDOUT |
| `exit_code_42.nv` | EXPECT_EXIT_CODE |
| `f10_fn_return_str.nv` | EXPECT_STDOUT |
| `f11_corpus_06_pattern_regression.nv` | EXPECT_STDOUT |
| `f12_corpus_02_pattern_regression.nv` | EXPECT_STDOUT |
| `f13_char_not_97_regression.nv` | EXPECT_STDOUT |
| `f14_legacy_workaround_still_works.nv` | EXPECT_STDOUT |
| `f1_static_method_str_from.nv` | EXPECT_STDOUT |
| `f2_protocol_dispatch_method_survives.nv` | EXPECT_STDOUT |
| `f2_static_method_str_from_bool.nv` | EXPECT_STDOUT |
| `f2_unreferenced_method_pruned.nv` | EXPECT_STDOUT |
| `f3_generic_body_const_kept.nv` | EXPECT_STDOUT |
| `f3_generic_mono_on_use.nv` | EXPECT_STDOUT |
| `f3_generic_transitive_from_main.nv` | EXPECT_STDOUT |
| `f3_method_chain_str.nv` | EXPECT_STDOUT |
| `f4_if_expr_str.nv` | EXPECT_STDOUT |
| `f4_no_import_char_methods.nv` | EXPECT_STDOUT |
| `f5_match_expr_str.nv` | EXPECT_STDOUT |
| `f6_char_literal.nv` | EXPECT_STDOUT |
| `f7_char_var.nv` | EXPECT_STDOUT |
| `f8_nested_str_from.nv` | EXPECT_STDOUT |
| `f9_record_field_str.nv` | EXPECT_STDOUT |
| `fiber_stack_overflow.nv` | EXPECT_RUNTIME_PANIC |
| `hunt_const_in_method.nv` | EXPECT_STDOUT |
| `hunt_const_to_const.nv` | EXPECT_STDOUT |
| `hunt_const_via_callchain.nv` | EXPECT_STDOUT |
| `hunt_eq_operator_method.nv` | EXPECT_STDOUT |
| `hunt_str_concat_operator.nv` | EXPECT_STDOUT |
| `multi_expect_stdout.nv` | EXPECT_RUNTIME_PANIC, EXPECT_STDOUT |
| `mutexguard_invariant_balanced.nv` | EXPECT_TIMEOUT |
| `n5_sleep_in_measure_warning.nv` | EXPECT_COMPILE_WARNING |
| `n6_opaque_literal_warning.nv` | EXPECT_COMPILE_WARNING |
| `n7_io_in_measure_warning.nv` | EXPECT_COMPILE_WARNING |
| `neg_garbage_max.nv` | EXPECT_STDERR |
| `neg_garbage_stack.nv` | EXPECT_STDERR |
| `neg_max_clamp.nv` | EXPECT_STDERR |
| `neg_negative_max.nv` | EXPECT_STDERR |
| `neg_stack_clamp.nv` | EXPECT_STDERR |
| `neg_stack_floor.nv` | EXPECT_STDERR |
| `once_deprecation_warning.nv` | EXPECT_COMPILE_WARNING |
| `once_stress_mn_4workers_slow.nv` | EXPECT_TIMEOUT |
| `perf_contract_hot_loop_slow.nv` | EXPECT_EXIT_CODE, EXPECT_STDOUT |
| `permit_balanced_prop.nv` | EXPECT_TIMEOUT |
| `pos_full_unicode.nv` | EXPECT_STDOUT |
| `pos_partial_unicode.nv` | EXPECT_STDOUT |
| `select_all_closed.nv` | EXPECT_RUNTIME_PANIC |
| `stderr_panic.nv` | EXPECT_STDERR |
| `stdout_hello.nv` | EXPECT_STDOUT |
| `supervised_cancel_double_bind.nv` | EXPECT_RUNTIME_PANIC |
| `t2_proven_elided.nv` | EXPECT_EXIT_CODE, EXPECT_STDOUT |
| `t4_unchecked_optout.nv` | EXPECT_EXIT_CODE, EXPECT_STDOUT |
| `t5_build_policy_off.nv` | EXPECT_EXIT_CODE, EXPECT_STDOUT |

Дополнительно в M (не маркер, а известный stable-red, чтобы не травил merged CU, НЕ ослабляя тест — падает отдельной строкой): `view_descriptor_stack.nv` (RUN-FAIL `after - before == 0`, кандидат в регрессию codegen 172.14 — файл известен с прошлой волны, требует компиляторного расследования; module → `standalone.view_descriptor_stack`).

Кластеры, растворяющиеся батчем M (fix не нужен): `Foo`, `I`, `Widget`, `box_it`, `helper`.

## Батч T — `fn main` → `test`-блок (14 файлов)

Замена сигнатуры `fn main() ... {` на `test "<stem> smoke" {`; тело НЕ меняется, кроме случая `fn main() -> int { <expr> }` — там тело оборачивается в `ro _ = <expr>` (значение больше не возвращается). `return` в телах отсутствует (проверено).

| Файл | Сигнатура | Замечание |
|---|---|---|
| `compiles_ok.nv` | `fn main() -> () {` |  |
| `composition.nv` | `fn main() -> int { both(5) }` | `-> int`: тело обернуть `ro _ = …` |
| `enum_tree_result.nv` | `fn main() {` | есть свои test-блоки — main просто становится ещё одним test |
| `option_self_linked_list.nv` | `fn main() {` | есть свои test-блоки — main просто становится ещё одним test |
| `p172_3_typeset_parse_smoke_positive.nv` | `fn main() -> int {` | `-> int`: тело обернуть `ro _ = …` |
| `p1_canonical_range.nv` | `fn main() -> () {` |  |
| `p2_contract_real_bounds.nv` | `fn main() -> () {` |  |
| `p3_single_and_ordered.nv` | `fn main() -> () {` |  |
| `p4_bool_equality_legal.nv` | `fn main() -> () {` |  |
| `p5_paren_not_chain.nv` | `fn main() -> () {` |  |
| `pos_extension_new_name.nv` | `fn main() -> () {` |  |
| `pos_newtype_override.nv` | `fn main() -> () {` |  |
| `pos_overload_sig.nv` | `fn main() -> () {` |  |
| `stdlib_use.nv` | `fn main() -> int { pick(5) }` | `-> int`: тело обернуть `ro _ = …` |

## Батчи D1..D12 — dedup кластеров (file-disjoint, можно параллельно)

Стратегии:
- **PRIV** — на КАЖДОЙ декларации имени в перечисленных файлах поставить префикс `priv(file) ` (если декларация начинается с `export ` — ЗАМЕНИТЬ `export ` на `priv(file) `). Только для `fn`/`const`/типов БЕЗ методов (D307 §2/§3).
- **RENAME** — механический word-boundary rename `\b<Old>\b` → `<New>` по ВСЕМУ файлу (включая строки-литералы и комментарии — это корректно: debug-format-assert'ы переименовываются согласованно). Для типов С МЕТОДАМИ (ordinal-канон конвенции).

### D1 (32 файлов)

| Имя | Стратегия | Файл → действие |
|---|---|---|
| `Color` (type) | PRIV | `p172_3_typeset_parse_smoke_positive.nv`; `plan123_3_2_v32_record_literal_args_ok.nv`; `pos_comma_inline.nv`; `t2_types.nv`; `unreachable_match_default.nv`; `v32_record_literal_args_ok.nv` — в каждом `priv(file) ` на декларацию |
| `Holder` (fn/type) | PRIV | `f1_anonymous_record_factory.nv`; `gc_forced_collect.nv`; `nested_value_record.nv` — в каждом `priv(file) ` на декларацию |
| `Node` (type) | PRIV | `option_self_linked_list.nv`; `plan174_6_extern_c_abi_pos.nv`; `sum_per_variant.nv` — в каждом `priv(file) ` на декларацию |
| `Pair` | RENAME | `f1_anonymous_record_factory.nv` → `Pair1`; `parfor_elem_matrix.nv` → `Pair2`; `plan123_3_prop_pure_semantic_ok.nv` → `Pair3`; `prop_pure_semantic_ok.nv` → `Pair4` |
| `Pixel4` | RENAME | `plan123_3_2_v32_record_literal_args_ok.nv` → `Pixel4_1`; `v32_record_literal_args_ok.nv` → `Pixel4_2` |
| `Point` | RENAME | `b_lvalue_contexts.nv` → `Point1`; `named_tuple_singleline_ok.nv` → `Point6`; `parfor_elem_matrix.nv` → `Point7`; `plan174_6_extern_c_abi_pos.nv` → `Point8`; `pos_comma_inline.nv` → `Point9`; `pos_impl_debug.nv` → `Point10`; `record_elem_regression.nv` → `Point11`; `repro_matrix.nv` → `Point12`; `t1_basic_named_tuple.nv` → `Point13`; `t1_vec_clone_deep.nv` → `Point14`; `t2_vec_of_record.nv` → `Point15` |
| `Rect` (type) | PRIV | `plan174_6_extern_c_abi_pos.nv`; `pos_newline_sep.nv`; `t4_defaults.nv` — в каждом `priv(file) ` на декларацию |
| `Shape2` | RENAME | `plan123_1_sum_type_no_field_cache_ok.nv` → `Shape2_1`; `repro_matrix.nv` → `Shape2_2`; `sum_type_no_field_cache_ok.nv` → `Shape2_3` |
| `Vec3` | RENAME | `named_tuple_singleline_ok.nv` → `Vec3_1`; `plan123_3_pure_self_method_ok.nv` → `Vec3_2`; `pos_newline_sep.nv` → `Vec3_3`; `pure_self_method_ok.nv` → `Vec3_4` |
| `pick` (fn) | PRIV | `method_call_never_static.nv`; `p172_3_typeset_parse_smoke_positive.nv`; `scalar_only_empty.nv`; `stdlib_use.nv` — в каждом `priv(file) ` на декларацию |

### D2 (17 файлов)

| Имя | Стратегия | Файл → действие |
|---|---|---|
| `Cv3` | RENAME | `m2_v5_4_1_nested_region_ok.nv` → `Cv3_1`; `v7_7_chain_receiver_sibling_safe_ok.nv` → `Cv3_2` |
| `Cv6` | RENAME | `m5_v2_1_licm_weighted_ok.nv` → `Cv6_1`; `v7_7_depth_3_chain_ok.nv` → `Cv6_2` |
| `Inn` (type) | PRIV | `plan123_4_3_v43_no_deep_prefix_three_distinct_chains_ok.nv`; `v43_no_deep_prefix_three_distinct_chains_ok.nv` — в каждом `priv(file) ` на декларацию |
| `Inner` | RENAME | `embed_field_no_cache_ok.nv` → `Inner3`; `ipa_chain_per_root_invalidation_ok.nv` → `Inner5`; `option_predicates_migrated.nv` → `Inner6`; `parser_ok_field_consume.nv` → `Inner7`; `record_mixed_ptr_scalar.nv` → `Inner8`; `v42_chain_prefix_sharing_ok.nv` → `Inner9`; `v72_explicit_ipa_threading_ok.nv` → `Inner10` |
| `Inner2` | RENAME | `chain_two_level_ok.nv` → `Inner2_1`; `v7_7_chain_receiver_sibling_safe_ok.nv` → `Inner2_2` |
| `Inner4` | RENAME | `m1_v7_6_heap_record_field_ok.nv` → `Inner4_1`; `v7_7_depth_3_chain_ok.nv` → `Inner4_2` |
| `Leaf2` | RENAME | `plan123_7_7_v7_7_depth_3_chain_ok.nv` → `Leaf2_1`; `v7_7_depth_3_chain_ok.nv` → `Leaf2_2` |
| `Lv3` (type) | PRIV | `plan123_4_3_v43_no_deep_prefix_three_distinct_chains_ok.nv`; `v43_no_deep_prefix_three_distinct_chains_ok.nv` — в каждом `priv(file) ` на декларацию |
| `Mid` (type) | PRIV | `plan123_4_2_v42_chain_prefix_sharing_ok.nv`; `v42_chain_prefix_sharing_ok.nv` — в каждом `priv(file) ` на декларацию |
| `Mid2` | RENAME | `v43_no_deep_prefix_three_distinct_chains_ok.nv` → `Mid2_1`; `v7_7_depth_3_chain_ok.nv` → `Mid2_2` |
| `Out3` | RENAME | `plan123_4_3_v43_no_deep_prefix_three_distinct_chains_ok.nv` → `Out3_1`; `v43_no_deep_prefix_three_distinct_chains_ok.nv` → `Out3_2` |
| `Outer` | RENAME | `parser_ok_field_consume.nv` → `Outer1`; `record_mixed_ptr_scalar.nv` → `Outer2` |
| `Outer3` | RENAME | `chain_two_level_ok.nv` → `Outer3_1`; `embed_field_no_cache_ok.nv` → `Outer3_2`; `ipa_chain_per_root_invalidation_ok.nv` → `Outer3_3`; `m1_v7_6_heap_record_field_ok.nv` → `Outer3_4` |
| `Root` | RENAME | `plan123_4_2_v42_chain_prefix_sharing_ok.nv` → `Root1`; `v42_chain_prefix_sharing_ok.nv` → `Root2` |

### D3 (22 файлов)

| Имя | Стратегия | Файл → действие |
|---|---|---|
| `Counter` | RENAME | `blanket_dup_neg.nv` → `Counter1`; `contract_exprdisplay_selfaccess_neg.nv` → `Counter5`; `f9_record_mut_invariant_fail.nv` → `Counter11`; `fluent_user_ok.nv` → `Counter12`; `pos_protocol_lit_dispatch.nv` → `Counter13`; `static_methods.nv` → `Counter14`; `v2_1_loop_body_weighted_ok.nv` → `Counter15`; `v3_generic_newtype_non_ptr_inner_ok.nv` → `Counter16`; `v5_4_explain_surfaces_nested_ok.nv` → `Counter17`; `v72_no_recv_skips_ipa_ok.nv` → `Counter18` |
| `Score` | RENAME | `pos_compare_dispatch.nv` → `Score1`; `static_methods.nv` → `Score2` |
| `Tag` | RENAME | `pos_equal_dispatch.nv` → `Tag1`; `v3_generic_newtype_non_ptr_inner_ok.nv` → `Tag2` |
| `compute` (fn) | PRIV | `plan123_7_2_v72_no_recv_skips_ipa_ok.nv`; `v72_no_recv_skips_ipa_ok.nv` — в каждом `priv(file) ` на декларацию |
| `Err3` (type) | PRIV | `defer_fail_ok_multi_lifo_composed.nv`; `multi_defer_all_fail_ok_chain.nv` — в каждом `priv(file) ` на декларацию |
| `WorkErr` (type) | PRIV | `body_fail_defers_fail_composition.nv`; `defer_fail_ok_composed_with_primary.nv` — в каждом `priv(file) ` на декларацию |
| `process1` (fn) | PRIV | `backward_compat_no_fail_defers.nv`; `defer_cancel_safe_ok.nv`; `defer_fail_ok_composed_with_primary.nv` — в каждом `priv(file) ` на декларацию |
| `process2` (fn) | PRIV | `body_fail_defers_fail_composition.nv`; `defer_db_drain_ok.nv`; `defer_fail_ok_multi_lifo_composed.nv` — в каждом `priv(file) ` на декларацию |
| `process3` (fn) | PRIV | `defer_fail_ok_nested_scope.nv`; `defer_multiple_suspend_ok.nv`; `multi_defer_all_fail_ok_chain.nv` — в каждом `priv(file) ` на декларацию |

### D4 (22 файлов)

| Имя | Стратегия | Файл → действие |
|---|---|---|
| `Box4` | RENAME | `ipa_mut_survives_non_writing_call_ok.nv` → `Box4_1`; `licm_for_loop_ok.nv` → `Box4_2`; `pos_user_protocol_static.nv` → `Box4_3`; `v32_neg_non_literal_arg_not_cached_ok.nv` → `Box4_4` |
| `Buf` | RENAME | `fluent_wrapper_ok.nv` → `Buf1`; `pure_in_loop_ok.nv` → `Buf2`; `v7_5_sibling_survives_field_method_ok.nv` → `Buf3`; `v7_6_array_own_cache_survives_ok.nv` → `Buf5` |
| `Counter3` | RENAME | `f6_pure_bound_protocols.nv` → `Counter3_1`; `mut_field_straight_line_ok.nv` → `Counter3_2`; `shield_basic_mask_t3_1.nv` → `Counter3_3`; `v1_1_single_read_region_no_cache_ok.nv` → `Counter3_4` |
| `Counter4` | RENAME | `ipa_mut_invalidates_on_writing_call_ok.nv` → `Counter4_1`; `licm_mut_after_call_ok.nv` → `Counter4_2`; `m3_chain_norm_v2_user_fluent_ok.nv` → `Counter4_3` |
| `Holder8` | RENAME | `m3_chain_norm_v2_user_fluent_ok.nv` → `Holder8_1`; `plan123_followups_2026_06_05_m3_chain_norm_v2_user_fluent_ok.nv` → `Holder8_2` |
| `Account` (type) | PRIV | `contracts_record_invariant_fail.nv`; `f9_invariant_msg_violation.nv`; `unchecked_invariant_pos.nv` — в каждом `priv(file) ` на декларацию |
| `Bag3` | RENAME | `neg_ipa_unknown_callee_conservative_ok.nv` → `Bag3_1`; `prop_collision_avoidance_ok.nv` → `Bag3_2` |
| `helper_external` (fn) | PRIV | `neg_ipa_unknown_callee_conservative_ok.nv`; `plan123_7_1_neg_ipa_unknown_callee_conservative_ok.nv` — в каждом `priv(file) ` на декларацию |

### D5 (21 файлов)

| Имя | Стратегия | Файл → действие |
|---|---|---|
| `Container3` | RENAME | `ipa_chain_root_write_invalidates_ok.nv` → `Container3_1`; `licm_zero_iter_safe_ok.nv` → `Container3_2` |
| `Layer` (type) | PRIV | `ipa_chain_root_write_invalidates_ok.nv`; `plan123_7_1_ipa_chain_root_write_invalidates_ok.nv` — в каждом `priv(file) ` на декларацию |
| `Counter6` | RENAME | `mut_field_write_boundary_ok.nv` → `Counter6_1`; `pos_at_name_disambiguation.nv` → `Counter6_2`; `v1_1_two_regions_write_ok.nv` → `Counter6_3` |
| `Counter9` | RENAME | `plan123_1_prop_threshold_invariance_ok.nv` → `Counter9_1`; `pos_unbound_type_method.nv` → `Counter9_2`; `prop_threshold_invariance_ok.nv` → `Counter9_3` |
| `Point3` | RENAME | `plan123_1_ro_field_unconditional_ok.nv` → `Point3_1`; `record_multiline_ok.nv` → `Point3_2`; `ro_field_unconditional_ok.nv` → `Point3_3` |
| `Transaction6` | RENAME | `consume_ok_rvalue_to_consume_param.nv` → `Transaction6_1`; `for_view_iter_ok.nv` → `Transaction6_2`; `tx_rollback.nv` → `Transaction6_3` |
| `Vec32` | RENAME | `named_tuple_ok.nv` → `Vec32_1`; `p1_mut_binding_member_chain_mut_method_ok.nv` → `Vec32_2`; `plan123_1_named_tuple_ok.nv` → `Vec32_3` |
| `Wrap3` | RENAME | `chain_with_conditionals_ok.nv` → `Wrap3_1`; `licm_loop_keyword_ok.nv` → `Wrap3_2` |
| `Wrap_Inner` (type) | PRIV | `chain_with_conditionals_ok.nv`; `plan123_4_chain_with_conditionals_ok.nv` — в каждом `priv(file) ` на декларацию |

### D6 (22 файлов)

| Имя | Стратегия | Файл → действие |
|---|---|---|
| `f` (fn) | PRIV | `resize_with_free_fn_shadow.nv`; `t8_arg_vec_accepts_literal.nv`; `unchecked_combined_kinds_pos.nv` — в каждом `priv(file) ` на декларацию |
| `must_be_positive` (fn) | PRIV | `contract_msg_interp_pos.nv`; `contracts_requires_fail.nv`; `module_unchecked_pos.nv` — в каждом `priv(file) ` на декларацию |
| `process4` (fn) | PRIV | `defer_fail_ok_normal_exit.nv`; `defer_sleep_ok.nv`; `multi_defer_continues_after_panic.nv` — в каждом `priv(file) ` на декларацию |
| `process5` (fn) | PRIV | `defer_fail_ok_question_mark.nv`; `defer_with_timeout_ok.nv`; `multi_defer_partial_fail_ok.nv` — в каждом `priv(file) ` на декларацию |
| `Acc3` | RENAME | `m3_chain_norm_v2_non_fluent_skip_neg.nv` → `Acc3_1`; `mut_field_compound_assign_ok.nv` → `Acc3_2` |
| `Acc4` | RENAME | `neg_mut_call_in_loop_ok.nv` → `Acc4_1`; `prop_ipa_semantic_equivalence_ok.nv` → `Acc4_2` |
| `Acc8` | RENAME | `m7_v7_6_realloc_non_self_param_neg.nv` → `Acc8_1`; `plan123_followups_2026_06_05_m7_v7_6_realloc_non_self_param_neg.nv` → `Acc8_2` |
| `Bag` | RENAME | `gtr_method_contract_neg.nv` → `Bag1`; `gtr_method_contract_pos.nv` → `Bag2` |
| `Bot` (type) | PRIV | `plan123_4_3_v43_mixed_length_chains_ok.nv`; `v43_mixed_length_chains_ok.nv` — в каждом `priv(file) ` на декларацию |
| `Lv2` (type) | PRIV | `plan123_4_3_v43_mixed_length_chains_ok.nv`; `v43_mixed_length_chains_ok.nv` — в каждом `priv(file) ` на декларацию |
| `Mid1` (type) | PRIV | `plan123_4_3_v43_mixed_length_chains_ok.nv`; `v43_mixed_length_chains_ok.nv` — в каждом `priv(file) ` на декларацию |
| `Mv2` (type) | PRIV | `plan123_4_3_v43_mixed_length_chains_ok.nv`; `v43_mixed_length_chains_ok.nv` — в каждом `priv(file) ` на декларацию |
| `Nv4` | RENAME | `plan123_4_3_v43_mixed_length_chains_ok.nv` → `Nv4_1`; `v43_mixed_length_chains_ok.nv` → `Nv4_2` |

### D7 (22 файлов)

| Имя | Стратегия | Файл → действие |
|---|---|---|
| `Both3` | RENAME | `plan123_1_threshold_boundary_ok.nv` → `Both3_1`; `threshold_boundary_ok.nv` → `Both3_2` |
| `Bottom` (type) | PRIV | `chain_multiple_paths_ok.nv`; `plan123_4_chain_multiple_paths_ok.nv` — в каждом `priv(file) ` на декларацию |
| `Mid3` | RENAME | `chain_multiple_paths_ok.nv` → `Mid3_1`; `plan123_4_chain_multiple_paths_ok.nv` → `Mid3_2` |
| `Bounded4` | RENAME | `method_with_args_ok.nv` → `Bounded4_1`; `plan123_1_method_with_args_ok.nv` → `Bounded4_2` |
| `Box8` | RENAME | `plan123_3_2_v32_tuple_literal_args_ok.nv` → `Box8_1`; `v32_tuple_literal_args_ok.nv` → `Box8_2` |
| `Box_C3` | RENAME | `neg_chain_with_write_skip_ok.nv` → `Box_C3_1`; `plan123_4_neg_chain_with_write_skip_ok.nv` → `Box_C3_2` |
| `Box_C_Inner` (type) | PRIV | `neg_chain_with_write_skip_ok.nv`; `plan123_4_neg_chain_with_write_skip_ok.nv` — в каждом `priv(file) ` на декларацию |
| `Calc` | RENAME | `neg_pure_with_args_skip_ok.nv` → `Calc1`; `test_explain_ok.nv` → `Calc2` |
| `Calc3` | RENAME | `plan123_1_static_method_no_cache_ok.nv` → `Calc3_1`; `static_method_no_cache_ok.nv` → `Calc3_2` |
| `Calc5` | RENAME | `plan123_2_prop_licm_semantic_equiv_ok.nv` → `Calc5_1`; `prop_licm_semantic_equiv_ok.nv` → `Calc5_2` |
| `Cell` | RENAME | `neg_mut_write_skips_ok.nv` → `Cell1`; `plan123_3_neg_mut_write_skips_ok.nv` → `Cell3` |
| `Cfg` (type) | PRIV | `chain_three_level_ok.nv`; `plan123_4_chain_three_level_ok.nv` — в каждом `priv(file) ` на декларацию |
| `Inner1` (type) | PRIV | `chain_three_level_ok.nv`; `plan123_4_chain_three_level_ok.nv` — в каждом `priv(file) ` на декларацию |
| `Parent3` | RENAME | `chain_three_level_ok.nv` → `Parent3_1`; `plan123_4_chain_three_level_ok.nv` → `Parent3_2` |
| `Cfg3` | RENAME | `neg_parallel_for_skip_ok.nv` → `Cfg3_1`; `plan123_2_neg_parallel_for_skip_ok.nv` → `Cfg3_2` |

### D8 (22 файлов)

| Имя | Стратегия | Файл → действие |
|---|---|---|
| `Clock` (type) | PRIV | `effect_op_int_result.nv`; `inline_mut_clock_advance.nv` — в каждом `priv(file) ` на декларацию |
| `Cnt` | RENAME | `neg_non_pure_skip_ok.nv` → `Cnt1`; `plan123_3_neg_non_pure_skip_ok.nv` → `Cnt2` |
| `Config3` | RENAME | `mixed_ro_mut_ok.nv` → `Config3_1`; `plan123_1_mixed_ro_mut_ok.nv` → `Config3_2` |
| `Counter10` | RENAME | `neg_mut_write_in_loop_ok.nv` → `Counter10_1`; `plan123_2_neg_mut_write_in_loop_ok.nv` → `Counter10_2` |
| `Counter7` | RENAME | `licm_while_let_ok.nv` → `Counter7_1`; `plan123_2_licm_while_let_ok.nv` → `Counter7_2` |
| `Counter8` | RENAME | `neg_ipa_disabled_fallback_ok.nv` → `Counter8_1`; `plan123_7_1_neg_ipa_disabled_fallback_ok.nv` → `Counter8_2` |
| `Cv10` | RENAME | `m6_v2_1_dyn_range_ok.nv` → `Cv10_1`; `plan123_followups_2026_06_05_m6_v2_1_dyn_range_ok.nv` → `Cv10_2` |
| `Cv13` | RENAME | `m6_v2_1_dyn_small_range_neg.nv` → `Cv13_1`; `plan123_followups_2026_06_05_m6_v2_1_dyn_small_range_neg.nv` → `Cv13_2` |
| `Deep_Cfg` (type) | PRIV | `chain_with_calls_ok.nv`; `plan123_4_chain_with_calls_ok.nv` — в каждом `priv(file) ` на декларацию |
| `Deep_Container4` | RENAME | `chain_with_calls_ok.nv` → `Deep_Container4_1`; `plan123_4_chain_with_calls_ok.nv` → `Deep_Container4_2` |
| `File` | RENAME | `consume_ok_reopen.nv` → `File1`; `folder_module_consume_ok.nv` → `File3` |
| `File_open` (fn) | PRIV | `consume_ok_reopen.nv`; `folder_module_consume_ok.nv` — в каждом `priv(file) ` на декларацию |
| `Handle` | RENAME | `share_capture_ok_test.nv` → `Handle1`; `t2_unit_ptr_type_ok.nv` → `Handle2` |

### D9 (22 файлов)

| Имя | Стратегия | Файл → действие |
|---|---|---|
| `Holder11` | RENAME | `m7_v7_6_realloc_method_ok.nv` → `Holder11_1`; `plan123_followups_2026_06_05_m7_v7_6_realloc_method_ok.nv` → `Holder11_2` |
| `Slot5` | RENAME | `m7_v7_6_realloc_method_ok.nv` → `Slot5_1`; `plan123_followups_2026_06_05_m7_v7_6_realloc_method_ok.nv` → `Slot5_2` |
| `Holder3` | RENAME | `chain_in_loop_ok.nv` → `Holder3_1`; `plan123_4_chain_in_loop_ok.nv` → `Holder3_2` |
| `Holder_Inner` (type) | PRIV | `chain_in_loop_ok.nv`; `plan123_4_chain_in_loop_ok.nv` — в каждом `priv(file) ` на декларацию |
| `Holder5` | RENAME | `m1_v7_6_value_record_field_neg.nv` → `Holder5_1`; `plan123_followups_2026_06_05_m1_v7_6_value_record_field_neg.nv` → `Holder5_2` |
| `Pt2` | RENAME | `m1_v7_6_value_record_field_neg.nv` → `Pt2_1`; `plan123_followups_2026_06_05_m1_v7_6_value_record_field_neg.nv` → `Pt2_2` |
| `Image3` | RENAME | `licm_ro_in_loop_ok.nv` → `Image3_1`; `plan123_2_licm_ro_in_loop_ok.nv` → `Image3_2` |
| `Interval` | RENAME | `plan123_3_pure_in_conditional_branches_ok.nv` → `Interval1`; `pure_in_conditional_branches_ok.nv` → `Interval2` |
| `Inv3` | RENAME | `licm_escape_hatch_ok.nv` → `Inv3_1`; `plan123_2_licm_escape_hatch_ok.nv` → `Inv3_2` |
| `IterData3` | RENAME | `loop_iteration_ok.nv` → `IterData3_1`; `plan123_1_loop_iteration_ok.nv` → `IterData3_2` |
| `Layer3` | RENAME | `chain_threshold_single_skip_ok.nv` → `Layer3_1`; `plan123_4_chain_threshold_single_skip_ok.nv` → `Layer3_2` |
| `Layer_Inner` (type) | PRIV | `chain_threshold_single_skip_ok.nv`; `plan123_4_chain_threshold_single_skip_ok.nv` — в каждом `priv(file) ` на декларацию |
| `Lock` | RENAME | `cross_pkg_consume_with_field_ok.nv` → `Lock1`; `parser_ok_consume_binding.nv` → `Lock2` |
| `Logger2` | RENAME | `method_call_never_user_type.nv` → `Logger2_1`; `pos_effect_type_alias.nv` → `Logger2_2` |
| `Lv1` (type) | PRIV | `plan123_4_3_v43_deep_prefix_three_level_ok.nv`; `v43_deep_prefix_three_level_ok.nv` — в каждом `priv(file) ` на декларацию |
| `Mv1` (type) | PRIV | `plan123_4_3_v43_deep_prefix_three_level_ok.nv`; `v43_deep_prefix_three_level_ok.nv` — в каждом `priv(file) ` на декларацию |
| `Nv1` (type) | PRIV | `plan123_4_3_v43_deep_prefix_three_level_ok.nv`; `v43_deep_prefix_three_level_ok.nv` — в каждом `priv(file) ` на декларацию |
| `Ov3` | RENAME | `plan123_4_3_v43_deep_prefix_three_level_ok.nv` → `Ov3_1`; `v43_deep_prefix_three_level_ok.nv` → `Ov3_2` |

### D10 (22 файлов)

| Имя | Стратегия | Файл → действие |
|---|---|---|
| `Mat4` | RENAME | `deep_expression_tree_ok.nv` → `Mat4_1`; `plan123_1_deep_expression_tree_ok.nv` → `Mat4_2` |
| `Matrix3` | RENAME | `licm_nested_loops_ok.nv` → `Matrix3_1`; `plan123_2_licm_nested_loops_ok.nv` → `Matrix3_2` |
| `Mix3` | RENAME | `plan123_2_prop_licm_composition_ok.nv` → `Mix3_1`; `prop_licm_composition_ok.nv` → `Mix3_2` |
| `Pair_C3` | RENAME | `neg_chain_below_threshold_skip_ok.nv` → `Pair_C3_1`; `plan123_4_neg_chain_below_threshold_skip_ok.nv` → `Pair_C3_2` |
| `Pair_C_Sub` (type) | PRIV | `neg_chain_below_threshold_skip_ok.nv`; `plan123_4_neg_chain_below_threshold_skip_ok.nv` — в каждом `priv(file) ` на декларацию |
| `Polynomial` | RENAME | `plan123_3_prop_pure_escape_hatch_ok.nv` → `Polynomial1`; `prop_pure_escape_hatch_ok.nv` → `Polynomial2` |
| `Rect3` | RENAME | `early_return_ok.nv` → `Rect3_1`; `plan123_1_early_return_ok.nv` → `Rect3_2` |
| `RegA4` | RENAME | `plan123_4_prop_chain_semantic_ok.nv` → `RegA4_1`; `prop_chain_semantic_ok.nv` → `RegA4_2` |
| `RegA_Sub` (type) | PRIV | `plan123_4_prop_chain_semantic_ok.nv`; `prop_chain_semantic_ok.nv` — в каждом `priv(file) ` на декларацию |
| `Shape4` | RENAME | `ipa_pure_frame_based_invalidation_ok.nv` → `Shape4_1`; `plan123_7_1_ipa_pure_frame_based_invalidation_ok.nv` → `Shape4_2` |
| `Stack5` | RENAME | `ipa_transitive_closure_ok.nv` → `Stack5_1`; `plan123_7_1_ipa_transitive_closure_ok.nv` → `Stack5_2` |
| `Stat` | RENAME | `plan123_3_pure_multiple_methods_ok.nv` → `Stat1`; `pure_multiple_methods_ok.nv` → `Stat2` |
| `State4` | RENAME | `plan123_1_1_v1_1_three_regions_mixed_ok.nv` → `State4_1`; `v1_1_three_regions_mixed_ok.nv` → `State4_2` |

### D11 (22 файлов)

| Имя | Стратегия | Файл → действие |
|---|---|---|
| `Sum` | RENAME | `plan123_3_pure_three_calls_ok.nv` → `Sum1`; `pure_three_calls_ok.nv` → `Sum2` |
| `Tally3` | RENAME | `nested_blocks_ok.nv` → `Tally3_1`; `plan123_1_nested_blocks_ok.nv` → `Tally3_2` |
| `Tracker` | RENAME | `plan123_7_5_v7_5_multiple_siblings_ok.nv` → `Tracker1`; `v7_5_multiple_siblings_ok.nv` → `Tracker2` |
| `Transaction10` | RENAME | `generic_bound_ok_identity.nv` → `Transaction10_1`; `view_ok_print_field.nv` → `Transaction10_2` |
| `Transaction3` | RENAME | `for_consume_break_ok.nv` → `Transaction3_1`; `tx_commit.nv` → `Transaction3_2` |
| `Transaction8` | RENAME | `generic_bound_ok_box.nv` → `Transaction8_1`; `view_ok_pass_through.nv` → `Transaction8_2` |
| `Tree` (type) | PRIV | `heap_embed_pos.nv`; `recursive.nv` — в каждом `priv(file) ` на декларацию |
| `UpperBound3` | RENAME | `licm_break_continue_ok.nv` → `UpperBound3_1`; `plan123_2_licm_break_continue_ok.nv` → `UpperBound3_2` |
| `Vl6` | RENAME | `plan123_1_prop_semantic_equivalence_ok.nv` → `Vl6_1`; `prop_semantic_equivalence_ok.nv` → `Vl6_2` |
| `WS` | RENAME | `ipa_no_regression_ok.nv` → `WS1`; `plan123_7_ipa_no_regression_ok.nv` → `WS2` |
| `Worker4` | RENAME | `ipa_licm_mut_hoist_with_non_writing_call_ok.nv` → `Worker4_1`; `plan123_7_1_ipa_licm_mut_hoist_with_non_writing_call_ok.nv` → `Worker4_2` |

### D12 (22 файлов)

| Имя | Стратегия | Файл → действие |
|---|---|---|
| `Xv` | RENAME | `plan123_3_pure_threshold_one_skip_ok.nv` → `Xv1`; `pure_threshold_one_skip_ok.nv` → `Xv2` |
| `build_list` (fn) | PRIV | `m4_chain_norm_v3_local_root_ok.nv`; `plan123_followups_2026_06_05_m4_chain_norm_v3_local_root_ok.nv` — в каждом `priv(file) ` на декларацию |
| `check_ok` (fn) | PRIV | `option_in_generic_fn.nv`; `result_param_inference.nv` — в каждом `priv(file) ` на декларацию |
| `identity` (fn) | PRIV | `pos_ro_mut_bindings.nv`; `t3_pos_two_char_generic_ok.nv` — в каждом `priv(file) ` на декларацию |
| `process6` (fn) | PRIV | `defer_fail_ok_with_explicit_handler.nv`; `panic_in_defer_alone_propagates.nv` — в каждом `priv(file) ` на декларацию |
| `process7` (fn) | PRIV | `defer_fail_ok_with_explicit_throw.nv`; `panic_in_defer_ok_composes.nv` — в каждом `priv(file) ` на декларацию |
| `pure_leaf` (fn) | PRIV | `positive_pure_leaf.nv`; `preempt_elision.nv` — в каждом `priv(file) ` на декларацию |
| `rec_sum` (fn) | PRIV | `baked_default.nv`; `env_overrides_toml.nv` — в каждом `priv(file) ` на декларацию |
| `sum_to` (fn) | PRIV | `negative_self_recursion.nv`; `pos_current_keywords_compile.nv` — в каждом `priv(file) ` на декларацию |
| `take_int` (fn) | PRIV | `pos.nv`; `pos_wide_int.nv` — в каждом `priv(file) ` на декларацию |
| `withdraw` (fn) | PRIV | `f2_contract_msg_interp_dce.nv`; `f3_contract_msg_violation.nv` — в каждом `priv(file) ` на декларацию |

## FIN — интеграция (после всех батчей)

1. `nova check spec_tests/conformance` → 0 строк `duplicate`; остаточные ошибки разбираются точечно (вероятные хвосты: имена, где компилятор молчал из-за каскада — например `Box`-кластер: 5 файлов определяют свой не-generic `type Box`, часть использует prelude `Box[T]` — решится после dedup-прогона, при остатке — RENAME по той же схеме).
2. Полный `nova test spec_tests/conformance --full` БЕЗ `--jobs` (один процесс). Гуляющие TIMEOUT (66s-kill на тривиальных тестах: consume_fixtures/, lint/, any_is/ — environmental под конкурентной нагрузкой) — точечный ре-ран, НЕ чинить.
3. Прогоны затронутых std-папок НЕ нужны (std не трогали), но `std_hygiene`-пара уехала в conformance ранее — покрыта общим прогоном.
4. Известные stable-red (НЕ ослаблять, доложить владельцу): `standalone/view_descriptor_stack.nv` (регрессия 172.14?), `standalone/permit_balanced_prop.nv` (EXPECT_TIMEOUT_MS 30000, стабильно ~47-50s — подозрение на race в permit-scheduling M:N).
5. Обновить статус-таблицу выше + docs/plans/wip/198-triage-progress.md, коммит.

## Журнал чекпойнтов

- 2026-07-13: карта сгенерирована (анализ: 1085 flat; 58 маркерных; 14 main-конверсий; 144 компилятор-подтверждённых dup-кластеров (51 PRIV / 93 RENAME, 346 файл-вхождений). Источник истины по dup — `nova check` лог; статический анализ давал 7 ложных кластеров (легальные D84-overload'ы и уже-priv(file) фикстуры D307) — исключены.
- 2026-07-13 (продолжение): батчи M (58+1 → standalone/, 57/59 PASS), T (14 main→test), D1-D6 (63 кластера / 136 файлов) исполнены. После D6 `nova check spec_tests/conformance` = 0 ошибок на flat-файлах, 0 duplicate вне двух НАМЕРЕННЫХ neg-фикстур (neg/blanket_conflict_neg, neg/neg_same_module_dup). Кластеры D7-D12 оказались каскадными артефактами исходного check-лога (снят ДО батча M — маркерные файлы ещё сидели в CU и порождали пары); после M/T/D1-D6 компилятор их не подтверждает — батчи D7-D12 объявлены НЕ ТРЕБУЮЩИМИСЯ. Если полный test-гейт вскроет codegen-дубли — чинить точечно по той же схеме (карта остаётся источником new-имён).

- 2026-07-13 (пост-D, codegen/link/runtime-хвосты): check-лог оказался НЕ полным
  источником дублей (бюджет ошибок) — D7-D12 применены (подтверждены codegen-стадией),
  плюс волна хвостов до линка и рантайма. Сделано: 50 twin-копий batch-2 удалены
  (`git mv`+copy артефакт миграции — prefixed-имена были ДОБАВЛЕННЫМИ копиями тех же
  файлов); type-кластеры переведены с priv(file) на rename; variant-ctor коллизии
  переименованы per-file; plan143_2 восстановлен как nova.toml-пакет с FFI-шимом
  (7/7 PASS); f2_whole_module_pos, f1_alias_call_pos, supervisor_*, d124/d289/d316,
  f3_typed_result_err, repro_cross_effect_throw, repro_silent_ub_throw_typed,
  t3_handle_pattern_ok, p1/p4/p5_bench -> standalone/ (все PASS standalone);
  p2_bench_namespace_callable -> fixtures/ice_blocked/ (ICE);
  t4_sqlite_e2e_ok -> standalone/ known-red (красный и изолированно).

## Находки-дефекты компилятора (Ф.4c-очередь; вскрыты merged CU — доложить владельцу)

1. **priv(file) типы не файл-дискриминируются в checker-резолве** — use-site биндится к
   чужому одноимённому priv-типу (`Rect`/`Holder` кейсы; D307 §1 для типов не работает).
   Конвенция знала про метод-символы, но ломается и БЕЗ методов. Обход: rename.
2. **Локал/параметр НЕ затеняет top-level fn при вызове** — `ro f = bp_taker; f(c)` биндится
   к top-level `fn f` чужого файла (E_NO_MATCHING_OVERLOAD/E7301/E_IMPLICIT_NARROWING).
   Родственно резолв-багу, который сторожит resize_with_free_fn_shadow (тот фикс покрыл
   closure-параметры std-методов, но не локалы юзер-кода). priv(file) fn ПОПАДАЕТ в
   overload-набор чужого файла (D307 §3 «не регистрируются в shared overload-registry»
   не выполняется).
3. **Alias-import (`import X as h`) в folder-module peer** — codegen эмитит `h.fn(...)`
   буквально (undeclared identifier). Жертвы: f1_alias_call_pos (сам guard этого фикса!),
   f2_whole_module_pos (whole-module вариант — недефинированный unqualified символ),
   supervisor_* (std-алиасы), d124/d289/d316.
4. **Handler-литерал: биндинг match-арма считается захватом внешнего локала** —
   `with Fail[E] = |e| interrupt (match e { Ctor(x) => x })` эмитит `ctx->x = x` из
   несуществующего внешнего скоупа (undeclared identifier). Только в merged CU.
5. **std-internal вызов захвачен пользовательским символом**: `std.net` internal `classify`
   в merged CU эмитился как вызов пользовательского `nova_fn_10spec_tests11conformance8classify`
   (несоответствие манглинга → undefined symbol; потенциально soundness-грейд захват).
6. **bench.* интринзики внутри test-блоков = ICE** emit_c.rs:48774 [P67-LEGACY] `.opaque`
   (и standalone, и merged) — регрессия после census (Jul 11 → Jul 12 бинарь).
7. **extern "nova" fn + tuple-return**: t4_sqlite_e2e_ok CC-FAIL (`_NovaTuple_2_6_void_p_8_nova_int`
   инициализируется int) — красный и standalone = pre-existing регрессия Plan 115 FFI.

> **Пере-проверено 2026-07-17 (Plan 212 пункт 7, sonnet, бинарь 696d834b4).** Находки
> (1)/(2) выше — ЖИВЫЕ, репро подтверждено на актуальном компиляторе (см.
> `[M-198-f4c-1-privfile-type-not-discriminated]` / `[M-198-f4c-2-local-not-shadow-crossfile-topfn]`
> в backlog-followups.md). (3)/(4) (alias-import folder-peer, handler-литерал match-arm capture) —
> НЕ воспроизводятся ни в изоляции, ни как genuine peer; исторически уже PASS на полном
> merged CU (FIN-6, 2026-07-13) — закрыты без маркера. (5) (std-internal `classify` capture) —
> не переверено на заявленном ~1000-файловом масштабе (полный conformance запрещён этой
> волной), изолированный репро чист, статус НЕОПРЕДЕЛЁН — `[M-198-f4c-5-std-internal-symbol-capture]`.
> (6)/(7) — ЖИВЫЕ, ICE/CC-FAIL подтверждены на существующих quarantine-фикстурах —
> `[M-198-f4c-6-bench-intrinsic-test-block-ice]` / `[M-198-f4c-7-extern-nova-tuple-return-ccfail]`.
> Далее в этом файле — «4 детерминированные жертвы» (priv(file)-fn bleed ×2, file-scoped
> `#unchecked` ×2): priv(file)-fn bleed ЗАКРЫТ фиксом `7542e0013` (2026-07-14, до этой
> волны); `#unchecked` MOOT — полностью ретрактирован Plan 194 (`#unchecked` больше не
> парсится). Полная таблица вердиктов — `docs/dev/simplifications.md` (запись 2026-07-17,
> `[M-198-f4c-compiler-findings]`).

8. **Merged CU ~1010 файлов / 2589 test-блоков: два runtime-блокера:**
   a) **stack overflow 0xC00000FD на старте** — main_impl держит 2589 NovaTestFrame/setjmp
      (адресозависимы, clang не переиспользует слоты) → кадр >1МБ дефолтного стека Windows.
      Верифицировано PE-патчем SizeOfStackReserve→64МБ: бинарь стартует и бежит.
      Фикс = /STACK линкер-флаг в test_runner (compile_c_to_exe) или чанкование main_impl.
   b) **access violation 0xC0000005 в panics-recovery** — с 64МБ стеком прогон доходит до
      ~520-го теста и падает на `contracts loop preentry fail` (panics-клаузула);
      изолированно тест PASS → баг runtime-паник-машинерии на большом CU.
   До фикса a+b merged flat CU **не запускаем**; отдельные единицы (neg/, standalone/,
   подпапки, soundness) — зелёные и составляют текущий гейт.

- 2026-07-13 FIN: гейт снят. Merged flat CU (1010 файлов, 2589 test-блоков) ПОЛНОСТЬЮ компилируется и линкуется (0 dup, 0 checker/codegen/link ошибок) — заблокирован только runtime-дефектами №8a/8b (стек main_impl + AV в panics-recovery), чинит компиляторная волна. До фикса №8 строка app_effect_basic_t8_1 в прогоне = ожидаемый RUN-FAIL.

- 2026-07-13 ПОСТ-MERGE main 29b5a8836 (chunked test-main): пересборка release, доводка,
  финальный гейт FIN-4 (conformance+soundness, эксклюзивно, без --jobs):
  **PASS 486 / FAIL 7 / SKIP 16** — 0 environmental. Все 7 FAIL объяснены:
  1) `app_effect_basic_t8_1` (merged flat CU, 1005 файлов) — КОМПИЛИРУЕТСЯ И ЛИНКУЕТСЯ
     начисто; RUN-FAIL = **layout-зависимая память-порча 0xC0000005**: точка падения
     ПЛАВАЕТ при изменении состава CU (562 → 1179 → 997 из 2589 тестов; все тесты до
     падения PASS; «виноватые» тесты изолированно PASS) — блокер 8b-real, чинит
     компиляторная волна (chunked-фикс снял 8a-стек, синтетический корпус чист,
     реальный — нет: подозрение на GC/конкурентные тесты в общем процессе);
  2) t4_sqlite_e2e_ok — known-red (post-merge форма: lld undefined mini_sqlite_* —
     FFI-шим sqlite_mini_ffi.h не линкуется у standalone);
  3) pos_max_fibers_concurrent — СВЕЖАЯ post-merge регрессия: user-вызовы методов
     CancelToken (`cancel()`/`is_cancelled()`) биндятся к чужим типам (WriteBuffer/Conn10);
     ICE-вариант (`is_cancelled` → emit_c.rs:50387) в fixtures/ice_blocked/nested_supervised_cancel.nv;
  4) m176_method_return_turbofish — СВЕЖАЯ post-merge регрессия (196.x mono-волна,
     method-return turbofish U-erasure: NovaOpt_Nova_T_p vs NovaOpt_nova_str);
  5) view_descriptor_stack — known-red (172.14);
  6-7) soundness/deferred ×2 — Ф.3(a)/(b) статус-кво.
  Драйф-фиксы канона: str.len() → byte_len() в from_codepoint_invalid_neg/from_codepoint_test/
  str_new_test (version-skew прятал). Два новых Plan-173 теста из merge размещены по
  конвенции (rt-panic → neg/, scope_multierror → standalone/ из-за alias-import-дефекта №3).

- 2026-07-13 ФИНИШ (после merge main 0b95302b4 / 196.6): плавающий AV убит фиксом
  auto-derive/worker-sweep/override-scoping; попутно закрылись m176_method_return_turbofish
  и pos_max_fibers_concurrent (CancelToken) — оба снова PASS. Остались 4 детерминированных
  жертвы merged CU (Ф.4c-очередь, изолированно все PASS → вынесены в standalone/):
  method_call_never_static + scalar_only_empty (priv(file)-fn bleed pick — чужой priv
  `pick` побеждает при вызове), module_unchecked_pos + unchecked_invariant_pos
  (file-scoped `#unchecked` теряется в folder-module — контракты НЕ элидируются).
  **MERGED CU (1005 файлов / 2585 test-блоков): PASS.**

- 2026-07-13 ФИНАЛЬНЫЙ TALLY (FIN-6, полный conformance+soundness, без --jobs, из лога): **PASS 501 / FAIL 4 / SKIP 16**, merged CU app_effect_basic_t8_1 = PASS (446s, 2585 блоков). 4 FAIL = t4_sqlite_e2e_ok + view_descriptor_stack (known-red) + 2 soundness/deferred (Ф.3 статус-кво). Гуляющих/environmental — ноль. Ф.2 REDO ЗАКРЫТ.

---

## Ф.5 — ревизия подпапок `spec_tests/conformance/` (2026-07-14, задание владельца)

**Мотив:** до 198 был чистый `conformance/` = один merged-CU. Миграция 198 добавила подпапки —
часть законна (по природе не-merged), часть может быть следом «свалили пачку файлов в подпапку»
(анти-паттерн test-conventions.md: «НЕ плоди per-задача/per-фича standalone-модули»).

**Задача:** инвентарь КАЖДОЙ подпапки `conformance/*/` → вердикт одной из трёх категорий:
1. **Законный отдельный CU** — оставить. Критерии: `neg/` (каждый = свой EXPECT_COMPILE_ERROR CU);
   `standalone/` (процессные EXPECT-маркеры, entry-only); многофайловые тесты
   (`d78_root_peers`, `d78_dup_decl_*`, `xmodule_struct_variant_ctor_*`, `plan143_2` FFI-пакет,
   `fixtures/`); особый prelude-режим (`partial_prelude`, `no_prelude_panic_assert`).
2. **Вернуть плоскими пирами в merged-CU** — если подпапка = просто группировка одиночных
   позитивов, выразимых как пиры `module spec_tests.conformance` (per-фича папка без нужды в своём CU).
   Проверить D307-совместимость (priv(file)/ordinal-rename при коллизиях имён).
3. **Карантин-бага** — вынесены из-за компилятор-дефекта (Ф.4c-очередь: priv(file)-bleed,
   #unchecked-folder-module). Судьба = по фиксу дефекта (bleed → фасет-B волна; #unchecked → ретракт 194).

**Кандидаты «под подозрением» (проверить каждую):** `any_is/`, `cm_box/`, `d372_canonical/`,
`lint/`, `plan70_1/`, `plan84/`, `consume_fixtures/`.

**Метод (CPU-лёгкий):** только чтение + классификация; таблица-вердикт в этот файл. Возврат в merged-CU
(если решён) — отдельным шагом с гейтом (авторитетный у оркестратора, серийно). НЕ ломать merged-CU.
