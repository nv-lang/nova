#!/usr/bin/env bash
# Селфтест scripts/guards/check-expect-markers.sh (№453 часть 3).
set -u
export LC_ALL=C

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
G="$ROOT/scripts/guards/check-expect-markers.sh"
FAILED=0
CASES=0
ok()  { CASES=$((CASES+1)); echo "  ok: $1"; }
bad() { CASES=$((CASES+1)); echo "  ПРОВАЛ: $1" >&2; FAILED=1; }

echo "== селфтест check-expect-markers =="

# 1. Реальная репа — зелено (иначе страж непригоден к подключению).
if bash "$G" "$ROOT" >/dev/null 2>&1; then
    ok "реальная репа: неизвестных маркеров нет"
else
    bad "реальная репа краснит"
fi

T=$(mktemp -d); mkdir -p "$T/spec_tests"

# 2. Ловит неизвестный маркер.
printf '// EXPECT_VYDUMANNYJ: ok\nmodule x\n' > "$T/spec_tests/a.nv"
if bash "$G" "$T" >/dev/null 2>&1; then
    bad "НЕ поймал неизвестный EXPECT_*"
else
    ok "ловит неизвестный EXPECT_* (код 1)"
fi

# 3. Известные маркеры принимаются — включая EXPECT_TIMEOUT_MS (бюджет, не лейн).
printf '// EXPECT_STDOUT: ok\n// EXPECT_TIMEOUT_MS 30000\nmodule x\n' > "$T/spec_tests/a.nv"
if bash "$G" "$T" >/dev/null 2>&1; then
    ok "принимает известные, включая EXPECT_TIMEOUT_MS"
else
    bad "ложно краснит на известных маркерах"
fi

# 4. Упоминание в прозе НЕ в начале строки — не маркер, не ловится.
printf 'module x\n// см. описание EXPECT_STDOUT в конвенции\n' > "$T/spec_tests/a.nv"
if bash "$G" "$T" >/dev/null 2>&1; then
    ok "не ловит упоминание внутри прозы"
else
    bad "ложно краснит на упоминании в прозе"
fi

rm -rf "$T"

# 5. №473: KNOWN обязан быть ВЫВЕДЕН из test_runner.rs, а не храниться копией
# в страже. Проба — не чтение исходного текста стража (это доказало бы
# только «где-то есть переменная», не «откуда она берётся»), а поведенческая:
# берём копию РЕАЛЬНОГО дерева, ВЫРЕЗАЕМ из СКОПИРОВАННОГО test_runner.rs
# литерал одного известного маркера и смотрим, перестаёт ли страж (запущенный
# на этой копии) считать этот маркер известным. Если бы KNOWN был
# захардкожен в самом check-expect-markers.sh, вырезание строки из копии
# раннера ничего бы не изменило — маркер остался бы известным.
GDIR=$(mktemp -d)
mkdir -p "$GDIR/scripts/guards" "$GDIR/compiler-codegen/src" "$GDIR/spec_tests"
cp "$ROOT/scripts/guards/check-expect-markers.sh" "$GDIR/scripts/guards/"
# Копия раннера БЕЗ EXPECT_LINT_WARNING (единственное вхождение — strip_prefix).
grep -v 'EXPECT_LINT_WARNING' "$ROOT/compiler-codegen/src/test_runner.rs" \
    > "$GDIR/compiler-codegen/src/test_runner.rs"
printf '// EXPECT_LINT_WARNING W_REDUNDANT_PAREN\nmodule x\n' > "$GDIR/spec_tests/a.nv"
if bash "$GDIR/scripts/guards/check-expect-markers.sh" "$GDIR" >/dev/null 2>&1; then
    bad "KNOWN не зависит от исходника раннера — вырезанный маркер всё ещё 'известен' (страж держит копию, а не выводит список)"
else
    ok "KNOWN выведен из test_runner.rs: вырезанный маркер стал неизвестным"
fi
rm -rf "$GDIR"

# 6. Исходника раннера нет вовсе — явный отказ, а не тихое «маркеров нет».
GDIR2=$(mktemp -d)
mkdir -p "$GDIR2/scripts/guards" "$GDIR2/spec_tests"
cp "$ROOT/scripts/guards/check-expect-markers.sh" "$GDIR2/scripts/guards/"
printf 'module x\n' > "$GDIR2/spec_tests/a.nv"
OUT=$(bash "$GDIR2/scripts/guards/check-expect-markers.sh" "$GDIR2" 2>&1)
CODE=$?
if [ "$CODE" -ne 0 ] && printf '%s' "$OUT" | grep -q "не найден исходник раннера"; then
    ok "нет test_runner.rs рядом со стражем — явный отказ, а не тихий 'ok'"
else
    bad "отсутствие исходника раннера не дало явного отказа (code=$CODE): $OUT"
fi
rm -rf "$GDIR2"

if [ "$FAILED" -eq 0 ]; then
    echo "селфтест check-expect-markers: $CASES/$CASES ok"
    exit 0
fi
echo "селфтест check-expect-markers: ЕСТЬ ПРОВАЛЫ" >&2
exit 1
