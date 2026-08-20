---
name: feedback-spec-tests-batch-salvage
description: "spec_tests D-conformance batch-workflow: низкий draft-yield (~4/52), агенты дрифтуют синтаксис; auto-salvage-loop + test-runner truncation-fix; likely_gap≈draft-defect, реальные gaps только из V-трека"
metadata: 
  node_type: memory
  type: feedback
  originSessionId: a48a9f3a-0403-4a44-a6e3-8894781d4b88
---

Закрытие spec_tests-стороны Plan 172 (224 uncovered D) через batch-workflow draft'ов: **draft-yield низкий**
(~4 green / 52) — агенты выдумывают синтаксис / используют retired API (`str.byte_at`/`len`, `[()]` вместо
`[]()`, неверный priv/pattern/array-синтаксис) ВОПРЕКИ инструкции «не выдумывай синтаксис».

**Why:** агентам мало контекста (одна spec-секция + 2 примера недостаточно); folder-module = ОДИН compile-unit
→ один bad draft рушит весь прогон (cascade), нужна изоляция/итеративное удаление.

**How to apply:**
- Process: workflow drafts (Write-forbidden, как data) → extract → **auto-salvage-loop** (компилит folder-module,
  итеративно удаляет parse-падающие drafts до green; см. scratchpad/salvage_loop.sh, docs/plans/172-spec-tests-salvage-state.md).
- **Harness fix (был нужен, СДЕЛАН):** test-runner обрезал FAIL-диагностику → `error.chars().take(400→1500)`
  (detail) + row-printer `take(120→600)` (test_runner.rs:1631/3711/4214). Без этого folder-module
  `import resolution: in entry-folder peer (<path>): <file>:<line>: <inner>` прятала `<inner>` → culprit неидентифицируем.
- **likely_gap-флаги агентов ≈ draft-дефекты, НЕ реальные компилятор-gaps.** Реальные base-gaps берутся из
  V-трека (adversarial-reviewed): d55.1/d156/D53/D277/D55.4.
- Будущие батчи: давать агентам 5+ реальных conformance-примеров + spec-секцию D + явный retired-API список.

**Update 2026-06-29 (две новые попытки, подтверждают низкий ROI):**
- **`agentType:'Explore'` для авторинга = ~0 yield (11+ из 13 битых на parse).** Explore — read/search-агент,
  авторит ОЧЕНЬ плохо (выдуманные методы `Vec.sort_of`, неверный `priv(...)`, `consume` unimpl, `ro` где
  нужен `mut`/`const`, redundant type-prefix, broken array/pattern-синтаксис). НЕ использовать Explore для авторинга.
- **General-агенты (default) авторят лучше (~4/7) НО пишут файлы НА ДИСК напрямую** вопреки schema-return —
  и при этом ДЕСТРУКТИВНО трогают tracked-файлы (в одну сессию мелькали deleted d109/d119/d122 + создавали
  файлы в `spec/tests/` вместо `spec_tests/`, имена с `/`/review-текстом). Если general — то с ЯВНЫМ запретом Write
  И обязательной проверкой `git status` после.
- **Hand-author ПОБЕЖДАЕТ решительно.** Самостоятельно написанные numeric/named-priority locks (d126/d129/d130-inferred)
  прошли с первого раза, высокое качество, залочили реальную базу-работу (STEP 1). Для conformance throughline
  (числовая distinctness, §0) hand-author >> любой workflow-batch по ROI. Workflow держать для DISCOVERY (что
  uncovered), не для AUTHORING.
Связано с [[feedback-large-tests-stored-not-in-regress]], [[feedback_nova_syntax]], [[feedback-nova-tests-not-correctness-gate]].
