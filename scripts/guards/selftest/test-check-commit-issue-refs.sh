#!/usr/bin/env bash
# Селфтест scripts/guards/check-commit-issue-refs.py.
#
# Оба направления, и второе важнее: страж, срывающий законный коммит, будет
# снесён в первый же день. Каждый случай — отдельный файл сообщения; страж
# чистый (git не нужен), поэтому прогон мгновенный.
#
# Случаи 8-13 — это и есть проба «RED при намеренном внесении голого #N»:
# фикстура несёт нарушение, страж обязан отказать и назвать токен. Случаи
# 1-7 — «GREEN после удаления» и ложные PASS: формы, похожие на нарушение,
# но законные.
set -u
export LC_ALL=C

HERE="$(cd "$(dirname "$0")" && pwd)"
G="$HERE/../check-commit-issue-refs.py"

PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); echo "  ok   $1"; }
bad() { FAIL=$((FAIL+1)); echo "  FAIL $1" >&2; }

TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
MSG="$TMP/msg.txt"

# want 0 — сообщение законно; want 1 — страж обязан отказать.
run() { python "$G" "$MSG" "$TMP" 2>&1; echo "rc=$?"; }

check() { # $1 имя, $2 ожидаемый rc, $3 подстрока вывода ('' — не проверять)
    local name="$1" want="$2" sub="$3" out rc
    out=$(run); rc="${out##*rc=}"
    if [ "$rc" != "$want" ]; then
        bad "$name (want rc=$want, got rc=$rc): $out"; return
    fi
    if [ -n "$sub" ] && ! printf '%s' "$out" | grep -qF -- "$sub"; then
        bad "$name (нет подстроки '$sub'): $out"; return
    fi
    ok "$name"
}

echo "== check-commit-issue-refs selftest =="

# 1. Чистое сообщение без номеров — проходит. Это направление ПЕРВЫМ: страж,
#    который не пропускает законное, бесполезен независимо от того, что ловит.
printf 'docs: refresh the roadmap map\n' > "$MSG"
check "a plain message passes" 0 "ok:"

# 2. Задача CrewHarness — минимум три цифры БЕЗ решётки (норма задачи 103).
printf 'guards: split task and issue numbers (task 103)\n\nSee task 094, task 101 and task 1000.\n' > "$MSG"
check "task 003/094/101/1000 without # pass" 0 "ok:"

# 3. Явные ссылки GitHub после закрывающих/ссылочных слов — законны.
printf 'fix: repair the flaky mirror sync\n\nFixes #12\nCloses #34\nResolves #56\nRefs #78\nSee issue #90\nissue #22 explained there\nPR #34\npull request #123\nfix #628 was verified\n' > "$MSG"
check "keyword-marked GitHub refs pass" 0 "ok:"

# 4. Ссылка на чужой репозиторий owner/repo#N — законна.
printf 'ci: mirror broke on the same cause as nv-lang/nova-web#45\n' > "$MSG"
check "owner/repo#N passes" 0 "ok:"

# 5. Строка-комментарий git (первый символ #) не судится: при cleanup=strip
#    она не попадает в сообщение. Важно: пропуск ТОЛЬКО этой формы.
printf 'subject\n\n#94 this line is a git comment, stripped by cleanup=strip\n# Closes #5 inside a comment is stripped too\n' > "$MSG"
check "git comment lines (leading #) are skipped" 0 "ok:"

# 6. Цепочка номеров за одним словом: каждый следующий тоже помечен.
printf 'fix: close the mirror race\n\nFixes #78, #79 and #80\n' > "$MSG"
check "a chained list Fixes #78, #79 and #80 passes" 0 "ok:"

# 7. Новая форма реестра без решётки — законна всегда.
printf 'fix(reg 628): the dispatcher asks the declared table\n\nrevert(reg 822) noted; also (reg 1865) is fine\n' > "$MSG"
check "the reg N registry form passes" 0 "ok:"

# 8. Голое #N задачи — отказ, токен назван.
printf 'merge: candidate for task #94\n' > "$MSG"
check "task #94 is refused" 1 "#94"

# 9. Голое #N с одной цифрой в теме — отказ.
printf 'docs: close out the work of #3\n' > "$MSG"
check "bare #3 in the subject is refused" 1 "#3"

# 10. Голое #N в теле без слова-маркера — отказ (GitHub самолинкует и его).
printf 'docs: notes\n\nMore context in #101 of the tracker.\n' > "$MSG"
check "bare #101 in the body is refused" 1 "#101"

# 11. СТАРАЯ форма реестра fix(#628) в НОВОМ сообщении — отказ: форма уехала
#     в историю, новая — fix(reg 628). Слово «fix» перед скобкой не маркер.
printf 'fix(#628): the dispatcher asks the declared table\n' > "$MSG"
check "the retired fix(#N) form is refused" 1 "#628"

# 12. Хвостовая форма (#1865) — отказ.
printf 'guards: count every failing outcome label (#1865)\n' > "$MSG"
check "a trailing (#1865) is refused" 1 "#1865"

# 13. Цитата с '>' СУДИТСЯ: GitHub линкует #N и в цитате (ответ на открытый
#     вопрос задачи 103 — «лучше FAIL, не skip»).
printf 'docs: record the old spelling\n\n> fix(#628) was the form before 2026-10-18\n' > "$MSG"
check "a quoted bare #N is refused" 1 "#628"

# 14. Обратная сторона цепочки: после законного Fixes #78 голый #79 через
#     текст («fixed in #79 later») не законен — клей цепочки только из
#     разделителей.
printf 'fix: x\n\nFixes #78, fixed in #79 later\n' > "$MSG"
check "a bare #N after prose inside a line is refused" 1 "#79"

# 15. Файл назван, но не читается — отказ, а не зелёный «судить нечего»
#     (класс №645: молчание читается как зелень).
check_missing=$(python "$G" "$TMP/no-such-file" "$TMP" 2>&1; echo "rc=$?")
if [ "${check_missing##*rc=}" = "1" ]; then ok "an unreadable message file refuses"; else bad "unreadable file passed: $check_missing"; fi

# 16. Без файла сообщения — «судить нечего», зелёный (хук зовёт всегда,
#     ручной запуск без аргумента не должен краснеть).
check_nofile=$(python "$G" 2>&1; echo "rc=$?")
if [ "${check_nofile##*rc=}" = "0" ]; then ok "no message file is a green no-op"; else bad "no-file run refused: $check_nofile"; fi

# 17-18. СКВОЗНАЯ ПРОВОДКА: страж обязан реально вызываться из
#     check-commit-hygiene.sh. Случаи выше гоняют страж напрямую, а самотест
#     hygiene работает в репе, где этого файла нет (вызов условный, как у
#     novac-подстражей), — значит, опечатка в строке вызова была бы невидима
#     обоими. Здесь подставная репа несёт ОБА файла, и отказ обязан прийти
#     именно от check-commit-issue-refs.
HREPO="$TMP/hrepo"; mkdir -p "$HREPO/scripts/guards"
cp "$HERE/../check-commit-hygiene.sh" "$HREPO/scripts/guards/"
cp "$G" "$HREPO/scripts/guards/"
git init -q -b main "$HREPO" 2>/dev/null || { bad "нет git для сквозной пробы"; }
git -C "$HREPO" config user.name selftest
git -C "$HREPO" config user.email selftest@example.com
echo base > "$HREPO/f.txt"; git -C "$HREPO" add f.txt >/dev/null 2>&1

printf 'fix(#628): the dispatcher asks the declared table\n' > "$HREPO/msg.txt"
out=$(NOVA_COMMIT_EMAIL=selftest@example.com bash "$HREPO/scripts/guards/check-commit-hygiene.sh" "$HREPO/msg.txt" "$HREPO" 2>&1); rc=$?
if [ "$rc" = "1" ] && printf '%s' "$out" | grep -qF 'check-commit-issue-refs'; then
    ok "wired: hygiene refuses a bare #N through the sub-guard"
else
    bad "wiring red failed (rc=$rc): $out"
fi

printf 'fix(reg 628): the dispatcher asks the declared table\n' > "$HREPO/msg.txt"
out=$(NOVA_COMMIT_EMAIL=selftest@example.com bash "$HREPO/scripts/guards/check-commit-hygiene.sh" "$HREPO/msg.txt" "$HREPO" 2>&1); rc=$?
if [ "$rc" = "0" ]; then
    ok "wired: hygiene passes the reg N form"
else
    bad "wiring green failed (rc=$rc): $out"
fi

echo "  passed: $PASS   failed: $FAIL"
[ "$FAIL" -eq 0 ] || exit 1
exit 0
