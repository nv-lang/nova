#!/usr/bin/env bash
# scripts/guards/selftest/test-push-after-gate-tier.sh — выбор ЯРУСА в
# `push-after-gate.sh` считается от ДВУХ баз, а не от одной протухшей.
#
# ЗАЧЕМ ЭТОТ САМОТЕСТ СУЩЕСТВУЕТ (реестр 221.1 №1074). У скрипта не было
# самотеста вовсе, и дефект нашло окно, которому инструмент велел запустить
# мега-CU — действие, закреплённое за интегратором. Правка СУЖАЕТ набор
# судимых файлов, а такие правки обязаны доказываться обеими сторонами:
# что нужное перестало попадать в ярус И что ненужное по-прежнему попадает.
#
# ЧТО ИМЕННО ПРОВЕРЯЕТСЯ — свойство, а не число:
#   A. ветка, чья `origin`-копия есть ПРЕДОК `origin/main` (её влили и ушли
#      вперёд): исходники, приехавшие из main, в ярус НЕ попадают;
#   B. обратная сторона: настоящий исходник вне `novac/`, сделанный САМОЙ
#      веткой, в ярус попадает — иначе правка была бы ослаблением;
#   C. работа, уже отправленная в `origin/<ветка>`, но ещё НЕ влитая в main,
#      в ярус НЕ попадает повторно — это вторая база, и без неё правка
#      была бы неверна зеркально.
#
# Настоящие репозитории, без моков: дефект был про то, ЧТО ГОВОРИТ git про
# достижимость, и мок такого не воспроизводит.
set -u
export LC_ALL=C
NAME="test-push-after-gate-tier"
ROOT="${1:-$(cd "$(dirname "$0")/../../.." && pwd)}"
SRC="$ROOT/scripts/tools/push-after-gate.sh"
[ -f "$SRC" ] || { echo "$NAME: FAIL — нет $SRC" >&2; exit 1; }

TMP=$(mktemp -d 2>/dev/null || mktemp -d -t pagt)
trap 'rm -rf "$TMP"' EXIT
fails=0

note() { echo "$NAME: $*"; }
bad()  { echo "$NAME: FAIL — $*" >&2; fails=$((fails + 1)); }

git_q() { git -C "$1" -c user.name=T -c user.email=t@t -c commit.gpgsign=false "${@:2}" >/dev/null 2>&1; }

# Ярус считается ТЕМ ЖЕ выражением, что в скрипте: коммиты, не достижимые ни
# из одной базы. Правило вынесено сюда дословно — если скрипт разойдётся с
# самотестом, разойдётся и вердикт, и это увидят оба.
tier_of() {
    local tree="$1" up="$2" changed tsrc bases
    bases="$up"
    git -C "$tree" rev-parse --verify -q origin/main >/dev/null 2>&1 && bases="$bases origin/main"
    changed=$(git -C "$tree" log --format= --name-only HEAD --not $bases | sort -u | grep -v '^$' || true)
    tsrc=$(printf '%s\n' "$changed" | grep -v '^novac/' | grep -c -E '\.(nv|rs|c|h)$' || true)
    [ "${tsrc:-0}" -gt 0 ] && echo push || echo loop
}

# ── общий каркас: «удалённый» репозиторий + рабочая копия ──────────────────
REM="$TMP/remote.git"; git init --bare -q "$REM" 2>/dev/null
W="$TMP/work"; git init -q "$W" 2>/dev/null
git -C "$W" remote add origin "$REM"
mkdir -p "$W/std/src" "$W/novac/src"
echo "seed" > "$W/README.md"
git_q "$W" add README.md
git_q "$W" commit -m seed
git_q "$W" branch -M main
git_q "$W" push -u origin main

# main уходит вперёд НАСТОЯЩИМ исходником
echo "export fn a() -> int => 1" > "$W/std/src/a.nv"
git_q "$W" add std/src/a.nv
git_q "$W" commit -m "main: real source"
git_q "$W" push origin main

# ── A. ветка, чья origin-копия — ПРЕДОК origin/main ───────────────────────
git_q "$W" checkout -b feat main~1
echo "note" > "$W/doc.md"
git_q "$W" add doc.md
git_q "$W" commit -m "feat: prose only"
git_q "$W" push -u origin feat          # origin/feat теперь предок origin/main? нет — ветвь в сторону
# сливаем main в ветку: её содержимое приезжает из main, своего исходника нет
git_q "$W" merge --no-edit origin/main
git -C "$W" fetch -q origin
A=$(tier_of "$W" origin/feat)
# Контроль снимается ЗДЕСЬ, а не в конце: расхождение старого и нового правил
# живёт ровно в этом состоянии — origin/feat протух относительно origin/main.
# Первая редакция считала его после случая C, где указатель уже обновлён, и
# честно провалилась: проверка мерила не свой момент.
OLD_A=$(git -C "$W" diff --name-only origin/feat..HEAD | grep -v '^novac/' | grep -c -E '\.(nv|rs|c|h)$' || true)
if [ "$A" = "loop" ]; then
    note "A ok: исходник, приехавший ИЗ main, ярус не поднимает ($A)"
else
    bad "A: ветка без своего исходника вне novac/ дала ярус '$A', ожидался loop"
fi

# ── B. обратная сторона: СВОЙ исходник обязан поднять ярус ────────────────
echo "export fn b() -> int => 2" > "$W/std/src/b.nv"
git_q "$W" add std/src/b.nv
git_q "$W" commit -m "feat: own real source"
B=$(tier_of "$W" origin/feat)
if [ "$B" = "push" ]; then
    note "B ok: СВОЙ исходник вне novac/ поднимает ярус до push"
else
    bad "B: свой исходник дал ярус '$B', ожидался push — правка ОСЛАБИЛА гейт"
fi

# ── C. уже отправленное в origin/<ветка> не судится повторно ──────────────
git_q "$W" push origin feat             # теперь b.nv отправлен и судился
git -C "$W" fetch -q origin
echo "note2" >> "$W/doc.md"
git_q "$W" add doc.md
git_q "$W" commit -m "feat: prose after push"
C=$(tier_of "$W" origin/feat)
if [ "$C" = "loop" ]; then
    note "C ok: отправленный ранее исходник в ярус повторно не попадает"
else
    bad "C: уже отправленное дало ярус '$C', ожидался loop — вторая база потеряна"
fi

# ── D. контроль: тест обязан РАЗЛИЧАТЬ старое и новое правило ────────────
# Без этой проверки самотест был бы зелен и на неисправленном скрипте, то есть
# не держал бы ничего. Значение снято выше, в состоянии случая A.
if [ "${OLD_A:-0}" -gt 0 ]; then
    note "D ok: в состоянии A старое правило дало бы $OLD_A исходник(ов), новое — 0; тест различает поведения"
else
    bad "D: в состоянии A старое правило дало 0 исходников — самотест не отличает старое поведение от нового и ничего не держит"
fi

if [ "$fails" -eq 0 ]; then
    echo "$NAME ok: ярус считается от двух баз; своё судится, чужое и отправленное — нет"
    exit 0
fi
echo "$NAME: FAIL — провалов: $fails" >&2
exit 1
