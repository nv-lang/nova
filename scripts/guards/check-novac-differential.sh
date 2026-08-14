#!/bin/sh
# scripts/guards/check-novac-differential.sh — дифференциальный прогон novac
# против оракула.
#
# ПРАВИЛО (план 274 §10.3, §10.3а: «контракт = оракулу; расхождения только из
# реестра»): novac обязан принимать/отвергать те же программы, что нынешний
# компилятор (оракул nova-cli/target/release/nova.exe check). Страж прогоняет
# оба бинаря по novac/fixtures/**/pos_*.nv и сравнивает ИСХОД (принял/отверг,
# по коду возврата). Расхождение, не записанное в novac/divergences.allow
# (строка = путь фикстуры от корня, с прямыми слэшами), — красное.
#
# НЕ проверяет: совпадение текстов/кодов диагностик (только исход),
# поведение на neg_* (их судят diag-schema и no-cascade), обоснованность
# записей allow — её судит приёмка и docs/dev/novac-divergences.md.
# Контракт вызова: '<bin> check <file>'; если CLI novac окажется иным —
# страж правится тем же коммитом, что вводит бинарь.
#
# Страж «ожидает бинарь»: пока novac/target/novac.exe не существует — зелёный
# честной строкой: страж до кода легален, молчание нелегально (№645).
#
# $1 — корень репозитория (default: вычислить от себя);
# $2 — override бинаря novac (для самотеста).
#
# Проверялся: Windows (Git Bash), 2026-08-14.
export LC_ALL=C
ROOT="${1:-$(cd "$(dirname "$0")/../.." && pwd)}"
BIN="${2:-$ROOT/novac/target/novac.exe}"
NAME=check-novac-differential

if [ ! -f "$BIN" ]; then
    echo "$NAME ok: судить нечего (novac ещё не собирается)"
    exit 0
fi

ORACLE="$ROOT/nova-cli/target/release/nova.exe"
if [ ! -f "$ORACLE" ]; then
    echo "$NAME ok: судить нечего (оракул nova-cli/target/release/nova.exe не собран)"
    exit 0
fi

FIXDIR="$ROOT/novac/fixtures"
ALLOW="$ROOT/novac/divergences.allow"
T="${TMPDIR:-/tmp}/novac-differential.$$"
mkdir -p "$T"
trap 'rm -rf "$T"' 0

if [ -d "$FIXDIR" ]; then
    find "$FIXDIR" -type f -name 'pos_*.nv' | sort > "$T/list"
else
    : > "$T/list"
fi
N=$(wc -l < "$T/list" | tr -d ' ')
if [ "$N" -eq 0 ]; then
    echo "$NAME ok: судить нечего (0 фикстур pos_*.nv в novac/fixtures)"
    exit 0
fi

bad=0
allowed=0
while IFS= read -r f; do
    rel=${f#"$ROOT"/}
    if "$BIN" check "$f" >/dev/null 2>&1 </dev/null; then b="принял"; else b="отверг"; fi
    if "$ORACLE" check "$f" >/dev/null 2>&1 </dev/null; then o="принял"; else o="отверг"; fi
    if [ "$b" != "$o" ]; then
        if [ -f "$ALLOW" ] && grep -Fxq "$rel" "$ALLOW"; then
            allowed=$((allowed+1))
        else
            printf '  %s: novac %s, оракул %s\n' "$rel" "$b" "$o" >> "$T/bad"
            bad=$((bad+1))
        fi
    fi
done < "$T/list"

if [ "$bad" -gt 0 ]; then
    echo "$NAME: FAIL — расхождений с оракулом вне novac/divergences.allow: $bad" >&2
    cat "$T/bad" >&2
    echo "  Чинить: либо баг novac (чинится той же волной, обходы запрещены)," >&2
    echo "  либо осознанное расхождение — тогда строка-путь в" >&2
    echo "  novac/divergences.allow + запись в docs/dev/novac-divergences.md" >&2
    echo "  (план 274 §10.3а)." >&2
    exit 1
fi
echo "$NAME ok: фикстур $N, исходы совпали с оракулом (в allow: $allowed)"
exit 0
