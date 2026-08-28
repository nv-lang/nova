#!/usr/bin/env bash
# scripts/guards/check-agent-definitions-wired.sh
# Определение субагента обязано быть НАЗВАНО командой, а команда — указывать на
# существующее определение. Связь в обе стороны.
#
# ДОМ И ОСНОВАНИЕ: план 276 шаг 2; вопрос владельца 2026-08-29 — «как агенты
# узнают о существовании `spec-reader` и какие механизмы принуждения есть?».
#
# ЗАЧЕМ. Честный ответ на тот вопрос: ПРИНУДИТЬ взять нужного агента машиной
# нельзя — гейт судит дерево, а вызов агента в дереве не остаётся. Значит
# машине достаётся не намерение, а СВЯЗНОСТЬ: чтобы окно могло узнать про
# агента, определение должно быть названо там, куда окно смотрит (команда), а
# команда не должна звать агента, которого нет. Обе половины ломаются молча:
# определение, о котором не сказано ни в одной команде, не будет найдено
# никогда; команда, зовущая несуществующий тип, отказывает в момент вызова, то
# есть посреди работы.
#
# ПРОВЕРЯЕТ ДВЕ ВЕЩИ:
#   1. каждый файл `.claude/agents/X.md` упомянут хотя бы в одном файле
#      `.claude/commands/*.md` (по имени `X`);
#   2. каждый `subagent_type: "X"` или `subagent_type: X`, встреченный в
#      `.claude/commands/*.md`, имеет файл `.claude/agents/X.md`.
#
# ЧЕГО НЕ ПРОВЕРЯЕТ (сказано прямо, чтобы не создавать ложного чувства
# защищённости): что агента ДЕЙСТВИТЕЛЬНО берут вместо самодельного чтения.
# Это не проверяемо из дерева ни этим стражем, ни любым другим. Осведомление
# держится на скилле `/read-spec` и разделе «Агент-читатель» в `/delegate`;
# доказуемая же часть — только артефакты доклада (см. `guard-stop.py`, условие
# про строку «Модели агентов:»).
#
# ИСПОЛЬЗОВАНИЕ:
#   bash scripts/guards/check-agent-definitions-wired.sh [КОРЕНЬ]
# Самотест — scripts/guards/selftest/test-check-agent-definitions-wired.sh

set -u
export LC_ALL=C

ROOT="${1:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}"
cd "$ROOT" || { echo "check-agent-definitions-wired: нет каталога $ROOT" >&2; exit 1; }

AG=".claude/agents"
CMD=".claude/commands"

if [ ! -d "$AG" ] && [ ! -d "$CMD" ]; then
    echo "check-agent-definitions-wired ok: судить нечего — ни $AG, ни $CMD нет"
    exit 0
fi

ORPHAN=""
NDEF=0
if [ -d "$AG" ]; then
    for a in "$AG"/*.md; do
        [ -f "$a" ] || continue
        name=$(basename "$a" .md)
        NDEF=$((NDEF + 1))
        if [ -d "$CMD" ] && grep -rql -- "$name" "$CMD" 2>/dev/null; then
            :
        else
            ORPHAN="$ORPHAN $name"
        fi
    done
fi

DANGLING=""
NREF=0
if [ -d "$CMD" ]; then
    refs=$(grep -rhoE 'subagent_type: *"?[A-Za-z0-9_-]+"?' "$CMD" 2>/dev/null \
           | sed -E 's/.*subagent_type: *"?([A-Za-z0-9_-]+)"?.*/\1/' | sort -u)
    for r in $refs; do
        NREF=$((NREF + 1))
        [ -f "$AG/$r.md" ] || DANGLING="$DANGLING $r"
    done
fi

norph=0; for m in $ORPHAN; do norph=$((norph + 1)); done
ndang=0; for m in $DANGLING; do ndang=$((ndang + 1)); done

echo "check-agent-definitions-wired: определений $NDEF, ссылок из команд $NREF, безымянных $norph, висячих $ndang"

if [ "$norph" -ne 0 ] || [ "$ndang" -ne 0 ]; then
    if [ "$norph" -ne 0 ]; then
        echo "check-agent-definitions-wired: НАРУШЕНИЕ — определение, о котором не говорит ни одна команда:" >&2
        for m in $ORPHAN; do echo "    $AG/$m.md" >&2; done
        echo "    Окно смотрит в команды, а не в каталог определений: про такого" >&2
        echo "    агента оно не узнает никогда, и напишет своё чтение заново." >&2
    fi
    if [ "$ndang" -ne 0 ]; then
        echo "check-agent-definitions-wired: НАРУШЕНИЕ — команда зовёт агента без определения:" >&2
        for m in $DANGLING; do echo "    subagent_type: $m (нет $AG/$m.md)" >&2; done
        echo "    Такой вызов отказывает В МОМЕНТ работы, а не на гейте." >&2
    fi
    echo "check-agent-definitions-wired: FAIL" >&2
    exit 1
fi

echo "check-agent-definitions-wired ok: каждое определение названо командой, каждая ссылка ведёт в существующий файл"
exit 0
