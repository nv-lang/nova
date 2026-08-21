---
name: feedback-push-after-green-gate
description: "Стоячее правило владельца (2026-07-16) — пушить main в github СРАЗУ после каждого зелёного авторитетного гейта, без отдельного спроса"
metadata: 
  node_type: memory
  type: feedback
  originSessionId: a48a9f3a-0403-4a44-a6e3-8894781d4b88
---

Владелец (2026-07-16, явное «ДА» на прямой вопрос): **пушить `main` в github сразу после каждого
ЗЕЛЁНОГО авторитетного гейта** (conformance FAIL:0 + флагман-examples-build по конвенции
test-conventions.md/696556c86), НЕ спрашивая каждый раз.

**Why:** до этого я держал main локально до отдельного слова — владелец несколько раз подряд отвечал
«пуш ок» и включил правило, чтобы убрать лишний вопрос-цикл.

**How to apply:** после успешного авторитетного гейта волны: `git fetch github` → проверить
behind==0 (иначе разбор, не force) → `git push github main` → verify ahead==0. Пуш ТОЛЬКО
гейтнутого состояния; негейтнутые/промежуточные коммиты в main не пушить отдельно от волны.
Force-push по-прежнему запрещён без явного слова. Связано: [[feedback-verify-index-before-commit]],
[[feedback-conflict-marker-grep]].
