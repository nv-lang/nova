# №1872 — novac emission: поля/имена из `C_NAME_MEMBERS` печатаются сырыми (класс «no member named 'ctx'»)

Класс: три пути печати в эмиссии novac обходят дверь `c_ident`
(`novac/src/sem/mangle_spelling.nv`, `C_NAME_MEMBERS = ["ctx", "schedlink"]`),
хотя вторая сторона той же пары (объявление поля записи, объявление локала/параметра)
эскейпится. Итог — C, который clang не компилирует.

## Пробы

`self_field_ctx.nv` — запись с полем `ctx`, метод читает/пишет `@ctx`,
рекорд-литерал с паннингом `Box { ctx }`. Проверяется и компилируется
(`novac check` / `novac emit` rc=0), дефект виден только на этапе clang.
Записанная команда воспроизведения — `cmd.sh` рядом (№695 п. 2а):
`sh scripts/tools/novac-e1-smoke.sh docs/plans/repro/1872/self_field_ctx.nv`.

`self_field_ctx.before.c.txt` — эмиссия ДО фикса (novac из коммита 3f4b68a8b,
23136 строк). Скомпилирована cflags+PCH из кэша `novac-e1-smoke.sh --prepare`,
clang `-ferror-limit=0` — 4 ошибки:

```
18187:20: error: no member named 'ctx' in 'struct Nova_Box'
18187:67: error: no member named 'ctx' in 'struct Nova_Box'
18188:27: error: no member named 'ctx' in 'struct Nova_Box'
18196:29: error: use of undeclared identifier 'ctx'
```

Дефектные места ДО-эмиссии (объявление структуры — с `nv_ctx`, доступ — сырой `ctx`):

```c
struct Nova_Box {
    Nova_Ctx* nv_ctx;
};
...
    ((_novac_self->ctx)->n) = nova_int_checked_add(((_novac_self->ctx)->n), ((nova_int)1LL));
    return ((_novac_self->ctx)->n);
...
    Nova_Ctx* nv_ctx = _novac_tmp_t1;          // локал эскейпнут
    _novac_tmp_t2->nv_ctx = ctx;               // запись в поле эскейпнута, чтение локала — сырое
```

## Корень (три пути одного класса)

1. `novac/src/emit_c/emit_expr.nv:462` — `SelfField` (`@ctx`) печатает
   `leaf_text(kids[1])` сырым, а поля структур объявляются эскейпнутыми
   (`emit_decls.nv:66`, `emit_instance_structs.nv:61` → `nv_ctx`).
   Носитель в самосборке: 1098 ошибок «no member named 'ctx'»
   (Nova_Checker 774, Nova_Emitter 273, Nova_Lowerer 51).
2. `novac/src/emit_c/emit_c.nv:838` — паннинг в рекорд-литерале
   (`Leaf(_) => @body.append(fname)`) печатает имя поля как значение сырым,
   а локалы/параметры эскейпнуты (`ir.nv:583`, `emit_c.nv:542`)
   → `_novac_tmp_tN->nv_ctx = ctx;`. Носитель: 6 ошибок
   «use of undeclared identifier 'ctx'» (check/run.nv:71, :182;
   emit_c/emit_c.nv:171; pipeline/pipeline.nv:607; pipeline/program.nv:165 ×2).
3. `novac/src/emit_c/emit_handler.nv:120` — сигнатура op в хендлер-литерале
   печатает `${pname}` сырым, а тело читает эскейпнутое имя. Носителя в
   самосборке нет (op-параметры std-эффектов не называются `ctx`/`schedlink`);
   фикс в том же коммите, проба — фикстура `carina_c_names/pos_3.nv`.

Не входят в класс (сырые с ОБЕИХ сторон, консистентно): vtable-члены
(`emit_handler.nv:242`, `emit_decls.nv:106/129`, `emit_place.nv:292`).

## Замер самосборки (весь класс)

ДО (коммит 3f4b68a8b): `CLANG_ERRORS_TOTAL=1104` =
1098 «no member named 'ctx' in 'struct Nova_Checker/Emitter/Lowerer'» +
6 «use of undeclared identifier 'ctx'». ПОСЛЕ (ветка задачи #52):
`CLANG_ERRORS_TOTAL=0` — эмиссия самосборки (165169 строк C) компилируется
clang чисто (rc=0). Дословно оба замера — в строке №1872 реестра
`docs/plans/221.1-bug-sweep.md`.
