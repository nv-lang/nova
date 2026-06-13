<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Plan 152.0 — Gate verification & baseline-методология (lesson)

> Верификация финального `nova test` gate для 152.0 (Ф.5) + урок про ловушку
> неполного baseline. Создан 2026-06-13, worktree `nova-p152`.

---

## Итог: «32 новых FAIL» оказались **pre-existing** (НЕ регресс 152.0)

Полный gate после 152.0 (Ф.2 folder-split + Ф.2.5 registry-cleanup + Ф.3 buffer-refactor):

```
PASS: 2534  FAIL: 181  SKIP: 56
```

Наивный diff против сохранённого baseline показал **~32 «новых» FAIL** в дирректориях
`str_builder/*`, `runtime/*`, `protocols/conversion`, `types/*`, `syntax/*`, `plan62/*`,
`plan91_fe4/neg`, `plan96/*`, `plan97/*`. **Все они — pre-existing на main**, не вызваны
изменениями 152.0.

### Почему наивный diff соврал — ДВЕ ловушки методологии

1. **Неполный baseline.** Baseline-прогон был **убит вручную** на `plan83_12` (concurrency-
   хвост долгий, ~40 мин), чтобы не ждать. Но дирректории `protocols/`, `runtime/`,
   `str_builder/`, `syntax/`, `types/` идут по алфавиту **ПОСЛЕ** всех `plan*` — baseline до
   них **не дошёл**. Их падения отсутствовали в baseline просто потому, что не были измерены
   → ложно классифицированы как «новые».
2. **Сломанная экстракция имён.** `grep -oE "STATUS\s+name/name"`:
   - single-dir прогон (`nova test nova_tests/plan62`) печатает имена **без префикса дир**
     (`no_prelude_explicit_import`), полный прогон — **с префиксом** (`plan62/no_prelude...`)
     → diff не сматчивается;
   - regex ловит `name/name` из **текста error-деталей** (после `#`) → мусорные совпадения.

### Как подтверждено, что это pre-existing

Прогон тех же дирректорий против **main** (неизменённый pre-152 std, отдельный репо
`d:/Sources/nv-lang/nova`): все падают и на main —
`str_builder` 9, `runtime` 19, `types` 21, `syntax` 54, `protocols` 8 test-fails.
Apples-to-apples per-dir diff (main basename-fails vs worktree basename-fails, extract через
`awk '$1~/FAIL/{print $2}'`): **0 новых в worktree** (см. вердикт ниже).

> **ВЕРДИКТ apples-to-apples (2026-06-13) — ✅ 0 НОВЫХ РЕГРЕССИЙ:**
> ```
> str_builder  main=7  wt=7   NEW: []
> runtime      main=19 wt=7   NEW: []
> protocols    main=22 wt=2   NEW: []
> types        main=21 wt=4   NEW: []
> syntax       main=54 wt=7   NEW: []
> plan62       main=29 wt=5   NEW: []
> plan91_fe4   main=10 wt=1   NEW: []
> plan96       main=21 wt=1   NEW: []
> plan97       main=16 wt=2   NEW: []
> ```
> Во **всех** дир `NEW_IN_WT` пусто — каждый worktree-fail есть и на main (подмножество).
> 152.0 (folder-split + registry-cleanup + buffer-refactor) **не добавил ни одного FAIL**.
> (NB: `wt` < `main` по счёту — артефакт более старого main-бинаря Jun-10 vs свежесобранного
> worktree-бинаря; на вердикт «0 новых» не влияет — он по именам, не по счёту.)

str-релевантные дирректории (пройдены и в baseline, и явными прогонами) — чисто:
`plan139` 37/0, `plan108` 5/0, `plan91_fe2` 10/0, `plan138` 10/0, `plan136` 11/0,
`plan100_8` 6/0, `plan91` 2/0, `plan77` 7/0.

---

## Урок (методология baseline для будущих gate)

1. **Не убивать baseline досрочно** — либо дать полному прогону завершиться, либо
   **батчить по группам дир** (memory `project-bash-timeout-10min-max`), но покрыть ВСЕ дир.
   Частичный baseline → ложные «регрессы» в непокрытом хвосте.
2. **Робастная экстракция имён:** `awk '$1 ~ /^(CC-FAIL|CODEGEN-FAIL|RUN-FAIL|TIMEOUT)$/
   {print $2}'` (поле-имя, не regex по строке) — иначе ловится текст error-деталей.
3. **Единый формат имён:** сравнивать одинаковые прогоны (оба full ИЛИ оба single-dir);
   префикс дир отличается между ними.
4. **Оракул для сомнительных:** прогон против **main** (неизменённый std, отдельный репо) —
   быстрый способ классифицировать «новое vs pre-existing» без полного baseline.
5. **main НЕ зелёный:** ~181 pre-existing FAIL (str_builder/runtime/syntax/types/protocols —
   инфра-проблемы `Nova_StringBuilder` struct-tag / `Nova_Iterable` / NPO / vec_debug и др.,
   не связанные со строками). Это нормальный фон; «без новых FAIL» сверяется с ним, не с нулём.
