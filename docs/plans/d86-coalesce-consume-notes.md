# D86-followup: `??` consume-propagation fix — session notes (checkpoint)

Дата: 2026-07-14. Worktree: `d:/Sources/nv-lang/nova-174`, ветка `fix-coalesce-consume`
(база `f77ac0c96`). Модель: sonnet.

## Диагноз (проверено и уточнено против исходной гипотезы владельца)

Исходная гипотеза владельца указывала на `types/mod.rs:~8558` (Coalesce-арм в
`infer_expr_type` — «типовой канал» для codegen). Проверено: этот арм уже
корректно берёт unwrapped-inner-тип из `a` и игнорирует `b` — там бага нет.

Реальный root-cause — ДРУГАЯ, отдельная (string-only, AST-heuristic) функция
`ConsumeCtx::infer_value_type` (used to fill `var_types` для D133-линейного
чекера — НЕ тот же путь, что codegen-канал). У неё вообще не было веток для
`Try`/`Bang`/`RefArg`/`Coalesce` — RHS через ЛЮБОЙ unwrap-оператор оставлял
`var_types` = `None`. Подтверждено экспериментально: `consume lst =
T.bind(...)!!` (Bang) БЕЗ фикса ТОЖЕ показывал `тип ``` в D133-сообщении —
т.е. баг был общий для `!!`/`?`/`??`, не Coalesce-специфичный.

Почему `!!` «работал» практически (без видимой ошибки) — coincidence: реальное
consume-обязательство X дискредитировалось через ГРУБЫЙ fallback
`is_any_consume_method` (types/mod.rs ~28381: "вызванный метод ЕСТЬ
consume-метод ХоТЬ ДЛЯ КАКОГО-ТО типа" → считать consumed), а не через точный
per-type lookup. Sound (не давал false-negative), но type-blind — отсюда
пустой тип в диагностике и хрупкость (не hardened против случаев, когда точный
тип реально нужен, напр. диагностика/D180 keyword-checks/suggestion-текст).

Отдельно: `method_return_types`/`fn_return_types` (регистрируются в
`ConsumeRegistry::build`/`absorb_external`) при `Result[T,E]`/`Option[T]`
return-типе сохраняли ИМЯ ОБЁРТКИ (`"Result"`/`"Option"`), а не T — потому что
`path.len() == 1` истинно и для внешнего `Named{"Result"}` тоже (generics
хранятся отдельно). Для голого (не-unwrap) call-site это безвредно
(`"Result"` не матчит ни один зарегистрированный consume-тип, эквивалентно
`None`), но не даёт возможности unwrap'нуть при наличии `?`/`!!`/`??`.

## Фикс (compiler-codegen/src/types/mod.rs)

1. Новый helper `unwrap_result_option_name(rt, self_ty)` — если `rt` есть
   `Result[T,E]`/`Option[T]` с single-segment `T` (Self резолвится в `self_ty`),
   вернуть имя T.
2. Два новых companion-поля `ConsumeRegistry`: `unwrapped_method_return_types`
   (`(recv_type, method) -> T`), `unwrapped_fn_return_types` (`fn_name -> T`).
   Заполняются РЯДОМ с существующими `method_return_types`/`fn_return_types`
   (build() И absorb_external()) — старые карты НЕ трогаем (bare call-site
   поведение не меняется).
3. `ConsumeCtx::infer_value_type` получил арм для
   `Try(inner) | Bang(inner) | RefArg(inner)` и отдельно `Coalesce(a, _)`:
   `infer_unwrapped_call_type(inner/a).or_else(|| infer_value_type(inner/a))`.
4. Новый helper `ConsumeCtx::infer_unwrapped_call_type` — резолвит Call-форму
   (`Type.method()` / `recv.method()` / free `fn()`) через `unwrapped_*` карты.

## Тесты

- `spec_tests/conformance/d86_coalesce_consume.nv` — pos: Result+Option
  Ok/Some-ветка потребляется через `.close()`.
- `spec_tests/conformance/neg/d86_coalesce_consume_neg.nv` — neg: не
  потреблённая `??`-consume-var по-прежнему ловится D133 (D133 НЕ ослаблен).
  Verified: сообщение теперь показывает РЕАЛЬНЫЙ тип (`D86NegRes`) и реальный
  метод (`close`) вместо пустого `тип ``.

## Гейт (статус на момент чекпоинта)

- `cargo build` (nova-cli) — OK, без ошибок от фикса.
- per-file `nova check` на pos/neg — OK (см. выше).
- full `spec_tests/conformance --positive --compile-error` — ЗАПУЩЕН в фоне,
  ждём SUMMARY (обновить этот файл при завершении).
- std/src/collections, std/src/net — ещё не запущены (после conformance).
- nova-tls smoke (read-only) — ещё не запущен.

Если сессия оборвалась ДО обновления этого файла — начать с полного
`nova test --positive --compile-error spec_tests/conformance`, затем
std/src/collections + std/src/net, затем commit.
