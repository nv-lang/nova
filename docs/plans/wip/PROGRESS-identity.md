# PROGRESS-identity — окно p-identity: скрипт приёмки «build и test-build порождают одинаковый код»

Worktree: `d:/Sources/nv-lang/nova-ident`, ветка `p-identity`, база — `main`
на коммите `54abcdcab` (момент branch-off; главный `nova` ушёл дальше в
процессе окна на несмежный коммит `b8ff7e537` — D156-амендмент/consume-
проверка контейнеров, не трогает `cmd_build`/`test_runner.rs`; окно
работало и отчитывается против своей базы `54abcdcab`).

Компилятор (`compiler-codegen/`, `nova-cli/`) НЕ менялся — только читался.

## Что сделано

1. **`scripts/tools/check-build-test-identity.sh`** — оркестратор. Для
   каждой фикстуры копирует ОДИН `.nv`-файл в ДВА изолированных каталога
   (ловушка p-build304: «модуль = папка» — проба рядом с другими `.nv`
   утянула бы их в компиляцию), собирает `nova build --keep-artifacts`
   (с `TEMP`/`TMP`, указанными на свежий каталог — `.c` находится
   однозначным `find`, без воспроизведения хэш-функции `path_hash`) и
   `nova test-build --keep-artifacts` (`.c` — рядом с исходником, путь
   известен заранее), передаёт оба `.c` компаратору.
2. **`scripts/tools/check-build-test-identity.py`** — компаратор.
   Короткий список `KNOWN_EXCEPTIONS` (сейчас один пункт — dead-функция
   `nova_fn_7runtime7fmt_buf7scratch`, найдена окном p-build304, 0 мест
   вызова в ОБОИХ выводах; исключение самопроверяется на каждом прогоне и
   отказывается применяться, если у функции вдруг появится реальный
   вызов), канонизация синтетических temp-имён (`_nv_tmp_N` и т.п. —
   переименование по порядку первого появления в файле, независимо на
   каждой стороне) убирает каскад renumbering от dead-функции. При
   расхождении — `diff -u -p` (функция-контекст от `-p` бесплатно) плюс
   список имён функций, чьи тела разошлись.
3. **Самотест** `scripts/guards/selftest/test-check-build-test-identity.sh`
   — часть А: сравнивающая логика через синтетические `.c` (ловит
   реальное расхождение; не ловит известное исключение; отказывается от
   исключения при реальном вызове; PASS на идентичных файлах). Часть Б:
   полный оркестратор (bash-скрипт целиком) через поддельный `nova`-стаб
   — без реального компилятора, быстро и детерминированно (ловит
   расхождение / не даёт ложняк на чистой паре).
4. **`docs/dev/test-conventions.md`** — абзац «Build/test byte-identity —
   когда гонять `check-build-test-identity.sh`» в разделе «Как запускать
   тесты»: когда гонять (волны, трогающие конвейер сборки/кодоген; НЕ в
   `gate.sh`), что означает красный (реальный, пользователь-видимый
   риск — регресс это не ловит; чинится в `cmd_build`, не в скрипте).

## Найденный по ходу баг компаратора (пойман собственным самотестом, не сдан)

Первая редакция `count_call_sites`/`strip_function` классифицировала
строку как «просто объявление/заголовок» по суффиксу `endswith(");")`.
Реальный вызов ВНУТРИ большего выражения (`... + symbol(1);`) тоже
оканчивается на `");"` — ложно считался объявлением, инвариант «0 мест
вызова» не срабатывал, исключение маскировало бы реальный вызов. Поймано
свойством (3) самотеста ДО коммита; исправлено якорными regex'ами на ВСЮ
строку (`_decl_and_def_re`). Не публиковалось наружу.

## Гейты окна (дословно)

`bash -n scripts/tools/check-build-test-identity.sh`:
```
(без вывода — синтаксис ОК)
```

Самотест:
```
самотест check-build-test-identity:
  ok: ловит реальное расхождение (--compare)
  ok: НЕ ловит известное исключение (extra dead-функция + renumbering)
  ok: исключение НЕ применяется, если у dead-функции есть реальный вызов
  ok: идентичные файлы — PASS
  ok: оркестратор ловит расхождение (поддельный nova)
  ok: оркестратор НЕ даёт ложняк на чистой паре (поддельный nova)
самотест ok: инструмент ловит расхождения, не даёт ложняков, известное исключение короткое и самоограничено
```
Exit: 0.

Прогон на фикстурах по умолчанию (`bench/field_cache/01_ro_hot_loop.nv`,
`02_chain_heavy.nv`), собранный `nova.exe` (release, эта же база):
```
check-build-test-identity: nova = /d/Sources/nv-lang/nova-ident/nova-cli/target/release/nova.exe
check-build-test-identity: python = python
check-build-test-identity: рабочий каталог = /tmp/tmp.1aR3LHQABs

=== 01_ro_hot_loop (/d/Sources/nv-lang/nova-ident/bench/field_cache/01_ro_hot_loop.nv) ===
[EXCEPTION APPLIED] nova_fn_7runtime7fmt_buf7scratch: window p-build304 (2026-08-04, commit ac684356f): test-build emits this helper, build does not; ZERO call sites in EITHER output (verified: `grep -c` both sides) -> dead code, a DCE/reachability-registration difference in compiler-codegen, not a semantic gap. Exception only applies while both sides stay at zero call sites (checked below on every run). (stripped 0 line(s) from A side, 12 line(s) from B side)
IDENTICAL (modulo applied exceptions above, if any)

=== 02_chain_heavy (/d/Sources/nv-lang/nova-ident/bench/field_cache/02_chain_heavy.nv) ===
[EXCEPTION APPLIED] nova_fn_7runtime7fmt_buf7scratch: window p-build304 (2026-08-04, commit ac684356f): test-build emits this helper, build does not; ZERO call sites in EITHER output (verified: `grep -c` both sides) -> dead code, a DCE/reachability-registration difference in compiler-codegen, not a semantic gap. Exception only applies while both sides stay at zero call sites (checked below on every run). (stripped 0 line(s) from A side, 12 line(s) from B side)
IDENTICAL (modulo applied exceptions above, if any)

=== итог ===
  01_ro_hot_loop: PASS (build и test-build породили идентичный C)
  02_chain_heavy: PASS (build и test-build породили идентичный C)
check-build-test-identity: PASS — build и test-build идентичны на всех 2 фикстур(ах)
```
Exit: 0. Новых расхождений НЕ найдено (ожидаемо — фикс №304 уже в базе
`54abcdcab`).

`arch-ratchet.sh` (компилятор не трогался — сдвига быть не должно):
```
arch-ratchet ok: lines=64542 <= 64545
arch-ratchet ok: infer=348 <= 348
```
Exit: 0.

Мега-CU не гонялся (по заданию окна).

## Побочные заметки об окружении (не находки компилятора, для повторного запуска)

- В свежем worktree нужны: `git submodule update --init
  compiler-codegen/nova_rt/libuv` (не инициализируется автоматически для
  worktree) и `NOVA_GC_LIB_DIR`/`NOVA_GC_INCLUDE_DIR`, указывающие на
  `vcpkg_installed` главной репы (в самой worktree его нет) — штатный
  механизм «сборка вне главной репы», см.
  `scripts/guards/check-no-runtime-copy.sh`. Ничего не копировалось.
- `nova.exe` собран в СВОЁМ target worktree (`nova-cli/target/release`),
  НЕ в `target/` главной репы — во время окна там работали чужие
  процессы (`nova check`, `nova test`), делить target было бы небезопасно.

## Статус

Все пять требований задания выполнены: изоляция каталогов, содержательный
diff-вывод (функции + первые строки), короткий список исключений с
обоснованием и самопроверкой инварианта, самотест (ловит/не ловит),
инструмент НЕ в `gate.sh` + абзац в `test-conventions.md`. Новых находок
класса «cmd_build/test_runner разошлись» в этом окне не возникло — фикс
№304 уже нёс их в базе.
