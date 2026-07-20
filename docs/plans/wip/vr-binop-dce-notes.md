# Чекпоинт — [M-vr-binop-wrapper-decl-order-standalone-cu] фикс (2026-07-20)

Worktree: `d:/Sources/nv-lang/nova-vrbinop2` (branch `p-fix-vr-binop-dce`).
Статус: **ЗАВЕРШЕНО** (готово к ревью/интеграции; НЕ смёржено, НЕ запушено).

## Диагноз (принят готовым, подтверждён эмпирически ДО правки)

Корень — НЕ decl-order (как было заведено в backlog изначально), а DCE-seed:
`compiler-codegen/src/lints.rs::collect_expr`, ветка `ExprKind::Binary`
(~1095-1128) сидит `equal`/`compare`/`concat` в reachability-DCE seed-набор
(Plan 159 Ф.1), но НЕ сидел `plus`/`minus`/`times`/`div`/`rem` — селекторы
value-record арифметики (Plan 175 Ф.1b/Ф.3, `emit_c.rs` ~29716-29738,
`nova_vr_binop_*`-обёртка). Без литерального `.plus(...)`-вызова в CU
type∧name closure никогда не помечает метод живым → DCE дропает decl+body
→ безусловно эмитированная обёртка зовёт несуществующий C-символ → CC-FAIL.

## Фикс

`compiler-codegen/src/lints.rs`, `ExprKind::Binary` рукав (после `concat`,
~1123-1128): добавлено
```rust
out.insert("plus".to_string());
out.insert("minus".to_string());
out.insert("times".to_string());
out.insert("div".to_string());
out.insert("rem".to_string());
```
+ комментарий с маркером и механизмом (см. файл).

## Репро/гейты (все зелёные)

- Новая фикстура: `spec_tests/conformance/standalone/vr_binop_arith_dce.nv`
  (`fn main`, `Monotonic + Duration`/`Monotonic - Duration`/
  `Duration * i64`/`Duration / i64`, БЕЗ литерального `.plus(...)`).
- **PRE-fix binary** (revert lints.rs → HEAD, rebuild) на этой фикстуре:
  CC-FAIL на `plus`/`times`/`div` одновременно — ровно предсказанная
  ошибка (`returning 'int' from a function with incompatible result type
  'NovaValue_Monotonic'`/`'NovaValue_Duration'`).
- **POST-fix binary**: builds + runs, stdout `VR_BINOP_ARITH_DCE_OK`.
- `nova test spec_tests/conformance/standalone --mode release`: **PASS 69
  FAIL 0** (включая новую фикстуру).
- `nova test std/src/time --mode release --strict-effects`: **PASS 6 FAIL 0
  SKIP 1** (skip = `cron` module без test-блоков/`fn main`, не провал).
- `nova build examples/flagship/aggregator/src/main.nv --strict-effects`:
  **built OK**.
- Мега-CU (`spec_tests/conformance` целиком) — **НЕ гонялся** (вне гейта
  этой задачи по прямому указанию).

## Маркер

`[M-vr-binop-wrapper-decl-order-standalone-cu]` закрыт:
- строка убрана из `docs/plans/backlog-followups.md`;
- запись закрытия (с корректировкой диагноза decl-order→DCE-seed) добавлена
  в `docs/history/simplifications-closed.md` (конец файла).

## Изменённые файлы (по имени, для `git add`)

- `compiler-codegen/src/lints.rs` (фикс)
- `spec_tests/conformance/standalone/vr_binop_arith_dce.nv` (новая фикстура)
- `docs/plans/backlog-followups.md` (маркер убран)
- `docs/history/simplifications-closed.md` (запись закрытия)
- `docs/plans/wip/vr-binop-dce-notes.md` (этот чекпоинт)

`examples/nova.lock` в статусе `M` — pre-existing, НЕ моё, НЕ добавлять.
