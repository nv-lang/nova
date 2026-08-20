---
name: feedback-worktree-on-d-drive
description: "Все worktree создавать в d:\\Sources\\nv-lang\\ (рядом с репами), НЕ в C:\\Users\\Public — несмотря на exFAT"
metadata: 
  node_type: memory
  type: feedback
  originSessionId: a48a9f3a-0403-4a44-a6e3-8894781d4b88
  modified: 2026-07-31T18:13:15.185Z
---

Владелец: «мы все ворктри создаем внутри `d:\Sources\nv-lang`» (2026-07-29, замечание
после того, как я создал четыре worktree в `C:/Users/Public/`).

enforcement: немашинное — правило касается пути, который выбирает сессия при
`git worktree add`; страж в гейте невозможен (гейт исполняется УЖЕ внутри worktree и
про место его создания ничего не знает), а pre-commit-хук не видит `worktree add`
вовсе. Проверяется глазами: `git worktree list` — все пути обязаны начинаться
с `D:/Sources/nv-lang/`.

**Why:** репы проекта (`nova`, `www`, `nova-tls`, `nova-http`, `nova-polaris`) лежат
в `d:\Sources\nv-lang\`, и worktree владелец держит там же — рядом, а не в системной
папке на другом диске. Я скопировал сложившуюся в репозитории практику (9 из 9
существующих worktree были на `C:/Users/Public`) вместо того, чтобы следовать правилу
владельца — практика в репе ≠ правило владельца.

**Важный факт, который эту практику породил:** `D:` — **exFAT** (кластер 1 МБ,
219 ГБ свободно), `C:` — NTFS (19 ГБ свободно). В реестре 221.1 есть заметка волны
`nova-mncrash`: worktree положен на `C:` — «НЕ на D: (exFAT, кластер 1 МБ, диск полон)».
Цена размещения на `D:`: ~4700 файлов worktree × кластер 1 МБ ≈ 4-5 ГБ на worktree
вместо ~0.5 ГБ. Места хватает — правило владельца перевешивает.

**Расширение (владелец 2026-07-31):** правило касается НЕ только git-worktree —
ЛЮБЫЕ .nv-репро-пакеты, временные Nova-проекты и scratch-код агентов тоже только
в `d:\Sources\nv-lang` (внутри своего worktree, напр. `<wt>/_repro/`, в коммиты не
включать; у интегратора — `nova/scratch38`). Claude-scratchpad на C: для Nova-файлов
ЗАПРЕЩЁН (прецедент: box-окно создало `scratchpad/polaris_repro_pkg` — владелец
заметил). В каждый бриф агента с репро-работой — явный пункт об этом.

**How to apply:** `git worktree add D:/Sources/nv-lang/nova-pNN -b pNN-<тема> main`.
Именование — `nova-pNN` (см. [[feedback-worktree-naming]]). Влитые worktree убирать
(`git worktree remove`), чтобы не копились — на 2026-07-29 накопилось 14 штук, из них
7 с уже влитыми ветками. Невлитые (`p-fix-mn-crash`, `p-fix-n38-workertls`,
`p-val-research`) не трогать — там незакрытая работа.

Связано: [[feedback-isolated-worktree]], [[feedback-worktree-naming]],
[[feedback_worktree_cwd_clarity]], [[project-worktree-nova-test-setup]].
