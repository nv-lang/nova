#!/bin/sh
# Самотест check-guard-honesty.py.
#
# Доказывает мутацией, что страж ловит все три формы «вердикт есть, проверки
# нет», и — отдельно — что он НЕ краснеет на здоровом коде. Последнее здесь не
# формальность: первая редакция этого стража покраснела на правильно
# экранированном апострофе, в том числе в собственном тексте. Ложная краснота
# ровно так же обесценивает гейт, как пропущенная поломка.
export LC_ALL=C

GD="$(cd "$(dirname "$0")/.." && pwd)"
ROOT="$(cd "$GD/../.." && pwd)"
G="$GD/check-guard-honesty.py"
T="${TMPDIR:-/tmp}/guard-honesty-selftest.$$"
mkdir -p "$T"
trap 'rm -rf "$T"' 0

fails=0
ok()  { echo "  ok: $1"; }
bad() { echo "  FAIL: $1" >&2; fails=$((fails+1)); }

# ── 1. живое дерево — зелёный СО СТРОКОЙ ok: ──────────────────────────────
if python "$G" "$ROOT" > "$T/out" 2> "$T/err"; then
    if grep -q "^check-guard-honesty ok:" "$T/out"; then
        ok "живое дерево — зелёный со строкой ok:"
    else
        bad "зелёный без строки ok: [$(head -n 1 "$T/out")]"
    fi
else
    bad "живое дерево красное: [$(head -n 2 "$T/err")]"
fi

# ── 2. слепота на Linux — красный ────────────────────────────────────────
mkdir -p "$T/blind/scripts/guards"
cat > "$T/blind/scripts/guards/check-x.sh" <<'SH'
#!/bin/sh
ORACLE="$ROOT/nova-cli/target/release/nova.exe"
[ -f "$ORACLE" ] || exit 0
SH
if python "$G" "$T/blind" > "$T/o2" 2> "$T/e2"; then
    bad "файл, знающий только .exe, прошёл: [$(head -n 1 "$T/o2")]"
else
    grep -q "знает только" "$T/e2" \
        && ok "слепота на Linux — красный" \
        || bad "красный, но не про слепоту: [$(head -n 1 "$T/e2")]"
fi

# ── 3. запасное имя рядом — зелёный (правило про слепоту, не про букву) ──
mkdir -p "$T/seeing/scripts/guards"
cat > "$T/seeing/scripts/guards/check-y.sh" <<'SH'
#!/bin/sh
ORACLE="$ROOT/nova-cli/target/release/nova.exe"
[ -f "$ORACLE" ] || ORACLE="$ROOT/nova-cli/target/release/nova"
SH
if python "$G" "$T/seeing" > "$T/o3" 2>&1; then
    ok "файл с запасным именем законен"
else
    bad "файл, знающий оба имени, покраснел: [$(head -n 2 "$T/o3")]"
fi

# ── 4. НЕэкранированный апостроф в сообщении — красный ───────────────────
mkdir -p "$T/tick/scripts/guards"
printf '#!/bin/sh\necho "smells like `date` here"\n' > "$T/tick/scripts/guards/check-z.sh"
if python "$G" "$T/tick" > "$T/o4" 2> "$T/e4"; then
    bad "неэкранированный апостроф прошёл: [$(head -n 1 "$T/o4")]"
else
    grep -q "выполнит его как команду" "$T/e4" \
        && ok "апостроф, выполняемый оболочкой, — красный" \
        || bad "красный, но не про апостроф: [$(head -n 1 "$T/e4")]"
fi

# ── 5. ЭКРАНИРОВАННЫЙ апостроф — зелёный (ловушка первой редакции) ───────
mkdir -p "$T/esc/scripts/guards"
printf '#!/bin/sh\necho "the door is \\`nova update\\` and nothing else"\n' > "$T/esc/scripts/guards/check-w.sh"
if python "$G" "$T/esc" > "$T/o5" 2>&1; then
    ok "экранированный апостроф законен — страж не краснеет на здоровом"
else
    bad "экранированный апостроф покраснел (ловушка первой редакции): [$(head -n 2 "$T/o5")]"
fi

# ── 6. пустой scripts — красный: судить нечего там, где судить обязано ───
mkdir -p "$T/empty/scripts"
if python "$G" "$T/empty" > "$T/o6" 2> "$T/e6"; then
    bad "дерево без единого .sh прошло: [$(head -n 1 "$T/o6")]"
else
    ok "scripts без единого .sh — красный"
fi

# ── 7. нет scripts вовсе — честное «судить нечего» ───────────────────────
mkdir -p "$T/bare"
if python "$G" "$T/bare" > "$T/o7" 2>&1; then
    grep -q "судить нечего" "$T/o7" \
        && ok "нет scripts — судить нечего" \
        || bad "зелёный без честной формулировки: [$(head -n 1 "$T/o7")]"
else
    bad "отсутствие scripts сделано красным: [$(head -n 1 "$T/o7")]"
fi

# ── 8. .py, знающий только nova.exe, — красный ───────────────────────────
# Каждая питоновская фикстура несёт и пустой .sh: без единого .sh страж
# краснеет РАНЬШЕ, на правиле «судить нечего там, где судить обязано», и
# случай доказал бы не то, что проверяет.
mkdir -p "$T/pyblind/scripts/guards"
printf '#!/bin/sh\nexit 0\n' > "$T/pyblind/scripts/guards/check-ok.sh"
cat > "$T/pyblind/scripts/guards/check-p.py" <<'PY'
import pathlib
ORACLE = pathlib.Path("nova-cli/target/release/nova.exe")
PY
if python "$G" "$T/pyblind" > "$T/o8" 2> "$T/e8"; then
    bad ".py, знающий только .exe, прошёл: [$(head -n 1 "$T/o8")]"
else
    grep -q "знает только" "$T/e8" \
        && ok ".py со слепотой на Linux — красный" \
        || bad "красный, но не про слепоту: [$(head -n 1 "$T/e8")]"
fi

# ── 9. check-*.py, печатающий вердикт без перевода потока на LF, — красный ─
mkdir -p "$T/pycrlf/scripts/guards"
printf '#!/bin/sh\nexit 0\n' > "$T/pycrlf/scripts/guards/check-ok.sh"
cat > "$T/pycrlf/scripts/guards/check-q.py" <<'PY'
import sys
print("check-q ok: nothing to judge")
PY
if python "$G" "$T/pycrlf" > "$T/o9" 2> "$T/e9"; then
    bad "печать вердикта без LF прошла: [$(head -n 1 "$T/o9")]"
else
    grep -q "не переведя поток на LF" "$T/e9" \
        && ok "вердикт без перевода потока на LF — красный" \
        || bad "красный, но не про LF: [$(head -n 1 "$T/e9")]"
fi

# ── 10. здоровый .py — зелёный: правило про слепоту, а не про наличие print
# Здесь же названная слепая зона: helper.py печатает без LF и НЕ судится —
# правило адресовано печатающим вердикт, то есть check-*.py.
mkdir -p "$T/pygood/scripts/guards"
printf '#!/bin/sh\nexit 0\n' > "$T/pygood/scripts/guards/check-ok.sh"
cat > "$T/pygood/scripts/guards/check-r.py" <<'PY'
import sys
sys.stdout.reconfigure(encoding="utf-8", newline="\n")
print("check-r ok: nothing to judge")
PY
cat > "$T/pygood/scripts/guards/helper.py" <<'PY'
print("not a verdict printer")
PY
if python "$G" "$T/pygood" > "$T/o10" 2>&1; then
    ok "здоровый .py законен, helper вне суда (слепая зона названа)"
else
    bad "здоровый .py покраснел: [$(head -n 2 "$T/o10")]"
fi

# ── 11. съеденный возврат каретки — красный ──────────────────────────────
# Живой случай 2026-08-19: в check-novac-row-fields.py стояло удаление
# ПЕРЕВОДОВ СТРОК там, где имелся в виду возврат каретки. Файл склеивался в
# одну строку, и правило П23 засчитывало пометку с чужой строки плана каждому
# полю — молчало полтора дня и пропустило живое нарушение.
mkdir -p "$T/eaten/scripts/guards"
printf '#!/bin/sh\nX=$(cat f | tr -d %s\n%s | grep -F x)\n' "'" "'" > "$T/eaten/scripts/guards/check-e.sh"
if python "$G" "$T/eaten" > "$T/o11" 2> "$T/e11"; then
    bad "удаление переводов строк прошло: [$(head -n 1 "$T/o11")]"
else
    grep -q "съеденный" "$T/e11" \
        && ok "удаление переводов строк вместо возвратов каретки — красный" \
        || bad "красный, но не про съеденный возврат каретки: [$(head -n 1 "$T/e11")]"
fi

# ── 12. ЗАМЕНА переводов на пробел законна — зелёный ─────────────────────
# Названная граница правила: склеить список в строку — законная идиома, и
# красить её значило бы платить ложной краснотой за букву.
mkdir -p "$T/join/scripts/guards"
printf '#!/bin/sh\nX=$(cat f | tr %s\n%s " " )\n' "'" "'" > "$T/join/scripts/guards/check-j.sh"
if python "$G" "$T/join" > "$T/o12" 2>&1; then
    ok "замена переводов на пробел законна — ложной красноты нет"
else
    bad "законная замена покрашена: [$(head -n 2 "$T/o12")]"
fi

# ── 13. СЛОМАННЫЙ страж не печатает зелёный ─────────────────────────────
# Мутация ПОДСУДИМОГО: ломаем разбор в копии стража. В shell-редакции скан был
# отдельным процессом (awk), и его крах давал пустой результат, неотличимый от
# «находок нет» — 2026-08-19 такой страж напечатал «ok». В python скан живёт в
# самом страже: поломка поднимает исключение, процесс умирает ненулевым. Случай
# держит границу: зелёного при сломанном разборе быть не может.
mkdir -p "$T/broken"
sed 's|    bad = \[\]|    bad = [] + 1|' "$G" > "$T/broken/g.py"
if python "$T/broken/g.py" "$ROOT" > "$T/o13" 2> "$T/e13"; then
    bad "сломанный страж напечатал зелёный: [$(head -n 1 "$T/o13")]"
elif grep -q "ok:" "$T/o13"; then
    bad "сломанный страж вышел ненулём, но напечатал строку ok:"
else
    ok "сломанный разбор — красный, строки ok: нет"
fi


# ── 14. проба-улика с НЕэкранированным апострофом — красный ──────────────
# Живой случай (реестр №1190): апостроф в двойных кавычках стоял в СВОЕЙ же
# пробе окна, оболочка выполнила `Node`, и в улику упало «Node: command not
# found». Судья его не видел: смотрел только scripts/**.
mkdir -p "$T/probe/scripts/guards" "$T/probe/docs/plans/repro/x"
printf '#!/bin/sh\nexit 0\n' > "$T/probe/scripts/guards/check-ok.sh"
printf '#!/bin/sh\necho "planted %s Node %s here"\n' "\`" "\`" > "$T/probe/docs/plans/repro/x/cmd.sh"
if python "$G" "$T/probe" > "$T/o14" 2> "$T/e14"; then
    bad "апостроф в пробе-улике прошёл: [$(head -n 1 "$T/o14")]"
else
    grep -q "docs/plans/repro/x/cmd.sh" "$T/e14" \
        && ok "апостроф в пробе-улике — красный, и отказ называет ФАЙЛ" \
        || bad "красный, но без имени файла пробы: [$(head -n 1 "$T/e14")]"
fi

# ── 15. апостроф в КОММЕНТАРИИ пробы — зелёный ───────────────────────────
# Обратная сторона, без которой правило считает символы, а не исполнение:
# в живых пробах апострофы стоят в комментариях десятками (в носителе №1190 их
# два — строки 7 и 83), и оболочка их не исполняет.
mkdir -p "$T/probecomment/scripts/guards" "$T/probecomment/docs/plans/repro/y"
printf '#!/bin/sh\nexit 0\n' > "$T/probecomment/scripts/guards/check-ok.sh"
printf '#!/bin/sh\n# tema: komanda %s Node %s i eyo vyvod\necho "quiet"\n' "\`" "\`" \
    > "$T/probecomment/docs/plans/repro/y/cmd.sh"
if python "$G" "$T/probecomment" > "$T/o15" 2>&1; then
    ok "апостроф в комментарии пробы законен — правило судит исполнение"
else
    bad "комментарий пробы покрашен — страж считает символы: [$(head -n 2 "$T/o15")]"
fi

# ── 16. объявленная территория проб БЕЗ единого .sh — красный ────────────
# Пустая мишень читается как чистая: ровно этот класс дефекта территория и
# закрывает, поэтому ноль подсудимых здесь — отказ, а не «нарушений 0».
mkdir -p "$T/noprobe/scripts/guards" "$T/noprobe/docs/plans/repro"
printf '#!/bin/sh\nexit 0\n' > "$T/noprobe/scripts/guards/check-ok.sh"
if python "$G" "$T/noprobe" > "$T/o16" 2> "$T/e16"; then
    bad "территория проб без единого .sh прошла: [$(head -n 1 "$T/o16")]"
else
    grep -q "судить в ней нечего" "$T/e16" \
        && ok "объявленная территория проб без .sh — красный" \
        || bad "красный, но не про пустую территорию: [$(head -n 1 "$T/e16")]"
fi

# ── 17. территории проб нет вовсе — зелёный ──────────────────────────────
# Граница предыдущего случая: отказ вызывает ПУСТОЙ объявленный каталог, а не
# его отсутствие, иначе всякое дерево без docs/plans/repro стало бы красным.
mkdir -p "$T/noterritory/scripts/guards"
printf '#!/bin/sh\nexit 0\n' > "$T/noterritory/scripts/guards/check-ok.sh"
if python "$G" "$T/noterritory" > "$T/o17" 2>&1; then
    ok "отсутствие территории проб законно — красит пустота, а не отсутствие"
else
    bad "дерево без docs/plans/repro покрашено: [$(head -n 2 "$T/o17")]"
fi

# ── 18. съеденный возврат каретки В ПРОБЕ — красный ──────────────────────
mkdir -p "$T/probeeaten/scripts/guards" "$T/probeeaten/docs/plans/repro/z"
printf '#!/bin/sh\nexit 0\n' > "$T/probeeaten/scripts/guards/check-ok.sh"
printf '#!/bin/sh\nX=$(cat f | tr -d %s\n%s | grep -F x)\n' "'" "'" \
    > "$T/probeeaten/docs/plans/repro/z/cmd.sh"
if python "$G" "$T/probeeaten" > "$T/o18" 2> "$T/e18"; then
    bad "съеденный возврат каретки в пробе прошёл: [$(head -n 1 "$T/o18")]"
else
    grep -q "съеденный" "$T/e18" \
        && ok "съеденный возврат каретки в пробе — красный" \
        || bad "красный, но не про съеденный возврат: [$(head -n 1 "$T/e18")]"
fi

# ── 19. знаменатели ОБОИХ периметров печатаются, и оба НЕнулевые ─────────
# Требование интегратора 2026-09-20: число по каждому периметру ОТДЕЛЬНО.
# Вердикт без знаменателя не даёт отличить «проверено 166» от «проверено 0».
if python "$G" "$ROOT" > "$T/o19" 2>&1; then
    n_probe=$(sed -n 's/.*проб-улик \([0-9][0-9]*\) .*/\1/p' "$T/o19")
    n_guard=$(sed -n 's/.*стражей проверено \([0-9][0-9]*\) .*/\1/p' "$T/o19")
    if [ -n "$n_probe" ] && [ -n "$n_guard" ] && [ "$n_probe" -gt 0 ] && [ "$n_guard" -gt 0 ]; then
        ok "знаменатели обоих периметров напечатаны: стражей $n_guard, проб $n_probe"
    else
        bad "знаменатель периметра отсутствует или нулевой: [$(head -n 1 "$T/o19")]"
    fi
else
    bad "живое дерево красное на знаменателях: [$(head -n 2 "$T/o19")]"
fi

# ── 20. проба, знающая только nova.exe, — ЗЕЛЁНАЯ (названная слепая зона) ─
# Замер 2026-09-20: 70 проб из 166 знают только .exe. Это не долг — проба
# фиксирует команду КОНКРЕТНОЙ машины, а на Linux отсутствие файла даёт ГРОМКУЮ
# ошибку, тогда как правило заведено против МОЛЧАНИЯ. Случай держит границу:
# перенос правила сюда покрасил бы 70 честных улик.
mkdir -p "$T/probeexe/scripts/guards" "$T/probeexe/docs/plans/repro/w"
printf '#!/bin/sh\nexit 0\n' > "$T/probeexe/scripts/guards/check-ok.sh"
printf '#!/bin/sh\nNOVA="$R/nova-cli/target/release/nova.exe"\n"$NOVA" check x.nv\n' \
    > "$T/probeexe/docs/plans/repro/w/cmd.sh"
if python "$G" "$T/probeexe" > "$T/o20" 2>&1; then
    ok "проба со своим путём к .exe законна — слепая зона названа, а не молчалива"
else
    bad "правило про имя бинаря уехало на пробы: [$(head -n 2 "$T/o20")]"
fi
if [ "$fails" -ne 0 ]; then
    echo "итог: FAIL $fails" >&2
    exit 1
fi
echo "итог: PASS"
echo "test-check-guard-honesty ok: все случаи, включая ловушку экранированного апострофа, оба питоновских правила, съеденный возврат каретки, сломанный разбор и вторую территорию — пробы-улики с обеими её границами (комментарий зелен, пустая территория красна)"
exit 0
