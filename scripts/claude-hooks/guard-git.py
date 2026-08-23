#!/usr/bin/env python3
"""guard-git.py — PreToolUse-хук Claude Code (Bash + PowerShell): блокирует
запрещённые git-команды ДО их исполнения.

ПОЧЕМУ. Правила «git config user.* правит только владелец руками», «git add
только по именам файлов», «git stash запрещён (worktree делят один .git)»
раньше жили только в тексте памяти/конвенций — соблюдение зависело от того,
вспомнит ли агент их в моменте. Измеренная цена промаха: 349 коммитов ушли
под авторством «Claude Haiku» через общий .git нескольких worktree
(инцидент 2026-07-25) — правку авторства по всей истории пришлось делать
отдельной волной. План 231 трек Д п.6 (docs/plans/231-bug-cycle-exit.md,
«правила из памяти/конвенций переезжают в перехватчик») + исполнительный
дом docs/plans/231.2-enforcement-infra.md §1.

ЧТО ПРОВЕРЯЕТ (RULES ниже; совпадение → exit 2 + причина в stderr):
  - `git config ... user.name|user.email` С ЗАПИСЬЮ значения (голое чтение
    `git config user.name` разрешено — иначе ложные срабатывания на штатных
    проверках авторства перед коммитом);
  - `git add -A` / `git add .` / `git add --all` (конвенция: добавлять
    только по именам файлов);
  - `git stash` (worktree этой репы делят один `.git` — конвенция требует
    temp-commit/reset вместо stash).

КАК (защита от ложных срабатываний). Матчится ТОЛЬКО исполняемая часть
команды: литеральный текст в `'...'`/`"..."` и содержимое heredoc
(`<<EOF ... EOF`) вырезается регэксполм (`_QUOTED`, `_HEREDOC`) ПЕРЕД
прогоном правил — иначе commit-сообщение или содержимое скрипта, где
«git add -A» упоминается как ТЕКСТ (а не выполняется), ложно блокируется.
Доказанный на практике класс регресса — см. таблицу самотестов, план 231
§4в (docs/plans/231-bug-cycle-exit.md).

Fail-open по ошибкам самого хука (не смог распарсить stdin-JSON → exit 0),
fail-closed по правилам (паттерн совпал → exit 2).

ИСПОЛЬЗОВАНИЕ. Не запускается вручную — подключается декларативно через
локальный (НЕ в git) `.claude/settings.json`:
    hooks.PreToolUse[].matcher = "Bash|PowerShell"
    → command: python scripts/claude-hooks/guard-git.py
Хук получает JSON вызова инструмента через stdin (`tool_input.command`).
Самотеста в scripts/guards/selftest/ ПОКА НЕТ — см. таблицу покрытия план 231 §4в
и scripts/guards/selftest/README.md.
"""
from __future__ import annotations

import json
import re
import sys

RULES = [
    # ЗАПИСЬ user.* (со значением) — чтение `git config user.name` разрешено
    # (иначе ложные срабатывания на heredoc-текстах и проверках авторства).
    (re.compile(r"\bgit\b[^|;&\n]*\bconfig\b[^|;&\n]*\buser\.(name|email)\s+\S", re.IGNORECASE),
     "FORBIDDEN: git config user.* write — avtorstvo pravit tolko vladelets vruchnuyu "
     "(urok 2026-07-25: 349 commitov pod 'Claude Haiku' cherez obshchiy .git worktree)."),
    (re.compile(r"\bgit\b[^|;&\n]*\badd\b\s+(-A\b|--all\b|\.(\s|$))", re.IGNORECASE),
     "FORBIDDEN: git add -A/--all/. — tolko po imenam faylov (konventsiya)."),
    (re.compile(r"\bgit\b[^|;&\n]*\bstash\b", re.IGNORECASE),
     "FORBIDDEN: git stash — worktree delyat .git (konventsiya: temp-commit/reset)."),
    # СОСТОЯНИЕ-МЕНЯЮЩАЯ КОМАНДА БЕЗ ЯВНОГО -C.
    #
    # Наблюдение 2026-08-10: рабочий каталог оболочки уехал в worktree окна
    # (nova-p564), и `git add` + `git commit` отработали НЕ в том дереве —
    # обнаружилось случайно, по строке «On branch p564-module-name» в выводе.
    # Одновременно в дереве живут 60 worktree, и «помни, где ты» механизмом
    # не является: cwd дрейфует между вызовами, а вывод инструмента о нём
    # может врать.
    #
    # Читающие команды (status/log/diff/show/branch/ls-files) НЕ трогаем:
    # ошибиться деревом на чтении дёшево и заметно. Ловим только те, что
    # МЕНЯЮТ состояние.
    (re.compile(r"\bgit\s+(?!-C\b|--git-dir\b|--work-tree\b)"
                r"(add|commit|push|merge|checkout|switch|reset|rm|mv|worktree|tag|branch\s+-[dDmM])\b",
                re.IGNORECASE),
     "FORBIDDEN: git <state-changing> bez -C — ukazhi derevo yavno: "
     "git -C /d/Sources/nv-lang/nova <cmd>. Prichina: cwd obolochki dreyfuet mezhdu "
     "vyzovami (2026-08-10: add+commit ushli v worktree okna nova-p564)."),
    # ТЕКСТ С ОБРАТНЫМИ АПОСТРОФАМИ ЧЕРЕЗ ОБОЛОЧКУ (реестр 221.1 №596).
    #
    # Оболочка выполняет подстановку команд ВНУТРИ того, что ей передали как
    # данные, и делает это молча. За 2026-08-11 — три случая подряд: из
    # сообщения коммита пропал `Ok(consume stream)`; со страницы правил
    # исчезли ИМЕНА СТРАЖЕЙ; `printf` выполнил `require-diff-base.sh` как
    # команду. Каждый раз это замечалось только при перечитывании результата.
    #
    # Ловим ровно ту форму, которой обжигались: `python -c` или `printf` с
    # обратным апострофом в тексте. Правильный путь — записать текст файлом
    # (Write) и скормить его файлом же (`-F`, `python <script>`).
    (re.compile(r"(python[0-9.]*\s+-c|printf)[^\n]*`", re.IGNORECASE),
     "FORBIDDEN: tekst s obratnymi apostrofami cherez obolochku — ona vypolnit ih "
     "kak komandu i tiho s'est kusok teksta (221.1 №596; tri sluchaya 2026-08-11). "
     "Zapishi tekst faylom (Write) i peredavay faylom: git commit -F <file>, "
     "python <script.py>."),
    # ЗАПИСЬ ТЕКСТА В ФАЙЛ ЧЕРЕЗ POWERSHELL (реестр 221.1 №630).
    #
    # В PowerShell обратный апостроф — СИМВОЛ ЭКРАНИРОВАНИЯ, и в двойных
    # кавычках и here-string (@"…"@) markdown-апострофы съедаются МОЛЧА:
    # `f -> 0x0C, `a -> 0x07, `r -> 0x0D (склеивает строки), `n -> перевод
    # строки, остальные просто исчезают.
    #
    # 2026-08-12: так были испорчены ОБЕ обзорные страницы spec/syntax — в тот
    # же день, когда завели №620 про этот же класс в bash. Правило знали и
    # наступили снова, потому что оболочка была ДРУГАЯ: запрет, привязанный к
    # инструменту, не переносится сам.
    #
    # Само правило — в RAW_RULES ниже: оно обязано смотреть ВНУТРЬ кавычек,
    # а основной список их срезает.
]

# ПРАВИЛА ПО СЫРОЙ КОМАНДЕ — без срезания кавычек и here-string.
#
# Основной список (RULES) намеренно матчит только ИСПОЛНЯЕМУЮ часть: литерал в
# кавычках — это данные, и правило «git без -C» не должно краснеть на строке,
# которая лишь УПОМИНАЕТ git. Но правило №630 — ровно про содержимое кавычек:
# порча происходит ВНУТРИ них. Поэтому отдельный список, и в нём — только те
# правила, для которых текст в кавычках и есть предмет проверки.
RAW_RULES = [
    # ЗАПИСЬ ТЕКСТА В ФАЙЛ ЧЕРЕЗ POWERSHELL (реестр 221.1 №630).
    #
    # Ловим ДВА случая, и второй — против КЛАССА, а не против сегодняшнего
    # текста:
    #   (1) запись в файл + обратный апостроф где-либо в команде — порча уже
    #       происходит;
    #   (2) here-string в ДВОЙНЫХ кавычках (@"…"@) вместе с записью в файл,
    #       даже если апострофов сейчас нет: форма сама по себе мина —
    #       достаточно дописать в неё markdown-апостроф, и текст испортится
    #       молча. Безопасная форма @'…'@ стоит ровно один символ.
    (re.compile(
        r"((Set-Content|Out-File|Add-Content|WriteAllText|WriteAllLines)"
        r"(.|\n)*?`)|"
        r"(`(.|\n)*?(Set-Content|Out-File|Add-Content|WriteAllText|WriteAllLines))|"
        r"(@\"(.|\n)*?(Set-Content|Out-File|Add-Content|WriteAllText|WriteAllLines))|"
        r"((Set-Content|Out-File|Add-Content|WriteAllText|WriteAllLines)(.|\n)*?@\")|"
        r"(@\"(.|\n)*?`)",
        re.IGNORECASE),
     "FORBIDDEN: zapis' teksta v fayl cherez PowerShell — obratnyy apostrof tam "
     "SIMVOL EKRANIROVANIYA, i markdown-apostrofy s'edayutsya molcha: `f -> 0x0C, "
     "`a -> 0x07, `r -> 0x0D (skleivaet stroki). 2026-08-12 tak isporcheny obe "
     "stranicy spec/syntax (221.1 №630). Pishi cherez Write/Edit libo skript-faylom "
     "na python; odinarnye kavychki i @'...'@ bezopasny, no proshche ne pisat' tekst "
     "cherez obolochku voobshche."),

    # СООБЩЕНИЕ КОММИТА С ОБРАТНЫМ АПОСТРОФОМ ЧЕРЕЗ `-m` (реестр 221.1 №637).
    #
    # В двойных кавычках bash делает ПОДСТАНОВКУ КОМАНДЫ: `mut sender` в тексте
    # сообщения исполняется как команда, а на её место встаёт вывод (обычно
    # пусто). Сообщение уезжает в историю с дырами там, где автор писал имена
    # в апострофах — и историю уже не поправить после отправки.
    #
    # Ловится 2026-08-13 в ПЯТЫЙ раз за проект и ВТОРОЙ раз за один час: сперва
    # так испортился комментарий внутри стража, затем — сообщение коммита
    # gate(#636), где выпали `mut sender`, `out` и `|| echo 0`.
    #
    # Правило узкое НАМЕРЕННО: только `git commit` + `-m` + обратный апостроф.
    # Форма `-F файл` не трогается — она и есть верный ответ, вместе с
    # heredoc в ОДИНАРНЫХ кавычках.
    (re.compile(r"git\s+(-C\s+\S+\s+)?commit\b(.|\n)*?-m(.|\n)*?`"),
     "FORBIDDEN: soobshchenie kommita s obratnym apostrofom cherez -m. V dvoynyh "
     "kavychkah bash DELAET PODSTANOVKU KOMANDY: `mut sender` ispolnitsya, a v "
     "tekste ostanetsya dyra (221.1 №637, pyatyy sluchay klassa). Pishi soobshchenie "
     "v FAYL (Write) i peredavay: git commit -F <fayl>."),
]


_QUOTED = re.compile(r"'[^']*'|\"[^\"]*\"", re.DOTALL)
_HEREDOC = re.compile(r"<<-?\s*'?(\w+)'?.*?\n\1\b", re.DOTALL)

# ── КОММИТ БЕЗ ОБЛАСТИ: индекс может быть ЧУЖОЙ ──────────────────────────
#
# Инцидент 2026-08-23. В общем дереве `nova-p274` окно закоммитило свой один
# файл — и забрало 49: в индексе лежали pre-staged правки соседнего окна (их
# волна спеки и фикстур). Проверка `git diff --cached --stat` в брифе БЫЛА и
# даже выполнялась — но в одной цепочке с коммитом: она печатает вывод, на
# который уже никто не смотрит. Правило «посмотри индекс перед коммитом» есть
# в конвенции с первого дня и не удержало ни разу, потому что держится на
# внимании в момент, когда внимание занято другим.
#
# ФОРМА, КОТОРАЯ НЕ ЗАВИСИТ ОТ ВНИМАНИЯ: `git commit --only -- <файлы>`
# (или `git commit <файлы>`) — коммитится ровно названный путь, что бы ни
# лежало в индексе.
#
# ЗАКОННЫЕ ИСКЛЮЧЕНИЯ (частичный коммит там невозможен либо бессмыслен):
#   * `--amend` — правка последнего коммита;
#   * слияние/cherry-pick/revert/rebase в процессе: git ОТКАЗЫВАЕТ в
#     частичном коммите («cannot do a partial commit during a merge»), значит
#     запрет требовал бы невозможного. Определяется по дереву, а не по слову;
#   * осознанный override `# index-verified: <причина>` — коммит всего индекса
#     остаётся возможным, но становится НАЗВАННЫМ и грепаемым.
_COMMIT = re.compile(r"\bgit\s+(?:-C\s+\S+\s+|--git-dir[= ]\S+\s+|--work-tree[= ]\S+\s+)*commit\b",
                     re.IGNORECASE)
_SCOPED = re.compile(r"(--only\b|\s-o\b|--include\b|\s-i\b|--amend\b|\s--\s)", re.IGNORECASE)
_DASH_C = re.compile(r"\bgit\s+-C\s+(\S+)")
_OVERRIDE = re.compile(r"#\s*index-verified", re.IGNORECASE)

_COMMIT_SCOPE_MSG = (
    "FORBIDDEN: git commit bez oblasti — v indekse mogut lezhat' CHUZHIE pre-staged "
    "pravki (2026-08-23: kommit odnogo fayla zabral 49 v obshchem dereve nova-p274). "
    "Forma, ne zavisyashchaya ot vnimaniya: git -C <derevo> commit -s --only -- <fayly> "
    "(soobshchenie faylom: -F <fayl>). Nuzhen ves' indeks — napishi v komande "
    "kommentariy '# index-verified: <prichina>'; pri sliyanii/cherry-pick i --amend "
    "pravilo ne primenyaetsya."
)


def _merge_in_progress(cmd: str) -> bool:
    """Идёт ли слияние/cherry-pick/revert/rebase в дереве, названном через -C.

    Спрашиваем git, а не угадываем по словам команды. Не смогли спросить —
    считаем, что не идёт: тогда сработает запрет, а у него есть законный
    override, то есть цена ошибки — одна строка комментария, а не потерянная
    работа.
    """
    m = _DASH_C.search(cmd)
    if not m:
        return False
    import os
    import subprocess
    try:
        out = subprocess.run(["git", "-C", m.group(1), "rev-parse", "--absolute-git-dir"],
                             capture_output=True, text=True, timeout=10)
        gd = out.stdout.strip()
        if not gd:
            return False
        return any(os.path.exists(os.path.join(gd, n)) for n in
                   ("MERGE_HEAD", "CHERRY_PICK_HEAD", "REVERT_HEAD",
                    "rebase-merge", "rebase-apply"))
    except Exception:
        return False


def check_commit_scope(cmd: str, stripped: str):
    """Сообщение об отказе или None. `stripped` — команда без литералов."""
    if not _COMMIT.search(stripped):
        return None
    if _SCOPED.search(stripped) or _OVERRIDE.search(cmd):
        return None
    if _merge_in_progress(cmd):
        return None
    return _COMMIT_SCOPE_MSG


def main() -> int:
    try:
        data = json.loads(sys.stdin.read() or "{}")
        cmd = (data.get("tool_input") or {}).get("command") or ""
    except Exception:
        return 0
    # Матчим только ИСПОЛНЯЕМУЮ часть: литеральный текст в кавычках/heredoc
    # (коммит-сообщения, содержимое скриптов) — не команды (ложняки ×2 доказаны).
    stripped = _HEREDOC.sub(" ", cmd)
    stripped = _QUOTED.sub(" ", stripped)
    for rx, msg in RULES:
        if rx.search(stripped):
            print(msg, file=sys.stderr)
            return 2
    # Правила, для которых содержимое кавычек и ЕСТЬ предмет проверки —
    # по сырой команде (см. комментарий у RAW_RULES).
    for rx, msg in RAW_RULES:
        if rx.search(cmd):
            print(msg, file=sys.stderr)
            return 2
    # Коммит без названной области — правило с логикой, а не регэксп: ему нужно
    # спросить дерево о слиянии (см. check_commit_scope).
    msg = check_commit_scope(cmd, stripped)
    if msg:
        print(msg, file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())
