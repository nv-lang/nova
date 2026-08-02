# БРИФ p-cu-fixpack (принят интегратором 2026-08-02; исполняется ОДНИМ окном с №274)
# Источник: параллельная сессия. №277 заведён при приёмке; П.2 — № по итогам проверки на дубль (правило окна).
# Поправки интегратора: worktree nova-pcufix от свежего main; фоны ЖДАТЬ СИНХРОННО,
# ход «жду уведомление» запрещён; ≤2 nova-процессов; осиротевшие nova.exe убивать;
# env C-тестов: NOVA_GC_LIB_DIR=<main>/compiler-codegen/vcpkg_installed/x64-windows-static/lib, --toolchain clang.

П.1 — №277 [M-time-vtable-typedef-redefinition-standalone-cu]: standalone-CU вне std с Time в effect-row → C «typedef redefinition NovaVtable_Time». Репро 6 строк (module scratch / fn build() Time -> int => 1 / main Time { ro _ = build() }), nova build напрямую. Корень-контекст: NovaVtable_Time — единственное hand-written исключение (effects.h; D431-амендмент Ф.2-v3, 04-effects.md:7578 — emit_effect_type скипает шаги 1+2 для "Time"); второй путь эмиссии минует скип. Диагноз: оба места эмиссии в сгенерированном .c (grep NovaVtable_Time), фикс в общем скип-канале (НЕ второй point-if по имени в новом месте); шаткость исключения — архитектурная нота, не перепроектирование. Матрица Os/Fs/Net/Io той же формой (4 пробы). Фикстура conformance standalone pos по образцу соседа. Реестры: №277 → ✅ той же веткой.

П.2 — P67 «Path call return type unknown» на Duration.zero() в standalone-CU (ICE emit_c:59434). ПЕРВЫМ проверить на дубль сородичей ([M-os-env-get-raw-path-call-p67-ice], заметка плана 200 П10 overflow_safe/to_nanos). Свой корень → № по правилу №217. Фикс КАНАЛОМ: чекер аннотирует return-type статик-path-вызова (resolved_types), emit читает — образец [M-d216-write-at-return-type-unknown-cc-panic]. НЕ point-fix по имени. Фикстура standalone pos.

Гейты: cargo чистый; обе фикстуры standalone зелёные; nova check std/src FAIL-канон не вырос; ratchet не вырос. Мега-CU/флагман — интегратор. Отчёт рус.: корни (файл:канал), обе эмиссии Time, дубль/№ для П.2, вердикты буквально, модель.
