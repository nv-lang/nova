<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# План 209 Ф.3-fix — duplicate-symbol в multi-TU split (checkpoint, sonnet)

База: `9fada7e3d` (main, включает 209 Ф.1 + Ф.2). Ветка `plan209-f3fix`,
worktree `d:/Sources/nv-lang/nova-209f3`. В main НЕ мёржил (гейт — интегратор).
Суб-агентов не спавнил.

## Баг (найден оркестратором прогоном `NOVA_MULTI_TU=1` на conformance)

3 CC-FAIL — `lld-link: error: duplicate symbol` на const-value ОПРЕДЕЛЕНИЯХ
`_nova_const_<name>_value` (lazy-const хранилище из `emit_lazy_const`),
дублируемых между `_partK.c`:
- `_nova_const_collate_single_map_value` / `_contraction_map_value` (e2e_collate)
- `_nova_const_lower_map_value` / `_title_map_value` (pos_full_unicode)
- `_nova_const_ZERO_value` / `_SECOND_value` (репро-класс) +
  `_nova_const_module_dim_value` / `_module_len_value` (app_effect_basic_t8_1)

## Минимальный репро

`spec_tests/conformance/standalone/repro_const_dup.nv` — 4 `ro NAME = free_fn()`
без аннотации типа + 2 функции-потребителя. Под `NOVA_MULTI_TU=1` (сниженный
порог) режется на >1 part → `lld-link: error: duplicate symbol:
_nova_const_ZERO_value / _SECOND_value`. Воспроизведено до фикса.

## Корень — НЕ один баг, а ДВА класса const-value + 2 сегментер-бага split_tu

### Класс A: типизированное lazy-const хранилище (split_tu-классификатор)
`nova_int _nova_const_lower_map_value;` (uninitialized tentative-definition
global — `emit_lazy_const` эмитит `{storage}{ty_c} _nova_const_{name}_value;`,
присваивание позже в runtime через `nova_consts_init()`).

`split_tu::classify_unit` НЕ распознавал этот shape (нет `=`, нет `{}`, нет
top-level `(`) → падал в `DeclOnly { name: None }`. **Unnamed** ⇒ A3-дедуп не
может перекрыть повтор ⇒ определение оставалось **ВЕРБАТИМ** (не `extern`) в
`_common.h`. Каждый part инклудит header ⇒ своя копия strong-definition под
Clang `-fno-common` ⇒ duplicate symbol.

**Фикс (split_tu.rs):** новый `decl_from_uninitialized_global` — для bare
`TYPE NAME;` (не `typedef`/`extern`, без `(`/`{}`) выдаёт `(name,
"extern TYPE NAME;")` → `classify_unit` возвращает `GlobalDef` ⇒ определение
в РОВНО ОДИН part + `extern` в `_common.h`. Тот же инвариант, что для
инициализированных глобалов (recon §4 «глобал с ОПРЕДЕЛЕНИЕМ»). Пропускает
leading doc-comment через `skip_leading_trivia` (иначе `starts_with("typedef")`
не срабатывал бы на комментированном typedef — см. класс-C-баг ниже).

### Класс B: ТИПЕЛЕСС const-value — `ty_c == ""` (codegen-defect, emit_c.rs)
`ro module_dim = compute_dim()` (module-level, БЕЗ аннотации типа). Тип
хранилища инферится `infer_expr_c_type → infer_call_ret_c → user_fn_sigs`
(B10f — авторитет для bare free-fn-call ret). Но `user_fn_sigs` наполняется
ТОЛЬКО в `emit_fn_forward_decl` (§2), который бежит **ПОСЛЕ** цикла эмиссии
module-level `ro`. На момент эмиссии `user_fn_sigs` ПУСТ ⇒ lookup miss ⇒
`ty_c == ""` ⇒ эмитится `static  _nova_const_module_dim_value;` (без типа).
В single-TU/`static` это компилируется (C implicit-int → `static int`), что
**маскировало** дефект (и латентно ломало: implicit int = 32-bit, а значение
= nova_int 64-bit → тихая усечка на присваивании; «работало» на малых
значениях). Под multi-TU типелесс-декларатор нельзя `extern`-промоутить
(`decl_from_uninitialized_global` корректно отказывается) → остаётся вербатим
дубль → duplicate symbol.

**Фикс (emit_c.rs):** пред-скан `module.items` ПЕРЕД циклом module-level `ro`
пре-сидит `user_fn_sigs` для каждой eligible top-level free fn
(`f.receiver.is_none() && f.generics.is_empty()`) — зеркало реальной
регистрации в `emit_fn_forward_decl`. Best-effort (`.ok()`). §2 переинсертит
идентичное значение позже — идемпотентно. Теперь `ty_c == "nova_int"` ⇒
`static nova_int _nova_const_module_dim_value;` ⇒ split_tu видит типизованный
глобал ⇒ single-part + extern. Побочно чинит латентную 32→64-bit усечку в
дефолте.

### Класс C (сегментер-баги split_tu, всплыли на РЕАЛЬНОМ выводе)
1. **typedef с leading doc-comment** (`/* Plan 36... */\ntypedef int64_t
   Nova_RawMem;`) — braceless/`=`-free/`(`-free unit; `decl_from_
   uninitialized_global`'s наивный `trimmed.starts_with("typedef")` промахивался
   мимо комментария → wrongly промоутил в `extern /* ... */\ntypedef...`
   (нонсенс-C, «cannot combine with previous 'extern'»). Фикс: `skip_leading_
   trivia` перед проверкой keyword'ов typedef/extern.
2. **`static inline` fn-DEFINITION** (2 постоянных исключения Ф.1:
   `nova_typeid_user_name`, per-E throw `_nova_throw_typed_<m>`) — обычная
   `FnDef`-классификация резала на прототип (common.h) + тело (один part).
   Для `inline` это неверно: `static`-функция с одним прототипом в TU не имеет
   тела ТАМ → `lld-link: undefined symbol` во всех прочих part'ах. Фикс:
   `sig_has_inline_keyword` → весь unit (сигнатура+тело) остаётся вербатим в
   `_common.h` (безопасен для per-TU дублирования).

## Unit-тесты split_tu (25, все зелёные — standalone `rustc --edition 2021
--test src/codegen/split_tu.rs`; `cargo test` крейта по-прежнему сломан
пре-существующим дефектом вне периметра, см. f1-notes)

Добавлено к 22 существующим (+3):
- `classify_global_def_extracts_name_and_extern` — переписан: типизованный
  `nova_int _nova_const_X_value;` → `GlobalDef` + `extern` (был `DeclOnly`).
- `split_tu_global_with_initializer_gets_extern_and_single_definition` —
  переписан: header содержит ТОЛЬКО extern-форму, определение в 1 part.
- `split_tu_lazy_const_storage_single_part_no_duplicate_across_multiple_parts`
  (NEW) — e2e: 2 storage-cell + consts_init + 2 fn через сниженный порог →
  каждое определение ровно в 1 part, extern в header, 0 дублей.
- `classify_typedef_with_leading_comment_is_not_promoted_to_global_def` (NEW).
- `classify_static_inline_fn_def_stays_header_verbatim_whole` (NEW).

## Верификация (targeted; мега-CU conformance НЕ гонял — авторитет интегратор)

### Флаг ON (`NOVA_MULTI_TU=1`, сниженный порог для форсинга split; порог
РЕВЕРЧЕН перед коммитом — `git diff` на threshold-константах пуст):
| фикстура | результат |
|---|---|
| `standalone/repro_const_dup` | PASS |
| `standalone/e2e_collate` | PASS |
| `standalone/pos_full_unicode` | PASS |
| `app_effect_basic_t8_1` | PASS |

Все 4 — БОЛЬШЕ не duplicate-symbol. e2e_collate режется на 4 part'а; каждый
`_nova_const_*_value` определён ровно в 1 part, `extern` в `_common.h`.

### Флаг OFF (байт-идентичность дефолта): `getting_started.nv` без
`NOVA_MULTI_TU`, сравнение patched-бинарь vs baseline-бинарь (`9fada7e3d`,
temp-worktree, оба flag-off, `NOVA_CACHE=0`, `.c` через `--keep-artifacts`):
- `diff` показал ТОЛЬКО reorder 4 строк `typedef struct Nova_X Nova_X;`.
- `sort baseline.c | diff sort patched.c` → **пусто (exit 0)**: множества
  строк идентичны, разница только в ПОРЯДКЕ этих typedef — пре-существующий
  HashMap-iteration недетерминизм (f1-notes уже задокументировал ровно это,
  воспроизводимо между двумя прогонами ОДНОГО бинаря). Моя правка добавила/
  убрала/изменила 0 строк во flag-off пути. Вердикт: байт-идентичен.

⚠ Оговорка (как в f2-notes про Task B): фикс класса B (emit_c.rs pre-seed) НЕ
gated по флагу — для CU с `ro X = free_fn()` без аннотации дефолтный `.c`
ТЕПЕРЬ отличается от pre-fix (`ty_c` был "", стал корректным типом). Это
НАМЕРЕННЫЙ баг-фикс (типелесс/implicit-int-32bit → корректный nova_int-64bit),
не регрессия. Для CU без этого shape (getting_started) дефолт байт-идентичен.

## Правки
- `compiler-codegen/src/codegen/emit_c.rs` — pre-seed `user_fn_sigs` (класс B);
  threshold-константы реверчены к оригиналу.
- `compiler-codegen/src/codegen/split_tu.rs` — `decl_from_uninitialized_global`
  + `skip_leading_trivia` (класс A/C1) + `sig_has_inline_keyword` (класс C2) +
  3 новых теста, 2 переписанных.
- `spec_tests/conformance/standalone/repro_const_dup.nv` — минимальный репро.
