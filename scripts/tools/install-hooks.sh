#!/usr/bin/env bash
# scripts/tools/install-hooks.sh — поставить git-хуки этого репозитория.
#
# ЗАЧЕМ ОТДЕЛЬНЫЙ СКРИПТ. Хуки живут в `.git/hooks`, который НЕ версионируется:
# новый worktree, новый клон, новая машина — хуков нет, и правило, на них
# опирающееся, молча перестаёт действовать. Скрипт делает установку явным,
# повторяемым шагом, а не разовой настройкой в чьей-то голове.
#
# ЧТО СТАВИТСЯ:
#   commit-msg       — гигиена коммита: маркеры конфликта в индексе, запрет
#                      Co-Authored-By, сверка авторства
#                      (scripts/guards/check-commit-hygiene.sh). Закрывает четыре
#                      правила, которые интегратор выполнял РУКАМИ в каждом
#                      коммите — замер 2026-08-08: из 74 правил в памяти у 57 не
#                      было механизма вовсе.
#   post-merge       — пересборка компилятора, если слияние задело
#                      `compiler-codegen/**` или `nova-cli/**`
#                      (scripts/tools/rebuild-if-stale.sh). Причина — №482:
#                      слияние в 22:13, бинарь от 17:08, и проверка молча шла
#                      против кода, которого в бинаре нет. Слияния планов и доков
#                      пересборку НЕ вызывают: лишняя минута на каждое слияние
#                      ведёт к обходу дисциплины.
#   pre-merge-commit — отказ вливать в главную ветку при красном или
#                      несвежем гейте (scripts/guards/check-merge-discipline.sh).
#                      Причина — вопрос владельца 2026-08-08 «почему допускаешь
#                      семьдесят пять коммитов без зелёного гейта?»: красный
#                      гейт закрывал отток, но не приток.
#
# ВАЖНО ПРО WORKTREE. Все worktree делят один `.git`, поэтому хук ставится один
# раз и действует всюду. Сам хук проверяет имя ветки и в ветках окон молчит.
#
# ИСПОЛЬЗОВАНИЕ:
#   bash scripts/tools/install-hooks.sh [КОРЕНЬ]

set -u
export LC_ALL=C

ROOT="${1:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}"
cd "$ROOT" || exit 1

GITDIR=$(git rev-parse --git-common-dir 2>/dev/null)
[ -n "${GITDIR:-}" ] || { echo "install-hooks: не git-репозиторий: $ROOT" >&2; exit 1; }
case "$GITDIR" in /*|[A-Za-z]:*) ;; *) GITDIR="$ROOT/$GITDIR" ;; esac

HOOKS="$GITDIR/hooks"
mkdir -p "$HOOKS" || exit 1

H="$HOOKS/pre-merge-commit"
cat > "$H" <<'HOOK'
#!/usr/bin/env bash
# Поставлен scripts/tools/install-hooks.sh. Не редактировать здесь —
# правь scripts/guards/check-merge-discipline.sh и переустанови.
set -u
TOP=$(git rev-parse --show-toplevel 2>/dev/null) || exit 0
G="$TOP/scripts/guards/check-merge-discipline.sh"
[ -f "$G" ] || exit 0
exec bash "$G" "$TOP"
HOOK
chmod +x "$H" 2>/dev/null

echo "install-hooks: поставлен $H"

H2="$HOOKS/commit-msg"
cat > "$H2" <<'HOOK'
#!/usr/bin/env bash
# Поставлен scripts/tools/install-hooks.sh. Не редактировать здесь —
# правь scripts/guards/check-commit-hygiene.sh и переустанови.
set -u
TOP=$(git rev-parse --show-toplevel 2>/dev/null) || exit 0
G="$TOP/scripts/guards/check-commit-hygiene.sh"
[ -f "$G" ] || exit 0
exec bash "$G" "$1" "$TOP"
HOOK
chmod +x "$H2" 2>/dev/null
echo "install-hooks: поставлен $H2"

H3="$HOOKS/post-merge"
cat > "$H3" <<'HOOK'
#!/usr/bin/env bash
# Поставлен scripts/tools/install-hooks.sh. Не редактировать здесь —
# правь scripts/tools/rebuild-if-stale.sh и переустанови.
#
# Пересобирает компилятор ТОЛЬКО если слияние задело compiler-codegen/nova-cli.
# Слияния планов, реестра и доков проходят без пересборки: лишняя минута на
# каждое слияние — цена, которая ведёт к обходу дисциплины.
set -u
TOP=$(git rev-parse --show-toplevel 2>/dev/null) || exit 0
R="$TOP/scripts/tools/rebuild-if-stale.sh"
[ -f "$R" ] || exit 0
if git diff --name-only ORIG_HEAD HEAD 2>/dev/null | grep -qE '^(compiler-codegen|nova-cli)/'; then
    bash "$R" || echo "post-merge: пересборка не удалась — компилятор УСТАРЕЛ, проверки против него недостоверны" >&2
fi
exit 0
HOOK
chmod +x "$H3" 2>/dev/null
echo "install-hooks: поставлен $H3"


echo "install-hooks: проверка — bash scripts/guards/selftest/test-check-merge-discipline.sh"
exit 0
