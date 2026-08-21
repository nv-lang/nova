---
name: reference-three-codegen-drivers
description: "У кодогена ТРИ драйвера (test_runner.rs, nova-cli cmd_build, compiler-codegen/main.rs) — чекер-канал 196 обязан проводиться во всех трёх; страж check-driver-channel-parity держит это машиной"
metadata: 
  node_type: memory
  type: reference
  originSessionId: a48a9f3a-0403-4a44-a6e3-8894781d4b88
  modified: 2026-08-15T14:21:55.611Z
---

**Три драйвера кодогена, каждый сам кормит эмиттер чекер-каналами** (`emitter.set_<channel>(&env.<field>)`):
1. `compiler-codegen/src/test_runner.rs` (~4615) — `nova test`, эталон;
2. `nova-cli/src/main.rs` `cmd_build` (~5385) — `nova build`, путь пользователя и флагманов;
3. `compiler-codegen/src/main.rs` (~552) — standalone `nova-codegen`.

**Почему это важно:** класс «канал проведён в test, забыт в build» повторился ТРИЖДЫ
(Ф.4c: resolved_types/callees; №669 2026-08-15: pattern_variant_types №279,
resolved_variant_ctors №658, node_substs 196.5). Симптом всегда один: conformance
(через `nova test`) зелёный, а `nova build` пользователя падает by-name/легаси-фолбэком.
Std-обходы маскируют дыру годами.

**Как применять:** новый канал 196 = правка ВСЕХ ТРЁХ драйверов одной волной; после —
`bash scripts/guards/check-driver-channel-parity.sh .` (в гейте с №669). Диагностика
канала — env-gated `NOVA_DIAG_658`-стиль (fill/read печать) — за минуту показывает,
в каком драйвере канал пуст. Сентинел BUILD-пути — пример в anti-rot списке
`docs/plans/wip/197-f5-gate-list.txt` (гейт и CI собирают `nova build`).

Смежное: [[feedback-compiler-fixes-checker-channel-196]], [[project-include-str-touch-trap]].
