---
name: feedback-background-shell-dies-use-foreground-timeout
description: "Bash run_in_background и nohup/&/cmd start в этой среде молча умирают (RC=1, лог 0 байт) — долгие команды (гейт, cargo build) запускать foreground с timeout 600000; харнесс сам уводит в фон через 120с и процесс живёт"
metadata: 
  node_type: memory
  type: feedback
  originSessionId: a48a9f3a-0403-4a44-a6e3-8894781d4b88
  modified: 2026-08-16T15:39:17.846Z
---

Долгие команды (`gate.sh`, `cargo build --release`) запускать **foreground с
`timeout: 600000`** — харнесс через 120с сам переводит их в фон, и процесс
живёт до конца. НЕ полагаться на `run_in_background: true`, `nohup … &`,
`cmd //c start`, `powershell Start-Process`: посреди смены 2026-08-16 все они
начали молча умирать (лог 0 байт или `RC=1`, дочерний spawn `EPERM`), при том
что foreground-запуск работал.

enforcement: немашинное — среда харнесса; проверка одна: после запуска через
20–40с `ls -la --time-style=+%H:%M:%S <лог>` и `grep -c "^\[" <лог>` — если
размер не растёт, процесса нет, перезапускать foreground.

**Why:** потерял ~1 час на «гейт идёт» при мёртвом гейте: `run_in_background`
возвращался мгновенно, лог оставался старым (от предыдущего прогона —
одинаковое имя файла маскировало проблему), три перезапуска разными способами
дали то же. Плюс убил собственную оболочку `kill`-ом по паттерну `gate.sh`
(cmdline родителя содержал текст команды). Диагноз стал ясен только по
`bash -c "echo probe" > log & → RC=1` при живом foreground `bash -c`.

**How to apply:** (1) долгое — foreground + timeout 600000, ждать среза в фон,
затем `sleep 100–115` проверки (потолок Bash-вызова 120с); (2) новый лог-файл
на каждый прогон (`gate-run2.log`, `-run3`…) — иначе старый лог выдаёт себя за
новый; (3) убивать процессы только по PID из `ps`, не по grep cmdline;
(4) `run_in_background` — только для команд, чью смерть заметишь по отсутствию
результата в течение минуты. Связано: [[project-bash-timeout-10min-max]],
[[feedback-subagent-must-not-wait-for-notifications]].
