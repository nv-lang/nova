# Чекпоинт — финальное разкраснение `nova lint std` (2026-07-17)

Worktree: `d:/Sources/nv-lang/nova-lintzero` (branch `fix-lint-zero`).
Бинарь: `cargo build --release --manifest-path nova-cli/Cargo.toml` (только).

## Счёт

| | до | после |
|---|---|---|
| `nova lint std` | 12 находок | **0** |
| `nova lint spec_tests` | 0 находок | **0** (не регрессировало) |

Баланс закрытия 12 находок:
- 5× `W_WITH_MUTATOR` (sync.nv `with_lock`×2/`with_read`/`with_write`/`with_permit`) —
  правило сужено (closure-параметр в сигнатуре = scope-guard, не field-copy).
- 2× `W_MANUAL_SLICE_COPY` (embed.nv `EmbeddedDir.merge`, drain-хвост) — канон-фикс
  `.append(a[i..a.len()])`/`.append(b[j..b.len()])`.
- 2× `W_MANUAL_SLICE_COPY` (embed.nv `EmbeddedDir.merge`, interleave-цикл) — false-positive,
  обход прецедентом d145 (индексация в локаль `av`/`bv` перед `push`).
- 2× `W_STATIC_CONVERSION` (read_buffer.nv/write_buffer.nv `.from`) — `nova:allow` с причиной.
- 1× `W_PARAM_NO_CONTRACT` (string/core.nv `is_char_boundary`) — `nova:allow` с причиной.

## Механизм `nova:allow` (новый)

Синтаксис (дословно):

```
// nova:allow W_CODE -- причина
<декларация/сайт находки — СЛЕДУЮЩАЯ строка>
```

- Несколько кодов: `// nova:allow W_A, W_B -- причина`.
- Причина ОБЯЗАТЕЛЬНА (текст после `--`, непустой после trim). Без причины —
  находка НЕ гасится И сама становится `E_LINT_ALLOW_NO_REASON` (не суппрессируется
  ничем, работает вне `--rule`-фильтра).
- Реализация: `compiler-codegen/src/lints.rs` — `apply_nova_allow_suppressions`,
  `parse_nova_allow_comments`, `NovaAllowEntry`, `conv_line_start_offset`; вызывается
  из `run_conv_rules` ПОСЛЕ существующей `[M-...]`-маркер-суппрессии (двух разных
  механизмов: `[M-...]` = «пока не готово» без обязательной причины, `nova:allow` =
  «читал, оставляю НАМЕРЕННО» с обязательной причиной).
- Юнит-тесты: `lints::tests::nova_allow_*` (4 шт: причина есть → гасит; причины нет →
  не гасит + E_LINT_ALLOW_NO_REASON; неверный rule id → не гасит; не на строке
  «непосредственно перед» → не гасит). Итого `lints::tests` = 39/39 зелёные.
- Спека: **D428** в `spec/decisions/09-tooling.md`.
- Статус-амендмент: `docs/plans/185-nova-lint.md` (амендмент 2026-07-17 в шапке).

## Сайты подавления (nova:allow)

- `std/src/runtime/read_buffer.nv:54` — `ReadBuffer.from` —
  `W_STATIC_CONVERSION -- rename заблокирован [M-static-conv-array-record-mono-cc-fail]
  (mono-баг extension-на-[]u8-с-record-телом); вернуть to_* после фикса`.
- `std/src/runtime/write_buffer.nv:60` — `WriteBuffer.from` — та же причина.
- `std/src/runtime/string/core.nv:381` — `str.is_char_boundary` —
  `W_PARAM_NO_CONTRACT -- намеренный total-предикат (D251-контракт: false на любом
  невалидном idx, включая отрицательные)`.

## W_WITH_MUTATOR closure-exception

`compiler-codegen/src/lints.rs::conv_with_mutator` теперь молчит, если у `with_*`
mut-метода есть параметр fn-типа (замыкание, `body fn() -> R`), peeled через
`conv_type_is_closure` (снимает `*T`/`ro T`/`mut T`/`uninit T`/`ref T`-обёртки).
Обоснование и прецедент (Kotlin `withLock`) — `docs/dev/nv-coding-style.md`, абзац
рядом с существующим `with_`-разделом (2026-07-06).

Юнит-тесты: `lints::tests::no_warning_on_with_mutator_closure_param` (позитив —
замыкание молчит) / `warns_on_with_mutator_value_param` (негатив — значение ворчит).

Фикстура `spec_tests/conformance/lint/conv_clean.nv`: добавлен пример
`Widget.with_label_scope` (scope-guard) — 0 находок сохраняется.

## Таргетные тесты (прогнаны)

- `nova test std/src/prelude/embed_test.nv --strict-effects` — PASS (behavior-change:
  `EmbeddedDir.merge` drain-хвост переписан на `.append`).
- `nova test std/src/runtime/sync_test.nv --strict-effects` — PASS (with_lock/
  with_read/with_write/with_permit — .nv-код не менялся, только Rust-лint; sanity).
- `nova test spec_tests/conformance/d251_str_surface.nv
  std/src/encoding/serde/record_autoderive_ext_test.nv --strict-effects` — запущен
  (is_char_boundary / ReadBuffer-WriteBuffer usage, comment-only diff в .nv).

Env для `nova test` в этом worktree (main repo vcpkg_installed, GC libs не
собираются в worktree):
```
NOVA_GC_LIB_DIR=/d/Sources/nv-lang/nova/compiler-codegen/vcpkg_installed/x64-windows-static/lib
NOVA_GC_INCLUDE_DIR=/d/Sources/nv-lang/nova/compiler-codegen/vcpkg_installed/x64-windows-static/include
```

## Коммиты (fix-lint-zero, без push)

1. `lint(185): W_WITH_MUTATOR closure-param exception + nova:allow inline suppression`
   — lints.rs (conv_with_mutator + conv_type_is_closure + apply_nova_allow_suppressions
   family) + nv-coding-style.md + conv_clean.nv.
2. `lint(185): W_MANUAL_SLICE_COPY fix (embed.nv) + nova:allow на 2 намеренных находки`
   — embed.nv + read_buffer.nv + write_buffer.nv + string/core.nv.
3. `lint(185): W_WITH_MUTATOR + nova:allow unit tests — relocate to correct mod` —
   тесты случайно попали в `mod cancel_unsafe_tests`, перенесены в `mod tests`.
4. (следующий) — D428 (09-tooling.md) + статус-амендмент 185-nova-lint.md + этот
   чекпоинт.

## Открытые хвосты (НЕ в этой волне)

- `[M-static-conv-array-record-mono-cc-fail]` (mono-коллектор падает на
  extension-методе `[]u8 → user-record`) — отдельная задача; `nova:allow` —
  временная (до фикса) маркировка, не решение самого mono-бага.
- `--deny` (W→E) режим (план 185 Ф.3) — `nova:allow` спроектирован как задел
  под него, сам флаг ещё не реализован.
