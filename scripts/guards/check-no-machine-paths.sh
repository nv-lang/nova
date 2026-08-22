#!/bin/sh
# scripts/guards/check-no-machine-paths.sh — в отслеживаемых скриптах нет
# абсолютных путей к машине владельца.
#
# Реестр: docs/plans/221.1-bug-sweep.md №698. Владелец открыл
# nova-p274/scripts/gate.sh и спросил «что за пути?»: в гейте с первого дня
# (2026-07-26) стояло `NOVA_GC_LIB_DIR="D:\Sources\nv-lang\nova\..."` — путь к
# ЕГО машине в публичном скрипте; dev-env.sh «сверял значения с gate.sh» — то
# есть копировал их; cdb_session.sh — то же. Три копии одного костыля.
# Через него worktree 274 линковал GC из чужого дерева и не знал об этом.
#
# ПРАВИЛО: расположение ВЫВОДИТСЯ (от $ROOT, от `git rev-parse
# --git-common-dir`, от `dirname "$0"`), а не пишется. Путь к машине —
# только в комментарии-примере или в строке «Проверялся: ...».
#
# ЧТО СЧИТАЕТСЯ ПУТЁМ К МАШИНЕ: `d:/Sources`, `D:\Sources`, `/d/Sources`,
# `/mnt/d/Sources`, `C:\Users\<имя>` — в строке, которая НЕ комментарий.
#
# ИСКЛЮЧЕНИЯ (осознанные, названные):
#   * политика «worktree только в d:/Sources/nv-lang» (№561,
#     check-worktree-location.sh, repo-hygiene.sh) — это ПРАВИЛО владельца,
#     не раскладка, и оно уже под NOVA_WORKTREE_ROOT; исключено по имени файла;
#   * docs/plans/repro/** — улики, там пути — часть улики.
#
# $1 — корень. Самотест — selftest/test-check-no-machine-paths.sh
export LC_ALL=C
# Корень приводится к АБСОЛЮТНОМУ пути: относительный `.` уводил поиск
# бинаря мимо цели, и страж писал «сломан раннер» о здоровом дереве
# (2026-08-18). Ложная краснота стоит дороже отсутствующей проверки:
# по ней идут искать поломку, которой нет, и в стража перестают верить.
# Если cd не удался — значение СОХРАНЯЕТСЯ как было: пустой ROOT судил бы
# корень файловой системы, а это хуже исходной болезни.
ROOT="${1:-$(dirname "$0")/../..}"
ROOT="$(cd "$ROOT" 2>/dev/null && pwd || printf '%s' "$ROOT")"
NAME=check-no-machine-paths
FILES=$(git -C "$ROOT" ls-files -- 'scripts/*.sh' 'scripts/**/*.sh' 'scripts/*.py' 'scripts/**/*.py' '.github/workflows/*.yml' 2>/dev/null \
    | grep -vE '^scripts/guards/check-worktree-location\.sh$|^scripts/tools/repo-hygiene\.sh$|^scripts/guards/check-no-machine-paths\.sh$|^scripts/guards/selftest/test-check-no-machine-paths\.sh$' \
    | grep -vE '^scripts/claude-hooks/selftest/')
# scripts/claude-hooks/selftest/** — самотесты хуков ДЕРЖАТ такие пути как
# ТЕСТОВЫЕ ДАННЫЕ (строки, которые хук обязан распознать) — исключены каталогом.
# `Prichina:` — текст подсказки хука с примером команды; `step "` — текст шага
# гейта (политика №561 названа словами). Оба — сообщения, не пути исполнения.
if [ -z "$FILES" ]; then
    echo "$NAME: FAIL — git не отдал списка скриптов под $ROOT" >&2
    exit 1
fi
# ОДИН grep НА ВСЕ ФАЙЛЫ вместо форка grep+grep+printf+sed НА КАЖДЫЙ файл в
# цикле (план 275-Ф.1, гейт-стоимость — профиль показал ~29с на этом месте
# при сотнях отслеживаемых скриптов). `-H` держит имя файла в выводе даже
# когда xargs-пачка ужалась до одного файла (без -H grep одиночного файла
# молчит об имени — тогда группировка ниже развалилась бы).
RAW=$(printf '%s\n' "$FILES" | sed "s#^#$ROOT/#" \
    | xargs -d '\n' -r grep -nHE '([Dd]:[/\\]+Sources|/d/Sources|/mnt/d/Sources|[Cc]:[/\\]+Users[/\\])' 2>/dev/null \
    | sed "s#^$ROOT/##" \
    | grep -vE '^[^:]*:[0-9]+:[[:space:]]*#' \
    | grep -vE 'Проверялся|проверялся|# |Prichina:|^[^:]*:[0-9]+:step ')
# Группировка по файлу — тем же порядком, каким grep выдаёт совпадения
# (файлы обходятся в порядке $FILES, строки внутри файла — по возрастанию
# номера), поэтому один проход awk без сортировки воспроизводит ровно то
# же дерево вывода, что раньше строил цикл на bash.
BAD=$(printf '%s\n' "$RAW" | awk -F: '
    NF < 2 { next }
    {
        path = $1
        rest = substr($0, length(path) + 2)
        if (!started) { print ""; started = 1 }
        if (path != cur) { print "  " path ":"; cur = path }
        line = "    " rest
        if (length(line) > 140) line = substr(line, 1, 140)
        print line
    }')
if [ -n "$BAD" ]; then
    echo "$NAME: FAIL — абсолютный путь к машине в отслеживаемом скрипте (№698):$BAD" >&2
    echo "    Расположение выводится от \$ROOT / git-common-dir / dirname \$0, а не пишется." >&2
    exit 1
fi
N=$(printf '%s\n' "$FILES" | wc -l)
echo "$NAME ok: скриптов проверено $N, путей к машине вне комментариев 0 (№698)"
exit 0
