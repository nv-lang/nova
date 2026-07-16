# [M-187-http-serde-setcookie-serialize-collision] — прогресс фикса

**Ветка:** `fix-serde-dispatch`, worktree `d:/Sources/nv-lang/nova-serdefix`.
**Модель:** sonnet.

## Root-cause (найден эмпирически, НЕ там, где ожидалось из задания)

Задание предполагало dispatch-баг в `emit_c.rs` (mono-инстанциация generic fn →
name-keyed резолв `.serialize()`, тот же класс что 196.7/98e3663cc). Эмпирическая
проверка (минимальная фикстура: `Dto` с `#impl(Serialize)` + `FooCookie` с ручным
`@serialize() -> str`, ОДИН CU, `nova build`) показала:

- `nova test`/`nova-codegen`'s `cmd_compile` (main.rs) — оба вызывают
  `auto_derive::inject_synthesized_methods_filtered(&mut module, |p| p ==
  "Serialize" || p == "Deserialize")` ПЕРЕД numbering/check (test_runner.rs
  ~3864, main.rs ~401) — синтезированный `@serialize` для `#impl(Serialize)`
  типа становится РЕАЛЬНЫМ `FnDecl` в `module.items`.
- **`nova build` (`cmd_build`, nova-cli/src/main.rs) НИКОГДА не вызывал
  `inject_synthesized_methods_filtered`/`inject_synthesized_methods` вообще.**
  Чекер (`check_module`) проверяет `v.serialize(s)` через СВОЙ on-demand
  bridge (`AutoDeriveQueryBridge`/`synthesize_method`, types/mod.rs) —
  ВИРТУАЛЬНО, не мутируя `module.items`. Type-check проходит, но codegen
  (emit_c.rs), сканируя `module.items` для `method_overloads`/
  `mono_method_decls`, НЕ находит НИКАКОГО FnDecl для `Dto.serialize` — запись
  под ключом `("Dto","serialize")` попросту ПУСТАЯ.
- Дальше `v.serialize(s)` внутри mono'нного `json_encode[T=Dto]` проходит
  ВСЕ receiver-typed dispatch-окна (concrete-key, generic-instance 5b,
  Ф.3 protocol-blanket, 196.7 facade — все НЕ находят кандидата) и падает в
  единственный ОСТАВШИЙСЯ путь: single-key name-only `method_receivers`
  last-wins fallback → берёт ЛЮБОЙ ДРУГОЙ конкретный `@serialize` в CU,
  зарегистрированный последним (в проде — `http`'s `SetCookie @serialize()
  -> str`; в изолированной фикстуре без http — `[]T @serialize` сентинел или
  `FooCookie`).

Подтверждено: убрать `FooCookie`/http — баг НЕ пропадает (падает на
`__mono_method__[]T__serialize` unresolved sentinel) → это НЕ per-call
mis-dispatch эвристики, а ПОЛНОЕ отсутствие регистрации derived-метода на
`nova build`-пути. Подтверждено доп. пробой: тип, объявленный ВНУТРИ
`std/src/encoding/serde/` (module `encoding.serde`, тот же модуль что
`json_encode`), собранный через `nova build` (не `nova test`) — ТА ЖЕ
поломка. `nova test` того же файла — PASS. Значит переменная — не «модуль
записи типа», а «build vs test_runner.rs pipeline».

Комментарий в существующем коде cmd_build (~4826, Ф.4c) прямым текстом уже
документирует ЭТОТ ЖЕ класс истории: "the `nova build` path had silently
omitted" каналы, которые `test_runner.rs`/`main.rs` уже кормили — здесь то же
самое для serde auto-derive injection, просто не было пофикшено раньше.

## Фикс

`nova-cli/src/main.rs::cmd_build` — добавлен
`auto_derive::inject_synthesized_methods_filtered(&mut module, |p| p ==
"Serialize" || p == "Deserialize")` ПЕРЕД alpha-rename (после import-resolve/
embed-resolve), зеркалируя точную позицию в `test_runner.rs`. НЕ добавлен
unfiltered `inject_synthesized_methods` (Equal/Clone/Compare/Hash/Display/
Debug) — вне заявленного скоупа (Serialize/json_encode), отдельный потенциальный
follow-up, не трогаю чтобы не расширять дифф.

`emit_c.rs` НЕ тронут — существующие receiver-typed dispatch-окна (36309
direct-key, has_sentinel_here → mono_method_decls) уже корректно резолвят
ПО ТИПУ, как только FnDecl реально зарегистрирован.

## Верификация минимальной фикстуры

`scratch_repro_final/main.nv` (Dto#impl(Serialize) + FooCookie ручной
@serialize, коллизия имён, ОДИН CU): ДО фикса — либо compile-error
(too many arguments to Nova_FooCookie_method_serialize) либо (без коллизии)
undeclared identifier `__mono_method__[]T__serialize`. ПОСЛЕ фикса — builds,
runs: `{"x":42,"y":"hi"}sess=1` (оба serialize правильно диспетчированы,
раздельные C-символы `Nova_Dto_method_serialize____Nova_JsonSerializer_p` /
`Nova_FooCookie_method_serialize`).

## Реальный репро (examples/flagship/aggregator) — ПРОЙДЕНО

`examples/nova.local.toml` создан (gitignored) — `[replace] tls = { path =
"../../nova-tls" }` (http уже `path`-dep в `examples/nova.toml` самом,
../../nova-http). `examples/flagship/aggregator/src/main.nv`:
- `snapshot_body`/`events_body`'s `run_summary` теперь — `snapshot_to_json(dto)`
  (typed, `report_json.nv`) вместо hand-written `snapshot_dto_json`.
- Удалены hand-written renderer'ы `status_dto_json`/`result_dto_json`/
  `handlers_dto_json`/`snapshot_dto_json` + весь WORKAROUND-комментарий-блок
  (был "[M-187-...] compiler codegen bug... WORKAROUND below") — заменён на
  короткую RESOLVED-заметку с точным root-cause + ссылкой на фикс.
- `emit_record_json`/`EmitRecord` (SSE per-event payload) СОЗНАТЕЛЬНО оставлен
  hand-written — НЕ баг-обход, а wire-shape решение (условно опускает
  `"error"` при kind != lane_failed; plain derive всегда эмитил бы поле) —
  follow-up отмечен в комментарии, не в scope этого фикса.
- `json_escape` восстановлен (используется только `emit_record_json`
  теперь). Импорты подчищены (`ResultDto`/`StatusDto`/`SnapshotDto` больше не
  нужны в main.nv; добавлен `snapshot_to_json` из `./api`).

Собрано СВОИМ компилятором (`nova build`, worktree, `NOVA_CACHE=0`,
`--strict-effects`) — **built** без ошибок (http+tls оба в CU). Запущено и
проверено curl'ом:
- `/api/snapshot` — корректный typed JSON (все поля `SnapshotDto`, вложенные
  `ResultDto`/`StatusDto`/`HandlersDto`, `Option[str]` error → `null`/строка).
- `/api/run?legend=health&mode=chaos&seed=7` — корректный typed JSON.
- `/api/events` (SSE replay) — работает, `run_summary` event несёт тот же
  typed JSON, per-event payload (`emit_record_json`, ручной) не тронут.

Порт 8187 освобождён после smoke (`taskkill` по PID из netstat).

## Регрессы

- `std/src/encoding/serde/*_test.nv` (все 6 файлов, каждый отдельно через
  `nova test`) — PASS, без изменений (эти уже шли через `test_runner.rs`,
  которая ВСЕГДА вызывала inject — фикс их не касается, byte-identical).
- `examples/tls/echo_server.nv` + `examples/tls/echo_client.nv` — оба собраны
  через `nova build` (тот же путь, что чиню) — **built** без ошибок; smoke
  прогон (server фон + client) — `TLS established (tls 1.3)` + `Echo:
  echo_ok`. Порт 7778 освобождён.

## nova.lock

`examples/nova.lock` показывал `M` в git status, но побайтовое сравнение с
HEAD — IDENTICAL (CRLF-шум git, не реальное изменение) — НЕ застейджен.

## Осталось

1. Закрыть маркер [M-187-http-serde-setcookie-serialize-collision] в
   docs/plans/backlog-followups.md (✅ РЕШЕНО + root-cause + коммит-хэш).
2. Запись в docs/simplifications.md.
3. Коммит main.nv (typed serde + снятый обход).
