<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Checkpoint — `nova build` vendor-FFI автосборка (`[M-nova-build-vendor-ffi-no-autobuild]`)

**Ветка:** `fix-build-vendor-ffi` (worktree `d:/Sources/nv-lang/nova-buildffi`,
от `main` HEAD 73ab1c44f + смёржена `fix-consume-cleanup` 70c4eff02).
**Модель:** sonnet. **Дата:** 2026-07-15. **Статус:** ГОТОВО (фикс + верификация),
финальный LINK echo_server/echo_client блокирован ОТДЕЛЬНЫМ баг-диспатчем (новый маркер).

## Корень

`nova test` (`compiler-codegen/src/test_runner.rs::build_and_run_one`, ~строка 3275)
перед линковкой звал `build_missing_vendor_ffi_libs(ffi, vcvars)` — build-and-cache
любого `[ffi] vendor_src_dirs`-native-депа (mbedTLS в nova-tls) ИЗ ИСХОДНИКОВ.
`nova build` (`nova-cli/src/main.rs::cmd_build`) этот шаг НИКОГДА не звал — только
мёржил `[ffi] libs`/`lib_dirs` своего пакета + зависимостей и шёл прямо на линк.
→ на чистом чекауте (без вручную-скопированных `.lib`) `nova build` примера с
vendor-source-депом не собирался; обход = ручная копия прекомпилированных либ.

## Правки (минимальные, без дублирования)

1. **`compiler-codegen/src/test_runner.rs`**
   - `build_missing_vendor_ffi_libs` — `fn` → **`pub fn`** (была module-private;
     общий хелпер, НЕ вынесен в новый модуль — просто открыт для nova-cli;
     `ResolvedFfiConfig` уже был `pub` и виден из nova-cli).
   - `build_vendor_ffi_lib` (BOM-фикс, вскрыт при верификации): cl.exe и lib.exe
     `.rsp`-файлы теперь пишутся с `\u{FEFF}` (UTF-8 BOM) — иначе на профиле с
     не-ASCII (кириллица) в пути к git-кэшу cl.exe читает rsp в ANSI-cp процесса
     и коверкает путь → ложный `C1083: file not found` на каждом `.c` mbedTLS.
2. **`nova-cli/src/main.rs::cmd_build`** — после мёржа `[ffi]` своего пакета +
   зависимостей, ДО `BuildOpts`/`compile_c_to_exe`, добавлен вызов
   `test_runner::build_missing_vendor_ffi_libs(ffi, tc.vcvars_path())` —
   зеркалит call-site `build_and_run_one`. No-op/never-fatal контракт не тронут.

Не язык-меняющий → D-амендмент не нужен.

## Верификация (репро обхода → фикс)

Убраны вручную-скопированные `mbedcrypto/mbedtls/mbedx509.lib` из ОБОИХ
резолвленных git-чекаутов `~/.nova/git/co/nova-tls-*/…/native/lib` (backup в
scratchpad; не восстановлены — автосборка теперь единственный путь).

- **echo_server.nv** — ДО (vendor-ffi-патч временно откачен): падает БЕЗ единого
  упоминания vendor-FFI, сразу на compile-стадии. ПОСЛЕ (с фиксом):
  `nova: FFI lib(s) ["mbedtls", "mbedx509", "mbedcrypto"] not found in …native/lib,`
  `building from vendored source (108 files, one-time)...` →
  `nova: vendor FFI lib(s) ["mbedtls", "mbedx509", "mbedcrypto"] built (108 files)`
  → три `.lib` реально на диске (подтверждено `ls`, ~3.4МБ каждая).
- **echo_client.nv** — так же автособирает vendor-FFI.
- **aggregator (`examples/flagship/aggregator/src/main.nv`)** — чистый build+LINK:
  `built: D:\Sources\nv-lang\nova\main.exe (46.12s)`, регрессии нет
  (у aggregator нет реальной TLS-терминации — `http.server` без TLS).
- **nova test std/src/net** — `PASS: 1  FAIL: 0` (та же vendor-FFI-логика в test
  не регрессировала).

## Оставшийся ОТДЕЛЬНЫЙ блокер (НЕ vendor-ffi)

Полный LINK echo_server/echo_client НЕ достигнут — валится на C-compile стадии
ПОСЛЕ успешной автосборки mbedTLS, из-за пред-существующего cross-package
method/sum-type dispatch бага (подтверждён на исходном pre-session бинаре —
идентичная ошибка ДО начала волны, и с `NOVA_CACHE=0`):
- `msg_bytes->decode_utf8() /*?? unsupported */` → `no member named 'decode_utf8'`
  (nova-tls `src/stream.nv:57`, cross-package extension-метод на `[]u8`).
- `unknown type name 'Nova_TlsVersion_p'` + value/pointer mismatch (echo_client,
  cross-package sum-type `TlsVersion.@to_str()`).
Новый маркер: `[M-tls-xpkg-decode_utf8-tlsversion-dispatch-broken]` P1
(backlog-followups.md). Чинить отдельным заходом compiler-codegen/checker.

## Коммит

git add по именам: `compiler-codegen/src/test_runner.rs`, `nova-cli/src/main.rs`,
`docs/plans/backlog-followups.md`, `docs/simplifications.md`,
`docs/plans/build-vendor-ffi-progress.md`. nova.lock/артефакты НЕ коммитить.
В main НЕ мёржить — гейт+merge оркестратор.
