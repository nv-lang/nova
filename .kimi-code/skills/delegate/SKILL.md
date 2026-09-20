---
name: delegate
description: "экономия лимитов: дешёвые модели агентам, брифы под механического исполнителя, дисциплина фона — адаптер .claude/commands/delegate.md"
type: prompt
disableModelInvocation: true
---

**Это АДАПТЕР, а не вторая копия.** Источник правды —
`.claude/commands/delegate.md`. Прочитай его целиком и следуй ему, заменяя:

- Модели `haiku/sonnet/opus` → в Kimi Code выбор модели для субагентов делается
  через `/secondary-model` (пул моделей) и параметр `model` у инструмента
  `Agent`. Дешёвую/быструю модель настрой в `/secondary-model`, а в брифе
  указывай, на какой модели агент должен бежать.
- `Task` (фоновое задание Claude) → `Agent` с `run_in_background=true` или
  `Bash` с `run_in_background=true`. Фон умирает молча — прогресс в файл,
  идемпотентные шаги, фильтрация пустых результатов, проверка каждые 5 минут
  через `TaskOutput`/`TaskList`.
- Агент-читатель и агент-охотник → порты заведены в
  `.kimi-code/agents/spec-reader.md` и `.kimi-code/agents/defect-hunter.md`;
  вызывай их через `Agent` с `subagent_type="spec-reader"` /
  `"defect-hunter"`. Форма брифа — `.claude/skills/read-spec/SKILL.md` и
  `docs/dev/delegation-agent-briefs.md`.
- `opencode` → в Kimi Code нет встроенного аналога; если OpenCode установлен
  отдельно, используй его по его собственным правилам, иначе поручай ту же
  работу субагенту на дешёвой модели.
- Строка «Модели агентов: …» в докладе сохраняется.

Общие правила сохраняются: суб-агентов не спавнить без нужды, агенту — не в
main-репу (только своё дерево), не гонять мега-CU/полный `nova test`, греп
маркеров конфликта перед коммитом, языковые слияния со спекой.

$ARGUMENTS
