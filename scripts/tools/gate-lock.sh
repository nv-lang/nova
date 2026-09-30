# scripts/tools/gate-lock.sh — замок «один гейт на дерево» (реестр 221.1 №1389).
# Не запускается сам: его подключают `scripts/gate.sh` и `scripts/gate-novac.sh`
# командой `. scripts/tools/gate-lock.sh` и зовут `gate_lock_acquire <имя> <корень>`.
#
# ЗАЧЕМ. 2026-09-30 гейт `main` трижды за день отказал на стражах, которые при
# отдельном запуске на том же дереве зелены. Причина — ВТОРОЙ живой гейт в том же
# дереве: `TaskStop` фоновой задачи снимает только обёртку-оболочку, а сам
# `gate.sh` живёт дальше (в 07:26 найдены два живых `gate.sh`, у первого 33
# дочерних процесса). Два прогона делят бинарь, кэш и временные файлы, и каждый
# читает то, что другой в этот миг переписывает. Отказ по среде неотличим от
# отказа по коду, пока не посмотришь в `ps`; замок делает второй прогон
# НЕВОЗМОЖНЫМ и называет держателя, вместо того чтобы дать ему покраснеть.
#
# КАК. Замок — каталог в git-каталоге ДЕРЕВА (`git rev-parse --absolute-git-dir`:
# у worktree он свой, так что соседние деревья друг друга не держат). `mkdir`
# атомарен — два одновременных старта не возьмут его оба. Внутри файл `owner`:
# PID, время старта, ярус. Держатель мёртв (`kill -0` не проходит) — замок
# протух (гейт снят сигналом, EXIT-ловушка не успела), его снимают вслух и берут.
#
# ВЛОЖЕННЫЙ ЗАПУСК своего же гейта (стражи зовут `gate.sh` сухим ходом, самотесты
# — в том же дереве) замком НЕ отказывается: держатель экспортирует путь замка в
# `NOVA_GATE_LOCK_HELD`, а переменная окружения доходит только до ПОТОМКОВ — это
# и есть доказательство родства, без разбора дерева процессов.
#
# `NOVA_GATE_LOCK_DIR` — подмена каталога замка; нужна самотесту
# (scripts/guards/selftest/test-gate-lock.sh), в прогонах не задаётся.

gate_lock_acquire() {
    _gl_name="$1"
    _gl_root="$2"
    _gl_gitdir="${NOVA_GATE_LOCK_DIR:-$(git -C "$_gl_root" rev-parse --absolute-git-dir 2>/dev/null)}"
    if [ -z "$_gl_gitdir" ] || [ ! -d "$_gl_gitdir" ]; then
        # Не git-дерево (распакованный архив) — замку негде жить; говорим вслух.
        echo "gate-lock: у дерева $_gl_root нет git-каталога — замок не ставится (№1389)"
        return 0
    fi
    GATE_LOCK_PATH="$_gl_gitdir/nova-$_gl_name.lock"
    case ":${NOVA_GATE_LOCK_HELD:-}:" in
        *":$GATE_LOCK_PATH:"*)
            echo "gate-lock: вложенный запуск под замком предка ($GATE_LOCK_PATH) — не берётся"
            GATE_LOCK_PATH=""
            return 0 ;;
    esac
    _gl_try=0
    while ! mkdir "$GATE_LOCK_PATH" 2>/dev/null; do
        _gl_try=$((_gl_try + 1))
        _gl_owner=$(cat "$GATE_LOCK_PATH/owner" 2>/dev/null || :)
        _gl_pid=$(printf '%s\n' "$_gl_owner" | sed -n 's/^PID=//p')
        if [ -z "$_gl_owner" ] && [ "$_gl_try" -le 3 ]; then
            # держатель только что создал каталог и ещё не дописал `owner`
            sleep 1; continue
        fi
        if [ -n "$_gl_pid" ] && kill -0 "$_gl_pid" 2>/dev/null; then
            echo "GATE FATAL: в этом дереве уже идёт $_gl_name (реестр №1389) — второй прогон читал бы то, что первый переписывает." >&2
            printf '%s\n' "$_gl_owner" | sed 's/^/  держатель: /' >&2
            echo "  замок:     $GATE_LOCK_PATH" >&2
            echo "  дождись его вердикта или сними его ДЕРЕВО процессов (TaskStop снимает только обёртку): ps -ef | grep $_gl_name" >&2
            GATE_LOCK_PATH=""
            exit 1
        fi
        echo "gate-lock: держатель замка мёртв — замок протух, снимается (${_gl_owner:-owner пуст})" | tr '\n' ' '
        echo
        rm -rf "$GATE_LOCK_PATH"
        [ "$_gl_try" -gt 5 ] && { echo "GATE FATAL: замок $GATE_LOCK_PATH не берётся после 5 попыток" >&2; exit 1; }
    done
    printf 'PID=%s\nSTART=%s\nTIER=%s\n' "$$" "$(date '+%Y-%m-%d %H:%M:%S')" "${3:-?}" \
        > "$GATE_LOCK_PATH/owner"
    NOVA_GATE_LOCK_HELD="${NOVA_GATE_LOCK_HELD:+$NOVA_GATE_LOCK_HELD:}$GATE_LOCK_PATH"
    export NOVA_GATE_LOCK_HELD
    echo "gate-lock: замок взят ($GATE_LOCK_PATH, PID $$)"
}

# Снимает замок, только если он НАШ: чужой (или уже перехваченный после протухания)
# не трогается. Зовётся из EXIT-ловушки гейта; код возврата не меняет.
gate_lock_release() {
    [ -n "${GATE_LOCK_PATH:-}" ] || return 0
    if [ "$(sed -n 's/^PID=//p' "$GATE_LOCK_PATH/owner" 2>/dev/null)" = "$$" ]; then
        rm -rf "$GATE_LOCK_PATH"
    fi
    GATE_LOCK_PATH=""
}
