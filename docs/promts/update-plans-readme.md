// SPDX-License-Identifier: MIT OR Apache-2.0
# Промпт: обновить `docs/plans/README.md` после закрытия плана/фазы

## Цель

Синхронизировать сводную таблицу всех планов
`docs/plans/README.md` с фактическим состоянием закрытого плана
(или закрытой фазы существующего плана). README — единый source of
truth о статусе всех планов; должен оставаться актуальным после
каждого significant commit, чтобы будущие сессии видели правильное
состояние без чтения отдельных plan-docs.

## Когда применять

- Закрыт новый план (commit `docs(N): closure ...`) → добавить или
  пометить ✅ строку.
- Закрыта новая фаза существующего плана (Ф.X / Phase Y) → обновить
  ту же строку: статус, краткое описание phase, count тестов, обновить
  commit count.
- Открыт новый план → добавить новую строку в правильное место (по
  номеру).
- Переоценили priority (P0/P1/P2) или статус → обновить.

## Входы

- `docs/plans/README.md` — текущая таблица.
- `docs/plans/<N>-*.md` — закрытый/обновлённый plan-doc (источник истины).
- `git log --oneline` для подсчёта commits в worktree (если worktree
  есть) или relevant range на main.
- `docs/project-creation.txt` — для сверки итоговых test counts.

## Выходы

- Обновлённая строка(и) в таблице `## Текущие планы` `docs/plans/README.md`.
- Один commit `docs(plans-readme): закрытие Plan N Phase X` (или
  similar) без других изменений.

## Шаги

1. **Прочитать plan-doc** (`docs/plans/<N>-*.md`) полностью, найти
   `## 5. Acceptance criteria` (или эквивалентную секцию closure'а).
   Извлечь:
   - Список phases / sub-items с `[x]` маркером.
   - Финальные test counts (unit / .nv / e2e).
   - SHA-references на ключевые commits (если есть в `## Эволюция`).
   - "Что НЕ делает" / "deferred" — если что-то осталось.

2. **Найти строку плана в README** через `grep -n "^| <N> \|^| <N>\\."
   docs/plans/README.md`. Если строки нет — добавить (новый план),
   соблюдая числовой порядок.

3. **Обновить содержимое cell'ов**:
   - **Описание (`О чём`)**: краткое summary всех закрытых phases —
     по одному предложению per phase. Group phases logically (MVP /
     A-D core / E-F production / G-H polish). Перечисли key features
     каждой phase, BUT keep cell под 1500 chars (markdown table ужёт).
   - **Статус**: формат `✅ <Phase1>+<Phase2>+...+<PhaseN> ЗАКРЫТЫ
     YYYY-MM-DD (worktree <wt>, N+ commits)` если closed; `🟡 partial`
     с rationale если частично; `план, не начат` если untouched.

4. **Согласовать с другими docs**:
   - Если в строке `См. <doc>` ссылается на conventions / spec
     references — verify пути existing.
   - Не дублируй detail из самого plan-doc — README cell ≈ "TL;DR";
     детали остаются в plan-doc.

5. **Verify**: open `docs/plans/README.md` в browser/preview, убедись
   что table renders correctly (markdown pipe escaping OK, no broken
   links).

6. **Commit**:
   ```
   git add docs/plans/README.md
   git commit -m "docs(plans-readme): закрытие Plan <N> <Phase>"
   ```
   Никаких других файлов в этот commit.

## Acceptance

- `grep -n "^| <N>" docs/plans/README.md` показывает обновлённую
  строку с правильным статусом.
- Markdown table renders без broken pipes.
- В описании упомянуты ВСЕ закрытые phases (если commit закрывает
  фазу — она должна появиться).
- Test count в описании ≥ фактическому (без округления вверх,
  но допустимо округление вниз если cell слишком длинный).
- В status указана дата + worktree + примерный commit count.
- Commit — atomic (только README), не смешан с другими изменениями.

## Ограничения

- НЕ обновляй plan-doc сам в этом commit'е — это отдельная задача.
- НЕ дублируй full acceptance list в README cell — это для plan-doc.
- НЕ переписывай статус "план, не начат" → "✅" если plan-doc
  фактически не closed (verify через `## 5. Acceptance criteria`).
- НЕ trogай rows других planов без явной просьбы.
- НЕ меняй table format (column order, header style) — backwards-
  compatible только.

## Пример вызова

> Закрыли Plan 57 Phase H (cross-platform additions — H.1 multi-group
> geomean, H.2 hyperfine, H.3 valgrind callgrind). Обнови
> `docs/plans/README.md` запись Plan 57 — добавь Phase H в описание
> и обнови статус. Используй промпт docs/promts/update-plans-readme.md.

Агент должен:
1. Прочитать `docs/plans/57-perf-benchmark-infrastructure.md` §5
   Acceptance + §Эволюция.
2. Найти `^| 57 ` в README.
3. Обновить description (добавить H.1/H.2/H.3 phrase + features) +
   status (`MVP+A+B+C+D+E+F+G+H ЗАКРЫТЫ`) + commit count.
4. Commit `docs(plans-readme): закрытие Plan 57 Phase H`.
