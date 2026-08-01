# Checkpoint — [M-nested-generic-receiver-method-mono] (реестр 221.1 №247), окно p-nestmono, sonnet

## Итог: маркер №247 ЗАКРЫТ, transpose добавлен, побочно найден №248 (не в объёме)

Ветка `p-nestmono`, worktree `d:/Sources/nv-lang/nova-nestmono`, база main `1a3d18ffe`.
Два коммита: `0f8adbfb8` (fix) + `f945ae74c` (feat prelude/фикстура/D26). НЕ запушено —
ветка сдана интегратору.

## Ф.0 — где терялась подстановка (найдено чтением сгенерированного C, не догадкой)

Builtin Option/Result `DeclaredBody`-диспетч (`emit_c.rs` ~39580 Option / ~39900
Result, Plan 95/99.1) сидит `type_subst` ОДНИМ слотом: carrier-параметр самого
`Option`/`Result` (в prelude ТОЖЕ называется "T"/"E") ↦ ЦЕЛИКОМ конкретный элемент
ресивера. Для ФЛАТ-приёмника (`Option[T] @flat_map`) это ровно метод-level T —
работает. Для ВЛОЖЕННОГО (`Option[Result[T, E]] @transpose`) метод-level T/E
(введённые декомпозицией `Result[T,E]` — checker-side, `E_DUPLICATE_GENERIC_DECL`
avoidance подтверждена рабочей) СОВПАДАЮТ ПО ИМЕНИ с carrier-level "T" — single-slot
bind оставлял T = целый Result-моно (не int), E — вообще небинденным.

Матрица форм (проверено repro, не в conformance):
- (а) вложенность в приёмнике / плоский возврат — `Option[T] @wrap_ok() -> Result[Option[T], str]` — работало и ДО фикса (плоский приёмник).
- (б) плоский приёмник / вложенный возврат — то же самое (а) на самом деле покрывает и это.
- (в) обе — `transpose` сам (Option[Result[T,E]] → Result[Option[T],E]) — ЭТА координата ломала: carrier-level/method-level коллизия на ИМЕНИ, а не на глубине как таковой.
- Тройная вложенность `Option[Option[Result[T,E]]]` — работает после фикса (repro-матрица, все 9+ ассертов PASS).

## Ф.1 — фикс

Новый `debt_rebind_nested_receiver_typevars` (`emit_c.rs`, рядом с
`receiver_ty_is_nested`/`collect_receiver_typevars`, Plan 153.5 precedent).
Вызывается ПОСЛЕ того, как `recv_c` уже вычислен из shallow-bind'а (иначе ломается
self-деструктуризация `@`) — структурно перевязывает `current_type_subst` через
`infer_type_param_binding(fn_decl.receiver.receiver_ty, recv_c, ..)`, только для
`receiver_ty_is_nested`. 4 точки вызова: `register_mono_method_instance` +
`emit_monomorphized_method_scoped_inner` (обе mono-emission функции, после их
собственного recv_c) + оба builtin-dispatch pre-registration блока
(`ensure_novaopt_decls_for_typeref(ret)` сайты, Option и Result).

## Вердикты (дословно)

- `spec_tests/conformance/plan200_flat_map_err.nv` standalone: `PASS: 1  FAIL: 0`
- `spec_tests/conformance/plan200_14_option_result_flat_map_filter.nv` standalone:
  `PASS: 1  FAIL: 0` (брифовое предупреждение «standalone даёт ложный E_D78» НЕ
  воспроизвелось на этой волне — чисто PASS)
- `spec_tests/conformance/plan200_transpose.nv` standalone: `PASS: 1  FAIL: 0`
  (компилируется ~2m40s — HashMap+closures+несколько test-блоков, НЕ зависание,
  дождался)
- `nova test std/src/math`: `PASS: 5  FAIL: 0  SKIP: 3`
- `cargo build --release`: зелёный (несколько итераций, финальная чистая)

## Побочно найден №248 — [M-nested-builtin-second-instance-typedef-splice-gap]

Второй `transpose`-инстанс с (T,E) ≠ буквально `(nova_int,nova_str)` (проверено:
`(bool,int)`, `(str,int)` — ОБА CC-FAIL) падает на C-typedef уровне
(`unknown type name 'NovaRes_...'`), НЕ на subst — подстановка внутри mono'd
тела/сигнатуры доказанно КОРРЕКТНА (прочитан generated C). Root: `(int,str)` —
единственная жёстко прописанная в `nova_rt/array.h` пара; ЛЮБАЯ другая требует
`register_novares_decl`, чей typedef-буфер сплайсится ОДИН раз в
`/*__NOVARES_TYPEDEFS__*/`. Две ПОПЫТКИ фикса в рамках этого окна ОБЕ провалились
байт-в-байт идентично: (1) структурная RT-подстановка через checker-канал
`resolved_types[obj.id]` (подтверждено eprintln — RT корректна), (2) принудительный
`resolved_type_to_c` на ПОЛНОМ RT ресивера (должен триггерить регистрацию
побочным эффектом). Подозрение — ДВА разных Emitter/pass-прохода (probe vs real),
side-effects одного не переживают до финального сплайса другого — требует
понимания ОБЩЕЙ архитектуры проходов, вне бюджета этого окна. НЕ в объёме №247
(тот — про subst-биндинг, доказанно решён). Заведён в оба реестра (backlog +
221.1 №248), P3, обход — не смешивать разные (T,E) одного nested builtin-метода
в одном CU.

## Что НЕ сделано и почему

- Второй ТИПОВОЙ инстанс `transpose` (`Option[Result[str,int]]`) в фикстуре —
  упирается в №248, убран из `plan200_transpose.nv` с explicit-комментарием;
  заменён на «повторная инстанциация ТОЙ ЖЕ (T,E) пары» (слабее, но честно).
- `ParseIntError` (record error type) как E в `Option[Result[int,ParseIntError]]`
  — СОВСЕМ ДРУГОЙ, тоже pre-existing, НЕ задокументированный отдельным маркером
  этой волной (обнаружен, обойдён в фикстуре через `.map_err(|_| "...")`→str,
  НЕ заведён как маркер — вне бюджета, стоит завести отдельно интегратору).
- Мега-CU/flagship gate НЕ гонялся (по инструкции брифа — точечная верификация,
  авторитетный гейт у интегратора).
