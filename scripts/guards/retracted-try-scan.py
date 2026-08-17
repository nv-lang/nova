# -*- coding: utf-8 -*-
"""Ядро check-retracted-try-semantics: считает по зонам осадок СНЯТОЙ трактовки
оператора `?`.

D85 сделал `?` return-стилем: он законен только в функции, возвращающей
`Result[T,E]`/`Option[T]`, эффекта не несёт, а внутри функции, объявившей
`Fail`, — ошибка `E_TRY_IN_FAIL_FN`. Тексты, написанные ДО D85, описывают `?`
как сахар над `throw` и как «требующий `Fail[E]` в подписи».

Считаются ДВА семейства, и это не педантизм — их ловят разные инструменты:

  (A) ЛЕКСИЧЕСКОЕ — проза, утверждающая старую трактовку. Ловится регуляркой
      по строке.

  (B) СТРУКТУРНОЕ — код-примеры: `?` внутри функции, объявившей `Fail`.
      Регуляркой по строке НЕ ловится в принципе: признак разнесён между
      строкой подписи и строкой тела. Поэтому ядро — питон, а не grep
      (тот же довод, что у registry-routes-scan.py).

ЧЕГО СЧИТАТЬ НЕЛЬЗЯ (иначе страж заставит удалять правду):
  * форма D196 `consume X = expr? { body }` — ЗАКОННА, чекер называет её в
    тексте самой ошибки E_TRY_IN_FAIL_FN как рекомендуемый выход;
  * `??` — живой оператор, не `?`;
  * `desugar` — не слово `sugar`: «`?` desugar'ится в `match` + ранний
    `return None`» описывает ДЕЙСТВУЮЩУЮ норму, а не снятую;
  * строки, которые САМИ помечают форму снятой (амендменты, «СНЯТО»,
    «retracted», ссылки на D85/D196) — их вычитает EXC.
"""
import io
import os
import re
import sys

# ── (A) лексическое семейство ────────────────────────────────────────────────
# Якорим на ОПЕРАТОР, а не на голый знак вопроса: русская проза полна вопросов.
LEX = re.compile(
    u"(`\\?`[^\\n]{0,80}(сахар|(?<!de)sugar|throw-стил|throw style|бросает|throws)"
    u"|(`expr\\?`|`\\?`)[^\\n]{0,80}(требует|requires)[^\\n]{0,20}`?Fail"
    u"|(требует|requires)[^\\n]{0,20}`?Fail\\[E\\]`?[^\\n]{0,40}(если|if)[^\\n]{0,30}`?(Result|expr)"
    u")",
    re.IGNORECASE,
)

# Строки, которые сами говорят «эта форма снята» — не нарушение, а его пометка.
EXC = re.compile(
    u"(E_TRY_IN_FAIL_FN|retract|СНЯТ|снят|ПЕРЕКРЫТО|перекрыт|УСТАРЕВ|устарев"
    u"|АМЕНДМЕНТ|Амендмент|амендмент|amend|SUPERSEDED|D85|D196|D445"
    u"|было|раньше|до сегодня|pre-D85|№442|No\\. 442)",
    re.IGNORECASE,
)

# ── (B) структурное семейство ────────────────────────────────────────────────
FN = re.compile(u"^\\s*(export\\s+)?(priv\\([a-z]+\\)\\s+)?fn\\b")
FAIL_IN_SIG = re.compile(u"\\bFail\\b")
# законная форма D196: consume IDENT = <что-то>? {
D196 = re.compile(u"^\\s*consume\\s+[A-Za-z_][A-Za-z0-9_]*\\s*=\\s*.*\\?\\s*\\{")
# оператор `?` — после идентификатора/закрывающей скобки и НЕ `??`
TRY_OP = re.compile(u"[A-Za-z0-9_\\)\\]]\\?(?!\\?)")

ZONES = [u"docs/guide", u"spec", u"docs/plans", u"docs/dev"]


def scan_file(path):
    """(lex_hits, struct_hits) — списки (номер строки, текст)."""
    try:
        src = io.open(path, encoding="utf-8", errors="replace").read().split(u"\n")
    except Exception:
        return [], []

    lex, struct = [], []
    state = {"in_block": False, "fn_head": None, "body": []}

    def close_fn():
        head = state["fn_head"]
        if head is None or not FAIL_IN_SIG.search(head):
            return
        for ln, txt in state["body"]:
            if D196.match(txt):
                continue
            if TRY_OP.search(txt) and not EXC.search(txt):
                struct.append((ln, txt.strip()[:100]))

    for i, line in enumerate(src, 1):
        if line.startswith(u"```"):
            if state["in_block"]:
                close_fn()
                state["fn_head"], state["body"] = None, []
            state["in_block"] = not state["in_block"]
            continue

        if state["in_block"]:
            if FN.match(line):
                close_fn()
                state["fn_head"], state["body"] = line, []
            elif state["fn_head"] is not None:
                state["body"].append((i, line))
        elif LEX.search(line) and not EXC.search(line):
            lex.append((i, line.strip()[:100]))

    if state["in_block"]:
        close_fn()
    return lex, struct


def scan_zone(root, zone):
    lex_n = struct_n = 0
    hits = []
    base = os.path.join(root, *zone.split(u"/"))
    if not os.path.isdir(base):
        return 0, 0, hits
    for dirpath, _dirnames, filenames in os.walk(base):
        for fn in sorted(filenames):
            if not fn.endswith(".md"):
                continue
            p = os.path.join(dirpath, fn)
            lex, struct = scan_file(p)
            lex_n += len(lex)
            struct_n += len(struct)
            rel = os.path.relpath(p, root).replace(os.sep, u"/")
            for ln, txt in lex:
                hits.append(u"  LEX    %s:%d  %s" % (rel, ln, txt))
            for ln, txt in struct:
                hits.append(u"  STRUCT %s:%d  %s" % (rel, ln, txt))
    return lex_n, struct_n, hits


def main():
    # Консоль может быть в cp1251: цитаты из русских доков её роняют.
    try:
        sys.stdout.reconfigure(encoding="utf-8", errors="backslashreplace")
    except Exception:
        pass
    root = sys.argv[1] if len(sys.argv) > 1 else "."
    show = "--list" in sys.argv
    w = sys.stdout.write
    all_hits = []
    for zone in ZONES:
        lex_n, struct_n, hits = scan_zone(root, zone)
        key = zone.replace(u"docs/", u"").replace(u"/", u"_")
        w("%s=%d\n" % (key, lex_n + struct_n))
        w("%s_lex=%d\n" % (key, lex_n))
        w("%s_struct=%d\n" % (key, struct_n))
        all_hits.extend(hits)
    if show:
        w("\n".join(all_hits) + ("\n" if all_hits else ""))
    return 0


if __name__ == "__main__":
    sys.exit(main())
