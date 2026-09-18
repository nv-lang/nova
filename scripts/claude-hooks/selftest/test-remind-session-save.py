# -*- coding: utf-8 -*-
"""Самотест хука-напоминания о сохранении сессии (план 290 п.3).

ДОКАЗЫВАЕТ ОБЕ СТОРОНЫ, как требует П16: свежая записка — молчит; устаревшая —
говорит; повтор внутри остуды — молчит снова. Третий случай здесь не украшение:
напоминание на каждый вызов инструмента читается как фон, и тогда его не видно
ровно тогда, когда оно нужно.

Работает на ВРЕМЕННОМ дереве и настоящих файлов проекта не трогает.
"""
import io
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time

sys.stdout.reconfigure(encoding="utf-8", errors="replace", newline="\n")

HOOK = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..",
                    "remind-session-save.py")
HANDOFF_REL = os.path.join("docs", "dev", "prompts", "integrator-handoff.md")
CARINA_REL = os.path.join("docs", "dev", "prompts", "carina-handoff.md")
CONTROLLER_REL = os.path.join("docs", "dev", "prompts", "controller-handoff.md")


def make_tree(branch):
    """Настоящий git с НАЗВАННОЙ веткой: роль хук берёт именно у git."""
    root = tempfile.mkdtemp(prefix="nova-remind-")
    subprocess.run(["git", "init", "-q", "-b", branch, root],
                   capture_output=True)
    return root

fails = 0
cases = 0


def run(root, cwd=None, project_dir=None):
    """`cwd` — дерево, ИЗ КОТОРОГО работает окно; `project_dir` — главное.

    Раньше параметра не было, и оба всегда совпадали: самотест не мог
    отличить «возраст взят из моего дерева» от «из главного» — то есть не
    мерил того, что хук на самом деле делает.
    """
    env = dict(os.environ)
    env["CLAUDE_PROJECT_DIR"] = project_dir or root
    p = subprocess.run([sys.executable, HOOK], input=b"{}",
                       capture_output=True, env=env, cwd=cwd or root)
    return p.stdout.decode("utf-8", "replace").strip(), p.returncode


def ok(msg):
    global cases
    cases += 1
    print("  ok   %s" % msg)


def bad(msg):
    global fails, cases
    cases += 1
    fails += 1
    print("  ПРОВАЛ %s" % msg)


root = make_tree("main")
try:
    path = os.path.join(root, HANDOFF_REL)
    os.makedirs(os.path.dirname(path))
    io.open(path, "w", encoding="utf-8").write(u"# записка\n")

    # (1) свежая записка — молчание
    out, rc = run(root)
    if out == "" and rc == 0:
        ok(u"свежая записка — хук молчит")
    else:
        bad(u"свежая записка, а хук заговорил: %r" % out[:120])

    # (2) записка старше часа — говорит, и НАЗЫВАЕТ возраст
    st = os.stat(path)
    os.utime(path, (st.st_atime, time.time() - 7200))
    out, rc = run(root)
    if not out:
        bad(u"устаревшая записка, а хук промолчал")
    else:
        try:
            ctx = json.loads(out)["hookSpecificOutput"]["additionalContext"]
        except Exception as e:
            ctx = ""
            bad(u"вывод не разбирается как JSON хука: %s" % e)
        if ctx and u"/save" in ctx and u"2ч" in ctx:
            ok(u"устаревшая записка — хук называет возраст и команду")
        elif ctx:
            bad(u"хук заговорил, но без возраста или команды: %r" % ctx[:120])

    # (3) повтор внутри остуды — снова молчание
    out2, _ = run(root)
    if out2 == "":
        ok(u"повтор внутри остуды — молчит (иначе напоминание станет фоном)")
    else:
        bad(u"повтор внутри остуды заговорил снова")

    # (4) записки нет вовсе — молчит (чужой проект, свежий клон)
    os.remove(path)
    out3, rc3 = run(root)
    if out3 == "" and rc3 == 0:
        ok(u"записки нет — хук молчит, а не падает")
    else:
        bad(u"без записки хук отреагировал: rc=%d out=%r" % (rc3, out3[:80]))
    # (4) РОЛЬ: на ветке Карины хук зовёт ЕЁ команду и ЕЁ записку, а не мои.
    #     Без этого случая правка «хук учитывает роль» осталась бы недоказанной:
    #     до неё он звал /save в любом окне, включая то, которому /save не адресована.
    croot = make_tree("p274-novac")
    cpath = os.path.join(croot, CARINA_REL)
    os.makedirs(os.path.dirname(cpath))
    io.open(cpath, "w", encoding="utf-8").write(u"# записка окна\n")
    st = os.stat(cpath)
    os.utime(cpath, (st.st_atime, time.time() - 7200))
    out, rc = run(croot)
    ctx = ""
    if out:
        try:
            ctx = json.loads(out)["hookSpecificOutput"]["additionalContext"]
        except Exception:
            ctx = ""
    if ctx and u"/save" in ctx and u"carina-handoff" in ctx:
        ok(u"ветка Карины — хук зовёт ЕЁ команду и ЕЁ записку")
    else:
        bad(u"на ветке Карины хук назвал не то: %r" % (ctx or out)[:140])
    shutil.rmtree(croot, ignore_errors=True)

    # (6) КОНТРОЛЁР НА `main` — записка СВОЯ, а не интеграторская. Ветка роли не
    #     различает: контролёр работает в ГЛАВНОМ дереве, то есть на `main`, как
    #     и интегратор, — и до правки 2026-09-18 он получал напоминание про чужой
    #     файл. Роль берётся ВИЗИТКОЙ, сопоставление по session_id.
    kroot = make_tree("main")
    kpath = os.path.join(kroot, CONTROLLER_REL)
    os.makedirs(os.path.dirname(kpath))
    io.open(kpath, "w", encoding="utf-8").write(u"# записка контролёра\n")
    st = os.stat(kpath)
    os.utime(kpath, (st.st_atime, time.time() - 7200))
    # Записка ИНТЕГРАТОРА тоже лежит и тоже протухла: без визитки хук назвал бы
    # именно её, и клетка обязана отличать «выбрал верную» от «нашёл хоть какую».
    ipath = os.path.join(kroot, HANDOFF_REL)
    io.open(ipath, "w", encoding="utf-8").write(u"# записка интегратора\n")
    st = os.stat(ipath)
    os.utime(ipath, (st.st_atime, time.time() - 7200))
    gitdir = subprocess.run(["git", "-C", kroot, "rev-parse", "--git-common-dir"],
                            capture_output=True, text=True).stdout.strip()
    if not os.path.isabs(gitdir):
        gitdir = os.path.join(kroot, gitdir)
    io.open(os.path.join(gitdir, "nova-session-controller.card"), "w",
            encoding="utf-8").write(
        u"role=controller\nname=nova-test\nsession_id=SID-TEST\n")
    env_sid = dict(os.environ)
    env_sid["CLAUDE_PROJECT_DIR"] = kroot
    env_sid["CLAUDE_CODE_SESSION_ID"] = "SID-TEST"
    p = subprocess.run([sys.executable, HOOK], input=b"{}",
                       capture_output=True, env=env_sid, cwd=kroot)
    out = p.stdout.decode("utf-8", "replace").strip()
    ctx = ""
    if out:
        try:
            ctx = json.loads(out)["hookSpecificOutput"]["additionalContext"]
        except Exception:
            ctx = ""
    if ctx and u"controller-handoff" in ctx and u"integrator-handoff" not in ctx:
        ok(u"контролёр на main — хук зовёт ЕГО записку, не интеграторскую")
    else:
        bad(u"контролёру названа не та записка: %r" % (ctx or out)[:140])
    shutil.rmtree(kroot, ignore_errors=True)

    # (8) ОКНО КАРИНЫ, РАБОТАЮЩЕЕ В WORKTREE. Находка самого окна 2026-09-18:
    #     ему пришло «ЗАПИСКА ИНТЕГРАТОРА», хотя оно на `p274-novac`. Таблица
    #     веток верна — неверен КОРЕНЬ: у окна в worktree `CLAUDE_PROJECT_DIR`
    #     указывает на ГЛАВНОЕ дерево, и ветка читается оттуда, то есть `main`.
    #     Клетка воспроизводит именно это: дерево на `main`, а визитка говорит
    #     `carina`, — и адресатом обязана стать ЕЁ записка.
    wroot = make_tree("main")
    wcar = os.path.join(wroot, CARINA_REL)
    os.makedirs(os.path.dirname(wcar))
    io.open(wcar, "w", encoding="utf-8").write(u"# записка окна Карины\n")
    st = os.stat(wcar)
    os.utime(wcar, (st.st_atime, time.time() - 7200))
    # Записка интегратора тоже протухла: без визитки хук назвал бы ЕЁ.
    wint = os.path.join(wroot, HANDOFF_REL)
    io.open(wint, "w", encoding="utf-8").write(u"# записка интегратора\n")
    st = os.stat(wint)
    os.utime(wint, (st.st_atime, time.time() - 7200))
    wgit = subprocess.run(["git", "-C", wroot, "rev-parse", "--git-common-dir"],
                          capture_output=True, text=True).stdout.strip()
    if not os.path.isabs(wgit):
        wgit = os.path.join(wroot, wgit)
    io.open(os.path.join(wgit, "nova-session-carina.card"), "w",
            encoding="utf-8").write(
        u"role=carina\nname=nova-test\nsession_id=SID-CARINA\n")
    wenv = dict(os.environ)
    wenv["CLAUDE_PROJECT_DIR"] = wroot
    wenv["CLAUDE_CODE_SESSION_ID"] = "SID-CARINA"
    wp = subprocess.run([sys.executable, HOOK], input=b"{}",
                        capture_output=True, env=wenv, cwd=wroot)
    wout = wp.stdout.decode("utf-8", "replace").strip()
    wctx = ""
    if wout:
        try:
            wctx = json.loads(wout)["hookSpecificOutput"]["additionalContext"]
        except Exception:
            wctx = ""
    if wctx and u"carina-handoff" in wctx and u"integrator-handoff" not in wctx:
        ok(u"окно Карины при корне главного дерева — записка ЕГО, не интегратора")
    else:
        bad(u"окну Карины названа не та записка: %r" % (wctx or wout)[:140])
    shutil.rmtree(wroot, ignore_errors=True)

    # (9) ЧУЖОЕ ОКНО В ВЕТКЕ РОЛИ. Находка окна nova-1a 2026-09-18: разовое окно,
    #     работавшее в ГЛАВНОМ дереве по просьбе владельца, получило напоминание
    #     про записку ИНТЕГРАТОРА. Ветка `main` роль называет верно, но окно не
    #     её ведёт, и исполнить указание может только испортив чужой файл.
    #     Зеркало клетки (8): там роль не совпадала с веткой, здесь окно без роли
    #     сидит в ветке роли.
    froot = make_tree('main')
    fint = os.path.join(froot, HANDOFF_REL)
    os.makedirs(os.path.dirname(fint))
    io.open(fint, 'w', encoding='utf-8').write(u'# записка интегратора\n')
    st = os.stat(fint)
    os.utime(fint, (st.st_atime, time.time() - 7200))
    fgit = subprocess.run(['git', '-C', froot, 'rev-parse', '--git-common-dir'],
                          capture_output=True, text=True).stdout.strip()
    if not os.path.isabs(fgit):
        fgit = os.path.join(froot, fgit)
    # Визитка роли есть, и она НЕ моя.
    io.open(os.path.join(fgit, 'nova-session-integrator.card'), 'w',
            encoding='utf-8').write(
        u'role=integrator\nname=nova-other\nsession_id=SID-OWNER\n')
    fenv = dict(os.environ)
    fenv['CLAUDE_PROJECT_DIR'] = froot
    fenv['CLAUDE_CODE_SESSION_ID'] = 'SID-GUEST'
    fp = subprocess.run([sys.executable, HOOK], input=b'{}',
                        capture_output=True, env=fenv, cwd=froot)
    fout = fp.stdout.decode('utf-8', 'replace').strip()
    if fout == '':
        ok(u'чужое окно в ветке роли — хук МОЛЧИТ')
    else:
        bad(u'чужому окну адресовали записку роли: %r' % fout[:140])

    # (10) ТА ЖЕ ОБСТАНОВКА, НО ВИЗИТКИ НЕТ — хук ГОВОРИТ. Без этой клетки
    #      предыдущая доказывала бы лишь умение молчать: окно роли могло не
    #      успеть написать визитку, и тогда молчание хуже ложного адреса —
    #      записка перестала бы напоминать о себе совсем.
    os.remove(os.path.join(fgit, 'nova-session-integrator.card'))
    fp2 = subprocess.run([sys.executable, HOOK], input=b'{}',
                         capture_output=True, env=fenv, cwd=froot)
    fout2 = fp2.stdout.decode('utf-8', 'replace').strip()
    if fout2 and u'integrator-handoff' in fout2:
        ok(u'визитки роли нет — прежнее поведение, хук напоминает')
    else:
        bad(u'без визитки хук замолчал: %r' % fout2[:140])
    shutil.rmtree(froot, ignore_errors=True)

    # (5) НЕЗНАКОМАЯ РОЛЬ — молчание, и это решение, а не дыра: у пакетных окон
    #     своя передача (/stop), выдумывать им адресата хук не вправе.
    oroot = make_tree("p999-other")
    opath = os.path.join(oroot, HANDOFF_REL)
    os.makedirs(os.path.dirname(opath))
    io.open(opath, "w", encoding="utf-8").write(u"# чужая записка\n")
    st = os.stat(opath)
    os.utime(opath, (st.st_atime, time.time() - 7200))
    out, rc = run(oroot)
    if out == "" and rc == 0:
        ok(u"незнакомая ветка — молчит, а не зовёт чужую команду")
    else:
        bad(u"на незнакомой ветке хук заговорил: %r" % out[:140])
    shutil.rmtree(oroot, ignore_errors=True)
finally:
    shutil.rmtree(root, ignore_errors=True)

# --- ДВА ДЕРЕВА: возраст обязан браться из дерева СЕССИИ (дефект №TBD) ------
#
# Обе клетки нужны вместе. Одна проверяет, что хук ЗАМОЛЧАЛ, когда моя копия
# свежая; вторая — что он ЗАГОВОРИЛ, когда устарела именно моя. Порознь первая
# проходит и у хука, который онемел совсем.
print(u"-- два дерева: возраст из дерева сессии --")
main_tree = make_tree("main")
my_tree = make_tree("p274-novac")
for _t, _rel in ((main_tree, HANDOFF_REL), (my_tree, CARINA_REL)):
    _p = os.path.join(_t, _rel)
    os.makedirs(os.path.dirname(_p), exist_ok=True)
    io.open(_p, "w", encoding="utf-8").write(u"x")

# (1) моя записка СВЕЖАЯ, чужая в главном дереве — древняя: хук молчит.
os.utime(os.path.join(main_tree, HANDOFF_REL), (time.time() - 99999,) * 2)
out, _rc = run(my_tree, cwd=my_tree, project_dir=main_tree)
if out == "":
    ok(u"свежая записка МОЕГО дерева — молчит, хотя в главном копия древняя")
else:
    bad(u"взят возраст ЧУЖОГО дерева: %s" % out[:120])

# (2) обратная сторона: моя записка древняя, в главном — свежая: хук говорит.
os.utime(os.path.join(main_tree, HANDOFF_REL), None)
os.utime(os.path.join(my_tree, CARINA_REL), (time.time() - 99999,) * 2)
_stamp = os.path.join(my_tree, "target", ".session-save-reminder")
if os.path.isfile(_stamp):
    os.remove(_stamp)
out, _rc = run(my_tree, cwd=my_tree, project_dir=main_tree)
if u"КАРИНЫ" in out:
    ok(u"древняя записка МОЕГО дерева — говорит, и адресует моей роли")
else:
    bad(u"молчит либо адресует не туда: %s" % (out[:120] or u"(пусто)"))

for _t in (main_tree, my_tree):
    shutil.rmtree(_t, ignore_errors=True)

print(u"селфтест remind-session-save: %d/%d ok" % (cases - fails, cases)
      if fails == 0 else u"селфтест remind-session-save: ЕСТЬ ПРОВАЛЫ (%d)" % fails)
sys.exit(1 if fails else 0)
