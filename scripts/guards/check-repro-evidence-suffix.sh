#!/usr/bin/env bash
# scripts/guards/check-repro-evidence-suffix.sh — улики хранятся как `.nv.txt`.
#
# ЗАЧЕМ (правило №695, шапка `docs/plans/repro/README.md`, п. 2). Улика — не
# фикстура. Фикстура обязана быть зелёной; улика обязана ВОСПРОИЗВОДИТЬ дефект и
# потому часто красная. Суффикс `.nv.txt` держит её вне раннера, линта и стражей,
# которые ходят по `*.nv`, и делает ПЕРЕЕЗД улики в тест видимым: улика,
# ставшая фикстурой, меняет имя и уезжает в `spec_tests/`.
#
# ПОЧЕМУ СТРАЖ ЗАВЕДЁН ТОЛЬКО СЕЙЧАС, спустя месяцы после правила. Замер
# 2026-09-04: под `docs/plans/repro/` лежало 57 файлов `.nv` без суффикса, и ни
# один страж суффикс не судил. Правило было документом.
#
# ОДНО ВОЗРАЖЕНИЕ ПРОТИВ ЭТОГО СТРАЖА НАДО НАЗВАТЬ, потому что оно верное и я
# сам его выдвинул, прежде чем завести. Обоснование правила («по `**/*.nv` ходят
# восемь стражей») сегодня НЕ ВОСПРОИЗВОДИТСЯ: полный гейт прошёл все ярусы при
# этих 57 файлах — те стражи ходят по `std/`, `spec_tests/`, `novac/`,
# `examples/`, по названным каталогам, а не по дереву целиком. То есть защита
# от подметания — угроза будущая, не сегодняшняя.
#
# Страж всё равно заведён, и вот почему. У правила ДВЕ причины, и вторая живая
# независимо от первой: читатель обязан отличать улику от фикстуры по имени, а
# не по каталогу. Сегодня в дереве красные `.nv`, неотличимые от тестов; первое
# же окно, расширившее любой из восьми стражей на `docs/`, получит красноту не
# от своей работы. Цена проверки — один `find` в самом дешёвом ярусе.
#
# ХРАПОВИК, А НЕ НОЛЬ, и по прямой причине: 57 существующих улик принадлежат
# чужим каталогам (`p274-6-*`, `p791`, `p889`), переименовывать их за авторов
# нельзя — ссылки на них живут в их планах и строках реестра. Число обязано
# ходить ТОЛЬКО ВНИЗ, и опускает его тот, кто переименовывает.
#
# ИСПОЛЬЗОВАНИЕ:
#   bash scripts/guards/check-repro-evidence-suffix.sh [КОРЕНЬ]
#   bash scripts/guards/check-repro-evidence-suffix.sh --selftest

set -u
export LC_ALL=C

SELFTEST=0
ROOT="."
for a in "$@"; do
  case "$a" in
    --selftest) SELFTEST=1 ;;
    *) ROOT="$a" ;;
  esac
done

NAME="check-repro-evidence-suffix"
BASELINE="$ROOT/scripts/guards/repro-evidence-suffix.baseline"

count_bare_nv() {
  find "$1/docs/plans/repro" -name '*.nv' -type f 2>/dev/null | grep -c . || true
}

read_baseline() {
  local v
  v=$(grep -E '^bare_nv=[0-9]+$' "$BASELINE" 2>/dev/null | tail -1 | cut -d= -f2)
  if [ -z "${v:-}" ]; then echo "MISSING"; else echo "$v"; fi
}

if [ "$SELFTEST" = "1" ]; then
  fail=0
  TMP="$(mktemp -d)"
  trap 'rm -rf "$TMP"' EXIT
  mkdir -p "$TMP/docs/plans/repro/x" "$TMP/scripts/guards"

  # (1) счётчик видит голый .nv и не считает .nv.txt
  : > "$TMP/docs/plans/repro/x/a.nv"
  : > "$TMP/docs/plans/repro/x/b.nv.txt"
  got=$(count_bare_nv "$TMP")
  if [ "$got" = "1" ]; then
    echo "  ok: считает голый .nv и НЕ считает .nv.txt (1 из 2)"
  else
    echo "  ПРОВАЛ: счётчик дал $got вместо 1"; fail=1
  fi

  # (2) рост — красный
  printf 'bare_nv=0\n' > "$TMP/scripts/guards/repro-evidence-suffix.baseline"
  if bash "$0" "$TMP" >/dev/null 2>&1; then
    echo "  ПРОВАЛ: рост 1 > 0 не покрашен"; fail=1
  else
    echo "  ok: рост над базой — красный"
  fi

  # (3) равенство базе — зелёный, без ложного срабатывания
  printf 'bare_nv=1\n' > "$TMP/scripts/guards/repro-evidence-suffix.baseline"
  if bash "$0" "$TMP" >/dev/null 2>&1; then
    echo "  ok: равенство базе — зелёный"
  else
    echo "  ПРОВАЛ: равенство базе покрашено"; fail=1
  fi

  # (4) падение — красный С ТРЕБОВАНИЕМ опустить базу (иначе храповик стоит)
  printf 'bare_nv=5\n' > "$TMP/scripts/guards/repro-evidence-suffix.baseline"
  if bash "$0" "$TMP" >/dev/null 2>&1; then
    echo "  ПРОВАЛ: падение прошло молча — база осталась бы высокой"; fail=1
  else
    echo "  ok: падение требует опустить базу, а не проходит молча"
  fi

  # (5) нет каталога — судить нечего, но НЕ тихо зелено
  rm -rf "$TMP/docs/plans/repro"
  printf 'bare_nv=0\n' > "$TMP/scripts/guards/repro-evidence-suffix.baseline"
  if bash "$0" "$TMP" >/dev/null 2>&1; then
    echo "  ok: каталога нет — судить нечего, зелёный"
  else
    echo "  ПРОВАЛ: отсутствие каталога покрашено"; fail=1
  fi

  # (6) базы нет — красный, а не тихо зелёный (№813)
  mkdir -p "$TMP/docs/plans/repro"
  rm -f "$TMP/scripts/guards/repro-evidence-suffix.baseline"
  if bash "$0" "$TMP" >/dev/null 2>&1; then
    echo "  ПРОВАЛ: базы нет, а страж промолчал"; fail=1
  else
    echo "  ok: базы нет — красный, а не тихо зелёный"
  fi

  echo "селфтест $NAME: $([ "$fail" = "0" ] && echo "6/6 ok" || echo "ЕСТЬ ПРОВАЛЫ")"
  [ "$fail" = "0" ] || exit 1
  exit 0
fi

if [ ! -d "$ROOT/docs/plans/repro" ]; then
  echo "$NAME ok: каталога улик нет — судить нечего"
  exit 0
fi

N=$(count_bare_nv "$ROOT")
BASE=$(read_baseline)
if [ "$BASE" = "MISSING" ]; then
  echo "$NAME: FAIL — база не читается ($BASELINE); нужна строка bare_nv=N"
  exit 1
fi

echo "$NAME: улик с голым .nv — $N (база $BASE)"

if [ "$N" -gt "$BASE" ]; then
  echo "$NAME: FAIL — голых .nv стало БОЛЬШЕ: $BASE -> $N"
  echo "    Улика хранится как \`.nv.txt\` (правило №695, docs/plans/repro/README.md п.2):"
  echo "    фикстура обязана быть зелёной, улика обязана ВОСПРОИЗВОДИТЬ дефект и потому"
  echo "    часто красная. Суффикс держит её вне стражей, ходящих по \`*.nv\`, и делает"
  echo "    переезд улики в тест видимым переименованием."
  exit 1
fi
if [ "$N" -lt "$BASE" ]; then
  echo "$NAME: FAIL — число УПАЛО ($N < $BASE): опусти базу тем же слиянием,"
  echo "    строкой-летописью, как заведено в самом файле базы. Храповик, оставленный"
  echo "    высоким, молча разрешает вернуть ровно столько, сколько ты убрал."
  exit 1
fi
echo "$NAME ok: роста нет"
exit 0
