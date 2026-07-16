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

## Следующие шаги

1. Реальный репро: examples/flagship/aggregator (SnapshotDto + http в CU) —
   переключить main.nv::snapshot_body() на typed путь, собрать через
   `nova build` (diamond nova.local.toml [replace] tls/http → ../../nova-tls,
   ../../nova-http), curl-smoke.
2. Снять обход (WORKAROUND-маркер) из main.nv насовсем.
3. Таргетные regress: std/src/encoding/serde, tls/echo_server, echo_client.
4. Закрыть маркер [M-187-http-serde-setcookie-serialize-collision] в
   docs/plans/backlog-followups.md.
5. simplifications.md запись.
