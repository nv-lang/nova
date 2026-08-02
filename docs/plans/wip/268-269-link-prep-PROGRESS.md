# PROGRESS — окно p-linkprep: №268 [M-tls-vendor-autobuild-not-on-build-path] + №269 [M-gc-lib-not-bundled-clean-install]

worktree: `d:/Sources/nv-lang/nova-plinkprep`, ветка `p-linkprep`, база `main` (30bd4e36e на момент старта).

## Фаза 1 (№268) — РЕАЛИЗОВАНО, гейты 2-5 зелёные, гейт 1 частично (блокирован №269, ожидаемо)

Диагноз (первые чтения, до кода): пред-фикс №152 (c137d2d9b, "vendor-FFI build per-provider")
УЖЕ добавил вызов `build_missing_vendor_ffi_libs` в `cmd_build` (nova-cli/src/main.rs) как побочный
эффект (M-nova-build-vendor-ffi-no-autobuild, 2026-07-15, отдельный More ранний маркер, тоже уже
закрыт). Эмпирическая проверка (throwaway-пакет с `tls` git-депом на чистом чекауте, native/lib
удалён) подтвердила: авто-сборка vendor mbedTLS на `nova build` **уже работает** до моей правки.
Реальный оставшийся разрыв — ТОЛЬКО диагностика: `cmd_build` не звал `first_missing_ffi_lib` после
попытки авто-сборки, поэтому genuine-failure (сборка не удалась / vendor_src_dirs нет) падал в
сырую ошибку линкера (`lld-link: could not open 'mbedtls.lib'`) без указания пакета/причины.

Сделано:
- Новый модуль `compiler-codegen/src/link_prep.rs` — вынесены `ffi_lib_candidate_names`,
  `first_missing_ffi_lib`, `VENDOR_FFI_BUILD_LOCK`, `build_missing_vendor_ffi_libs`,
  `build_vendor_ffi_lib` из `test_runner.rs` (были уже `pub`, звались из обоих путей через
  `test_runner::`, теперь — общий модуль `link_prep::`, `test_runner.rs` ре-экспортирует
  `build_missing_vendor_ffi_libs` через `pub use` для обратной совместимости остальных call sites
  внутри файла). `strip_verbatim_prefix`/`collect_c_files` остались в `test_runner.rs`
  (`pub(crate)`), `link_prep.rs` их импортирует — слишком много других мест в test_runner.rs их
  использует, риск непропорционален пользе переноса.
- НОВОЕ: `link_prep::diagnose_missing_vendor_ffi(&[(String, ResolvedFfiConfig)]) -> Option<String>`
  — громкая диагностика для build-пути: после auto-build-попытки проверяет каждого провайдера
  (ИМЕНОВАННОГО, ещё не смёрженного — чтобы называть конкретный пакет), формирует
  `nova: FATAL missing native [ffi] library ...` с именем пакета/либы/searched-путей/подсказкой
  (vendor_src_dirs есть → «сборка была, не удалась, см. warning выше»; нет →
  «нужен prebuilt drop-in вручную»). `None`, если всё на месте — `nova build` идёт дальше как
  раньше, поведение НЕ меняется для success-пути (голая проверка `first_missing_ffi_lib`, без
  побочных эффектов).
- `nova-cli/src/main.rs::cmd_build`: `all_ffi` теперь `Vec<(String, ResolvedFfiConfig)>` (имя
  пакета из `manifest.package_name`/`dep_manifest.package_name`), после build-loop зовёт
  `link_prep::diagnose_missing_vendor_ffi` — если `Some(msg)`, печатает и `std::process::exit(1)`
  ДО реального линк-шага (никогда не доходит до сырой ошибки линкера в genuine-failure случае).
- `test_runner.rs::run_one` (nova test путь) — **НЕ тронут по семантике**: тот же
  `first_missing_ffi_lib` на merged-config → `SkipReason::FfiLibNotFound` (detect-and-degrade
  SKIP), никакого exit/FATAL там нет — по ТЗ брифа.
- `lib.rs`: `pub mod link_prep;` добавлен (между `lexer` и `lints`).

Диагноз Docker-вопроса (§ открытого вопроса (а) из бэклога): не установлен окончательно в этом
окне — не нашёл прямых следов отдельного Linux-механизма; наиболее вероятная гипотеза: Docker-путь
2026-07-20 либо собирался ДО того, как #268 стало заметно (репро специфично для git-чекаута с
изначально пустым native/lib — Docker-сборка могла унаследовать уже собранный кэш из промежуточного
слоя, или использовать `nova test` вместо `nova build` на каком-то шаге). Не подтверждено фактом —
гипотеза, не находка.

### Эмпирическая верификация (throwaway-пакеты, scratchpad, вне репы)
- `pkg268` (git dep `tls` @ v0.1.2, свежий чекаут, native/lib пуст) → `nova build`:
  автосборка сработала («building from vendored source (108 files, one-time)...» → «built»),
  повторный build = cache-hit (без сообщения о пересборке). Полный build+link+run с
  `NOVA_GC_LIB_DIR` на главную репу — «Done.», код работает.
- `pkg268c` (git dep `tls` @ ТОЧНО тот коммит из `examples/nova.lock.toml`,
  `910e14be86c3690f4b5ddd1d30d365437336f910`) — гейт 2 буквально: `.bak`-копия native/lib →
  очистка → 1-й build 33.75s (авто-сборка) → 2-й build 13.15s, БЕЗ сообщения о пересборке
  (cache-hit подтверждён) → native/lib восстановлен из `.bak` (repo НЕ испорчен).
- `pkg268b` (заведомо отсутствующая `totallyfakelib`, без vendor_src_dirs) → `nova build`
  завершается `exit 1` с громким `nova: FATAL missing native [ffi] library ...` (имя пакета,
  имя либы, searched-пути, подсказка) — ДО этой правки такой сценарий падал бы в сырую
  `lld-link: could not open` ошибку без контекста.

### Гейты (вердикты дословно)
1. **ГЛАВНЫЙ (чистый клон, /install/, без env/vcpkg):** блокирован №269 (GC не забандлен) —
   FATAL Boehm GC ожидаемо после того, как libuv+vendor-tls уже автособрались успешно (это
   доказывает #268 закрыт независимо от #269). Полный клон-тест — см. отдельный прогон
   (в процессе на момент записи чекпоинта).
2. **Репро №268:** см. «Эмпирическая верификация» выше (`pkg268c`, дословно на пиновом коммите
   lock-файла) — ДО фикса (по коду/истории, M-nova-build-vendor-ffi-no-autobuild) сырая ошибка
   линкера; ПОСЛЕ — самосборка (33.75s) + cache-hit (13.15s, без пересборки).
3. **Основная репа (vcpkg_installed есть):** не гонял `nova build` в самой ГЛАВНОЙ репе (там
   свой закоммиченный бинарь, не мой), но логически `diagnose_missing_vendor_ffi` — чистая
   read-only проверка ПОСЛЕ auto-build, `None` при все-на-месте → поведение success-пути
   НЕ меняется по построению. `pkg268`/`pkg268c` full-build с `NOVA_GC_LIB_DIR` на главную репу
   прошли (см. выше).
4. **nova test путь:** `nova-polaris master ./nova.sh test src --strict-effects` (через
   свежесобранный `nova-plinkprep`-бинарь, env главной репы) = **PASS: 37 FAIL: 0 SKIP: 19** —
   БАЙТ-В-БАЙТ канон брифа, регрессий нет (включая brotli-провайдер polaris'а — та самая
   #152-цепочка провайдеров).
5. **cargo build --release чисто** (exit 0, только pre-existing dead-code warnings, НЕ мои).
   **nova check std/src** = **PASS: 147 FAIL: 26 WARN: 60** — байт-в-байт канон.

## Фаза 2 (№269) — НЕ НАЧАТА в этом чекпоинте

Следующий шаг (если продолжает то же окно): bdwgc git-сабмодул рядом с
`compiler-codegen/nova_rt/libuv`, тег `v8.2.8` (совпадает с версией, которую тянет vcpkg —
подтверждено локальным vcpkg-кэшем `ivmai-bdwgc-v8.2.8.tar.gz` на этом хосте), pin коммитом.
Сложность выше libuv/mbedtls: bdwgc официально собирается через cmake (генерирует
config-заголовки), не голым `cl.exe`/`cc` по списку `.c` — нужно либо звать cmake из драйвера
(новая зависимость на PATH), либо вручную повторить минимальный флаг-сет (threads support,
`GC_THREADS`, `PARALLEL_MARK` и т.п.) под `/MT` статикой, сверяя с тем, что даёт vcpkg
x64-windows-static triplet (portfile — `vcpkg/registries/git-trees/.../bdwgc.json` на этом хосте,
не читан подробно в этом чекпоинте). Честный стоп после Фазы 1 — приемлемый исход по брифу;
если не продолжаю сам — следующему окну нужно: (1) прочитать bdwgc portfile для точного флаг-сета
vcpkg, (2) решить cmake-vs-ручной-флаг-сет, (3) добавить `detect_or_build_boehm_gc` (по образцу
`detect_or_build_libuv`) как 4-й tier в `resolve_gc_or_exit` (после `$NOVA_GC_LIB_DIR` →
local vcpkg → `$VCPKG_ROOT` → NEW: submodule fallback, вместо FATAL), (4) заголовки gc.h из
сабмодуля в fallback-ветке, (5) доки (read-project.md строка про NOVA_GC_LIB_DIR, AGENTS.md).

## Правила соблюдены
- `git config` не трогал. Все изменения — по конкретным файлам (`test_runner.rs`, `main.rs`,
  `lib.rs`, новый `link_prep.rs`).
- Throwaway-тесты — в scratchpad (вне репы), не засоряют nova_tests/examples.
- Чистый клон — `d:/Sources/nv-lang/tmp-cleanclone` (отдельно от рабочих реп).
- ≤2 nova-процессов моих (проверено через `Get-Process nova` — сторонние процессы других
  worktree/сессий не трогал).
