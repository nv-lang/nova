#!/bin/sh
# scripts/guards/check-novac-differential.sh — дифференциальный прогон novac
# против оракула.
#
# ПРАВИЛО (план 274 §10.3, §10.3а: «контракт = оракулу; расхождения только из
# реестра»): novac обязан принимать/отвергать те же программы, что нынешний
# компилятор (оракул nova-cli/target/release/nova.exe check). Страж прогоняет
# оба бинаря по novac/fixtures/**/pos_*.nv и сравнивает ИСХОД (принял/отверг,
# а с 2026-08-16 у принятых обеими сторонами — ещё и ОТВЕТ: stdout и код
# возврата через смоук, потому что одинаковый вердикт при разном значении
# зелёным быть не должен (274.3/F18: `1 + 2 * 3` считалось как 9),
# по коду возврата). Расхождение, не записанное в novac/divergences.allow
# (строка = путь фикстуры от корня, с прямыми слэшами), — красное.
#
# НЕ проверяет: совпадение текстов/кодов диагностик (только исход),
# поведение на neg_* (их судят diag-schema и no-cascade), обоснованность
# записей allow — её судит приёмка и docs/dev/novac-divergences.md.
# Контракт вызова: '<bin> check <file>'; если CLI novac окажется иным —
# страж правится тем же коммитом, что вводит бинарь.
#
# С Э2 (274 §10.4) страж — ещё и судья храповика НА ПРОГРЕСС: зовёт
# scripts/tools/novac-diff-corpus.sh (корпус examples/), парсит его машинную
# строку baseline-numbers и сверяет с novac-corpus.baseline В ОБЕ СТОРОНЫ:
# меньше базы — откат (красный), больше базы — рост без поднятия базы тем же
# коммитом (тоже красный). Корзины «вне точки»/«заблокировано оракулом»/
# «расстояние до самосборки» печатаются раннером рядом с базой (§10.4).
# БЮДЖЕТ: корпусная часть ~2–6 мин (замер 2026-08-14: 334с под нагрузкой,
# из них ~200с — поведенческие смоуки). Для быстрых локальных итераций:
# NOVAC_CORPUS=0 отключает корпусную часть (фикстуры остаются); отключение
# в ГЕЙТЕ — только сознательным решением интегратора, не тихим дефолтом.
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
. "$(dirname "$0")/lib/novac.sh"

novac_require_bin "$NAME" "$ROOT" "$BIN"

ORACLE="$ROOT/nova-cli/target/release/nova.exe"
if [ ! -f "$ORACLE" ]; then
    # Worktree без своего target: оракул главного дерева (приём №650 —
    # главная репа выводится из git, не из памяти).
    MAINROOT=$(git -C "$ROOT" rev-parse --path-format=absolute --git-common-dir 2>/dev/null)
    [ -n "$MAINROOT" ] && ORACLE="$MAINROOT/../nova-cli/target/release/nova.exe"
fi
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

# ---- ОТВЕТ, а не только вердикт (274.3/F18, урок 2026-08-16) -------------
# Вердикт «оба приняли» ничего не говорит о ЗНАЧЕНИИ. Живой случай: фикстура
# int_arith/pos_1.nv содержит `1 + 2 * 3 - 4`, novac парсил плоской цепью и
# считал 9 вместо 7 — и страж был зелёным, потому что сравнивал только
# «принял/отверг». Поэтому каждая принятая обеими сторонами фикстура ещё и
# ЗАПУСКАЕТСЯ через смоук: stdout и код возврата novac сверяются с оракулом
# байт в байт. Это дорого (сборка двух бинарей на фикстуру), поэтому шаг
# отключаем тем же переключателем, что и корпус.
if [ "${NOVAC_CORPUS:-1}" != "0" ]; then
    SMOKE="$ROOT/scripts/tools/novac-e1-smoke.sh"
    if [ -x "$SMOKE" ] || [ -f "$SMOKE" ]; then
        beh=0; behbad=0
        while IFS= read -r f; do
            rel=${f#"$ROOT"/}
            "$BIN" check "$f" >/dev/null 2>&1 </dev/null || continue
            "$ORACLE" check "$f" >/dev/null 2>&1 </dev/null || continue
            if bash "$SMOKE" "$f" >"$T/smoke.out" 2>&1; then
                beh=$((beh+1))
            else
                behbad=$((behbad+1))
                printf '  %s: поведение разошлось с оракулом\n' "$rel" >> "$T/behbad"
                sed 's/^/      /' "$T/smoke.out" | head -n 4 >> "$T/behbad"
            fi
        done < "$T/list"
        if [ "$behbad" -gt 0 ]; then
            echo "$NAME: FAIL — фикстура принята обоими, но ОТВЕТ разный: $behbad" >&2
            cat "$T/behbad" >&2
            echo "  Одинаковый вердикт при разном ответе — худший вид зелёного:" >&2
            echo "  подмножество имеет право ОТКАЗАТЬ, но не имеет права посчитать иначе." >&2
            exit 1
        fi
        echo "$NAME ok: поведение фикстур сверено с оракулом: $beh из $N (запусков байт-в-байт)"
    fi
fi

# ---- Храповик корпуса (274 §10.4; с Э2) ----------------------------------
if [ "${NOVAC_CORPUS:-1}" = "0" ]; then
    echo "$NAME: корпусная часть пропущена (NOVAC_CORPUS=0 — локальная итерация)"
    exit 0
fi
BASE="$ROOT/scripts/guards/novac-corpus.baseline"
if [ ! -f "$BASE" ]; then
    echo "$NAME ok: храповика ещё нет (novac-corpus.baseline отсутствует — Э1)"
    exit 0
fi
RUN="$ROOT/scripts/tools/novac-diff-corpus.sh"
if ! sh "$RUN" > "$T/corpus.out" 2>&1; then
    echo "$NAME: FAIL — корпусный прогон красный:" >&2
    tail -10 "$T/corpus.out" >&2
    exit 1
fi
NUMS=$(grep '^novac-diff-corpus baseline-numbers:' "$T/corpus.out")
cm=$(echo "$NUMS" | sed -n 's/.*contract-match=\([0-9]*\).*/\1/p')
bm=$(echo "$NUMS" | sed -n 's/.*behavior-match=\([0-9]*\).*/\1/p')
base_cm=$(tr -d '\r' < "$BASE" | sed -n 's/^contract-match \([0-9]*\)$/\1/p')
base_bm=$(tr -d '\r' < "$BASE" | sed -n 's/^behavior-match \([0-9]*\)$/\1/p')
if [ -z "$cm" ] || [ -z "$bm" ] || [ -z "$base_cm" ] || [ -z "$base_bm" ]; then
    echo "$NAME: FAIL — не распарсил числа храповика (прогон: '$NUMS'; база: cm='$base_cm' bm='$base_bm')" >&2
    exit 1
fi
grep -E '^novac-diff-corpus: (файлов|поведенчески|цена)' "$T/corpus.out" | sed "s/^/$NAME: /"
# Cost ratchet (P14): the corpus run's wall must stay within its budget.
wall_ms=$(sed -n 's/^novac-diff-corpus: цена прогона.*стена \([0-9]*\)ms.*/\1/p' "$T/corpus.out")
bud_ms=$(tr -d '\r' < "$ROOT/scripts/guards/novac-iteration-cost.baseline" 2>/dev/null | sed -n 's/^diff-corpus-ms \([0-9]*\)$/\1/p')
# Бюджет масштабируется загрузкой машины (274.3/F10, общая дверь
# lib/novac.sh): абсолютная стена под полным прогоном гейта даёт 146с
# против 50с на тихой машине — без поправки это ложный красный.
if [ -n "$wall_ms" ] && [ -n "$bud_ms" ]; then
    scale=$(novac_load_scale "$ROOT")
    ebud_ms=$(( bud_ms * scale / 100 ))
    if [ "$wall_ms" -gt "$ebud_ms" ]; then
        echo "$NAME: FAIL — ПРОСАДКА цены дифф-раннера: ${wall_ms}мс > бюджет ${bud_ms}мс (с поправкой на загрузку ${ebud_ms}мс, калибровка x${scale}/100; П14)" >&2
        exit 1
    fi
    echo "$NAME: цена дифф-раннера ${wall_ms}мс при бюджете ${ebud_ms}мс (калибровка x${scale}/100)"
fi
# --- Монотонность САМОГО файла базы (адверсарная проверка 2026-08-17) ------
# Храповик сверял ПРОГОН с файлом и ни разу — файл с его историей. Значит
# откат делался в одну строку: опусти число в базе, и прогон снова "== база",
# зелено. Ратчет, который можно отпустить рукой, — не ратчет. Сравниваем с
# версией файла в HEAD; допускается только рост.
BFILE="$ROOT/scripts/guards/novac-corpus.baseline"
HEAD_B=$(git -C "$ROOT" show "HEAD:scripts/guards/novac-corpus.baseline" 2>/dev/null || true)
if [ -n "$HEAD_B" ]; then
    for key in contract-match behavior-match; do
        was=$(printf '%s\n' "$HEAD_B" | sed -n "s/^$key[[:space:]][[:space:]]*\([0-9][0-9]*\).*/\1/p" | head -1)
        now=$(sed -n "s/^$key[[:space:]][[:space:]]*\([0-9][0-9]*\).*/\1/p" "$BFILE" 2>/dev/null | head -1)
        [ -n "$was" ] && [ -n "$now" ] || continue
        if [ "$now" -lt "$was" ]; then
            echo "$NAME: FAIL — база ОПУЩЕНА рукой: $key было $was, стало $now (§10.4: числа могут только расти)" >&2
            echo "  Если падение законно (корпус сузился, файл вышел из точки) — объясни это" >&2
            echo "  строкой в самом файле базы и подними её отдельным решением, а не молча." >&2
            exit 1
        fi
    done
fi


if [ "$cm" -lt "$base_cm" ] || [ "$bm" -lt "$base_bm" ]; then
    echo "$NAME: FAIL — ОТКАТ храповика: contract $cm (база $base_cm), behavior $bm (база $base_bm)" >&2
    exit 1
fi
if [ "$cm" -gt "$base_cm" ] || [ "$bm" -gt "$base_bm" ]; then
    echo "$NAME: FAIL — прогресс без поднятия базы: contract $cm (база $base_cm), behavior $bm (база $base_bm)." >&2
    echo "  Подними числа в scripts/guards/novac-corpus.baseline ТЕМ ЖЕ коммитом (§10.4)." >&2
    exit 1
fi
echo "$NAME ok: храповик корпуса — contract $cm, behavior $bm (== база)"
exit 0
