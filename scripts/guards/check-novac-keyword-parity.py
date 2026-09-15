# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-keyword-parity.py — слово языка, которого лексер не
знает, обязано быть ОБЪЯВЛЕНО (план docs/plans/274.7-subset-to-spec.md, волна В10).

ПОЧЕМУ. Ключевое слово, которого лексер не знает, не молчит — оно ЛЖЁТ. `loop`
лексится именем, `{` после имени читается литералом записи, и автор получает
«record constructor only as a binding initializer» про цикл (реестр №1089). Замер
2026-09-13 нашёл ВТОРУЮ маску того же класса: `and` и `not` тоже лексятся именами,
и отказ звучит «unknown name: nothing with this name is bound at this point» —
утверждение о ПРОГРАММЕ, а не о себе. Эта маска хуже первой: «не форма языка» хотя
бы указывает на синтаксис, а «неизвестное имя» отправляет автора искать опечатку в
слове, которое он написал верно.

ЧЕГО ЭТОТ СТРАЖ НЕ ДЕЛАЕТ, и это его главное свойство: он НЕ требует, чтобы лексер
знал все слова списка. Требовать этого было бы неверно по существу. В нормативном
списке есть `true`, `false` и `Self` — булевы литералы лексер обязан знать не
ключевыми словами, а `Self` есть тип; вписать их в `keyword_kind` значило бы
испортить устройство ради числа. Список к тому же частично протух: `let`
ретрактирован (D184), `external` заменён на `extern "nova"`, и novac знает именно
`extern`. Разность множеств тут негодна В ОБЕ СТОРОНЫ.

ЧТО СУДИТСЯ: каждая разница между нормативным списком и таблицей лексера обязана
быть ОБЪЯВЛЕНА — одной строкой в блоке объявлений рядом с самой таблицей
(`novac/src/lex/lex.nv`). Объявление говорит, ПОЧЕМУ слова нет:
    KEYWORD-PARITY: loop -- debt (274.7 B10)
    KEYWORD-PARITY: true -- by design (a literal, not a keyword of the table)
    KEYWORD-PARITY: let -- retracted (D184: `let` -> `ro`/`mut`)
Вид объявления — одно из трёх слов: `debt` (со скобкой-этапом), `by design`,
`retracted`. Необъявленная разница краснеет.

ПОЧЕМУ ОБЪЯВЛЕНИЕ ЖИВЁТ В `lex.nv`, А НЕ В БАЗЕ РЯДОМ СО СТРАЖЕМ. Его читает тот,
кто смотрит на таблицу и спрашивает «а где `loop`?». Ответ обязан лежать там же, где
вопрос; база рядом со стражем — это место для ЧИСЛА, а не для причины. База здесь
тоже есть, но она держит ровно число необъявленных, и только.

ПОЧЕМУ ХРАПОВИК, А НЕ НОЛЬ. Разниц двадцать девять, и каждое объявление — СУЖДЕНИЕ:
долг это или устройство, и если долг — к какой волне. Проставить двадцать девять
таких решений росчерком значило бы поставить правдоподобные слова вместо
продуманных — ровно та ошибка, которую база долга подмножества уже однажды назвала у
себя. Разбор идёт волной; храповик запрещает РОСТ и опускается за каждым объявлением.

ПРАВИЛО СЧЁТА (без него число несравнимо):
  * сторона спеки — слова в обратных кавычках под заголовком «Полный список
    зарезервированных слов» в `spec/decisions/03-syntax.md`, от заголовка до
    следующего заголовка того же уровня, минус `_` (не слово);
  * сторона novac — строковые образцы `match` двери `keyword_kind` в
    `novac/src/lex/lex.nv`; это единственный дом написаний по её же комментарию.

ОБРАТНАЯ РАЗНИЦА СЧИТАЕТСЯ ТОЖЕ. В лексере есть `consume`, `extern`, `unsafe`,
которых в списке спеки нет, — и это не ошибка лексера, а неполнота списка (`consume`
стал токеном 2026-08-23 после замера против оракула). Она объявляется тем же
блоком и тем же словом `by design` либо `spec-gap`.

ИСПОЛЬЗОВАНИЕ:
  python scripts/guards/check-novac-keyword-parity.py [КОРЕНЬ]
ПЕРЕМЕННЫЕ:
  NOVA_KWPARITY_BASELINE — путь к базе, по умолчанию
                           scripts/guards/keyword-parity.baseline
"""
import os
import re
import sys
from pathlib import Path

# Вывод по-русски обязан доезжать ДО ЛОГА, а не только до терминала: на Windows stdout
# питона по умолчанию cp1251, и совет стража приходит в лог гейта строкой знаков вопроса
# ровно тогда, когда его читают. Поймано на себе при первом же прогоне; соседние стражи
# делают это первой строкой, и мой не делал только потому, что писался не по их образцу.
sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-novac-keyword-parity"

SPEC_HEADING = "Полный список зарезервированных слов"

# Вид объявления. `debt` обязан нести скобку с этапом -- иначе это обещание без срока,
# ровно то, что запрещает соседний храповик долга подмножества.
RE_DECL = re.compile(r"KEYWORD-PARITY:\s*([A-Za-z_][A-Za-z_0-9]*)\s*--\s*(.+?)\s*$")
KINDS = ("debt", "by design", "retracted", "spec-gap")
# Волна живёт и в подплане (`274.7 B10`), и в РОДИТЕЛЬСКОМ плане (`274 B3a`). Образец,
# знающий только подплан, заставил бы автора написать неверную ссылку ради зелени --
# страж, у которого правильное написание красное, учит обходить себя. Поймано на себе
# 2026-09-13: объявление `protocol` указывало на `274.7 B3a`, волны с таким номером в
# 274.7 нет, а образец её пропускал, потому что судил ФОРМУ ссылки, а не её цель.
RE_STAGE = re.compile(r"\(([^()]*\b(?:E\d[\w-]*|274(?:\.\d+)?\s+B\d+\w*)[^()]*)\)")


def spec_keywords(root: Path):
    """Слова нормативного списка. Возвращает (множество, номер строки заголовка)."""
    p = root / "spec/decisions/03-syntax.md"
    if not p.is_file():
        return None, 0
    lines = p.read_text(encoding="utf-8", errors="replace").replace("\r\n", "\n").split("\n")
    start = None
    for i, l in enumerate(lines):
        if l.startswith("#### ") and SPEC_HEADING in l:
            start = i
            break
    if start is None:
        return None, 0
    end = len(lines)
    for i in range(start + 1, len(lines)):
        if lines[i].startswith("#### "):
            end = i
            break
    block = "\n".join(lines[start:end])
    words = {w for w in re.findall(r"`([A-Za-z_][A-Za-z_0-9]*)`", block) if w != "_"}
    return words, start + 1


def lexer_keywords(root: Path):
    """Написания из двери `keyword_kind` -- единственного их дома."""
    p = root / "novac/src/lex/lex.nv"
    if not p.is_file():
        return None
    text = p.read_text(encoding="utf-8", errors="replace").replace("\r\n", "\n")
    m = re.search(r"fn keyword_kind\(t str\) -> TokenKind => match t \{(.*?)\n\}", text, re.S)
    if not m:
        return None
    return set(re.findall(r'^\s*"([a-z_]+)" =>', m.group(1), re.M))


def declarations(root: Path):
    """Объявления разниц. Живут рядом с таблицей, потому что там же живёт вопрос."""
    p = root / "novac/src/lex/lex.nv"
    text = p.read_text(encoding="utf-8", errors="replace").replace("\r\n", "\n")
    out, bad = {}, []
    for i, line in enumerate(text.split("\n"), 1):
        m = RE_DECL.search(line)
        if not m:
            continue
        word, rest = m.group(1), m.group(2)
        kind = next((k for k in KINDS if rest.startswith(k)), None)
        if kind is None:
            bad.append((i, word, f"вид не назван ({'/'.join(KINDS)}): {rest[:60]}"))
            continue
        if kind == "debt" and not RE_STAGE.search(rest):
            bad.append((i, word, "debt без этапа в скобках"))
            continue
        out[word] = (kind, i)
    return out, bad


def read_baseline(path: Path, key: str):
    if not path.is_file():
        return None
    for line in path.read_text(encoding="utf-8", errors="replace").splitlines():
        line = line.strip()
        if line.startswith(key + "="):
            try:
                return int(line.split("=", 1)[1].strip())
            except ValueError:
                return None
    return None


def main() -> int:
    root = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else Path(__file__).resolve().parents[2]
    baseline = Path(os.environ.get("NOVA_KWPARITY_BASELINE",
                                   root / "scripts/guards/keyword-parity.baseline"))

    spec, spec_line = spec_keywords(root)
    if spec is None:
        print(f"{NAME}: FAIL — нет нормативного списка в spec/decisions/03-syntax.md", file=sys.stderr)
        return 1
    lexer = lexer_keywords(root)
    if lexer is None:
        print(f"{NAME}: FAIL — не найдена дверь keyword_kind в novac/src/lex/lex.nv",
              file=sys.stderr)
        return 1

    decl, bad = declarations(root)
    diff = sorted((spec - lexer) | (lexer - spec))
    undeclared = [w for w in diff if w not in decl]

    want = read_baseline(baseline, "undeclared")
    if want is None:
        print(f"{NAME}: FAIL — в базе {baseline} нет ключа undeclared=", file=sys.stderr)
        return 1

    if bad:
        print(f"{NAME}: FAIL — объявление есть, но оно не отвечает на вопрос:", file=sys.stderr)
        for i, w, why in bad:
            print(f"    lex.nv:{i}  {w}: {why}", file=sys.stderr)
        print("  Вид — одно из: debt (со скобкой-этапом), by design, retracted, spec-gap.", file=sys.stderr)
        return 1

    if len(undeclared) > want:
        print(f"{NAME}: FAIL — необъявленных разниц стало БОЛЬШЕ: {len(undeclared)} > базы {want}", file=sys.stderr)
        for w in undeclared:
            side = "spec" if w in spec else "lexer"
            print(f"    {w}  ({side})", file=sys.stderr)
        print("  Ключевое слово, которого лексер не знает, не молчит — оно ЛЖЁТ (№1089).", file=sys.stderr)
        print("  Объяви разницу строкой рядом с таблицей в lex.nv:", file=sys.stderr)
        print("    // KEYWORD-PARITY: loop -- debt (274.7 B10)", file=sys.stderr)
        return 1

    extra = ""
    if len(undeclared) < want:
        extra = f" — храповик можно опустить до {len(undeclared)}"
    kinds = {}
    for w, (k, _) in decl.items():
        kinds[k] = kinds.get(k, 0) + 1
    shape = ", ".join(f"{k}: {v}" for k, v in sorted(kinds.items())) or "нет"
    print(f"{NAME} ok: список спеки {len(spec)} (03-syntax.md:{spec_line}), лексер {len(lexer)}, разниц {len(diff)}, объявлено {len(decl)} ({shape}), необъявленных {len(undeclared)} (база {want}){extra}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
