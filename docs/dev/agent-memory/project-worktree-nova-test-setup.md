---
name: project-worktree-nova-test-setup
description: "Свежий worktree собирает сам — env NOVA_GC_*/копия libuv больше НЕ нужны (авто-fallback, реестр №650)"
metadata: 
  node_type: memory
  type: project
  originSessionId: a48a9f3a-0403-4a44-a6e3-8894781d4b88
  modified: 2026-08-14T00:40:12.140Z
---

**С 2026-08-14 ручной ритуал заведения worktree ОТМЕНЁН** (реестр №650,
`test_runner.rs`): свежее дерево собирает hello без единого действия.

* `detect_boehm` шаг 2b: worktree сам находит vcpkg главной репы через свой
  `.git`-файл (`main_worktree_root`), с печатной строкой о fallback.
* `detect_or_build_libuv` при пустом сабмодуле сам запускает
  `git submodule update --init` — оффлайн, общий `.git/modules`, пин этой ветки.

`NOVA_GC_LIB_DIR`/`NOVA_INCLUDE_DIR`/`NOVA_RT_DIR` остаются как **override**
(standalone-пакеты вроде nova-tls указывают ими на монорепу — это по-прежнему
нужно: у них нет сабмодуля и fallback им недоступен).

Граница: подменяется только vcpkg (его в worktree не бывает); `rt_dir` и
исходники — никогда, правка рантайма веткой компилируется из ветки (№283).

Проверка при сомнении: свежее дерево от main обязано собрать
`examples/basics/hello.nv` с нуля; если нет — это регресс №650.
