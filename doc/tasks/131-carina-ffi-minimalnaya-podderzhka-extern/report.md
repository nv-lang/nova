# Отчёт задачи 131 — Carina FFI: минимальная поддержка extern для пакетных входов

Ветка: task/131-carina-ffi-minimalnaya-podderzhka-extern (worktree
nova-opencode-131-…, тот же путь, что дал плагин). Дата: 2026-10-31.

## РЕПРО

Минимальный репро через pinned сид (см. evidence/red-pinned-seed.txt):

    module ffi_probe
    extern "C" fn abs(v i32) -> i32
    fn main() { println(abs(0 - 5)) }

Pinned seed 0.2.1 (b1_sha256=824bf597…), вызов — форма драйвера
(`emit`, NOVA_STD_PATH=std/src):

    exit=1 — E_NOVAC_SUBSET «outside the subset: an `extern` declaration is
    read, but novac emits no FFI call yet»

— дословно блокер из задания (rules.nv:671).

## ТОЧКА

Отказ — не в парсере (форму читает с 2026-10-20) и не в таблицах (строка
регистрируется), а в ЧЕКЕРЕ: `@report_if_extern_decl` (check/rules.nv)
отказывал ВСЮ форму по одному факту ключевого слова. Попутно найден второй
носитель того же класса: collect и harvest читали имя свободной функции по
индексу 1 — у extern-формы там ABI-строка, и extern-строка регистрировалась
под литералом `"C"` (стало невидимым ровно потому, что чекер отказывал всю
форму раньше).

## ФИКС (novac, 9 файлов + тесты + фикстуры)

Минимальный класс ИЗМЕРЕН по фактическим входам (nova-duckdb v0.2.2
src/ffi.nv ~60 деклараций; nova-tls v0.2.1 src/ffi.nv ~30, входит в
postgres): ABI только "C"; свободная функция; без `unsafe`-листа, генериков,
ресивера и тела; параметры — целые/float/bool/char/str, сырые указатели,
C-newtype-хендлы (`type CTlsHandle(int)`); возврат — те же + `()`.

- sem/slots.nv: `extern_abi_at`/`is_extern_c` — ОДНА дверь ABI в листьях
  декларации (D282, план 91.12 Ф.-1: ABI — лист FnDecl).
- sem/callables.nv: поле строки `extern_c` + `@mark_extern_c`/`@extern_c`
  (узел декларации после collect не хранится — тот же аргумент, что у
  `tparam_cnt`, sem.nv П22).
- sem/collect.nv, sem/harvest.nv: имя свободной функции через дверь
  `decl_name_at` (чинит и регистрацию под `"C"`); метка строки у обоих
  читателей деклараций (своя единица и переданная).
- check/rules.nv: гейт сужен — ABI ≠ "C", `unsafe`-лист, ресивер, генерики,
  тело, не-C-ABI сигнатура (дверь `c_abi_ty`, D282 rule 2) отказываются ПО
  ИМЕНИ, по одной диагностике на форму; класс выше — ПРИНЯТ.
- emit_c/emit_expr.nv: вызов extern-C строки — ЛИТЕРАЛЬНОЕ имя (D282
  rule 3), без `novac_user_`/`nova_fn_` и без вопроса к шеллу.
- emit_c/emit_c.nv: `@emit_extern_c_protos` — `extern <ret> <name>(…);` из
  типов строки КАЖДОЙ единице, достигшей символ (владелец не обязан быть
  вызывающим — форма пакетов: ffi-модуль рядом с обёртками); `-> ()` = void
  (D282 rule 5); определение для extern-строк не пишется.
- pipeline/program.nv: extern-C строки исключены из wanted-name цикла (тело
  символу не должна ни одна единица программы).

## ФИКСТУРЫ

- novac/src/pipeline/extern_c_test.nv — 6 тестов: приём + прототип
  (`extern int32_t abs(int32_t v);`) + literal-call + отсутствие
  определения; `-> ()` → `void noop(…)`; хендл-newtype; отказы по имени
  (nova-ABI / unsafe / тело / не-C-ABI тип), по одному на форму;
  кросс-модульная программа (274.11 E.10) — export-extern + `import
  ./ffi.{abs}` + mark_program, как в реальном прогоне novac emit.
- novac/fixtures/extern_decl/pos_1.nv — позитивная фикстура (libc `abs`,
  печатает 5 — наблюдаемо); neg_1.nv переписан на ОСТАВШИЙСЯ класс
  (`extern "nova"`), прежний текст (fs_open, CStr) стал бы легальным
  минималным классом — тест не ослаблен, а ПЕРЕВЕДЁН на живой отказ.
- Модульные прогоны: pipeline PASS:1/0 (дважды), sem PASS:1/0, check
  PASS:1/0, types PASS:1/0; `nova check novac/src` PASS:17 FAIL:0 WARN:519.

## САБОТАЖ

evidence/sabotage-red-green.txt: гейт испорчен одной строкой (`abi != "C" ||
abi == "C"`) → 5 тестов краснеют ДОСЛОВНО refusal-текстом (PASS:0 FAIL:1);
возврат → PASS:1 FAIL:0 (и check PASS:1 FAIL:0). Обе стороны держат те же
тесты, что охраняют класс.

## std/src

`nova check std/src`: PASS: 162  FAIL: 26  WARN: 67. «До» = «после» по
построению: слияние не трогает ни Rust-компилятор, ни std (git status —
только novac/** и docs/**).

## ЧЕСТНЫЕ ПРЕДЕЛЫ (не сделано)

1. БЛОКЕР (главный): штатный путь обновления сида отсутствует — pinned B1
   0.2.1 не принимает ТЕКУЩИЙ std (базлайн HEAD без моих правок даёт тот же
   единственный отказ в std/src/os/os.nv — evidence/seed-path-blocker.txt),
   Carina-only пересборка novac из текущего дерева невозможна. Путь — задача
   130; здесь блокер назван явно, НЕ обойдён (Oracle не привлекался ни
   билдером, ни судьёй).
2. Из-за (1) НЕ перегонялись: 6 пакетных входов (duckdb×3, postgres×3) и
   корпус 118 (62 PASS) — бинарря с фиксом не существует; фиктивного PASS
   нет. Прогноз по исходникам: у duckdb хендлы объявлены `type CDbHandle
   *()` — newtype над указателем вне подмножества E10, отказ РАНЬШЕ
   extern-гейта (класс назван, не чинился — не «то, что нужно 6 входам»,
   сверх меры); tls-хендлы (`type CTlsHandle(int)`) в классе.
3. Не чинилось по границам: ICE type_of.nv:632 (130), own-package qualified
   импорты (№1924), остальной D282 (fn-ptr, Option[*T], value-record,
   tuple-возвраты), unsafe-call дверь (D424, отказ по имени).
4. Тег не ставился; реестр 221.1 — строка №TBD (номер даст интегратор);
   язык не меняется (D282 уже нормативен) — spec: не применимо.

## ВЕТКА/КОММИТ

task/131-carina-ffi-minimalnaya-podderzhka-extern @ <хеш коммита>;
кандидат integrate/t131 — CI.
