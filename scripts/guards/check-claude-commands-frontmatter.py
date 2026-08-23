# -*- coding: utf-8 -*-
"""scripts/guards/check-claude-commands-frontmatter.py — шапка слэш-команды
обязана ПАРСИТЬСЯ (заведён 2026-08-23).

ЗАЧЕМ. Сломанная шапка не выглядит сломанной. Файл в `.claude/commands/`
загружается и команда вызывается, но описание и список инструментов теряются —
то есть механизм работает наполовину и молча. Прецедент дня заведения: из
четырёх написанных за вечер команд ДВЕ (`/recheck`, `/flow`) несли двоеточие
внутри значения `description`, и YAML читал остаток строки как вложенное
отображение; `argument-hint`, начинающийся с `[`, читался как список. Нашёл это
владелец глазами в предпросмотре markdown — механизма, который бы поймал, не
было.

ЧТО ПРОВЕРЯЕТ у каждого `.claude/commands/**/*.md`:
  * файл начинается строкой `---` и шапка закрыта второй строкой `---`;
  * блок между ними парсится как YAML-отображение;
  * `description` присутствует и не пуст — это текст, который видно в меню
    команд, и без него команда безымянна.

ВТОРОЕ ПРАВИЛО (2026-08-23): команда `*-recheck.md` обязана СОСЛАТЬСЯ на общее тело
`docs/dev/recheck-common.md`, и файл обязан существовать. Повод: `/novac-recheck` и
`/oracle-recheck` выросли из одного списка и за вечер получили четыре одинаковые
правки — каждую руками в два файла. Общую часть вынесли в один файл; новый способ
сломаться — команда, потерявшая ссылку: она продолжит работать и просто перестанет
проверять половину, молча.

НЕ ПРОВЕРЯЕТ: смысл описания и тело команды (их судит ревью); наличие
`argument-hint`/`allowed-tools` — они необязательны; совпадение текста общей части с
тем, что было в командах (это работа ревью, а не грепа).

Отсутствие папки или ноль файлов — зелёное молчание: судить нечего.
Отсутствие PyYAML — КРАСНОЕ: страж, который не может проверить, обязан
сказать это громко, а не отвечать «ok».

$1 — корень репозитория; $2 — override папки команд (шов самотеста).
"""
import pathlib
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-claude-commands-frontmatter"
COMMON = "docs/dev/recheck-common.md"


def frontmatter_of(text):
    """Текст шапки или причина отказа. Возвращает (block, error)."""
    lines = text.replace("\r\n", "\n").replace("\r", "\n").split("\n")
    if not lines or lines[0].strip() != "---":
        return None, "нет шапки: первая строка не `---`"
    for i in range(1, len(lines)):
        if lines[i].strip() == "---":
            return "\n".join(lines[1:i]), None
    return None, "шапка не закрыта второй строкой `---`"


def main(argv):
    root = pathlib.Path(argv[1] if len(argv) > 1 else ".").resolve()
    cmd_dir = pathlib.Path(argv[2]).resolve() if len(argv) > 2 else root / ".claude" / "commands"

    if not cmd_dir.is_dir():
        print("%s ok: судить нечего (нет %s)" % (NAME, cmd_dir))
        return 0

    files = sorted(p for p in cmd_dir.rglob("*.md") if p.is_file())
    if not files:
        print("%s ok: судить нечего (ноль файлов команд)" % NAME)
        return 0

    try:
        import yaml
    except ImportError:
        print("%s FAIL: PyYAML не установлен — страж не может проверить шапки "
              "(молчаливое «ok» здесь хуже красноты)" % NAME)
        return 1

    bad = []
    for path in files:
        rel = path.relative_to(cmd_dir)
        text = path.read_text(encoding="utf-8", errors="replace")
        block, err = frontmatter_of(text)
        if err:
            bad.append((rel, err))
            continue
        try:
            data = yaml.safe_load(block)
        except Exception as exc:
            first = str(exc).replace("\n", " ")[:160]
            bad.append((rel, "шапка не парсится: %s" % first))
            continue
        if not isinstance(data, dict):
            bad.append((rel, "шапка не отображение (получилось %s)" % type(data).__name__))
            continue
        desc = data.get("description")
        if not isinstance(desc, str) or not desc.strip():
            bad.append((rel, "нет непустого `description` — команда будет безымянной в меню"))

        # Правило 2: у команд-перепроверок общее тело — одно, и на него ссылаются.
        if path.name.endswith("-recheck.md"):
            if COMMON not in text:
                bad.append((rel, "не ссылается на общее тело `%s` — половина проверок "
                                 "тихо выпадет" % COMMON))
            elif not (root / COMMON).is_file():
                bad.append((rel, "ссылается на `%s`, которого нет в дереве" % COMMON))

    if bad:
        print("%s FAIL: шапка команды не читается (%d из %d)" % (NAME, len(bad), len(files)))
        for rel, why in bad:
            print("  %s: %s" % (rel, why))
        print("  Чаще всего причина одна: двоеточие или `[` внутри значения. "
              "Возьми значение в двойные кавычки.")
        return 1

    print("%s ok: шапки читаются, у каждой есть описание (%d)" % (NAME, len(files)))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
