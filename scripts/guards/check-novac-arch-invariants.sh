#!/bin/sh
# scripts/guards/check-novac-arch-invariants.sh — счётчик инвариантов у каждого
# раздела карты архитектуры novac.
# План: docs/plans/274.1-novac-architecture.md §2б; зонтик docs/plans/274-novac-self-hosted-compiler.md.
#
# ПРАВИЛО (274.1 §2б, норма 253/conventions-governance): каждый раздел КАРТЫ
# (§1–§10 в docs/dev/novac-architecture.md — слои, модули, рёбра, информация,
# идентичность, дерево, требования, диагностика, семантика, кодоген, атомики)
# называет свои инварианты и несёт строку «Счётчик: N». Минимизация инвариантов
# начинается со СЧЁТА; раздел без счётчика — инварианты на честном слове (№636).
#
# Разведочные/эскизные разделы (§11+) счётчику не подсудны — у них нет своей
# конструкции. Страж проверяет наличие счётчика, не его минимальность —
# минимальность судит приёмка (три доказательства, check-novac-arch-class-proofs).
#
# $1 — корень репо; $2 — override файла (самотест).
# Проверялся: Windows (Git Bash), 2026-08-14.
export LC_ALL=C
ROOT="${1:-$(cd "$(dirname "$0")/../.." && pwd)}"
DOC="${2:-$ROOT/docs/dev/novac-architecture.md}"

if [ ! -f "$DOC" ]; then
    echo "check-novac-arch-invariants: FAIL — нет $DOC" >&2
    exit 1
fi

BAD=$(awk '
    /^## ([1-9]|10)[аб]?\./ {
        if (sec != "" && !cnt) { print "  " sec; bad++ }
        sec=$0; cnt=0
        if (sec ~ /^## (1[1-9])\./) sec=""   # §11+ не подсудны
        next
    }
    /^## /  { if (sec != "" && !cnt) { print "  " sec; bad++ }; sec="" }
    sec != "" && /Счётчик( раздела)?: *\*{0,2}[0-9]/ { cnt=1 }
    END { if (sec != "" && !cnt) print "  " sec }
' "$DOC")

if [ -n "$BAD" ]; then
    echo "check-novac-arch-invariants: FAIL — разделы карты без счётчика инвариантов:" >&2
    printf '%s\n' "$BAD" >&2
    echo "  Каждый раздел карты (§1–§10) обязан считать свои инварианты" >&2
    echo "  (274.1 §2б; №636 — инварианты прозой не считаются существующими)." >&2
    exit 1
fi

N=$(grep -c 'Счётчик\( раздела\)\?: *\**[0-9]' "$DOC")
echo "check-novac-arch-invariants ok: счётчики на месте ($N строк счёта)"
exit 0
