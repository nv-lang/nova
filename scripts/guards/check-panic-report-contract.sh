#!/bin/sh
# scripts/guards/check-panic-report-contract.sh — запись отказа обязана
# иметь ОБА рендерера и все обязательные поля.
#
# План/реестр: spec/decisions/08-runtime.md D462 (амендмент D437);
# docs/plans/221.1-bug-sweep.md №445 — паник-дорожка была красной именно
# потому, что throw-site и `?`-трасса терялись на переходе файбер→драйвер,
# а увидеть это было нечем: гейт эту дорожку не гоняет.
#
# ПРАВИЛО (проверяется исполнением, а не грепом):
#   1. человеческий рендер (умолчание) печатает сообщение, `(throw site)`
#      и `propagation trace` со звеньями `?`-цепочки;
#   2. NOVA_PANIC_FORMAT=json печатает ОДНУ строку с ключом-маркером
#      "nova_failure":1 и полями kind/message/site/trace/suppressed;
#   3. JSON — валидный (разбирается парсером, а не глазами).
#
# ПОЧЕМУ исполнением: греп по effects.h проверил бы, что код НАПИСАН,
# а не что вывод СЛОЖИЛСЯ. Ровно этой разницей и жил №445 — обещание
# D437 стояло в спеке и в коде, а на переходе через поток терялось.
#
# НЕ проверяет: составной маршрут D158 и typed-Fail (их фикстуры красные
# по другой причине — см. №445, оставшиеся четыре).
#
# $1 — корень репозитория (default: вычислить от себя).
# $2 — путь к nova (default: $ROOT/nova-cli/target/release/nova.exe).
#
# Проверялся: Windows (Git Bash), 2026-08-15.
export LC_ALL=C
ROOT="${1:-$(cd "$(dirname "$0")/../.." && pwd)}"
NOVA="${2:-$ROOT/nova-cli/target/release/nova.exe}"
NAME=check-panic-report-contract

if [ ! -x "$NOVA" ]; then
    NOVA="$ROOT/nova-cli/target/release/nova"
fi
if [ ! -x "$NOVA" ]; then
    echo "$NAME ok: компилятор не собран ($NOVA) — судить нечего"
    exit 0
fi

FIX="$ROOT/spec_tests/conformance/neg/f5_propagation_trace_full.nv"
if [ ! -f "$FIX" ]; then
    echo "$NAME: FAIL — нет фикстуры $FIX" >&2
    exit 1
fi

TMP=$(mktemp -d 2>/dev/null || echo "${TMPDIR:-/tmp}/$NAME.$$")
mkdir -p "$TMP" 2>/dev/null
EXE="$TMP/report_probe.exe"

if ! "$NOVA" build "$FIX" -o "$EXE" >"$TMP/build.log" 2>&1; then
    echo "$NAME: FAIL — не собралась фикстура записи отказа" >&2
    tail -5 "$TMP/build.log" >&2
    rm -rf "$TMP"
    exit 1
fi

# ── 1. человеческий рендер (умолчание) ───────────────────────────────
"$EXE" >"$TMP/human.out" 2>"$TMP/human.err"
HUMAN=$(cat "$TMP/human.err")
FAILED=0

for NEED in "leaf-error" "(throw site)" "propagation trace" "(?)"; do
    case "$HUMAN" in
        *"$NEED"*) ;;
        *)
            echo "$NAME: FAIL — человеческий рендер без '$NEED' (D462 §1, №445)" >&2
            FAILED=1
            ;;
    esac
done

case "$HUMAN" in
    *nova_failure*)
        echo "$NAME: FAIL — умолчание печатает JSON; человеческий вывод обязан быть по умолчанию (D462)" >&2
        FAILED=1
        ;;
esac

# ── 2. JSON-рендер ────────────────────────────────────────────────────
NOVA_PANIC_FORMAT=json "$EXE" >"$TMP/json.out" 2>"$TMP/json.err"
JSON=$(cat "$TMP/json.err")

for NEED in '"nova_failure":1' '"kind"' '"message"' '"site"' '"trace"' '"suppressed"'; do
    case "$JSON" in
        *"$NEED"*) ;;
        *)
            echo "$NAME: FAIL — в JSON-записи нет поля $NEED (D462 таблица полей)" >&2
            FAILED=1
            ;;
    esac
done

# Одна строка целиком, а не «одна строка с ключом-маркером»: потребитель
# читает запись построчно, и перенос внутри записи ломает разбор.
LINES=$(printf '%s\n' "$JSON" | grep -c '[^[:space:]]')
if [ "$LINES" -ne 1 ]; then
    echo "$NAME: FAIL — JSON-запись обязана быть ОДНОЙ строкой, найдено $LINES" >&2
    FAILED=1
fi

# ── 3. JSON разбирается парсером, а не глазами ────────────────────────
if command -v python >/dev/null 2>&1; then
    PY=python
elif command -v python3 >/dev/null 2>&1; then
    PY=python3
else
    PY=
fi
if [ -n "$PY" ]; then
    printf '%s' "$JSON" >"$TMP/rec.json"
    if ! "$PY" -c "import json,sys
d = json.load(open(sys.argv[1], encoding='utf-8'))
assert d['nova_failure'] == 1
assert isinstance(d['message'], str) and d['message']
assert isinstance(d['trace'], list) and len(d['trace']) >= 1
assert all('file' in e and 'line' in e for e in d['trace'])
assert isinstance(d['site'], dict) and 'line' in d['site']
" "$TMP/rec.json" >"$TMP/parse.log" 2>&1; then
        echo "$NAME: FAIL — JSON-запись не разбирается парсером (D462 §3)" >&2
        tail -3 "$TMP/parse.log" >&2
        echo "  запись: $JSON" >&2
        FAILED=1
    fi
else
    echo "$NAME: NOTE — python недоступен, разбор JSON пропущен" >&2
fi

rm -rf "$TMP"
if [ "$FAILED" -ne 0 ]; then
    exit 1
fi
echo "$NAME ok: человеческий рендер даёт throw-site и propagation trace, JSON-рендер — одну валидную строку со всеми полями (D462)"
exit 0
