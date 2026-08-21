---
name: feedback-worktree-file-links
description: "При работе в worktree файловые ссылки в ответах должны использовать полный абсолютный путь, а не относительный"
metadata: 
  node_type: memory
  type: feedback
  originSessionId: ee222fd8-1b20-4b5c-8c93-ef9ef349e879
---

При работе в worktree (напр. `d:\Sources\nv-lang\nova-p91`) ссылки на файлы в markdown должны использовать **полный абсолютный путь**:

✓ `[string.nv](../nova-p91/std/runtime/string.nv)` — относительный от main репо через `..`  
✗ `[string.nv](std/runtime/string.nv)` — ведёт в main репо  
✗ `[string.nv](d:/Sources/nv-lang/nova-p91/std/runtime/string.nv)` — абсолютный, не кликабельный  
✗ `[string.nv](d:\Sources\nv-lang\nova-p91\std\runtime\string.nv)` — обратные слеши, не кликабельный

Паттерн для других worktree:
- nova-p91 → `../nova-p91/path/to/file`
- nova-p108 → `../nova-p108/path/to/file`

**Why:** VSCode extension резолвит относительные ссылки от `primaryWorkingDirectory` = `d:\Sources\nv-lang\nova`, не от текущего worktree.

**How to apply:** Всегда проверять: если работаю не в main репо — вставлять полный путь worktree в каждую ссылку на файл.
