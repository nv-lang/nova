# [M-187-http-serde-setcookie-serialize-collision] — verify + cleanup (2026-07-16)

**Задача:** проверить, не закрыт ли баг серде-волной (гипотеза: `98e3663cc`),
и если закрыт — снять ручной JSON-обход в `examples/flagship/aggregator/src/main.nv`.

**Вердикт: НЕ ЗАКРЫТ.** Обход остаётся.

## Что проверено

1. `98e3663cc` (`fix(codegen/187 БАГ 0): [M-serde-encode-pointer-op-regression]`)
   — другой баг: type-qualified `fn_ret_<ресивер>_<метод>` для mono-инстанциаций
   blanket-методов (`fn[T] T @to_str()`), не про `.serialize()`/SetCookie.
   Соседние коммиты серде-волны (`git log --oneline --all | grep -i serde`)
   просмотрены — ни один явно не адресует name-only резолв `.serialize()`
   в generic `json_encode[T]`.

2. Эмпирическая проба: в worktree `nova-serdecheck` временно заменил
   `snapshot_dto_json(dto)` (ручной рендер) на уже существующий typed-путь
   `snapshot_to_json(dto)` (`src/api/report_json.nv`, `json_encode(dto)!!`)
   в `snapshot_body()`, собрал `examples/flagship/aggregator` готовым
   main-бинарём компилятора (`nova-cli/target/release/nova.exe`,
   commit `2381b8ce2`, компилятор НЕ пересобирался).

   Результат — **та же линковочная ошибка**, что задокументирована в маркере:
   ```
   error: compiler error:
   lld-link: error: undefined symbol: Nova_SetCookie_method_serialize
   >>> referenced by ...main.c:27952
   >>>               main-....o:(nova_fn_8encoding5serde11json_encode____NovaValue_SnapshotDto)
   ```

3. Обход main.nv откатан обратно байт-в-байт (`git diff` пуст после отката,
   проверено), маркер в main.nv дополнен секцией «RE-VERIFIED 2026-07-16»
   (эмпирика без изменения сути), финальная сборка с восстановленным обходом
   зелёная, smoke-тест `/api/snapshot` (curl) отдаёт корректный JSON как раньше.

## Правки

- `examples/flagship/aggregator/src/main.nv` — только маркерный комментарий
  дополнен (никакой логики не менял, обход на месте).
- `docs/plans/backlog-followups.md` — строка `[M-187-http-serde-setcookie-serialize-collision]`
  дополнена результатом перепроверки, статус остаётся OPEN/P1.

## Окружение сборки

- Worktree: `d:/Sources/nv-lang/nova-serdecheck` (ветка `fix-serde-setcookie-collision`).
- `nova.local.toml` в `examples/` (gitignored) скопирован из main-репо:
  `[replace] tls={path="../../nova-tls"} http={path="../../nova-http"}`.
- `compiler-codegen/nova_rt/libuv` в worktree был пуст (git submodule не
  инициализирован при `git worktree add`) — скопировано содержимое из
  main-репо (`cp -r`, без `.git`), иначе `nova build` падает с
  `FATAL libuv submodule not initialized`.
- Компилятор НЕ пересобирался — использован готовый
  `d:/Sources/nv-lang/nova/nova-cli/target/release/nova.exe`.
- Порт `AGGREGATOR_PORT=18734` для smoke-теста, сервер убит по PID сразу
  после curl, порт свободен (TIME_WAIT — не LISTEN).

## Дальнейший заход (если кто-то возьмётся чинить сам баг)

Codegen-заход: generic `json_encode[T]`'s dispatch для `@serialize()`
вызова на типе-параметре резолвится по ИМЕНИ метода, не по типу/receiver'у
— родня `[M-187-errorkind-parsejsonerror-variant-collision]` и уже
исправленному `recv_returning` name-only registry в `nova-http/server.nv`.
Нужно: type/arity-scoped резолв `.serialize()` внутри mono-инстанциации
`json_encode[T]` (аналогично тому, как `98e3663cc` сделал для `fn_ret`
blanket-методов, но для serialize-диспетчинга, а не для fn_ret).
