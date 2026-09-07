#!/usr/bin/env bash
# Селфтест: ВЕРДИКТ ГЕЙТА НАЗЫВАЕТ ДЕРЕВО, КОММИТ И ВЕТКУ — зелёный и красный.
#
# Класс, ради которого механизм заведён: «вердикт, снятый не с того дерева,
# читается точно так же, как снятый с нужного». Два носителя за один вечер
# 2026-09-07: окно 283 прогнало стражей из main вместо своей ветки (числа
# СОШЛИСЬ с верными — дырявый метод сам себя спрятал), а `check-commit-refs`
# дал 819 локально против 844 на CI, потому что у интегратора достижимы ветки
# соседей. Ни один из случаев не выглядел ошибкой: ни отказа, ни диффа —
# просто другое число без способа узнать, каким деревом оно получено.
#
# ЗАЧЕМ САМОТЕСТ, а не только правка гейта: строка вывода — самая удаляемая
# вещь в скрипте. Без него мой хвост снимут первой же чисткой «лишнего шума»,
# и класс вернётся молча, ровно как вернулся бы паритет драйверов без своего
# самотеста (реестр 221.1 №1012/№1023).
#
# Доказываем ПЯТЬ свойств, все машинно:
#   1. Сухой ход печатает шапку с деревом, коммитом и веткой.
#   2. Зелёная итоговая строка несёт хвост `[tree= head= branch=]`.
#   3. Якорь цел: строка по-прежнему НАЧИНАЕТСЯ с `GATE OK (final)` — её
#      грепают люди, планы и окно-интегратора, и ломать её нельзя.
#   4. КРАСНАЯ строка рубежа тоже несёт хвост (красное цитируют не реже).
#   5. Проба умеет краснеть: без `$TREE_TAIL` рубеж падает под `set -u`.
#   7–9. ТО ЖЕ ДЛЯ NOVAC-ГЕЙТА: все его итоговые выходы несут хвост
#      (без хвоста законны только пошаговые репортёры — те, что печатают `$1`),
#      строка вердикта
#      из ФАЙЛА печатает хвост и падает без переменной, присваивание раньше
#      первого вердикта. У novac-гейта НЕТ сухого хода, а живой прогон зовёт
#      компилятор — поэтому там структура и проба, а не прогон, и это сказано
#      вслух, а не выдано за равноценное доказательство.
#      Без этого случая пункт 4 ничего не значит — он бы «проходил» и на
#      сломанном механизме.
#
# Пункт 5 не только контроль: незаданный хвост убил бы гейт РОВНО на отказе,
# в худший момент, — поэтому присваивание стоит до первого рубежа, и это тоже
# проверяется (случай 6).
#
# Компилятор НЕ запускается: только `NOVA_GATE_DRYRUN=1` (шаги печатаются, не
# исполняются) и извлечённая из файла функция рубежа.
#
# Запуск: scripts/guards/selftest/test-gate-verdict-identity.sh
# Выход: 0 — механизм цел, 1 — сломан.

set -uo pipefail
export LC_ALL=C

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
GATE="$REPO_ROOT/scripts/gate.sh"

FAILED=0
CASES=0
ok()  { CASES=$((CASES+1)); echo "  ok: $1"; }
bad() { CASES=$((CASES+1)); echo "  ПРОВАЛ: $1" >&2; FAILED=1; }

echo "== селфтест: вердикт обоих гейтов называет дерево =="

if [ ! -f "$GATE" ]; then
    echo "  ПРОВАЛ: не найден $GATE" >&2
    exit 1
fi

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

# --- 1..3: зелёный путь, сухим ходом (без компилятора) --------------------
( cd "$REPO_ROOT" && NOVA_GATE_DRYRUN=1 NOVA_GATE_TIER=loop bash "$GATE" ) \
    > "$TMP/dry.txt" 2>&1
DRY_RC=$?

HEAD_SHORT="$(git -C "$REPO_ROOT" rev-parse --short HEAD 2>/dev/null || echo unknown)"

if [ "$DRY_RC" -ne 0 ]; then
    bad "сухой ход яруса loop вернул $DRY_RC (ожидался 0)"
elif head -1 "$TMP/dry.txt" | grep -q "$REPO_ROOT" \
     && head -1 "$TMP/dry.txt" | grep -q "$HEAD_SHORT"; then
    ok "шапка называет дерево и коммит"
else
    bad "первая строка сухого хода не называет дерево/коммит: $(head -1 "$TMP/dry.txt")"
fi

VERDICT="$(grep -m1 '^GATE OK (final)' "$TMP/dry.txt" || true)"
if [ -z "$VERDICT" ]; then
    bad "в сухом ходе нет строки 'GATE OK (final)'"
else
    case "$VERDICT" in
        *"[tree=$REPO_ROOT head=$HEAD_SHORT branch="*)
            ok "зелёная итоговая строка несёт дерево, коммит и ветку" ;;
        *)
            bad "зелёная итоговая строка без хвоста дерева: $VERDICT" ;;
    esac
    case "$VERDICT" in
        "GATE OK (final)"*)
            ok "якорь цел: строка начинается с 'GATE OK (final)'" ;;
        *)
            bad "якорь сломан, строка начинается иначе: $VERDICT" ;;
    esac
fi

# --- 4..5: красный путь, функцией, ВЫНУТОЙ ИЗ ФАЙЛА -----------------------
# Тело рубежа не переписывается рукой: рука разойдётся с файлом, и самотест
# начнёт проверять свою копию вместо гейта (класс «страж мерит не то дерево»,
# ровно тот же, ради которого всё и заведено).
awk '/^gate_barrier\(\) \{/,/^\}/' "$GATE" > "$TMP/barrier.sh"
if ! grep -q 'TREE_TAIL' "$TMP/barrier.sh"; then
    bad "в вынутом теле gate_barrier нет \$TREE_TAIL — механизм снят"
else
    {
        echo 'set -u'
        echo 'GATE_FAIL_N=2'
        echo 'GATE_FAILS=" step-a; step-b"'
        echo 'TREE_TAIL=" [tree=/probe head=deadbeef branch=probe-branch]"'
        cat "$TMP/barrier.sh"
        echo 'gate_barrier'
    } > "$TMP/red_with.sh"
    RED_OUT="$(bash "$TMP/red_with.sh" 2>&1)"; RED_RC=$?
    if [ "$RED_RC" -eq 1 ] && printf '%s' "$RED_OUT" | grep -q '\[tree=/probe head=deadbeef branch=probe-branch\]'; then
        ok "красная итоговая строка несёт дерево, коммит и ветку"
    else
        bad "красная строка без хвоста (rc=$RED_RC): $RED_OUT"
    fi

    {
        echo 'set -u'
        echo 'GATE_FAIL_N=2'
        echo 'GATE_FAILS=" step-a; step-b"'
        cat "$TMP/barrier.sh"
        echo 'gate_barrier'
    } > "$TMP/red_without.sh"
    NO_OUT="$(bash "$TMP/red_without.sh" 2>&1)"; NO_RC=$?
    if [ "$NO_RC" -ne 0 ] && printf '%s' "$NO_OUT" | grep -q 'TREE_TAIL'; then
        ok "проба умеет краснеть: без \$TREE_TAIL рубеж падает под set -u"
    else
        bad "без \$TREE_TAIL рубеж не покраснел (rc=$NO_RC): $NO_OUT"
    fi
fi

# --- 6: порядок — присваивание раньше любого рубежа ------------------------
ASSIGN_LINE="$(grep -n '^TREE_TAIL=' "$GATE" | head -1 | cut -d: -f1)"
FIRST_BARRIER="$(grep -n '^[[:space:]]*gate_barrier$' "$GATE" | head -1 | cut -d: -f1)"
if [ -n "$ASSIGN_LINE" ] && [ -n "$FIRST_BARRIER" ] && [ "$ASSIGN_LINE" -lt "$FIRST_BARRIER" ]; then
    ok "присваивание (строка $ASSIGN_LINE) идёт раньше первого рубежа (строка $FIRST_BARRIER)"
else
    bad "порядок нарушен: присваивание '$ASSIGN_LINE', первый рубеж '$FIRST_BARRIER' — незаданный хвост убьёт гейт на самом отказе"
fi


# --- 7..9: ВТОРОЙ ГЕЙТ (novac) — та же форма, тот же риск ------------------
# У novac-гейта сухого хода нет, а живой прогон зовёт компилятор. Поэтому здесь
# структура плюс проба строкой, ВЗЯТОЙ ИЗ ФАЙЛА, — не прогон. Живой вывод
# проверяется первым же прогоном интегратора, и это сказано вслух, а не скрыто.
NOVAC_GATE="$REPO_ROOT/scripts/gate-novac.sh"
if [ ! -f "$NOVAC_GATE" ]; then
    bad "не найден $NOVAC_GATE"
else
    # ПРАВИЛО, А НЕ ЧИСЛО. Строка `echo "NOVAC-GATE...` без хвоста законна ТОЛЬКО если
    # она принимает `$1`, то есть сообщает об ОДНОМ пункте и повторяется на каждом
    # (`NOVAC-GATE FAIL: $1`, `NOVAC-GATE РАССИНХРОН: $1`) — там хвост был бы шумом.
    # Строка, ЗАВЕРШАЮЩАЯ прогон, обязана нести хвост.
    #
    # Сначала здесь стояло число «без хвоста ровно 1», и самотест поймал на нём МЕНЯ:
    # пошаговых репортёров два, а я посчитал одного. Число в проверке устаревает при
    # первом же добавлении строки — тот же класс, что 168 стражей в AGENTS.md, ставшие
    # 188 за три дня. Правило не устаревает.
    UNTAILED_LINES="$(grep -n 'echo "NOVAC-GATE' "$NOVAC_GATE" \
        | grep -v 'NOVAC_TREE_TAIL' | grep -v ': \$1"' || true)"
    WITH_TAIL_N="$(grep 'echo "NOVAC-GATE' "$NOVAC_GATE" | grep -c 'NOVAC_TREE_TAIL' || true)"
    # `-gt 0` здесь НЕ счёт-снимок, а проверка на НЕПУСТОТУ: если завтра сменится
    # префикс `NOVAC-GATE`, оба грепа найдут ноль строк, пустой список
    # непомеченных прочтётся как «всё хорошо», и случай зазеленеет НА ПУСТОМ
    # МЕСТЕ. Числа `4` тут стоять не должно по правилу, которое этот же файл
    # и проверяет: критерий — СВОЙСТВО, а не снимок дерева; счёт покраснел бы на
    # ЗАКОННОМ удалении вердикта и ничего не поймал бы при добавлении.
    if [ "$WITH_TAIL_N" -gt 0 ] && [ -z "$UNTAILED_LINES" ]; then
        ok "novac: вердиктов с хвостом $WITH_TAIL_N, а без хвоста — только пошаговые репортёры"
    else
        bad "novac: итоговый вердикт без хвоста: ${UNTAILED_LINES:-нет} (с хвостом $WITH_TAIL_N)"
    fi

    # Строка берётся ИЗ ФАЙЛА — рука разошлась бы с гейтом.
    NOVAC_LINE="$(grep -m1 'echo "NOVAC-GATE OK (final)' "$NOVAC_GATE" | sed 's/^[[:space:]]*//')"
    if [ -z "$NOVAC_LINE" ]; then
        bad "novac: не найдена итоговая строка 'NOVAC-GATE OK (final)'"
    else
        OUT_WITH="$(bash -c 'set -u; NOVAC_TREE_TAIL=" [tree=/probe head=deadbeef branch=probe-branch]"; '"$NOVAC_LINE" 2>&1)"
        case "$OUT_WITH" in
            "NOVAC-GATE OK (final) [tree=/probe head=deadbeef branch=probe-branch]")
                ok "novac: итоговая строка несёт хвост и сохраняет префикс" ;;
            *)
                bad "novac: итоговая строка дала '$OUT_WITH'" ;;
        esac
        OUT_WITHOUT="$(bash -c 'set -u; '"$NOVAC_LINE" 2>&1)"; W_RC=$?
        if [ "$W_RC" -ne 0 ] && printf '%s' "$OUT_WITHOUT" | grep -q 'NOVAC_TREE_TAIL'; then
            ok "novac: проба умеет краснеть — без переменной строка падает под set -u"
        else
            bad "novac: без переменной строка не покраснела (rc=$W_RC): $OUT_WITHOUT"
        fi
    fi

    N_ASSIGN="$(grep -n '^NOVAC_TREE_TAIL=' "$NOVAC_GATE" | head -1 | cut -d: -f1)"
    N_FIRST="$(grep -n 'NOVAC_TREE_TAIL"' "$NOVAC_GATE" | head -1 | cut -d: -f1)"
    if [ -n "$N_ASSIGN" ] && [ -n "$N_FIRST" ] && [ "$N_ASSIGN" -lt "$N_FIRST" ]; then
        ok "novac: присваивание (строка $N_ASSIGN) идёт раньше первого вердикта (строка $N_FIRST)"
    else
        bad "novac: порядок нарушен — присваивание '$N_ASSIGN', первый вердикт '$N_FIRST'"
    fi
fi

if [ "$FAILED" -eq 0 ]; then
    echo "селфтест gate-verdict-identity: $CASES/$CASES ok"
    exit 0
fi
echo "селфтест gate-verdict-identity: ЕСТЬ ПРОВАЛЫ" >&2
exit 1
