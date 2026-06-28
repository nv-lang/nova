# spec_tests salvage-state — batch-workflow w8w3huvrz (2026-06-29)

> spec_tests-сторона двусторонней закрытия 172 (covered 62→66 D). Этот файл = состояние salvage
> batch-workflow'а draft'ов uncovered-D + путь для свежей сессии. Источник draft'ов: workflow
> `spec-d-conformance-batch` (script сохранён, re-runnable; output `w8w3huvrz.output`; копии в scratchpad/vstage2).

## Результат
- **52 draft'а** для uncovered D (диапазоны D1–D328, 8 агентов; skipped syntax-only/gated/covered).
- **Смержено (committed b0e82d55):** D19 match-arms, D22 closures, D23 return-early, D25 throw/Fail — 4 green
  в folder-module (PASS:1 FAIL:0 с полным conformance incl d39). covered 62→66.
- **Отложено 48.**

## Почему отложено 48 (диагноз)
1. **folder-module = ОДИН compile-unit** → один bad draft рушит весь прогон (cascade). Нельзя смержить
   green-подмножество без изоляции каждого.
2. **nova обрезает peer-parse-ошибку** (imports.rs:761 формат `in entry-folder peer '{mod}' ({path}):
   {file}:{line}: {inner}` — длинный {path} обрезается в выводе test-runner'а → `{inner}` не виден,
   name-based ID culprit'а невозможен). Это **DX-баг nova** (не сам Plan 172, но блокирует salvage).
3. **Изоляция draft'а не работает:** solo-файл вне dir → `E_D78_MODULE_PATH_MISMATCH` (module-декларация
   `spec_tests.conformance` обязана совпадать с dir-путём); solo вне репо → prelude не резолвится.

## Природа отложенных (смесь, НЕ только gaps)
- **12 agent-flagged likely-gaps:** D4, D20, D34, D38, D85, D86, D88, D90, D102, D108, D240, D255.
- **draft-дефекты (invented/unsupported syntax):** напр. D20 использовал `[()]` (array-of-unit) → parse-error
  `expected ], got (` — корректный синтаксис `[]()`, т.е. draft-БАГ, НЕ gap. d181/d255 имели `import`
  (folder-module peers не импортируют). Систематический "import resolution" для D>=100 advanced-draft'ов
  (d117/d132/d141/d143/d168/d171/d177/d178/d179/d199 + d2XX) = в основном parse-ошибки.
- Чтобы классифицировать (реальный gap vs draft-ошибка) — нужно прочитать каждый + сверить синтаксис со спекой.

## Путь для свежей сессии (proper harness)
1. **Починить salvage-harness** одним из:
   (a) исправить nova: не обрезать peer-parse-ошибку (DX-win, отдельный мелкий fix в test-runner display);
   (b) per-draft изоляция: временно ВЫНЕСТИ существующие conformance-файлы → тестить каждый draft ОДИН в
       conformance (module-path совпадает, prelude резолвится) → restore. Медленно но robust;
   (c) проверить, даёт ли `nova check <single-file>` полную ошибку (check прошёл все 52 — type-check shallow,
       не ловит parse в peer-collection? — перепроверить).
2. Прогнать каждый из 48 → точная ошибка → классифицировать:
   - **draft-ошибка** (invented syntax): починить синтаксис по спеке → смержить (это coverage, не gap).
   - **реальный gap** (spec-valid синтаксис, nova не парсит/codegen падает): → base-fix (двусторонняя
     конвергенция, named-priority «всё к спеке/D»).
3. Параллельно: следующий batch-workflow для ещё-uncovered D (остаётся ~224 после этого батча).

## Regeneration draft'ов
Workflow re-runnable: `Workflow({scriptPath: ".../spec-d-conformance-batch-wf_a2ca0678-be6.js"})`.
Извлечённые .nv: scratchpad/vstage2 (session-local). Output JSON: tasks/w8w3huvrz.output (drafts[].nv_content, html-escaped).

## ОБНОВЛЕНИЕ (2026-06-29, конец сессии) — harness FIXED + loop-результат
Salvage-harness РАЗБЛОКИРОВАН: 2 §1-фикса truncation (test-runner detail 400→1500 `3834222f` + row-printer
120→600 `aad39dfd`) → полная FAIL-диагностика видна (folder-module `import resolution: in entry-folder peer
(<path>): <file>:<line>: <inner>` больше не прячет `<inner>`). Auto-salvage-loop (scratchpad/salvage_loop.sh):
итеративно компилит folder-module, удаляет parse-падающие drafts до green.

**Результат: 12 удалено = ВСЕ draft-дефекты (НЕ компилятор-gaps):**
- parse-ошибки: d199 (`expected ], got identifier`), d27/d37/d38 (top-level/array syntax), d34 (pattern), d225 (priv-qualifier), d20 (`[()]` — корректно `[]()`)
- компилятор-ENFORCED правила: d35/d88 (`redundant type prefix on record literal`), d45 (`=> record-literal body needs return type`)
- retired API: d102/d108 (`str.byte_at()` retired D249)

**4 green смержено** (`b0e82d55`: D19/D22/D23/D25). **Вывод: likely_gap-флаги агентов ≈ draft-дефекты, НЕ реальные gaps.**
**d109-«поломка» в loop = misattributed cascade** (remaining draft с byte_at → ошибка folder-module смаплена на d109);
clean conformance (68) = GREEN, d109 в порядке.

**УРОК (durable):** batch-workflow draft-yield низкий (4 green / 52) — агенты выдумывают синтаксис / используют
retired API вопреки инструкции. Будущие батчи: (а) давать агентам БОЛЬШЕ примеров (5+ реальных conformance +
spec-секция D) + явный список retired-API (`byte_at`/`len`/`@slice`/raw-ptr-ops); (б) auto-salvage-loop теперь
работает — гонять на каждый батч.

**Реальные base-gaps для двусторонней конвергенции — НЕ из этого батча, а из V-трека** (adversarial-reviewed):
d55.1 sum-coercion, d156 consume-checker generic-return subst, D53 anon-protocol-param, D277 generic-value-record
mono, D55.4 field-range-check. См. 172-d-conformance-checklist.md.
