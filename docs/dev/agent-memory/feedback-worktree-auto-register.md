---
name: feedback-worktree-auto-register
description: "Агент сам регистрируется в hook-системе при explicit команде «работай в X», без напоминания от user"
metadata: 
  node_type: memory
  type: feedback
  originSessionId: 1b171c1a-1ebe-41ac-8384-bdab1b0efdce
---

**Правило:** Когда user даёт explicit instruction работать в конкретном
worktree — агент **первой Bash командой** регистрируется в hook системе
без напоминания.

**Why:** Установлен PreToolUse Bash hook ([[project-worktree-cwd-hook]])
который автоматически cd'ит в зарегистрированный worktree. User не
должен каждый раз напоминать про регистрацию — это его раздражало
в предыдущих сессиях.

**Триггеры (intent-based, не position-based):**

1. **Explicit work instruction** — независимо первое сообщение или 50-е:
   - «работай в d:/Sources/nv-lang/nova-pNN-foo»
   - «работай изолированно в X»
   - «переключись на X»
   - «продолжай в worktree X»
   → **сразу** `worktree-register.sh set X` первой Bash командой

2. **Implicit work context** — при first Bash в task связанном с worktree:
   - Если `worktree-register.sh show` показывает «NOT registered»
   - НО в conversation уже упоминался путь типа `d:/.../nova-pNN-*`
   - → спросить «зарегистрироваться в X?» (НЕ автоматически)

3. **Edge: уже registered, другой путь предложен:**
   - НЕ молча перерегистрироваться
   - Confirm: «сейчас зарегистрирован в Y, переключить на X или продолжать в Y?»
   - Молчаливое переключение = lost work risk

4. **Edge: user просит lookup без work intent:**
   - «глянь файл d:/.../nova-pNN/foo.rs» — это lookup, НЕ register
   - Distinguish work vs read

5. **Финализация плана:**
   - User говорит «закончили», «merge'нул», «сделано», «закрываем план»
   - → `worktree-register.sh clear`

**Команды (всегда абсолютным путём, helper лежит в main checkout):**
- Регистрация: `/d/Sources/nv-lang/nova/.claude/hooks/worktree-register.sh set "d:/Sources/nv-lang/nova-pNN-foo"`
- Проверка: `/d/Sources/nv-lang/nova/.claude/hooks/worktree-register.sh show`
- Снятие: `/d/Sources/nv-lang/nova/.claude/hooks/worktree-register.sh clear`

**Проверка что hook сработал:**
После регистрации запусти `pwd && git branch --show-current` — должно
показать worktree-path и нужную branch. Если показывает `/d/Sources/nv-lang/nova`
(main) — значит hook не загружен (нужен restart chat tab).

**How to apply:** При получении инструкции работать в конкретном worktree —
регистрируйся сразу первой Bash командой. Не жди напоминания.
