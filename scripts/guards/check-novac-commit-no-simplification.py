# -*- coding: utf-8 -*-
"""scripts/guards/check-novac-commit-no-simplification.py — коммит в novac ЗАЯВЛЯЕТ, по какой
норме сделана работа, и НЕ ВНОСИТ упрощений молча (слово владельца 2026-09-13: «каждый пункт
плана 274 делается без упрощений, ровно по спеке, как для прода»).

ЧЕСТНАЯ ГРАНИЦА, И ОНА НАЗВАНА ПЕРВОЙ. «Сделано как для прода» — СУЖДЕНИЕ, и никакой страж
его не выносит. Страж, который делает вид, что судит качество, хуже отсутствующего: он даёт
чувство гарантии вместо гарантии (ровно тот класс, из-за которого в `AGENTS.md` пришлось
править строку про DCO). Поэтому здесь проверяются ТРИ свойства, каждое из которых
механическое, и ОДНО из них нельзя удовлетворить набранной фразой.

ЧТО ПРОВЕРЯЕТСЯ

  1. НОВОЕ УПРОЩЕНИЕ НЕСЁТ СРОК. В диффе индекса ищутся ДОБАВЛЕННЫЕ строки с отказом
     подмножества (образец — общий дом `novac_subset_debt.py`, оба написания). Каждая обязана
     нести этап: `(E2-b3)`, `(E4)`, `(274.7 B1b)`. Это половина, которую нельзя закрыть
     сообщением: она смотрит на КОД.

  2. НОРМА НАЗВАНА И СУЩЕСТВУЕТ. Строка `Spec:` называет D-блок либо файл спеки, и страж
     ПРОВЕРЯЕТ НАЛИЧИЕ: `Spec: D86` без D86 в `spec/decisions/` — отказ. Указатель, который
     не резолвится, — это ритуал с видом ссылки; такой уже стоил проекту критерия, назвавшего
     несуществующее место (замер 2026-08-24). Законно и `Spec: none — <причина, 5+ слов>`:
     работа может не реализовывать ни одной формы языка (перенос, инструмент, страж), и
     скрывать это за выдуманной ссылкой хуже, чем сказать прямо.

  3. ЗАЯВЛЕНИЕ ОБ УПРОЩЕНИЯХ НЕ ПРОТИВОРЕЧИТ ДИФФУ. Строка `Simplifications:` обязана быть.
     `none` законно ровно тогда, когда дифф НЕ добавил ни одного отказа подмножества; если
     добавил — `none` есть ложь, и страж её ловит СВЕРКОЙ, а не доверием. Иначе строка обязана
     нести этап хотя бы одного упрощения.

ПОЧЕМУ ИМЕННО СВЕРКА (3) — СЕРДЦЕ СТРАЖА. Донорский страж рядом честно пишет о себе:
«страж заставляет НАЗВАТЬ, а не доказывает». Это его предел, и он назван. Здесь предел сдвинут
на один шаг: заявление сверяется с кодом, и разойтись они не могут молча.

ГРАНИЦА С СОСЕДНИМ СТРАЖЕМ, названная здесь, чтобы через месяц один из двух не сняли как
дубль. `check-novac-no-undeclared-simplification` (интегратор, 2026-09-13; приедет
слиянием из `main`) судит ДЕРЕВО и ловит МАРКЕР срезанного угла — `TODO`, `FIXME`,
`todo()`, «упрощ», «для простоты» — не несущий заявки. Этот судит КОММИТ и ловит ОТКАЗ
подмножества без срока плюс сверяет заявление с диффом. Предметы разные: дерево против
диффа, маркер против отказа. Ни один не полон без другого — маркера может не быть вовсе
(так и было у `??` над `Result`: ни TODO, ни отказа со сроком, просто сужение), а отказ
может приехать в дерево мимо коммита, судимого здесь.

ЧЕГО НЕ ПРОВЕРЯЕТ: правдивость `Spec:` по существу (реализовано ли то, что названо) — это
приёмка; полноту реализации формы — её держит корпус и дифференциал; и качество кода — его
держат линт, доки, размер файла и обзор.

$1 — файл сообщения коммита; $2 — корень; $3 — override списка проиндексированных файлов
(шов самотеста, по строке); $4 — override текста диффа индекса (шов самотеста).
Оба шва ГРОМКИЕ: подмена печатается в вердикте, иначе прогон со швом неотличим от настоящего.
"""
import io
import os
import pathlib
import re
import subprocess
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import novac_subset_debt as sd  # noqa: E402

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")
sys.stderr.reconfigure(encoding="utf-8", errors="replace", newline="\n")

NAME = "check-novac-commit-no-simplification"
RE_JUDGED = re.compile(r"^novac/src/.*\.nv$")
RE_DNUM = re.compile(r"\bD([0-9]{1,4})\b")


def fail(*lines):
    for l in lines:
        print(l, file=sys.stderr)
    return 1


def staged_files(root, override):
    if override is not None:
        rows = [l.strip() for l in override.split("\n")]
    else:
        r = subprocess.run(["git", "-C", root, "diff", "--cached", "--name-only"],
                           capture_output=True)
        rows = r.stdout.decode("utf-8", "replace").split("\n")
    return [p.strip() for p in rows
            if RE_JUDGED.match(p.strip()) and not p.strip().endswith("_test.nv")]


def staged_diff(root, files, override):
    if override is not None:
        return override
    r = subprocess.run(["git", "-C", root, "diff", "--cached", "-U0", "--"] + files,
                       capture_output=True)
    return r.stdout.decode("utf-8", "replace")


def added_refusals(diff):
    """Отказы подмножества в ДОБАВЛЕННЫХ строках диффа: (строка, несёт ли срок).

    Судится добавленное, а не весь файл: коммит отвечает за то, что ВНОСИТ. Иначе любая
    правка рядом со старым бессрочным отказом была бы виновата в чужом долге, и страж стал бы
    налогом на прикосновение к файлу.
    """
    out = []
    for line in diff.replace("\r\n", "\n").split("\n"):
        if not line.startswith("+") or line.startswith("+++"):
            continue
        body = line[1:]
        if sd.is_comment(body):
            continue
        m = sd.RE_DEBT.search(body)
        if m:
            out.append((m.group(0), bool(sd.RE_STAGE.search(m.group(0)))))
    return out


def dblocks_exist(root, nums):
    """Существуют ли названные D-блоки. Отсутствующий указатель — ритуал с видом ссылки."""
    d = pathlib.Path(root) / "spec" / "decisions"
    if not d.is_dir():
        return None          # спеки рядом нет — судить наличие нечем, молчим громко
    text = []
    for f in sorted(d.glob("*.md")):
        text.append(f.read_bytes().decode("utf-8", "replace"))
    blob = "\n".join(text)
    missing = []
    for n in nums:
        if not re.search(r"\bD%s\b" % re.escape(n), blob):
            missing.append("D" + n)
    return missing


def main():
    a = sys.argv
    msg_path = a[1] if len(a) > 1 else ""
    root = a[2] if len(a) > 2 else str(pathlib.Path(__file__).resolve().parents[2])
    seam_files = a[3] if len(a) > 3 else None
    seam_diff = a[4] if len(a) > 4 else None

    # ТРИ СЛУЧАЯ, А НЕ ДВА (правило Г16). Гейт зовёт стража с `/dev/null` НАРОЧНО —
    # проверяя, что он запускается; это не поломка, и вердикт обязан так и звучать.
    # Слить «нечего судить» с «не удалось прочитать» значило бы отдать читателю одно
    # слово там, где смыслов два.
    text = None
    if msg_path and os.path.exists(msg_path):
        try:
            text = io.open(msg_path, encoding="utf-8", errors="replace").read()
        except OSError as e:
            return fail(f"{NAME}: FAIL — файл сообщения не читается: {e}")
    if not text or not text.strip():
        print(f"{NAME} ok: судить нечего (сообщение пусто или его нет — так зовёт ГЕЙТ, "
              f"проверяя запускаемость; настоящий коммит приходит через хук commit-msg)")
        return 0

    files = staged_files(root, seam_files)
    seam_note = ""
    if seam_files is not None or seam_diff is not None:
        seam_note = " [ШОВ САМОТЕСТА: вход подменён]"
    if not files:
        print(f"{NAME} ok: судить нечего (novac/src не в индексе){seam_note}")
        return 0

    msg = text.replace("\r", "")
    diff = staged_diff(root, files, seam_diff)
    added = added_refusals(diff)
    undated = [lit for lit, staged in added if not staged]

    # --- 1. НОВОЕ УПРОЩЕНИЕ БЕЗ СРОКА: смотрит на КОД, фразой не закрывается ----------
    if undated:
        return fail(
            f"{NAME}: FAIL — коммит ВНОСИТ упрощение без срока ({len(undated)} шт.):",
            *[f"    {l[:110]}" for l in undated],
            "  План 274 делается без упрощений, ровно по спеке, как для прода (владелец,",
            "  2026-09-13). Упрощение законно, только когда НАЗВАНО и несёт этап, к которому",
            "  исчезнет: «... (E2-b3)», «... (E4)», «... (274.7 B1b)». Без этапа обещание",
            "  «пока не поддерживается» ничем не обеспечено и становится нормой.",
            "  Либо реализуй форму целиком по спеке, либо назови срок прямо в тексте отказа.")

    # --- 2. НОРМА НАЗВАНА И РЕЗОЛВИТСЯ -----------------------------------------------
    spec_line = next((l for l in msg.split("\n") if l.startswith("Spec:")), None)
    if spec_line is None:
        return fail(
            f"{NAME}: FAIL — коммит меняет novac/src, а строки 'Spec:' в сообщении нет.",
            "  Назови норму, по которой сделана работа: 'Spec: D86 04-effects.md — `??`",
            "  fallback for Result/Option, both arms implemented'.",
            "  Или честно: 'Spec: none — <почему работа не реализует формы языка, 5+ слов>'",
            "  (перенос, инструмент, страж — законные случаи).")
    body = spec_line[len("Spec:"):].strip()
    if re.match(r"(?i)^none\s*(—|--|-)", body):
        reason = re.sub(r"(?i)^none\s*(—|--|-)\s*", "", body)
        if len(reason.split()) < 5:
            return fail(f"{NAME}: FAIL — 'Spec: none' без причины (нужно 5+ слов): «{spec_line}»")
    else:
        nums = RE_DNUM.findall(body)
        if not nums and len(body.split()) < 2:
            return fail(
                f"{NAME}: FAIL — 'Spec:' без указателя: «{spec_line}».",
                "  Назови D-блок («D86») либо файл и раздел спеки, по которым сверяется работа.")
        if nums:
            missing = dblocks_exist(root, nums)
            if missing is None:
                pass
            elif missing:
                return fail(
                    f"{NAME}: FAIL — 'Spec:' называет D-блок, которого в spec/decisions НЕТ: "
                    f"{', '.join(missing)}.",
                    "  Указатель, который не резолвится, — ритуал с видом ссылки: он выглядит",
                    "  проверяемым, а проверить его нельзя. Назови существующий блок или",
                    "  спроси номер у интегратора, если блок ещё не заведён.")

    # --- 3. ЗАЯВЛЕНИЕ СВЕРЯЕТСЯ С ДИФФОМ ---------------------------------------------
    simp_line = next((l for l in msg.split("\n") if l.startswith("Simplifications:")), None)
    if simp_line is None:
        return fail(
            f"{NAME}: FAIL — коммит меняет novac/src, а строки 'Simplifications:' нет.",
            "  'Simplifications: none' — если работа сделана по норме целиком;",
            "  иначе назови каждое упрощение и его этап: 'Simplifications: Result left side",
            "  refused (E2-b3)'.")
    sbody = simp_line[len("Simplifications:"):].strip()
    says_none = bool(re.match(r"(?i)^none\b", sbody))
    if says_none and added:
        return fail(
            f"{NAME}: FAIL — сообщение говорит 'Simplifications: none', а дифф ВНОСИТ "
            f"{len(added)} отказ(а) подмножества:",
            *[f"    {lit[:110]}" for lit, _ in added],
            "  Заявление и код разошлись. Отказ подмножества И ЕСТЬ упрощение — назови его",
            "  в строке вместе с этапом, либо убери отказ, реализовав форму по спеке.")
    if not says_none and not sd.RE_STAGE.search(sbody):
        return fail(
            f"{NAME}: FAIL — 'Simplifications:' называет упрощение без этапа: «{simp_line}».",
            "  У каждого упрощения обязан быть срок: «(E2-b3)», «(E4)», «(274.7 B1b)».")

    dated = len(added) - len(undated)
    print(f"{NAME} ok: норма названа и резолвится; упрощений внесено {len(added)} "
          f"(все со сроком: {dated}); заявление сверено с диффом{seam_note}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
