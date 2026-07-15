# Прогресс: [M-tls-xpkg-tlsversion-value-ptr-dispatch] — ✅ ЗАКРЫТО (2026-07-15)

Ветка: `fix-tlsversion-dispatch` (worktree `d:/Sources/nv-lang/nova-tlsver`, от main `ae9dc90a3`).
Модель: opus. В main НЕ мёржено (гейт+merge — оркестратор).

## Задача
Cross-package sum-type метод `TlsVersion.@to_str()` (объявлен nova-tls `src/config.nv:41`),
вызванный из потребителя `examples/tls/echo_client.nv:56` (`version.to_str()` после
`stream.protocol_version() ?? TlsVersion.Tls13`), давал value/pointer ABI-mismatch в
сгенерированном C.

## Рекон
- **ДО:** `nova build examples/tls/echo_client.nv` → CC-FAIL:
  `unknown type name 'Nova_TlsVersion_p'`; метод-def
  `Nova_Nova_TlsVersion_p_method_to_str(Nova_TlsVersion_p* nova_self)`;
  `??`-локал `Nova_TlsVersion_p version = …`.
- **Ловушка окружения:** main-репа (`d:/Sources/nv-lang/nova`) checked out на
  `integ-206-v3` с dirty std (Plan 206 mixed `Ints` type-set → `E_TYPE_SET_MIXED_SIGNEDNESS`).
  Сборка ИЗ cwd main-репы читала этот битый std → маскировала настоящую ошибку.
  Решение: собирать ИЗ корня worktree (чистый std, коммит main).
- **Локальный контраст:** `type Ver enum V12|V13` + `@name` + `Option[Ver] ?? Ver.V13`
  СОБИРАЕТСЯ. Значит баг cross-package-специфичен (родня D39).
- **Корень:** `emit_c.rs` legacy `ExprKind::Coalesce` в `infer_expr_c_type` (~54063):
  стрипил `NovaOpt_` и возвращал payload-ident `Nova_TlsVersion_p` вербатим, не
  разворачивая sanitized-pointer-маркер `_p`. Coalesce-ЭМИССИЯ (~30988) уже звала
  `desanitize_c_from_ident` — рассинхрон. Битый C-тип `??`-локала отравлял вниз
  метод-диспатч-мэнглинг (on-demand эмиссия метода). Локальный кейс шёл через
  Channel-2 (чекерский resolved-type → чистый `Nova_Ver*`), cross-package — в legacy.

## Фикс
`compiler-codegen/src/codegen/emit_c.rs` ~54066: `Self::desanitize_c_from_ident(sani)`
вместо `.to_string()`. Идемпотентно для value-payload → byte-identical; разворачивает
только heap-pointer payload.

## Верификация (точечная — мега-CU за оркестратором)
- `echo_client.nv`: C-error → **linked** ✅
- Сген. C: `Nova_TlsVersion_method_to_str(Nova_TlsVersion* nova_self)`,
  `Nova_TlsVersion* version = (… ? _tmp.value : …)`, `Nova_TlsVersion_method_to_str(version)`;
  `_p` только в легитимном мэнгле `NovaOpt_Nova_TlsVersion_p`. ✅
- `echo_server.nv`: linked, не регрессировал ✅
- Локальный `Option[enum] ?? default` + метод: собирается ✅

## Наблюдение (вне периметра, не воспроизведено)
Соседняя `Try/Bang`-ветка (`infer_expr_c_type` ~54152) несёт тот же нераскрытый `_p`
для cross-package `Option[Sum]?`/`!!`. Другой символ/путь; echo_client использует `??`.

## Прочее
Runtime/codegen-фикс, не язык-меняющий → D-амендмент не нужен.
