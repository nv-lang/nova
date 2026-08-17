#!/usr/bin/env bash
# Самотест check-retracted-try-semantics.sh — обе стороны, на фикстурном корне.
#
# Страж, который не умеет краснеть, зелен ни о чём (класс F1, реестр №645).
# Поэтому здесь проверяются ОБА направления, и отдельно — те формы, которые
# страж ловить НЕ должен: законная D196 `consume X = expr? { body }`, оператор
# `??`, слово `desugar` и строки, которые сами помечают форму снятой.
set -u
export LC_ALL=C
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-retracted-try-semantics.sh"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); echo "  ok   $1"; }
bad() { FAIL=$((FAIL+1)); echo "  FAIL $1" >&2; }
check(){ if [ "$2" = "$3" ]; then ok "$1"; else bad "$1 (ждал '$3', получил '$2')"; fi; }

FIX="$TMP/root"
mkdir -p "$FIX/docs/guide" "$FIX/docs/plans" "$FIX/docs/dev" "$FIX/spec" "$FIX/scripts/guards"
cp "$ROOT/scripts/guards/retracted-try-scan.py" "$FIX/scripts/guards/"
mk_base() { printf 'spec=%s\nplans=%s\ndev=%s\n' "$1" "$2" "$3" > "$FIX/scripts/guards/retracted-try.baseline"; }
clean() { : > "$FIX/docs/guide/g.md"; : > "$FIX/docs/plans/p.md"; : > "$FIX/docs/dev/d.md"; : > "$FIX/spec/s.md"; }

echo "== проходит =="
clean; mk_base 0 0 0
sh "$G" "$FIX" >/dev/null 2>&1
check "чистое дерево — зелёный" "$?" "0"

clean
{ echo '```nova'; echo 'fn read(p str) -> Result[str, IoError] {'; echo '    ro raw = open(p)?'; echo '    Ok(raw)'; echo '}'; echo '```'; } > "$FIX/docs/guide/g.md"
sh "$G" "$FIX" >/dev/null 2>&1
check "канон D85 (\`?\` в fn, возвращающей Result) — зелёный" "$?" "0"

clean
{ echo '```nova'; echo 'fn read(p str) Fail[IoError] -> str {'; echo '    consume f = File.open(p)? {'; echo '        f.read_all()!!'; echo '    }'; echo '}'; echo '```'; } > "$FIX/docs/guide/g.md"
sh "$G" "$FIX" >/dev/null 2>&1
check "законная форма D196 'consume X = expr? { }' — НЕ ловится" "$?" "0"

clean
printf 'Оператор `expr ?? fb` подставляет запасное значение.\n' > "$FIX/docs/guide/g.md"
sh "$G" "$FIX" >/dev/null 2>&1
check "живой оператор \`??\` — не ловится" "$?" "0"

clean
printf 'Здесь `?` desugar'"'"'ится в `match` + ранний `return None`.\n' > "$FIX/docs/guide/g.md"
sh "$G" "$FIX" >/dev/null 2>&1
check "слово 'desugar' — не считается словом 'sugar'" "$?" "0"

clean
printf 'Раньше `?` был сахаром над `throw` — СНЯТО амендментом D85.\n' > "$FIX/docs/guide/g.md"
sh "$G" "$FIX" >/dev/null 2>&1
check "строка, которая САМА помечает форму снятой — не ловится" "$?" "0"

echo "== ловит =="
clean
printf 'Оператор `?` — это сахар над `throw`.\n' > "$FIX/docs/guide/g.md"
sh "$G" "$FIX" >/dev/null 2>&1
check "лексическое: снятая трактовка в ПУБЛИКУЕМОМ руководстве — красный" "$?" "1"

clean
{ echo '```nova'; echo 'fn read(p str) Fail[IoError] -> str {'; echo '    ro raw = f.read_all()?'; echo '    raw'; echo '}'; echo '```'; } > "$FIX/docs/guide/g.md"
sh "$G" "$FIX" >/dev/null 2>&1
check "структурное: \`?\` внутри Fail-функции в руководстве — красный" "$?" "1"

clean
printf 'Оператор `?` — это сахар над `throw`.\n' > "$FIX/spec/s.md"
mk_base 0 0 0
sh "$G" "$FIX" >/dev/null 2>&1
check "рост осадка в spec выше базы — красный" "$?" "1"

mk_base 1 0 0
sh "$G" "$FIX" >/dev/null 2>&1
check "тот же осадок в пределах базы — зелёный" "$?" "0"

clean
mk_base 0 0 0
rm -f "$FIX/scripts/guards/retracted-try.baseline"
sh "$G" "$FIX" >/dev/null 2>&1
check "нет файла базы — красный (а не 'считаем ноль')" "$?" "1"

mk_base 0 0 0
mv "$FIX/scripts/guards/retracted-try-scan.py" "$FIX/scripts/guards/retracted-try-scan.py.off"
sh "$G" "$FIX" >/dev/null 2>&1
check "нет ядра — красный (а не тихое 'ноль нарушений')" "$?" "1"
mv "$FIX/scripts/guards/retracted-try-scan.py.off" "$FIX/scripts/guards/retracted-try-scan.py"

echo
echo "selftest check-retracted-try-semantics: PASS=$PASS FAIL=$FAIL"
[ "$FAIL" -eq 0 ] || exit 1
exit 0
