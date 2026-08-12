# -*- coding: utf-8 -*-
"""Самотест правила №630 в guard-git.py — запись текста через PowerShell.

ЗАЧЕМ ОТДЕЛЬНЫЙ САМОТЕСТ. Перехватчик правит поведение агента, а не дерево, и
поэтому его поломка невидима: он просто перестаёт срабатывать, и всё выглядит
нормально ровно до следующей порчи. Проверяем ОБА свойства — ловит нарушение и
НЕ ложнит на законном (правило проверки проверок, план 231 трек Ж).

ПОЧЕМУ ПРАВИЛО СМОТРИТ СЫРУЮ КОМАНДУ. Основной список правил намеренно срезает
кавычки и here-string: там литерал — это данные, и «git без -C» не должен
краснеть на строке, которая git лишь упоминает. Правило №630 — ровно наоборот:
порча происходит ВНУТРИ кавычек, поэтому оно живёт в RAW_RULES и матчит сырой
текст. Случай 8 ниже сторожит границу: чтение файла с апострофом в пути
законно и блокироваться не должно.

ЗАПУСК:
    python scripts/claude-hooks/selftest/test-guard-git-powershell.py
"""
import json
import os
import subprocess
import sys

HOOK = os.path.join(
    os.path.dirname(os.path.abspath(__file__)), "..", "guard-git.py")
HOOK = os.path.normpath(HOOK)

CASES = [
    # (описание, команда, ДОЛЖНО ли блокироваться)
    (u"Set-Content с обратным апострофом",
     u'Set-Content -Path a.md -Value "text with `fn code"', True),
    (u"here-string в двойных кавычках с апострофом",
     u'$x = @"\nsome `fn text\n"@', True),
    (u"WriteAllText с апострофом",
     u'[System.IO.File]::WriteAllText($p, "a `b c")', True),
    # Мина без апострофа: сегодня безвредна, завтра испортит — ловим ФОРМУ.
    (u"here-string в двойных кавычках + запись в файл, без апострофов",
     u'@"\ntext\n"@ | Out-File a.md', True),
    # Законное — блокироваться НЕ должно.
    (u"обычный git-вызов с -C",
     u'git -C /d/Sources/nv-lang/nova status', False),
    (u"PowerShell без записи текста",
     u'Get-ChildItem d:\\Sources | Select-Object Name', False),
    (u"Set-Content с ОДИНАРНЫМИ кавычками (безопасная форма)",
     u"Set-Content -Path a.txt -Value 'plain text'", False),
    (u"чтение файла с апострофом в пути — не запись",
     u'Get-Content "d:\\a`b.md"', False),
]


def main():
    failed = 0
    for desc, cmd, should_block in CASES:
        payload = json.dumps({"tool_name": "PowerShell",
                              "tool_input": {"command": cmd}})
        r = subprocess.run([sys.executable, HOOK], input=payload,
                           capture_output=True, text=True, encoding="utf-8")
        out = (r.stdout or "") + (r.stderr or "")
        blocked = (r.returncode != 0) or ("FORBIDDEN" in out)
        ok = (blocked == should_block)
        if not ok:
            failed += 1
        print(u"  %s %-54s ловится=%s ожидали=%s" % (
            u"ok:" if ok else u"ПРОВАЛ:", desc, blocked, should_block))

    if failed:
        print(u"селфтест guard-git (№630): ПРОВАЛОВ %d" % failed)
        return 1
    print(u"селфтест guard-git (№630): %d/%d ok" % (len(CASES), len(CASES)))
    return 0


if __name__ == "__main__":
    sys.exit(main())
