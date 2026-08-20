---
name: feedback-maximize-nv-sourcing
description: "компилятор Nova должен по максимуму брать данные типов/функций из .nv; в Rust остаются только непортируемые extern \"nova\" fn"
metadata: 
  node_type: memory
  type: feedback
  originSessionId: b6f9282a-da61-4413-ac88-f82ea6a9f472
---

Сквозная директива владельца (2026-06-20): **компилятор должен по максимуму брать данные о типах/функциях/методах из кода на `.nv`, а не из хардкода в Rust.** В Rust-компиляторе в идеале остаются ТОЛЬКО непортируемые `extern "nova" fn` (C-трамплины/рантайм, у которых нет `.nv`-тела). Прецедент: `Vec` уже перенесён в `.nv` (методы/layout из `vec_seq.nv`, не хардкод).

Ещё «зашито» и подлежит переносу в `.nv` (prelude `std/prelude/core.nv`, `errors.nv`): схемы вариантов `str`/`Option`/`Result`/`RuntimeError` и др. Конкретный пример техдолга — `init_hardcoded_baseline` в `compiler-codegen/src/codegen/sum_schema_registry.rs`: захардкоженные `variants` Option `{Some/None}` / Result `{Ok:nova_int, Err:nova_str}` / RuntimeError → должны браться из `.nv`-деклараций. Маркер `[M-172.1-U6-sumschema-baseline-nv]`, финальная чистка гейтнута на 172.1 U.4/U.5 (typed IR).

**Исключение (НЕ переносить):** `method_routing` в baseline (имена C-трамплинов `Nova_Option_method_*`, `is_per_t`, `<inline>`) — это легитимный реестр C-реализации рантайма, у него нет и не может быть `.nv`-аналога (simplifications.md:10737). То же про `runtime_registry.rs`/`external_registry.rs`.

**Why:** §3 compiler-conventions (никакого хардкода типов/функций/методов Nova; общий механизм для builtin и user-кода; stdlib — просто пакет по search-path). Хардкод форкает/ломает user-код и дрейфует от реальности.

**How to apply:** при любом рефакторинге, трогающем типы/схемы/сигнатуры, проверять — можно ли источник данных взять из `.nv` вместо Rust-хардкода; если да и достижимо безопасно — делать заодно. При оценке миграции «читать из registry/реестра» сначала убедиться, что сам реестр уже `.nv`-источник, а не хардкод (иначе перенос чтения УГЛУБЛЯЕТ хардкод — анти-§3). См. [[feedback_nova_syntax]], [[feedback-no-external-memory-for-project-state]].
