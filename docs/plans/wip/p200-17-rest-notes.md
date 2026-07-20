# Plan 200 П17 — остаток 3 файлов, sonnet-заход (2026-07-20)

Worktree: `d:/Sources/nv-lang/nova-p17rest`, ветка `p200-17-rest`.
Задача: `[M-p200-17-remaining-3]` — конвертировать `base64.nv`, `fmt_buf.nv`, `handlers.nv`
в папка-модули (`X.nv` → `X/core.nv` + `X/core_test.nv`).

## Итог: 2/3 сделано, 1/3 заблокирован НАЙДЕННЫМ компиляторным багом

### base64 — ✅ done
`std/src/encoding/base64.nv` → `std/src/encoding/base64/{core.nv,core_test.nv}`.
9 test-блоков ДО (`grep -c '^test "'`), 9 ПОСЛЕ в `core_test.nv`, 0 в `core.nv`. Никаких
импортов в `core_test.nv` не потребовалось (те же неявные builtins/prelude, что у core.nv).
`nova test std/src/encoding/base64/core_test.nv` → PASS 1/0.

### handlers — ✅ done
`std/src/testing/handlers.nv` → `std/src/testing/handlers/{core.nv,core_test.nv}`.
20/20 совпадает. **Ловушка, которую нужно было поймать самому** (не в брифе): файл начинался
с `// ENV NOVA_MAXPROCS=1` / `// ENV NOVA_AUTOARM=0` — это директивы `test_runner.rs::parse_env`
(парсятся из ПЕРВЫХ 30 строк ФАЙЛА, переданного `nova test` как `opts.nv_file` — см.
`compiler-codegen/src/test_runner.rs:2523-2532,2897-2899,3313-3316`). Поскольку тесты теперь
запускаются как `nova test .../core_test.nv`, а не `.../handlers.nv`, директивы физически
ДОЛЖНЫ жить в `core_test.nv` (не в `core.nv` — там их некому больше парсить, `core.nv` без
тестов просто SKIP). Перенёс ОБЕ строки в `core_test.nv`, поправил формулировку в header-copy
`core.nv` (директивы теперь «в peer core_test.nv», не «во всём файле»). Прошлая haiku-попытка,
судя по диагнозу интегратора («parse-ошибка в пире core_test.nv:87 expected fn/... got
identifier»), скорее всего затянула в тесто-пир кусок ИМПЛЕМЕНТАЦИИ (не ENV-директивы) —
другой класс ошибки, но тот же файл. `nova test std/src/testing/handlers/core_test.nv` → PASS 1/0.

### fmt_buf — ЗАБЛОКИРОВАН, split ревертнут

Split (`fmt_buf/{core.nv,core_test.nv}`) был выполнен МЕХАНИЧЕСКИ КОРРЕКТНО: export-поверхность
(`int_fmt_into`/`f64_fmt_shortest_into`/`f32_fmt_shortest_into`/`Align`/`FloatKind`/`Sign`/
`FmtKind`) не тронута, module-private хелперы (`int_fmt`/`bool_fmt`/`char_fmt`/`FmtSpec`/
`fmt_f64`/extern `f64_fmt_into`) резолвятся из `core_test.nv` без `import` (те же co-equal-file
правила, что hashmap/range) — САМ `core_test.nv` компилируется чисто, его собственные вызовы
приватников ошибок не дают.

НО: `nova test std/src/runtime/fmt_buf/core_test.nv` → CODEGEN-FAIL:
```
std/src/runtime/string_builder.nv:174:14: error: undefined identifier `int_fmt_into`
std/src/runtime/string_builder.nv:178:14: error: undefined identifier `f64_fmt_shortest_into`
std/src/runtime/string_builder.nv:187:14: error: undefined identifier `f32_fmt_shortest_into`
```
`string_builder.nv` НЕ импортируется fmt_buf вообще — но затягивается в ТОТ ЖЕ CU и не видит
экспорты fmt_buf. `nova test std/src/runtime/string_builder_test.nv` (другой entry) — PASS,
той же цепочки не задевает.

**Root cause (найден, не догадка — см. `[M-imports-entry-folder-module-self-cycle-empty-exports]`
в `docs/plans/backlog-followups.md`, P2 Codegen):** баг в `compiler-codegen/src/imports.rs`.
Когда CU-entry — файл folder-модуля A, `entry_key`(=A) в `in_progress` (imports.rs:723) держится
до конца ВСЕЙ функции `resolve_imports_inline_ex` (снимается только на imports.rs:1133),
включая drain `pending_peer_preludes` (imports.rs:1075-1105). Тестовый `buf.ptr()` в
`core_test.nv` триггерит `needs_vec_injection` (imports.rs:572/582-591/998-1008) →
auto-import `std.collections.vec` → его peer'ы не `#no_prelude` → отложенный prelude-drain
затягивает `std/prelude/collections.nv:212` (`export import std.runtime.string_builder`) →
`string_builder.nv` в ТОТ ЖЕ CU. `string_builder.nv:28` сам импортирует
`std.runtime.fmt_buf.{...}` — но `resolve_one`'s cycle-guard (imports.rs:1646-1650:
`if in_progress.contains(&module_key) { return Ok(()) }`) молча возвращается БЕЗ заполнения
`visible_acc`, т.к. fmt_buf(=entry) всё ещё "in_progress". Комментарий imports.rs:1131-1132
постулирует «entry's exports not cached ... entry is never dedup'd as an import by others in
the same resolve call» — этот инвариант ЛОЖЕН именно в этом сценарии (entry-модуль, чьи
экспорты нужны ДРУГОМУ файлу, затянутому в тот же CU auto-injection'ом, а не прямым импортом).

Это НЕ регрессия сегодняшней волны и НЕ ошибка конверсии — предсуществующий баг
import-резолвера, который просто не встречался раньше: ни один из 6 (теперь 8) уже слитых
П17-split'ов не имел ВНЕШНЕГО cross-module потребителя своих экспортов (fmt_buf — единственный
из 9, чьи символы импортирует другой std-модуль).

**Действие:** split fmt_buf РЕВЕРТНУТ (`git reset` + `rm -rf fmt_buf/` + `git checkout --
fmt_buf.nv`) — вернул единый `fmt_buf.nv` с 8 инлайн-тестами как было. Конвертировать нельзя,
пока баг в `imports.rs` не починен (сам фикс — вне объёма этого захода: правка
cycle-detection логики в резолвере импортов, не std-конверсия).

## Верификация (все таргетные, все PASS)
- `nova test std/src/encoding/base64/core_test.nv` → PASS 1/0
- `nova test std/src/testing/handlers/core_test.nv` → PASS 1/0
- `nova test std/src/runtime/string_builder_test.nv` → PASS 1/0 (мосты fmt_buf живы, т.к. fmt_buf
  остался единым файлом)
- `nova test std/src/checksums` → PASS 3/0 SKIP 3 (δ0, как в П18-прецеденте)
- Грепы `^test "` в `base64/core.nv`/`handlers/core.nv` = 0; счётчики тестов до/после: base64
  9=9, handlers 20=20 (fmt_buf не трогался — остался 8 инлайн, как исходно)

## Документация
- `docs/plans/backlog-followups.md`: `[M-p200-17-remaining-3]` → сужен в
  `[M-p200-17-remaining-1-fmtbuf]` (P3, блокирован) + новый `[M-imports-entry-folder-module-self-cycle-empty-exports]`
  (P2 Codegen, полное root-cause описание с file:line).
- `docs/plans/200-std-improvements.md` Пункт 17: «6/9» → «8/9», остаток описан.
- Пункт 17 НЕ закрыт целиком (fmt_buf не входит) — `docs/history/simplifications-closed.md`
  получил ЧАСТИЧНУЮ запись (base64+handlers), НЕ финальное закрытие Пункта 17.
