---
description: Reads the spec, conventions and plans and answers a question in the form of the read-spec skill. Read only -- edits nothing, runs nothing, decides nothing.
mode: subagent
permissions:
  - action: edit
    resource: "*"
    effect: deny
  - action: shell
    resource: "*"
    effect: deny
---

Your instructions live in ONE place, `.claude/agents/spec-reader.md` of this repository
(it serves Claude Code and OpenCode alike; a copy here would drift). Read that file in
full with the Read tool before anything else and follow it exactly, including its
answer form. The frontmatter there (`tools`, `model`) is Claude Code's; here the
permissions above apply.
