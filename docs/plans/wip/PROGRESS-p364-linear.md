# PROGRESS — окно p364-linear-gate (№364, К1 звучность)

Модель: sonnet. Worktree `d:\Sources\nv-lang\nova-p364`, ветка `p364-linear-gate`.

## Диагноз — подтверждён полностью

Все три формы из брифа воспроизведены дословно на pre-fix коде (`nova check`):

| Форма | до фикса | после фикса |
|---|---|---|
| `consume lst TcpListener = …; spawn { …lst… }` | ловит | ловит (не тронуто) |
| `consume lst = …; spawn { …lst… }` (без аннотации) | **молчит** | `E_LINEAR_CAPTURE_IN_FIBER` |
| `match … { Ok(consume l) => spawn { …l… } }` | ловит | ловит (не тронуто) |
| `ro (r, w) = s.into_split(); spawn { sink(w) }` | **молчит** | `E_LINEAR_CAPTURE_IN_FIBER` |

Корень — `compiler-codegen/src/types/mod.rs`, `walk_stmt`'s `Stmt::Let` registration (не в
`flag_boundary_captures` самом — там код не менялся, дыра была в том, ЧТО кладётся в
`ScopeBinding` на входе): единый `ty`, склонированный на ВСЕ имена паттерна, и `linear_pattern`,
питаемый только из `Pattern::Ident.is_consume` (pattern-суб-бинды вида `Ok(consume x)`) — но НЕ
из `LetDecl.consume` (флаг верхнеуровневого `consume`-keyword, D133) и не из типа RHS для
tuple-деструктуризации.

## Место фикса (чекер-канал, §0/196)

`compiler-codegen/src/types/mod.rs`, `impl<'a> CapabilityCtx<'a>`:

1. `walk_stmt`'s `Stmt::Let` arm (~28019-28075) — переписан:
   - `linear_pattern: pat_consume || d.consume` — LetDecl-level `consume` теперь так же
     авторитетен, как pattern sub-bind (симметрия, которую уже обосновывал существующий
     комментарий над `pattern_linear_flagged`).
   - Per-element type resolution для `Pattern::Tuple`-леты: если есть явная tuple-аннотация —
     берём её элементы; иначе (RHS — `recv.method(..)`) — новый хелпер
     `resolve_tuple_call_return` резолвит return-тип через `self.sig.method_table`, когда
     receiver уже несёт известный статический тип в `state.scopes`. Вычисляется ДО
     `state.scopes.last_mut()` (borrow-конфликт иначе).
2. Новый метод `resolve_tuple_call_return(&self, e: &Expr, state: &CapState) -> Option<Vec<TypeRef>>`
   (~29408-29435) — консервативен: `None` при любой неоднозначности (RHS не
   `recv.method(..)`, receiver не голый уже-типизированный `Ident`, метод не резолвится
   РОВНО в одну перегрузку, возврат не `TypeRef::Tuple`).

`flag_boundary_captures` (обе диагностики, `linear_flagged`/`pattern_linear_flagged`) —
байт-в-байт без изменений; переиспользуются существующие тексты, третий вариант не заведён.

## Вердикты прогонов (дословно)

- `cargo build -p compiler-codegen --lib`, `cd nova-cli && cargo build --release` — чисто,
  без новых warning'ов/ошибок.
- 3 новые neg-фикстуры (`spec_tests/conformance/neg/`) — все три ловят
  `[E_LINEAR_CAPTURE_IN_FIBER]`:
  - `p364_consume_let_unannotated_spawn_neg.nv`
  - `p364_consume_let_unannotated_detach_neg.nv`
  - `p364_tuple_destructure_consume_spawn_neg.nv`
- Регресс старых фикстур (`nova check`, байт-в-байт те же тексты диагностик):
  `neg/d188_linear_capture_spawn_neg.nv`, `neg/d188_linear_capture_parfor_neg.nv`,
  `neg/detach_consume_escape_neg.nv` — FAIL (ожидаемо, тот же текст); `detach_consume_move_ok.nv`,
  `share_capture_ok_test.nv`, `d91_chan_writer_share.nv`, `pos_channel_send_consume_share.nv` —
  PASS.
- `nova check std/src` → `PASS: 148  FAIL: 26  WARN: 61` — канон без сдвига. Грепом по
  `E_LINEAR_CAPTURE_IN_FIBER` в полном выводе — 0 вхождений (в т.ч. `net/split_test.nv`,
  `net/byte_surface_test.nv`, `net/d432_cleanup_rollout_test.nv` — тяжёлые пользователи
  `into_split()` — все деструктуризации там происходят ВНУТРИ того же файбера, ни одна не
  пересекает spawn/detach-границу, поэтому строже-гейт молчит корректно).
- `arch-ratchet.sh` → `lines=64171 <= 64171`, `infer=348 <= 348` — без роста (фикс только в
  `types/mod.rs`, `emit_c.rs` не тронут).
- `examples/net/echo_server.nv` + `echo_client.nv` → `PASS: 2  FAIL: 0`.
- `examples/flagship/aggregator` → `PASS: 12  FAIL: 0  WARN: 57`.
- `nova test std` (полный) — заблокирован ПРЕДСУЩЕСТВУЮЩИМ, не относящимся к №364 CC-FAIL
  (`std/src/net/addr.c:20015` — `nova_unit` vs `NovaRes_nova_int_NovaValue_IoError*`
  incompatible-type); воспроизведено БАЙТ-В-БАЙТ идентично на немодифицированном
  `nova/nova-cli/target/release/nova.exe` (main, коммит `eef272301`) — не регрессия этого окна.
  Корректность подтверждена на уровне `nova check` (см. выше); `nova test` для net-модуля
  недостижим из-за этого несвязанного бага.

## Пакетные репы — счёт до/после

| Репо | до | после |
|---|---|---|
| `nova-polaris` (`src/`) | `PASS: 48  FAIL: 7` | `PASS: 55  FAIL: 0` |
| `nova-http` (`src/`) | `PASS: 4  FAIL: 3` (pre-existing, не относится к делу) | без изменений |
| `nova-tls` (`src/`) | `PASS: 1  FAIL: 1` (pre-existing, не относится к делу) | без изменений |

`nova-http`/`nova-tls`'s FAIL — существующие `neg/`-фикстуры (`D133-not-consumed`,
`использование потреблённой переменной`, `E_READONLY_FIELD`) — не про линейность в файберах,
не тронуты.

## Настоящие нарушения корпуса — найдены и мигрированы (nova-polaris)

Строже-гейт вскрыл РЕАЛЬНЫЙ, систематический баг в 7 smoke-тестах пакета `nova-polaris`
(идентичная форма, явно скопированная между файлами):

```
consume lst = match TcpListener.bind(...) { Ok(consume l) => l, Err(_) => ... }
...
supervised {
    spawn {                    // ← lst захвачен ГОЛЫМ (by-reference copy), не move
        match lst.accept() { ... }
    }
    spawn { /* client, lst не трогает */ }
}
lst.close()                    // снаружи supervised
```

Файлы (все с идентичной формой, все починены):

- `src/rt/background_tasks_smoke.nv`
- `src/rt/handle_connection_smoke.nv`
- `src/rt/recover500_smoke.nv`
- `src/rt/streaming_smoke.nv`
- `src/rt/ws_upgrade_hijack_smoke.nv`
- `src/ws/rt/socket_echo_load_smoke.nv`
- `src/ws/rt/socket_echo_smoke.nv`

Миграция (канонический паттерн — listener живёт ЦЕЛИКОМ в server-файбере, ср.
`std/src/net/split_test.nv`): `spawn { … }` → `spawn consume lst { …; lst.close() }`, внешний
`lst.close()` после `supervised{}` удалён (владение ушло в дочерний файбер на самом
`spawn`-statement'е, cleanup — на выходе из ЕГО тела). Все 7 файлов проверены `nova check`
индивидуально и в составе полного `src/` прогона — `PASS: 55  FAIL: 0`, 0 вхождений
`E_LINEAR_CAPTURE_IN_FIBER`.

**Эти изменения — в отдельном репозитории (`nova-polaris`, свой git, ветка `main` не
затронута этим коммитом в `nova`) — требуют своего собственного коммита в `nova-polaris`,
сделанного отдельно от этого слияния.**

## Судьба смежного маркера `[M-consume-param-spawn-defer-active]`

**НЕ закрывается целиком.** Перепроверено после фикса гейта:

- Форма (1) матрицы (`consume c = conn; spawn { f(c) }`, bare capture) — репро
  (`scratch_364/form1.nv`, не сохранено) теперь получает `[E_LINEAR_CAPTURE_IN_FIBER]` при
  `nova check` — программа больше НЕ доходит до кодогена. Codegen-баг для ЭТОГО конкретного
  пути недостижим (моот) — гейт закрыл дорогу симптому.
- Форма (2) матрицы (`consume c = conn; spawn consume c { f(c) }`, явный re-give move) — гейт
  её НЕ трогает (санкционированный синтаксис, вне scope №364). Минимальный репро этой формы
  (свежий тип с `@close`+`@cleanup`) собрался ЧИСТО — не воспроизвёл `_defer_N_M_active`.
  Референс-репро `docs/plans/repro/m_consume_param_spawn_defer_active.nv` (fn-параметр форма,
  `@close` БЕЗ `@cleanup`) по-прежнему бьётся в задокументированный баг #1
  (`undefined symbol: Nova_MIceCpsHandle_consume_cleanup`) — не изменилось, №364 сюда не
  относится (это не bare-capture путь).

Вывод: часть симптоматики маркера устранена ПОБОЧНО фиксом №364 (bare-capture путь к
codegen-багу больше не существует), но сами codegen-баги (missing-cleanup undefined symbol,
`_nv_tmp_N` scope-leak, непойманный в чистом виде `_defer_N_M_active`) остаются — отдельная
работа в `emit_c.rs` consume-scope/cleanup канале, область сужена до санкционированной
re-give-формы + fn-параметр формы. Обновлено в `docs/plans/backlog-followups.md` и
`docs/plans/221.1-bug-sweep.md` (№364 отмечен ✅ ЗАКРЫТ).

## Модель

sonnet, всё окно целиком (диагностика, фикс, фикстуры, регресс, миграция nova-polaris,
докидка маркеров).
