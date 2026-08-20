---
name: project-bash-timeout-10min-max
description: Bash/PowerShell tool timeout maxes at 600000ms (10 min) even for run_in_background — long nova test runs get killed; chunk them
metadata: 
  node_type: memory
  type: project
  originSessionId: dd5c4857-a3c7-46cd-9695-cab9b466d77d
---

Инструмент Bash/PowerShell имеет **жёсткий потолок таймаута 600000 мс (10 минут)**,
и он применяется ДАЖЕ к `run_in_background: true` — фоновая команда убивается на 10-й минуте.

**Наблюдение (Plan 153, 2026-06-13):** полный `nova test` (~2000 runnable фикстур ×
~30 c линковки Boehm-static / 16 jobs ≈ 60–90 мин) запущенный в фоне с `timeout: 600000`
был **обрезан на 16 из 205 директорий** (алфавитно до plan100_1), exit=1. Это не баг
nova test и не дефолтный subset — это таймаут.

**Как применять:**
- Полную регрессию (`nova test` без фильтра) нельзя гонять одним вызовом — **дробить**
  на батчи `< 10 мин` (по директориям / `--filter`-подмножествам, ~300 тестов/батч при
  jobs=16) и агрегировать FAIL-сеты.
- Для baseline-до-изменений (когда уже тронул `std/`) — temp-worktree на main HEAD +
  переиспользовать собранный `nova.exe` (компилятор не менялся) — см.
  [[project-worktree-nova-test-setup]].
- Любая долгая команда (сборка + тесты в одном скрипте) рискует упереться в 10 мин —
  разделять сборку (своя команда) и прогон (батчи).
