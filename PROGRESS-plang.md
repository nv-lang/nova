# PROGRESS — окно p-lang (языковая пачка №296+№300+№301+№309)

Ветка `plang` (worktree `d:/Sources/nv-lang/nova-lang`), от `main`. Модель:
Sonnet 5 (claude-sonnet-5). Не вливать, не пушить — приёмка интегратором.

## Статус: все четыре пункта + D-амендмент ЗАКРЫТЫ, гейты окна зелёные.

## Коммиты (в порядке применения)

1. `0455a9267` — fix(lang,№296): отрицательные discriminants sum-enum +
   приведение варианта к int. Парсер (leading `-` перед discriminant) +
   emit_c.rs (явный `= N` прокинут в C tag enum на 3 сайтах: non-generic
   sum, generic-instance mono, receiver-carrier sum). Фикстуры:
   `spec_tests/conformance/sum_discriminant_negative_cast_ok.nv` (pos),
   `neg/sum_discriminant_non_numeric_after_minus_neg.nv` (neg).
2. `4d4dc59a0` — fix(lang,№300): `[T consume Hash + Equal]`. Парсер-правка
   физически попала в коммит №296 (git commit с pathspec ре-застейджил
   файл целиком поверх частичного `git apply --cached` partial-staging —
   см. примечание ниже); этот коммит — фикстуры + housekeeping
   module-path фикс в neg-фикстуре №296.
3. `02cc5a20b` — fix(lang,№301): канон `-> consume T` (префикс),
   постфикс `-> T consume` ретрактирован (`E_RETURN_CONSUME_POSTFIX_
   RETRACTED`). Миграция 5 мест в `std/src/runtime/sync.nv`.
4. `9fde1af15` — fix(lang,№309): `var_consume` (зеркало `var_mutable`) +
   `is_consume_eligible_arg` в `narrow_by_param_mode` — забирающая
   привязка теперь тоже забирающая перегрузка. Ratchet-baseline bump
   64504→64573 с обоснованием (emit_c growth оправдан, см. baseline-файл).
5. `5427dc52b` — docs: D-амендмент (D445 в 02-types.md + D156-правка +
   D84-правка в 10-overloading.md), `[overview: n/a]` (План 232 Ф.3,
   NOVA_OVERVIEW_NA=1 — точечная правка существующих D-блоков).
6. `a13c9a54f` — lint-фикс фикстуры №301 (`W_CONSUME_NAKED_NAME` →
   `@into_id()`).

## Известная неточность процесса (не влияет на результат)

Коммит №296 (`0455a9267`) физически содержит ОБА парсер-хунка (296 —
leading `-` в discriminant; 300 — `[T consume Bound+Bound]`), потому что
`git commit -m ... -- <pathspec>` ре-стейджит текущее содержимое файла
пути целиком, а не оставляет частичный индекс от предыдущего `git apply
--cached`. Код корректен и оттестирован по обоим пунктам — расхождение
только в commit-message scope (сообщение №296 не упоминает 300-хунк,
сообщение №300 это explicitly называет).

## Гейты (вердикты дословно)

- `cargo build --release` (workspace: compiler-codegen + nova-cli) —
  чисто, без errors, финальный прогон `Finished \`release\` profile
  [optimized] target(s)`.
- `nova check std/src` — `PASS: 147  FAIL: 26  WARN: 60` — канон
  147/26/60 не сдвинут (миграция 5 мест sync.nv не двигает canon).
- polaris (`nova test src --strict-effects` через worktree-бинарь +
  worktree-std) — `PASS: 37  FAIL: 0  SKIP: 18 (skipped)` — канон
  37/0/18 байт-в-байт.
- `scripts/guards/arch-ratchet.sh` — `arch-ratchet ok: lines=64573 <=
  64573` / `arch-ratchet ok: infer=348 <= 348`. Baseline поднят с
  обоснованием (64504→64573, +69: +20 №296 codegen-lowering, +49 №309
  scope-hygiene glue поверх Plan 184 `narrow_by_param_mode`).
- `nova lint` на всех 9 правленых/новых .nv-файлах — `9 file(s), 0
  finding(s), 3 parse-failure(s) (text-rules only)` (3 parse-failure —
  ОЖИДАЕМЫЕ EXPECT_COMPILE_ERROR neg-фикстуры).
- Мега-CU и флагман — НЕ гонялись (за интегратором, по брифу).

## Фикстуры (все под `spec_tests/conformance/`)

- `sum_discriminant_negative_cast_ok.nv` / `neg/sum_discriminant_non_numeric_after_minus_neg.nv`
- `generic_consume_protocol_bound_ok.nv` / `neg/generic_consume_protocol_bound_missing_impl_neg.nv` / `neg/generic_consume_protocol_bound_reverse_order_neg.nv`
- `return_consume_prefix_canon_ok.nv` / `neg/return_consume_postfix_retracted_neg.nv`
- `overload_narrow_consume_binding_ok.nv`

## D-амендмент

- `spec/decisions/02-types.md`: новый `D445` (модификатор всегда перед
  тем, что описывает) + правка раздела D156 «Синтаксис bound» (`[T
  consume Hash + Equal]`, снят bootstrap-запрет на комбинацию).
- `spec/decisions/10-overloading.md`: правка D84 правило 3 (дословное
  правило владельца вместо «только временность»), явно разграничено с
  открытым маркером `[M-184-consume-dispatch-named-lastuse]` (тот шире —
  про last-use ЛЮБОГО биндинга, этим окном НЕ закрыт).

`docs/guide/` не тронута (её ведёт отдельная сессия по указанию брифа).
