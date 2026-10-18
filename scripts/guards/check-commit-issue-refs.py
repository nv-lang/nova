# -*- coding: utf-8 -*-
"""scripts/guards/check-commit-issue-refs.py — голое `#N` в сообщении коммита
отклоняется: `#N` закреплено за номером GitHub issue/PR и пишется только с
маркером.

ДОМ И ОСНОВАНИЕ: решение владельца 2026-10-18 («закрепить #{N} за номером
issue на github»), задача 103; норма — docs/dev/dev-workflow.md §7; запись о
носителе — в реестр 221.1 внесена тем же слиянием (№TBD, номер даёт
интегратор). Перевод ссылок на строки реестра 221.1 в коммитах на форму
`reg N` — в fixed-but-open-scan.py того же слияния.

ЗАЧЕМ. Три разных пространства номеров писались одной записью `#N`: задачи
CrewHarness (`task #14`), строки реестра дефектов (`fix(#628)`) и GitHub
issue/PR. GitHub самолинкует ЛЮБОЕ `#N` в сообщении коммита и ЗАКРЫВАЕТ issue
по словам fixes/closes/resolves: голое `#N` читается снаружи как ссылка на
issue, которого автор не имел в виду, а шаблон PR, учивший писать
`Closes #NNN` про номер реестра, закрыл бы чужой GitHub issue.

ЧТО СЧИТАЕТСЯ ЗАКОННЫМ `#N` (всё — на одной строке с номером):
  * после закрывающего/ссылочного слова GitHub (регистр свободный, GitHub сам
    судит без регистра): fix/fixes/fixed, close/closes/closed,
    resolve/resolves/resolved, ref/refs, references, issue/issues, PR,
    pull (request) — `Fixes #12`, `See issue #90`, `pull request #123`;
  * в цепочке за таким номером — `Fixes #78, #79 and #80`;
  * в форме чужого репозитория `owner/repo#123`.
Всё остальное `#`+цифры — отказ: `task #94`, `fix(#628)`, `(#1865)`, `see #101`.
Формы без решётки страж не судит, они законны всегда: `task 094` (задача
CrewHarness, минимум три цифры), `fix(reg 628)` (строка реестра 221.1).

ГРАНИЦЫ (сказаны честно, а не скрыты общим skip):
  * Строка, чей ПЕРВЫЙ символ `#`, — комментарий git: при cleanup=strip
    (умолчание) git вырезает её из сообщения, и судить текст, который не
    попадёт в публикуемое сообщение, значило бы краснеть на шаблонные
    комментарии самого git. Оговорка про `--cleanup=verbatim`: он такие строки
    сохранит; verbatim в репозитории не используется (ни `commit.cleanup` в
    конфиге, ни вызовов `--cleanup`), а хук файл сообщения получает ДО
    cleanup и режим из него не узнать — отсюда осознанный пропуск именно этих
    строк, а не всех подряд.
  * Цитаты (строка с `>`) СУДЯТСЯ: GitHub линкует `#N` и в цитате, поэтому
    цитата исторического `fix(#628)` переписывается формой `fix(reg 628)`
    (вопрос задачи 103: «лучше FAIL, не skip»).
  * История не судится и не переписывается: старые `fix(#N)` законны в
    прошлом и читаются `fixed-but-open-scan.py` (он читает и новую форму).
  * Человек в своём терминале мимо хуков не остановлен (ступень 1).

ИСПОЛЬЗОВАНИЕ (зовётся из check-commit-hygiene.sh, хук commit-msg):
  python scripts/guards/check-commit-issue-refs.py [ФАЙЛ-СООБЩЕНИЯ] [КОРЕНЬ]
Самотест — scripts/guards/selftest/test-check-commit-issue-refs.sh
"""
import re
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-commit-issue-refs"

# Закрывающие/ссылочные слова GitHub. Якорь на КОНЦЕ префикса: слово обязано
# стоять непосредственно перед `#N` (так судит и сам GitHub — «Fixes: #12»
# с двоеточием не закрывает). Необязательное второе слово — «pull request»,
# «see issue». Слова «fix» в форме `fix(#628)` здесь НЕТ: за ним стоит скобка,
# а не пробел — это и есть различие старой формы реестра от ссылки GitHub.
KEYWORD_AT_END = re.compile(
    r"(?i)(?:^|[^\w])"
    r"(?:fix(?:es|ed)?|close[sd]?|resolve[sd]?|ref(?:erence)?s?|issues?|prs?|pull)"
    r"(?:\s+(?:requests?|issues?))?"
    r"\s+$")

# `owner/repo#123` — явная ссылка на чужой репозиторий.
REPO_AT_END = re.compile(r"[\w][\w.-]*/[\w][\w.-]*$")

HASHNUM = re.compile(r"#(\d+)\b")

# Между номерами одной цепочки («Fixes #78, #79 and #80») стоят только
# разделители — тогда каждый следующий #N тоже помечен первым словом.
CHAIN_GLUE = re.compile(r"(?i)[\s,;/]*(?:and|or)?[\s,;/]*$")

HINT = (
    "`#N` — только GitHub issue/PR и только с маркером: `Fixes #12`, "
    "`Refs #34`, `See issue #56`, `owner/repo#78`. Задача CrewHarness пишется "
    "без решётки и минимум тремя цифрами (`task 094`), строка реестра 221.1 — "
    "формой `reg N` (`fix(reg 628)`). Норма: docs/dev/dev-workflow.md §7; "
    "решение владельца 2026-10-18.")


def violations(msgfile):
    out = []
    with open(msgfile, encoding="utf-8", errors="replace", newline="") as f:
        lines = f.read().split("\n")
    for ln, line in enumerate(lines, 1):
        # Комментарий git (первый символ строки) — не содержимое коммита
        # при cleanup=strip; пропуск ТОЛЬКО этой формы, не «строк с #».
        if line.startswith("#"):
            continue
        prev_end = None
        for m in HASHNUM.finditer(line):
            prefix = line[:m.start()]
            if KEYWORD_AT_END.search(prefix) or REPO_AT_END.search(prefix):
                prev_end = m.end()
                continue
            if prev_end is not None and CHAIN_GLUE.fullmatch(
                    line[prev_end:m.start()]):
                prev_end = m.end()
                continue
            out.append((ln, m.group(0), line.strip()[:100]))
    return out


def main():
    msgfile = sys.argv[1] if len(sys.argv) > 1 else ""
    if not msgfile:
        print("%s ok: судить нечего (нет файла сообщения — страж хука "
              "commit-msg)" % NAME)
        return 0
    try:
        bad = violations(msgfile)
    except OSError as e:
        # Файл назван, но не читается — это не «судить нечего»: молчание
        # читалось бы как зелёный (реестр 221.1 №645).
        print("%s: FAIL — файл сообщения %s не читается: %s"
              % (NAME, msgfile, e), file=sys.stderr)
        return 1
    if bad:
        print("%s: FAIL — голое `#N` в сообщении коммита (%d):"
              % (NAME, len(bad)), file=sys.stderr)
        for ln, tok, ctx in bad:
            print("    строка %d: %s  | %s" % (ln, tok, ctx), file=sys.stderr)
        print("    " + HINT, file=sys.stderr)
        return 1
    print("%s ok: голых `#N` в сообщении нет — `#N` только с маркером GitHub "
          "или не встречается" % NAME)
    return 0


if __name__ == "__main__":
    sys.exit(main())
