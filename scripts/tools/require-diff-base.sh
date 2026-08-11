#!/usr/bin/env bash
# scripts/tools/require-diff-base.sh — вычислить diff-base (HEAD~1) для
# gate.sh-подпроверок, которым он нужен: `guide_same_commit` в
# check-doc-conventions.sh и rule5/rule1 в check-test-fixture-coverage.sh.
#
# ЗАЧЕМ (реестр 221.1 №586). Раньше `gate.sh` вычислял базу инлайн, например:
#   DOC_GUARD_BASE="$(git -C "$ROOT" rev-parse --verify -q HEAD~1 2>/dev/null || true)"
# и передавал результат ДАЛЬШЕ безусловно — даже если он оказался пустым.
# Обе подпроверки-получательницы при пустой базе легитимно ПРОПУСКАЮТ себя:
# это их собственное, документированное поведение для случая «мне явно не
# передали диапазон» (обе — кросс-репные инструменты; CI передаёт диапазон
# из события явно, см. .github/workflows/nova-gate.yml). Но когда САМ
# `gate.sh` не смог вычислить HEAD~1 на обычном дереве с историей — это НЕ
# «диапазон неприменим», а локальный сбой вычисления на стороне вызывающего,
# и он обязан быть виден как ОТКАЗ шага, а не тихо провалиться в пропуск
# получателя, который выглядит зелёным (ровно так гейт 2026-08-11 напечатал
# «guide_same_commit пропущен» вместо явной ошибки).
#
# Единственный легитимный случай пустой базы для ЭТОГО инструмента — у
# коммита нет родителя (репозиторий с единственным коммитом). Это тоже
# ОТКАЗ (exit 1), а не тихий пропуск: гейт не рассчитан на такие деревья, и
# молчание здесь так же вредно, как и любой другой необнаруженный сбой.
#
# ИСПОЛЬЗОВАНИЕ:
#   require-diff-base.sh <корень-репы>   — печатает HEAD~1 в stdout, exit 0;
#                                            либо ошибку в stderr, exit 1.
#   require-diff-base.sh --selftest
set -u
export LC_ALL=C

if [ "${1:-}" = "--selftest" ]; then
    SELF="$0"
    TMP="$(mktemp -d)"
    trap 'rm -rf "$TMP"' EXIT
    fails=0
    ok()  { echo "  ok: $*"; }
    bad() { echo "  ПРОВАЛ: $*"; fails=$((fails + 1)); }

    # 1. Обычный репозиторий с ≥2 коммитами: печатает валидный HEAD~1, exit 0.
    R2="$TMP/two-commits"
    mkdir -p "$R2"
    git -C "$R2" init -q
    git -C "$R2" -c user.email=t@t -c user.name=t commit -q --allow-empty -m one
    git -C "$R2" -c user.email=t@t -c user.name=t commit -q --allow-empty -m two
    OUT="$(bash "$SELF" "$R2" 2>"$TMP/err2")"
    RC=$?
    EXPECT="$(git -C "$R2" rev-parse HEAD~1)"
    if [ "$RC" -eq 0 ] && [ "$OUT" = "$EXPECT" ]; then
        ok "≥2 коммита — печатает HEAD~1, exit 0"
    else
        bad "≥2 коммита дали rc=$RC out=[$OUT], ожидалось rc=0 out=[$EXPECT]"
    fi

    # 2. Единственный коммит (нет родителя) — явный отказ, не пустая строка.
    R1="$TMP/one-commit"
    mkdir -p "$R1"
    git -C "$R1" init -q
    git -C "$R1" -c user.email=t@t -c user.name=t commit -q --allow-empty -m only
    OUT="$(bash "$SELF" "$R1" 2>"$TMP/err1")"
    RC=$?
    ERR="$(cat "$TMP/err1")"
    if [ "$RC" -ne 0 ] && [ -z "$OUT" ] && printf '%s' "$ERR" | grep -qi "не удалось"; then
        ok "единственный коммит — явный отказ (exit $RC), stdout пуст, причина названа"
    else
        bad "единственный коммит дал rc=$RC out=[$OUT] err=[$ERR], ожидался явный отказ с пустым stdout"
    fi

    # 3. Несуществующий путь — тоже отказ, не пустой успех.
    OUT="$(bash "$SELF" "$TMP/no-such-dir" 2>"$TMP/err3")"
    RC=$?
    if [ "$RC" -ne 0 ] && [ -z "$OUT" ]; then
        ok "несуществующий корень — явный отказ"
    else
        bad "несуществующий корень дал rc=$RC out=[$OUT], ожидался явный отказ"
    fi

    if [ "$fails" -eq 0 ]; then
        echo "require-diff-base selftest: OK (3 проверки)"
        exit 0
    fi
    echo "require-diff-base selftest: ПРОВАЛ, отказов $fails" >&2
    exit 1
fi

ROOT="${1:-}"
[ -n "$ROOT" ] || { echo "require-diff-base: нужен корень репы первым аргументом" >&2; exit 1; }
[ -d "$ROOT" ] || { echo "require-diff-base: корень репы не существует: $ROOT" >&2; exit 1; }

BASE="$(git -C "$ROOT" rev-parse --verify -q HEAD~1 2>/dev/null || true)"
if [ -z "$BASE" ]; then
    echo "require-diff-base: не удалось вычислить HEAD~1 в $ROOT — либо это" >&2
    echo "  единственный коммит дерева (нет родителя), либо HEAD в непонятном" >&2
    echo "  состоянии. Подпроверки, ждущие diff-base (guide_same_commit," >&2
    echo "  rule5/rule1), не смогут выполниться — это ОТКАЗ вызывающего," >&2
    echo "  а не «диапазон неприменим»." >&2
    exit 1
fi
printf '%s\n' "$BASE"
exit 0
