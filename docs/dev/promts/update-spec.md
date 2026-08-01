// SPDX-License-Identifier: MIT OR Apache-2.0
# Промпт: аудит и обновление спецификации `spec/`

## Цель

Привести `spec/decisions/*.md` (D-blocks) и `spec/*.md` (overview /
syntax / types / effects / runtime / tooling reference) в соответствие
с фактической реализацией. Spec — формальный source-of-truth для
языка; должен оставаться **точным** (отражать что компилятор делает
сейчас) и **полным** (новые features из закрытых планов имеют D-block).

## Когда применять

- Юзер говорит "spec устарел" / "проверь спеку".
- Закрыт большой план (5+ commits), который менял language surface
  (новый keyword / DSL / type / namespace builtin).
- Перед release (v0.1, v0.2, …) — final accuracy pass.
- Подозрение на drift — code menyает поведение, никто не trogал spec.

## Входы

- `spec/decisions/*.md` (D1-D119+) — все decision blocks.
- `spec/decisions/README.md` — index с numbering convention.
- `spec/overview.md` — high-level язык overview.
- `spec/<topic>.md` (syntax / types / effects / runtime / tooling) —
  topical reference docs.
- `docs/plans/<N>-*.md` для всех recently-closed планов — что было
  promised / acceptance criteria.
- `docs/project-creation.txt` — running narrative actual closures.
- `compiler-codegen/src/**` — implementation source-of-truth для
  drift detection.
- `std/**` + `nova_tests/**` — usage patterns.

## Выходы

- Updated D-blocks с актуальным behavior, signature, examples.
- New D-blocks для features без spec coverage (нумерация — see step 3).
- Updated `spec/<topic>.md` references к новым features.
- Один или несколько commits `spec(D<NNN>): <what changed>` или
  `spec(<topic>): <what changed>`.

## Шаги

1. **Inventory recently-closed plans:**
   ```
   git log --oneline --grep="ЗАКРЫТ\|closed" -- docs/plans/ | head -30
   ```
   Для каждого извлеки список features (acceptance §5 в plan-doc).

2. **For each feature, locate spec coverage:**
   - `grep -rl "<feature-keyword>" spec/decisions/` — есть ли D-block?
   - `grep -rl "<feature>" spec/*.md` — есть ли в topical doc?
   - Если нет: feature нуждается в новом D-block ИЛИ in-place extension
     существующего D-block (если related).

3. **D-block numbering:**
   - Найди next free D-номер:
     ```
     grep -rh "^## D[0-9]" spec/decisions/ | sed 's/^## D\([0-9]*\).*/\1/' | sort -n | tail -1
     ```
     Pick `<last+1>`. **Verify через `grep -r "## D<N>" spec/`** что номер
     не использован (помни — Plan 60 D117 collision с Plan 33).
   - Если новый D — добавь его в `spec/decisions/<topic>.md`.
   - Update `spec/decisions/README.md` index если есть table.

4. **Update existing D-block** (drift fix):
   - Re-read existing D-block.
   - Compare с current implementation (read source files).
   - Update sections:
     - **Что**: краткое description.
     - **Правило**: точные правила / синтаксис / поведение.
     - **Почему**: rationale (не trogать unless changed).
     - **Что отвергнуто**: alternatives considered.
     - **Связь**: links to related D-blocks / planов.
   - Examples — verify still compile с current compiler.
   - Дата в title (если есть) — update к today.

5. **Topical doc updates:**
   - `spec/overview.md`: high-level feature list — добавить mentioned.
   - `spec/syntax.md` / `spec/types.md` / etc — если synatx изменился.
   - Cross-reference от D-block к topical doc и обратно.

6. **Verification:**
   - Все example code blocks в D-blocks должны type-check и compile
     с current `nova` binary. Запусти minimal sample если есть
     concrete syntax.
   - `nova check` на каждом spec example в isolated dir.
   - `grep -rn "TODO\|TBD\|FIXME" spec/` — отметить если осталось.

7. **Commit policy:**
   - Один commit per logical change (один D-block update OR один topical).
   - Если несколько D-blocks для одного плана — допустим single commit
     `spec(plan-N): closure D-blocks update`.
   - Message format: `spec(D<NNN>): <action>` или `spec(<topic>): <action>`.

## Acceptance

- Для каждого закрытого плана за последний месяц есть либо новый
  D-block либо update существующего.
- Examples в обновлённых D-blocks compile-clean.
- `spec/decisions/README.md` index up-to-date.
- Никаких dangling D-references (link к D<N> где D<N> не существует).
- Cross-references symmetric (spec ↔ docs/plans, spec ↔ code).
- Commits атомарны (один логический change per commit).

## Ограничения

- НЕ переписывай "Почему" / "Что отвергнуто" если rationale не
  изменился — это историческое решение, оно не drift'ит.
- НЕ удаляй D-blocks для отменённых features — пометь
  `**ОТМЕНЕНО <date>**` с reason; оставь record.
- НЕ выдумывай D-номера. Verify через grep before claim "next free".
- НЕ trogать `spec/sketches/` или `spec/research/` (если есть) —
  они experimental, не canonical.
- НЕ ребрендировать D-block (`D89` → `D89.5`) — если нужны под-разделы,
  пиши их sub-headings внутри D-block.
- При major spec changes — отдельный план `Plan <N>-spec-revision-X.md`
  + decision review, не silent edit.

## Пример вызова

> Я заметил, что spec/ сильно устарел — закрыли Plan 57 Phase F+G+H
> и Plan 60, но spec D-blocks не upd'тились. Прогони
> docs/promts/update-spec.md.

Агент должен:
1. `git log --oneline --grep="ЗАКРЫТ" -- docs/plans/` → найдёт
   Plan 57 / 60 закрытия.
2. Для Plan 57: D109 (bench DSL) — добавить bench.metric +
   --histogram + hyperfine + callgrind + per-group geomean.
3. Для Plan 60: D117 (size-accessor uniformity) — verify exists, sync
   с recent updates.
4. Если features wider language reach (e.g. effect system change) —
   topical doc updates.
5. Commits: `spec(D109): bench DSL Phase G/H additions`,
   `spec(D117): size-accessor closure`, etc.

## Связь

- [update-plans-readme.md](update-plans-readme.md) — sister promt для
  README sync. Часто используется вместе после plan closure.
- [read-project.md](read-project.md) — initial spec/plans/code reading.
