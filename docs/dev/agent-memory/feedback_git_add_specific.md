---
name: feedback-git-add-specific
description: "Всегда git add только конкретных файлов, никогда git add -A или git add ."
metadata: 
  node_type: memory
  type: feedback
  originSessionId: b0d63fe6-28de-4e3a-960e-cfb879e77d2e
---

Никогда не использовать `git add -A`, `git add .` или `git add *`.
Всегда указывать конкретные файлы: `git add path/to/file1 path/to/file2`.

**Why:** В репо параллельно работают другие агенты. Широкий add захватит их незакоммиченные изменения.

**How to apply:** Перед каждым коммитом — перечислить только те файлы, которые изменил сам в рамках текущей задачи.
