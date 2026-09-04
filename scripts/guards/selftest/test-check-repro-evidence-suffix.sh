#!/usr/bin/env bash
# scripts/guards/selftest/test-check-repro-evidence-suffix.sh
#
# Самотест стража суффикса улик (правило №695).
#
# ЗАЧЕМ ОТДЕЛЬНЫЙ ФАЙЛ ПРИ НАЛИЧИИ `--selftest` У САМОГО СТРАЖА. Внутренний
# режим проверяет свойства на поддельных деревьях — свои кирпичи. Он не может
# проверить двух вещей, ради которых этот файл и существует:
#   * что страж ЗЕЛЁН НА НАСТОЯЩЕМ дереве (страж, краснеющий на здоровом
#     репозитории, будет отключён первым же окном, и тогда он не ловит ничего);
#   * что он краснеет на дереве, отличающемся от настоящего РОВНО одной
#     подложенной уликой, — то есть измеряет предмет, а не окрестность.
# Плюс `check-guard-wiring` ищет именно файл: страж без самотеста в каталоге
# считается непокрытым, чем бы он ни хвастался внутри.

set -u
export LC_ALL=C

ROOT_REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
GUARD="$ROOT_REPO/scripts/guards/check-repro-evidence-suffix.sh"
PASS=0
FAIL=0
ok()  { echo "  ok: $1"; PASS=$((PASS + 1)); }
bad() { echo "  ПРОВАЛ: $1"; FAIL=$((FAIL + 1)); }

# 1. внутренние свойства
if bash "$GUARD" --selftest >/dev/null 2>&1; then
    ok "внутренний --selftest зелёный (семь свойств)"
else
    bad "внутренний --selftest красный"
fi

# 2. настоящее дерево — зелёное
if bash "$GUARD" "$ROOT_REPO" >/dev/null 2>&1; then
    ok "настоящая репа зелёная (база совпадает с деревом)"
else
    bad "настоящая репа покрашена — база разошлась с деревом"
fi

# 3. одна подложенная улика — красно. Дерево копируется НЕ целиком: нужен
#    только каталог улик и файл базы, и это делает случай отличающимся от
#    настоящего ровно одним файлом.
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
mkdir -p "$TMP/docs/plans/repro/901-planted" "$TMP/scripts/guards"
# С НАСТОЯЩЕЙ базой стража и одной уликой ПРАВИЛЬНОГО суффикса дерево обязано
# быть зелёным. Случай стоит именно с настоящей базой, а не с придуманной: он
# ловит расхождение базы с деревом, которое иначе заметит только гейт.
REAL=$(grep -E '^bare_nv=[0-9]+$' "$ROOT_REPO/scripts/guards/repro-evidence-suffix.baseline" | tail -1)
printf '%s\n' "$REAL" > "$TMP/scripts/guards/repro-evidence-suffix.baseline"
: > "$TMP/docs/plans/repro/901-planted/evidence.nv.txt"
if bash "$GUARD" "$TMP" >/dev/null 2>&1; then
    ok "правильный суффикс при настоящей базе -- зелено ($REAL)"
else
    bad "правильный суффикс красит (ложное срабатывание) при базе $REAL"
fi

# И отдельно — что падение ниже базы всё-таки красное. Раньше это свойство
# проверялось ПОБОЧНО, потому что настоящая база была 55, а дерево пустым; когда
# база стала нулём, случай молча перестал его проверять. Теперь база задаётся
# явно, и свойство не зависит от того, чему равна настоящая.
printf 'bare_nv=5\n' > "$TMP/scripts/guards/repro-evidence-suffix.baseline"
if bash "$GUARD" "$TMP" >/dev/null 2>&1; then
    bad "падение ниже базы прошло молча -- храповик остался бы высоким"
else
    ok "падение ниже базы красное (храповик не оставляют высоким)"
fi
printf 'bare_nv=0\n' > "$TMP/scripts/guards/repro-evidence-suffix.baseline"
if bash "$GUARD" "$TMP" >/dev/null 2>&1; then
    ok "только '.nv.txt' при базе 0 — зелено"
else
    bad "'.nv.txt' при базе 0 покрашен"
fi
: > "$TMP/docs/plans/repro/901-planted/evidence.nv"
if bash "$GUARD" "$TMP" >/dev/null 2>&1; then
    bad "подложенная улика '.nv' НЕ покрашена"
else
    ok "одна подложенная улика '.nv' — красно"
fi

echo "test-check-repro-evidence-suffix: $PASS/$((PASS + FAIL)) ok"
[ "$FAIL" -eq 0 ] || exit 1
exit 0
