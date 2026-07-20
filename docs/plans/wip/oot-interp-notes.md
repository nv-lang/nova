# [M-standalone-out-of-tree-interp-sb-typedef] — рабочие заметки

Ветка `p-fix-oot-interp`, worktree `nova-ootfix`. Модель sonnet.

## Репро (подтверждено)

`%TEMP%/oot_probe/f.nv` (BOM недопустим — писать через bash printf, не PowerShell
Set-Content, иначе `unexpected byte: 'ï'`):

```
test "t" {
    ro a = 1.5
    assert("${a}" == "1.5")
}
```

`nova test "C:\...\oot_probe\f.nv"` → CC-FAIL:
`must use 'struct' tag to refer to type 'Nova_StringBuilder'` +
`initializing 'nova_str' with an expression of incompatible type 'int'`.

## Root cause (подтверждён чтением + репро + control-check через `nova build`)

`compiler-codegen/src/test_runner.rs::codegen_to_c` (вызывается ТОЛЬКО из
`nova test`/`nova test-build`, НЕ из `nova build`) резолвил repo root САМ,
по пути ФАЙЛА: `find_repo_root_from(path)` — walk вверх от директории .nv-файла
в поисках `nova.toml`. Для файла вне дерева (`%TEMP%/...`) это возвращает
`None` → ВЕСЬ блок `resolve_imports_inline_ex` + `collect_all_signatures`
(prelude auto-import + explicit imports + cross-module sig table) тихо
пропускался целиком. Это НЕ узкий баг про typedef — пропускался ЛЮБОЙ
prelude-импорт для any out-of-tree файла.

`std.prelude` ре-экспортит `StringBuilder` (`std/src/prelude.nv:125`,
`export import std.prelude.collections.{... StringBuilder ...}`).
`StringBuilder` — Nova-defined type (Plan 109/D179,
`std/src/runtime/string_builder.nv`) — его C-typedef И тела методов
(`Nova_StringBuilder_static_new` и т.п.) эмитятся ТОЛЬКО когда его
Nova-декларация реально попала в `module.items` (через import).
`emit_interpolated_str` (emit_c.rs ~41352) синтезирует raw C-вызовы к этим
именам БЕЗУСЛОВНО, без AST-узла — рассчитывая, что StringBuilder уже
«где-то» пришёл через prelude. Раз prelude не импортировался — ни typedef,
ни тела вообще НЕ emit'ились (проверено чтением сгенерированного .c —
`Nova_StringBuilder_static_new` в файле нет НИГДЕ, ни typedef, ни extern).

Симптом №2 (`nova_str` ← `int`) — КАСКАД от №1, не отдельный дефект:
`nova_f64_to_str`/`Nova_StringBuilder_consume_into_str` возвращаемый тип не
резолвится (тип StringBuilder неизвестен) → codegen откатывается на int по
умолчанию для необнаруженного вызова. Отдельно не чинил — ушёл вместе с №1.

### Почему `nova build` не ловит баг
`cmd_build` (nova-cli/src/main.rs) уже резолвит `repo`/`paths.stdlib_dir`
через CWD-based `find_repo_root()` (не по пути файла) И передаёт их explicit
в `resolve_imports_inline`/`resolve_embeds` (main.rs:4921, 4933) — тот же
механизм, что я применяю к `nova test`. Control-check: `nova build` на ТОМ ЖЕ
репро (interpolation f64, out-of-tree) — PASS без изменений.

## Фикс

Унификация: `codegen_to_c` больше не вызывает `find_repo_root_from(path)`
внутри себя. Вместо этого принимает `repo: &Path, stdlib_dir: &Path` —
те же значения, что `nova-cli::cmd_test`/`cmd_test_build` уже резолвят ОДИН
раз через CWD-based `find_repo_root()` (нитка та же, что даёт
`cg_include`/`rt_dir` — они уже НЕ per-file). Threaded through:
`TestAllOpts`/`TestBuildOpts` (compiler-codegen/src/test_runner.rs) → новые
поля `repo`/`stdlib_dir` → `nova-cli/src/main.rs` (`cmd_test`, `cmd_test_build`)
+ `compiler-codegen/src/main.rs` (`nova-codegen test-build`, использует свой
CWD-only `repo_root`).

Тот же `repo` параметр заменил `find_repo_root_from(path)` и во ВТОРОМ месте
внутри `codegen_to_c` — `embed_dir_warnings`/`resolve_embeds` (project_root
для `embed()`/`embed_dir()`), т.к. `cmd_build` там ТОЖЕ уже использует
CWD-resolved `repo`, а не per-file derivation — тот же класс бага, тот же
фикс, одной волной (zero-tolerance).

`find_repo_root_from` НЕ удалён — используется в `compiler-codegen/src/main.rs`
(`nova-codegen check`/др., отдельный бинарь, вне скоупа) и в
`compiler-codegen/src/doc/test_runner.rs` (doc-test pipeline, отдельно).

`infer_call_ret_c` НЕ тронут (frozen, per задание).
`imports.rs` НЕ тронут — баг был в CALLER'е (test_runner.rs), не в резолвере.

## Статус

- [x] Root cause найден и подтверждён (репро + control-check nova build).
- [x] Фикс applied (test_runner.rs + nova-cli/main.rs + compiler-codegen/main.rs).
- [x] Билд + репро-гейт PASS (int/f64/f32/str — 4 ассерта, out-of-tree `%TEMP%`).
- [x] Rust regression test `nova-cli/tests/oot_interp_stringbuilder.rs` — PASS
      (`cargo test --release --test oot_interp_stringbuilder`).
- [x] In-tree parity: `string_builder_test.nv` — same-tree old-binary vs
      new-binary .c MD5 IDENTICAL (`f136c43d9cd6eb8c74d87edbdb12e80d`).
      ВАЖНО: первая попытка сравнить со старым .c из ГЛАВНОГО репо (`nova`,
      не worktree) дала ложный diff (7 строк, номера строк contract-сообщений
      сдвинуты на 2-3) — это ДРЕЙФ файлов между `nova`/`nova-ootfix` (другие
      агенты правят main repo параллельно), НЕ регрессия фикса. Правильный
      baseline = старый бинарь ПРОТИВ ТОГО ЖЕ дерева (worktree), не старый
      бинарь+старое дерево из другого репо.
- [x] Второй спот-фикстур `spec_tests/conformance/tuple_fixarr_typedef.nv` —
      old-binary vs new-binary .c diff exit 0 (identical).
- [x] Маркер закрыт в docs/history/simplifications-closed.md +
      строка убрана из docs/plans/backlog-followups.md.

## Итог

Полностью закрыт этой волной. Диагноз, фикс, оба регресс-канала (Rust
integration test + in-tree parity) зелёные. Готово к коммиту (НЕ мёржить в
main, НЕ пушить — по заданию).
