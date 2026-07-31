# PROGRESS — box/ptr capture P1 pair (A/B), окно 2026-07-31

Модель: sonnet.

## Итог (главное)

Оба дефекта (`[M-detach-box-capture-cancel-token-reachable]`,
`[M-effect-handler-mutex-hashmap-value-capture]`) НЕ ВОСПРОИЗВОДЯТСЯ на
текущем main tip (2e45ab920), несмотря на исчерпывающую добросовестную
реконструкцию обеих описанных форм для КАЖДОГО дефекта (детали — в финальном
отчёте сессии). compiler-codegen/src/codegen/emit_c.rs НЕ ТРОГАЛСЯ (git diff
пуст, wc -l = 63852 = baseline, δ0).

## Сделано

- Ф.0 (A): 7 изолированных фикстур + 2 прогона в РЕАЛЬНОМ CU nova-polaris
  (verbatim-копия src/, патч применён точь-в-точь по описанию serve.nv,
  ОБЕ формы триггера — bare-param и struct-field) — все компилируются и
  проходят.
- Ф.0 (B): 2 варианта капчура (ro-параметр, mut-локаль внутри escaping-
  фабрики) — оба компилируются и проходят, сгенерированный C проверен
  напрямую (никакого `NovaValue_T`/`NovaValue_T*` рассогласования).
- Ф.1: НЕ ТРЕБУЕТСЯ (дефект не наблюдается — нечего чинить).
- Ф.2: добавлены 2 pos-фикстуры (регресс-гварды) —
  `spec_tests/conformance/m_boxcap_detach_cancel_token_pos.nv`,
  `spec_tests/conformance/m_boxcap_effect_handler_mutex_hashmap_pos.nv`.
  Верифицированы standalone-двойниками (СВОЙ CU, не мега-CU) — PASS оба.
- Гейты: `cargo build --release` чисто; `nova check std/src` FAIL: 26
  (канон); флагман `--strict-effects` built; marker-registry-sync зелёный.

## Незакрытое / для интегратора

- Маркеры A/B НЕ сняты (не моё решение) — интегратору проверить свежей
  сборкой nova.exe (текущий main) на РЕАЛЬНОМ nova-polaris перед
  повторной попыткой graceful shutdown (222.22) / Metrics-эффекта (222.23).
- `_repro/` — рабочий мусор (не коммитить, не под git — не в индексе).
