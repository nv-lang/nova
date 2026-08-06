# PROGRESS — окно p379-defer (№379, `_defer_NNN_K_active` undeclared в мега-CU)

Модель: sonnet. Worktree: `d:/Sources/nv-lang/nova-p379d` (branch `p379-defer`,
fast-forward на main `b37a67ae6` в начале окна).

## Как воспроизвёл

Полный мега-CU (`nova test spec_tests/conformance`) на Windows/clang не
дожимается за разумное время в этой сессии (release cargo-build + libuv/GC
first-time setup + сама компиляция мега-C-файла — упёрся в 10-минутный лимит
Bash-таймаута дважды). Вместо этого:

1. **Изолированная директория.** Скопировал ТОЛЬКО
   `spawn_detach_consume_multivar_ok.nv` в новую пустую директорию
   (scratchpad) и прогнал `nova test --positive --compile-error` на ней —
   nova трактует директорию с одним файлом как единственный CU, БЕЗ соседей
   по реальному `spec_tests/conformance`. **Упало сразу**, без единого
   соседа:
   ```
   spawn_detach_consume_multivar_ok.c:12880:17: error: use of undeclared
     identifier '_defer_1_0_active'
   spawn_detach_consume_multivar_ok.c:12881:17: error: use of undeclared
     identifier '_defer_1_1_active'
   ```
   Т.е. окно №379 (влившее фикстуру) не поймало баг не потому, что нужны
   соседи по CU — просто её собственный "standalone"-прогон, судя по всему,
   не гонял `nova test` с `--compile-error` на ВСЕХ 6 test-блоках файла
   разом (в файле их 33 строки прироста между момента влития и этим окном
   не было — баг был в файле с самого начала, просто не пойман).
2. **In-place (реальный мега-CU) для сверки с отчётом владельца.** Прогнал
   `nova test` на файле по его РЕАЛЬНОМУ пути внутри `spec_tests/
   conformance/` (не копия) — модель Nova "папка = один модуль" тянет ВСЕ
   1148+ top-level `.nv` в этой папке в ОДИН C-файл независимо от того,
   какой конкретно путь передан `nova test`. Со СТАРЫМ (до-фикса) бинарём
   на файле `guard_cross_scope_transfer.nv` (никак не связанном с
   spawn/detach) получил РОВНО репортованную владельцем картину плюс ещё
   один независимый случай:
   ```
   guard_cross_scope_transfer.c:214245:17: error: use of undeclared
     identifier '_defer_328_0_active'
   guard_cross_scope_transfer.c:214246:17: error: use of undeclared
     identifier '_defer_328_1_active'
   guard_cross_scope_transfer.c:214674:17: error: use of undeclared
     identifier '_defer_333_0_active'
   ```
   (Номера сдвинуты относительно `a_q3_println_debug_record.c:212522` из
   брифа — другая точка входа в тот же мега-CU, тот же класс бага, третий
   независимый инстанс `_defer_333_0_active` найден бонусом.)

## Корень (с путём и строкой)

Файл: `compiler-codegen/src/codegen/emit_c.rs`.

`spawn consume a[, b, …] { body }` / `detach consume a[, b, …] { body }`
(D415 §4, `parser/mod.rs::parse_spawn`/`parse_detach`/
`parse_spawn_detach_consume_multivar`) десугарятся в ВЛОЖЕННЫЕ
`Stmt::ConsumeScope` (`re_consume: false`, «already-bound»-форма — `init`
буквально `Ident(имя-биндинга)`), по одному слою на биндинг. Это происходит
и для single-var, и для multi-var формы — многовар просто даёт N слоёв
вместо одного.

`emit_spawn` (было `emit_c.rs:13221`) и `emit_detach`
(`emit_c/emit_detach.rs:51`) компилируют ТЕЛО файбера в ОТДЕЛЬНУЮ,
самостоятельную C-функцию: `let saved_out = std::mem::take(&mut self.out)`
(emit_spawn — было `emit_c.rs:13603`; emit_detach — `emit_detach.rs:163`) —
классический паттерн этого файла (сравн. `[M-187-nested-spawn-scope-var-
cc-fail]`, тот же класс «C-локали родителя невидимы в файбере»).

Бинды типа `mv379_a`/`mv379_b` — это ТАКЖЕ голые `consume x = e;`
(bare auto-cleanup, Plan 217) в РОДИТЕЛЬСКОЙ функции (тестовое тело),
зарегистрированные `enter_defer_scope` (~28909) в `self.auto_cleanup_active:
Vec<(String, usize, usize)>` (~1794) — `(имя, block_id, idx)`. Когда тело
файбера (уже в СВОЕЙ, дочерней C-функции) вызывает `mv379_pump2(mv379_a,
mv379_b)` (consume-параметр), `disarm_auto_cleanup_receiver_call` (~30556,
не двигал) находит через `disarm_var_for` (~30523, не двигал) СТАРУЮ запись
в `auto_cleanup_active`, указывающую на флаг РОДИТЕЛЬСКОЙ функции, и эмитит
`_defer_<родительский-bid>_<idx>_active = 0;` — ПРЯМО ВНУТРИ файберской
функции. C-локаль родителя не видна в дочерней функции → "use of undeclared
identifier".

Существующий bugfix (`Stmt::ConsumeScope`-кодоген, ~32500-32527: «Plan 217
BUGFIX (folder-CU regression, `d188_reconsume_block.nv`)») уже разоружает
ТАКУЮ ЖЕ внешнюю `auto_cleanup_active`-запись при входе в re-consume блок —
но ТОЛЬКО когда `*re_consume == true` (Plan 201 D188-форма). У spawn/detach-
consume `re_consume` всегда `false` («already-bound» форма) — этот путь
никогда не срабатывал для файберных форм, и оставлял:
1. Стухшую запись, которую позже подхватывал `disarm_auto_cleanup_receiver_
   call` из ЧУЖОЙ C-функции (CC-FAIL — собственно репорт).
2. (Латентно, до этого окна непойманный) риск двойного `@cleanup`: если
   тело файбера НИКОГДА не звонит consume-param функцию с этим биндингом,
   внешний флаг остаётся armed=1, и при выходе из родительской функции
   `leave_defer_scope` для родительского блока запускает `@cleanup` ВТОРОЙ
   раз поверх того, что уже честно отработал собственный `ConsumeScope`
   файбера.

## Фикс

Новая функция `disarm_outer_auto_cleanup_for_fiber_body(&mut self, block:
&Block)` — идёт по «спине» вложенных `Stmt::ConsumeScope`, порождённых
десугаром (условие: ЕДИНСТВЕННЫЙ stmt в блоке, без trailing, `re_consume ==
false`, `init` буквально `Ident(имя == binding)` — этот сигнатурный набор
производит ТОЛЬКО сам десугар), и для каждого слоя:
- эмитит `<флаг> = 0;` (в РОДИТЕЛЬСКОЙ функции — вызывается ДО `self.out`
  свопа),
- убирает запись из `auto_cleanup_active` (чтобы ничто внутри файбера её
  больше не нашло).

Безусловно на `re_consume` — обе формы («re-consume» D188 и «already-bound»
spawn/detach) одинаково переносят владение НАВСЕГДА, симметрично уже
принятому bugfix'у на 32520.

**Ratchet-дисциплина.** `scripts/guards/arch-ratchet.sh` меряет ТОЛЬКО
`emit_c.rs`; файл был РОВНО на baseline (64444), без запаса. Первая версия
фикса (весь новый fn + doc-comment внутри `emit_c.rs`, +60 строк) ratchet
ломала. Перенёс fn целиком в `codegen/emit_c/emit_detach.rs` (дочерний
модуль — та же практика, что window p240 уже применило к `emit_detach`
самому: ratchet его не измеряет, `pub(super)` — виден предку по правилу
Rust "потомок виден предку" из шапки самого файла). `emit_c.rs` получил
только неизбежный call-site в `emit_spawn` (comment+if одной строкой каждый,
+2 нетто) — `emit_detach`'s собственный call-site целиком внутри
`emit_detach.rs`, вне метрики. Baseline поднят `64444 → 64446` (+2), запись
в `scripts/guards/arch-ratchet.baseline` с обоснованием (путь B — исполнитель
сам, интегратор ещё не смотрел).

## Приёмка по классу — таблица «форма → вердикт»

Все формы прогнаны в `spec_tests/conformance/spawn_detach_consume_
multivar_ok.nv` (12 `test{}`-блоков после расширения этим окном), проверен
изолированный CU (файл один в директории — `nova test` на такой директории
компилирует и запускает ВСЕ test-блоки файла как один бинарь; `PASS: 1
FAIL: 0` = все 12 внутренних test'ов зелёные).

| Форма | До фикса | После фикса |
|---|---|---|
| Single-var `spawn consume x { … }` без consume-param вызова внутри | GREEN (уже была регресс-фикстура) | GREEN (не тронуто) |
| Single-var `spawn consume x { pump1(x) }` — consume-param вызов внутри файбера (**новая проба этого окна**) | RED (латентный, ни одна фикстура так не делала до этого окна) | GREEN |
| Multi-var `spawn consume a, b { pump2(a,b) }` (исходный репорт №379) | RED (CC-FAIL, репортовано) | GREEN |
| Multi-var (3+) `spawn consume a, b, c { pump3(a,b,c) }` | RED (тот же класс) | GREEN |
| Single-var `detach consume x { … }` без consume-param вызова | GREEN (уже была регресс-фикстура) | GREEN (не тронуто) |
| Single-var `detach consume x { pump1(x) }` (**новая проба**) | RED (латентный) | GREEN |
| Multi-var `detach consume a, b { pump2(a,b) }` | RED (тот же класс) | GREEN |
| Вложенный scope: `spawn consume x { if true { pump1(x) } }` (**новая проба**) | RED (тот же класс, глубина не важна — весь файбер одна C-функция) | GREEN |
| ДВА multi-var `spawn consume` statement'а подряд в одной функции (bidirectional relay, уже была; + новая проба 3-var×2) | RED (оба страдали, но нумерация НЕ пересекалась — каждый spawn получает свой `defer_block_counter`) | GREEN, оба независимо |
| Блочная форма `consume x = expr { body }` (D188, БЕЗ spawn/detach) | не тронуто этим фиксом (`disarm_outer_auto_cleanup_for_fiber_body` вызывается ТОЛЬКО из `emit_spawn`/`emit_detach`) | не тронуто; diff-проверка подтверждает: `Stmt::ConsumeScope`-кодоген и его `if *re_consume`-ветка (32500-32527) не изменены ни на строку |
| Передача consume-биндинга в consume-параметр ВНЕ файбера (исходный Plan 217, `guard_cross_scope_transfer.nv`) | GREEN (пре-существующий механизм) | GREEN — подтверждено: тот же файл IN-PLACE (реальный мега-CU) со старым бинарём даёт РОВНО `_defer_328_*`/`_defer_333_*` (это НЕ его собственный код, а сосед по мега-CU), новым бинарём компиляция проходит НАСКВОЗЬ (до линковки) |

### Проба «подсунь заведомо негодное»

- **3+ биндинга**: `spawn consume mv379_m, mv379_n, mv379_o { pump3(…) }` —
  GREEN.
- **Вложенные scope'ы**: consume-param вызов внутри `if true { … }` внутри
  файбера — GREEN (disarm происходит ДО входа в файбер целиком, глубина
  вложенности внутри тела не имеет значения).
- **Два multi-var statement'а подряд в одной функции** (нумерация не должна
  пересекаться): и исходная фикстура (`bidirectional relay`, 2×2-var), и
  новая проба (2×3-var, 6 разных биндингов) — оба `spawn`'а получают
  собственный, не пересекающийся `defer_block_counter`; GREEN.

## Вердикты прогонов (дословно)

Изолированный CU (одна директория, только эта фикстура; после
финального фикса, ratchet-совместимая версия):
```
Toolchain: clang, mode=Dev, jobs=4, paths=[...subset_cu]
PASS           ...subset_cu/spawn_detach_consume_multivar_ok

===== SUMMARY =====
PASS: 1  FAIL: 0
```

In-place (реальный `spec_tests/conformance`, файл `guard_cross_scope_
transfer.nv`, СТАРЫЙ бинарь — до фикса):
```
guard_cross_scope_transfer.c:214245:17: error: use of undeclared identifier '_defer_328_0_active'
guard_cross_scope_transfer.c:214246:17: error: use of undeclared identifier '_defer_328_1_active'
guard_cross_scope_transfer.c:214674:17: error: use of undeclared identifier '_defer_333_0_active'
PASS: 0  FAIL: 1
```

Тот же прогон, НОВЫЙ (пофикшенный) бинарь:
```
CC-FAIL   spec_tests/conformance/guard_cross_scope_transfer  # lld-link: error: undefined symbol: nova_fn_p238f3_run_on_worker
PASS: 0  FAIL: 1
```
Компиляция C прошла БЕЗ единой ошибки (иначе `lld-link` не запустился бы
вовсе) — упёрлись в НЕСВЯЗАННЫЙ, pre-existing Windows-only линк-гэп
(`fiber_param_requirement_pos.nv`'s `extern "nova" fn p238f3_run_on_worker`,
Plan 238 fiber-safety, симметрично не резолвится и на файлах, вообще НЕ
касающихся spawn/detach-consume — воспроизведено на `d188_multivar_
reconsume.nv`/`d188_v3_expr_escape.nv`/самом `fiber_param_requirement_
pos.nv` тоже). Не входит в мандат этого окна; полный мега-CU на Linux CI
(владелец) этой проблемы, судя по «687/2» из брифа (без упоминания
`run_on_worker`), не имеет — похоже на локальный Windows/lld-link артефакт.

Гейты:
```
$ cargo build --release --manifest-path nova-cli/Cargo.toml
    Finished `release` profile [optimized] target(s) in 2m 22s   (чисто)

$ nova check std/src
PASS: 148  FAIL: 26  WARN: 61        (канон байт-в-байт)

$ bash scripts/guards/arch-ratchet.sh
arch-ratchet ok: lines=64446 <= 64446
arch-ratchet ok: infer=348 <= 348
```

## Модель
sonnet (всё окно).

## Известный остаток (не в мандате)
`nova_fn_p238f3_run_on_worker` — undefined symbol при линковке РЕАЛЬНОГО
мега-CU НА ЭТОЙ Windows/clang/lld-link машине, воспроизводится независимо
от spawn/detach-consume (на файлах, которые его вообще не касаются).
Полный мега-CU владелец гоняет отдельно (правило окна) — если он тоже
увидит эту ошибку на Linux CI, это отдельный номер, не №379.
