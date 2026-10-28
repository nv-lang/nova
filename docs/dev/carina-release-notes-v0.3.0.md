<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Nova Carina 0.3.0 — release notes (draft, unreleased)

This is a draft release-note home. It does not announce a release or claim
acceptance of the Carina 0.3 milestone.

## Ограничения объёма 0.3 — решения владельца 2026-10-25 17:58/18:05

Источник (проверяемый, дословно): письма оркестратора 2026-10-25 со словами
владельца; цепочка владелец → оркестратор → интегратор → постановка задачи
126 подтверждена ответом интегратора на вопрос qmvs757pe2b7j (2026-10-28
19:08, передан письмом приёмщика 2026-10-28 19:09) и записана в
[doc/tasks/126-amend-274-16-criteria-and-release-notes/task/owner-words.md](../../doc/tasks/126-amend-274-16-criteria-and-release-notes/task/owner-words.md):
17:58: «объём 0.3 = nova build через Карину на 118 примерах и 9 пакетах,
Windows+Clang, Ш.5/Ш.7; тег carina-v0.3.0 только после нового слова
владельца „тег разрешён“ (прежние разрешения отменены)»; 18:05: «№1890
(MSVC) выведен в 0.4, №1889 остаётся; вне 0.3 (в 0.4): Linux, nova test на
Карине, тесты пакетов (Ш.6)». Постановка —
[doc/tasks/126-amend-274-16-criteria-and-release-notes/task/message.md](../../doc/tasks/126-amend-274-16-criteria-and-release-notes/task/message.md),
раздел «Чего хочет владелец». Это более позднее решение: прежние записи
черновика, требующие обязательный native Linux fixed point и полные
build+test examples/packages как условия тега 0.3 (reg 1907, reg 1908,
«Отдельные блокеры тега 0.3», «Текущее состояние перед общей итоговой
приёмкой 092»), ниже помечены ЗАМЕЩЁННЫМИ в соответствующей части со ссылкой
на поправку 274.16 от 2026-10-27, рядом названы новые действующие условия;
raw evidence не удаляется, PASS не заявляется.

- **Объём ступени 0.3 = Windows + Clang** (слова 17:58): `nova build` через
  Карину на 118 примерах `examples/` и 9 пакетах, шаги Ш.5/Ш.7 плана 274.16;
  **тег carina-v0.3.0 — только после нового слова владельца «тег разрешён»
  (прежние разрешения отменены)**.
  №1890 (MSVC) выведен в 0.4:
  MSVC-линия не является ни критерием, ни блокером ступени 0.3.
- **№1889 (multi-module `nova build` через pinned Carina B1) остаётся в
  объёме 0.3 и закрыт задачей 120** (влита 2026-10-26, main `09c317de2`).
- **Вне 0.3, перенесены в 0.4:** Linux; `nova test` на Карине; тесты
  пакетов (тестовая часть плана 274.16 Ш.6). Этот черновик не заявляет
  ни одну из этих линий как PASS.
- **Фактическое состояние:** health-check Ш.5 — 62 из 118 примеров
  `examples/` дают build PASS через Карину. Это health-check, а не
  доказательство приёмки; Oracle не судья ожидаемого результата.
- **№1922 БЛОКИРУЕТ ТЕГ:** штатный драйвер при отказе `Carina B1 emit`
  теряет stdout-диагностику сида (в текст ошибки попадает только stderr,
  пустой), поэтому классификация каждого отказа корпуса примеров по 274.16
  Ш.5 штатным путём невозможна.

## Open nonblocking debt

- Registry entries 1895–1898 remain OPEN and nonblocking by owner decision
  dated 2026-10-23. Their scope is limited to the same-volume route; this note
  does not claim that they are fixed.
- The bridge-without-authorization refusal control is absent by explicit owner
  waiver dated 2026-10-24. Its absence is not a PASS or a class fix; its impact
  on the 0.3 milestone has not been established.
- Registry entry 1900 remains OPEN, P2, and nonblocking by owner decision dated
  2026-10-24. It records a bounded static source-algebra finding: for a bodied,
  free, nongeneric, not-in-program handed row,
  `sem/handed_bodies.nv:207–208` makes `instantiates(row, [])` the complement of
  the shell-symbol predicate, so the refusal condition at `check/calls.nv:605`
  is unreachable. This is not a runtime failure, a miscompile, or a claim about
  all callable forms. The owner waived only the F110 actual-refusal check; the
  waiver is not a fix for the compiler defect.

## Открытый долг компилятора: reg 1901

- Самостоятельный блок `{ ... }` в позиции оператора Carina может читать как
  `RecordCtor`, хотя D347 допускает вложенный блок (`spec/decisions/03-syntax.md:9360`).
  Вложенный `FnDef` в таком носителе даёт каскад восстановления парсера вплоть до
  ложного module-level `ro`; менять этот `ro` на `const` как исправление нельзя.
- Для `if`-значения справа от `==` checker не передаёт контекст через `Bin`.
  Наблюдаемый GMETA_E отказ — 37 diagnostic records в трёх private peers,
  а не 37 независимо установленных дефектов или ожидаемый отрицательный тест.
- По решению владельца 2026-10-24 для 092 выбран вариант А: адаптация только
  проверочной обвязки с сохранением всех случаев, порядка и свежих контекстов.
  Новые версионные pins не заменяют и не удаляют старые pins/raw/STOP.
  Адаптация **не исправляет класс Carina** и не является waiver отказа emit.
- Reg 1901 остаётся **OPEN, P2** и не блокирует тег по этому решению владельца.
  Вариант Б — отдельная задача исправления класса после 0.3; сейчас она не
  заведена. Исходники компилятора и D347 в адаптации не меняются. A-v1 прошла
  GMETA_E, но native GMETA_N остановлен по отдельной причине reg 1902;
  оставшаяся связка и независимая приёмка ещё не заявлены.

## Открытый долг компилятора: reg 1902

- Многострочное assert-условие сохраняется в `ContractNote.display` как точный
  source slice. `c_literal_escaped` в `emit_requires.nv` не экранирует реальные
  LF/CR: LF из двух условий CFG-06 попал внутрь C string аргумента
  `nova_assert_loc` и вызвал четыре Clang errors в двух string sites.
  Такой перенос выражения допустим (`spec/decisions/03-syntax.md:12906–12911`).
- Assert-носитель измерен; requires использует тот же helper, но sibling и CR
  отдельно не запускались. Это родственный escaping-класс закрытой reg 872,
  не установленная история регрессии и не причина reg 1901.
- По решению владельца 2026-10-24, А+, адаптация 0 ограничена обвязкой:
  записать два условия в одну физическую строку без потери проверок, порядка,
  operands и вычисляемых сообщений, затем новый emit перед native. Старый C
  не правится; старые pins/raw/STOP сохраняются. Это **не исправление класса**
  и не waiver native ошибки. Reg 1902 остаётся **OPEN, P2**, не блокируя тег
  по этому решению владельца; compiler fix требует отдельной работы после 0.3.
- Допуск А+ ограничен ещё двумя однотипными compiler-in-harness адаптациями,
  каждая требует STOP, read-only причины, новых версий и отдельной записи
  долга. Он не разрешает менять compiler/Carina/норму или игнорировать новые
  классы ошибок, окружение, план либо неожиданные отказы строк после GMETA_N.

## Открытые ограничения среды и процесса: reg 1903–1906

- **Reg 1903:** TC0 retry завершился exit 0 и получил 15 непустых ключей
  native-env, но stderr сообщил, что `vswhere.exe` не найден как команда.
  `harmlessness_proven=false`: только для этой ошибки владелец разрешил
  продолжение с оговоркой. Наличие файла на машине, причина поиска и необходимость
  этого вызова для MSVC-ветки не установлены. Auto `--vcvars` условен: текущий
  `find_vcvars` использует explicit path/`NOVA_VCVARS`, затем путь Installer из
  `ProgramFiles(x86)`, а не поиск vswhere в PATH; TC0 вызвал explicit batch.
- **Reg 1904:** первый TC0 остановился до компилятора с ошибкой синтаксиса cmd.
  Разделители тех же путей изменены только для одного разрешённого повтора.
  Гипотеза о slash spelling через `Popen/list2cmdline` не доказана независимо;
  исходный FAIL и retry evidence сохранены. Это не исправление компилятора.
- **Reg 1905:** закрытый dispatcher не имел штатного перехода для owner disposition
  сохранённого STOP. Продолжение потребовало отдельных pinned dispatchers;
  исходные I5/STOP/claims/raw остаются неизменными. Предложение штатного учёта
  disposition — открытый процессный долг, не разрешение игнорировать STOP.
- **Reg 1906:** интегратор Новы изменил уже закреплённый authority; по решению
  владельца поздняя правка точно откачена, erroneous bytes сохранены, hashes
  проверены до и после. Ответственность интегратора, не Astra. Решения/пометки
  далее хранятся отдельно; это раскрытие инцидента, а не class fix процесса.
- **Reg 1900 / SB_HOST_RED:** I5 явно связывает waiver отсутствующего refusal
  control с уже открытой reg 1900. SB_HOST_RED снят без replacement и **не PASS**;
  SB_HOST_GREEN — прежний baseline, не доказательство waived refusal. Дубликат
  долга не создаётся; статусы reg 1900–1902 этим учётом не меняются.

## Открытый bootstrap proof debt: reg 1907

- Исторический E3 остаётся raw FAIL: при одинаковом размере E2/E3 их SHA и
  4932 строки различаются. Производитель E2 (K1) читает entries без сортировки,
  производитель E3 (B1) уже сортирует имена. Это доказанная разница discovery
  algorithms, но не измеренный случайный nondeterminism одного executable;
  семантическая эквивалентность и сходимость ещё не доказаны.
- По отдельному решению владельца 2026-10-24 20:47 разрешена новая версия:
  C3 принимается только как build input, после независимой SOURCE — ровно
  N3G → E4 с полным сырым byte-equal C3 → B2G_USAGE, затем итоговая
  SOURCE+RESULTS. Старый E3 не становится PASS, старые pins/raw/STOP сохраняются.
  Нет E5, retry, normalization, C patch или waiver; новый mismatch — STOP.
- Reg 1907 остаётся **OPEN, P2**. Это проверка ограниченного bootstrap-перехода,
  не class fix и не обещание universal reproducibility/бинарного PE fixed point.
  Неблокирующий статус долгу не присвоен. Даже успех Windows-цепочки не заменит
  обязательный native Linux fixed point для тега 0.3; сам тег отдельно не разрешён.
  *(Требование Linux fixed point как условия тега 0.3 — ЗАМЕЩЕНО решением
  владельца 2026-10-25 17:58/18:05, см. поправку 274.16 от 2026-10-27 и
  «Ограничения объёма 0.3» выше. Действующие условия рядом: `nova build`
  через Карину на 118 примерах и 9 пакетах (Windows+Clang), exact-SHA CI,
  тег — только после нового слова владельца «тег разрешён»; Linux fixed
  point — работа 0.4. Запись и evidence сохраняются, PASS не заявляется.)*

## Открытый долг запускового helper: reg 1908

- Независимая preliminary SOURCE bootstrap-generation-v1 — **FAIL / BLOCKER**:
  перед usage не перепроверялся транзитивный N3G/B2.exe pin, SOURCE gate принимал
  truthy authority вместо exact приёмщика/реальной доставки, а native output PE
  не проверялся на отсутствие перед child. Integrity PASS этих обязательств
  не доказывает. Фактических подмен/перезаписей и child-запусков не было.
- После разрешения владельца 2026-10-24 21:21 получен bounded helper repair v2:
  executable provenance, exact session/index/SOURCE/delivery SHA/qid с evidence
  pins, отсутствие declared outputs через `lexists`. Неизменный v1 импортируется
  как база; его FAIL, прежние pins/raw/STOP и 48 PASS сохраняются. Новый пакет
  передан на повторную SOURCE; авторские sample3/all PASS не заменяют её.
- Reg 1908 остаётся **OPEN, P2**, без class-fix/CLOSED/nonblocking claim.
  Только независимый SOURCE PASS допускает три строки по одному, затем нужна
  итоговая SOURCE+RESULTS. Ни результатов строк, ни новых waiver этот учёт
  не создаёт. Compiler/AC/env/budgets и долги reg 1901/1902/1907 не изменены;
  обязательный native Linux fixed point и отдельный допуск тега не заменяются.
  *(Условие Linux fixed point для тега 0.3 — ЗАМЕЩЕНО более поздним решением
  владельца 2026-10-25 17:58/18:05, см. поправку 274.16 от 2026-10-27;
  действующие условия тега 0.3 — в «Ограничения объёма 0.3» выше. Строка
  сохранена как историческая.)*

## Отдельные блокеры тега 0.3 — решение владельца 2026-10-24 22:16

Продолжение Windows bootstrap-цепочки после SOURCE v2 PASS разрешено отдельно.
Это не разрешение тега и не PASS документных стражей. До письма «тег разрешён»
остаются явно предъявляемыми блокерами:

- **№1899:** открытая К1 без вердикта `БЛОКИРУЕТ ТЕГ`; entry-shape — raw FAIL.
  Нужен отдельный вердикт владельца: исправить либо принять с обоснованием.
- **№1900:** routes не распознаёт поле статуса; guard — raw FAIL. Нужна отдельная
  разрешённая правка поля; существующая строка данным решением не меняется.
- **Native Linux fixed point:** обязателен отдельно; Windows его не заменяет.
  *(ЗАМЕЩЕНО как условие тега 0.3 более поздним решением владельца 2026-10-25
  17:58/18:05, см. поправку 274.16 от 2026-10-27: Linux перенесён в 0.4.
  Действующие условия тега — в «Ограничения объёма 0.3» выше; запись
  историческая, guards и их receipts не тронуты.)*
- **Зелёный exact-SHA CI:** receipts должны относиться к предъявленному SHA,
  не к родительскому коммиту или иному кандидату.
- **Независимая итоговая приёмка 092 SOURCE+RESULTS:** preliminary SOURCE v2
  PASS — только допуск к трём single-run строкам, не RESULTS и не merge/tag.

Оба raw guard FAIL и их receipt сохраняются; guards не снимаются и не
ослабляются. После итоговой SOURCE+RESULTS интегратор отдельно предъявляет
оркестратору остаток до тега, оценки времени и требуемые решения владельца.

## Открытый compiler debt reg 1909

Class fixture №1899 выявила отдельный native CC-FAIL: method receiver
`t.rows[i].target_state(...)` внутри assert эмитится как адрес временного
`FnDef`; Clang дал три `cannot take the address of an rvalue` в двух assertions.
Это не planned cursor RED. По решению владельца 00:27 разрешён только обход
в новой fixture через `ro` locals; production/generated C и старые evidence
не меняются. Reg 1909 — **OPEN, P2**: адаптация fixture не class fix компилятора.

## Текущее состояние перед общей итоговой приёмкой 092

- №1899 остаётся **OPEN/pending**, явно **БЛОКИРУЕТ ТЕГ: ДА**. Исполнитель
  предъявил class-v2 fresh RED и восемь GREEN cases; окончательного independent
  verdict ещё нет. Учёт routes 109→110 отражает этот уже известный К1, не waiver.
- Doc №1900 имеет independent status-amendment/routes PASS; compiler debt
  остаётся OPEN/nonblocking. Старый raw guard FAIL сохранён как history.
- №1909 остаётся OPEN/P2; ro-local adaptation новой fixture — обход codegen,
  не исправление production. Старый R_NATIVE FAIL и generated C сохранены.
- По решению владельца Linux выделен в черновик 092b, но остаётся условием
  тега вместе с полными build+test examples/packages и exact-SHA CI.
  *(В части Linux, `nova test` и тестов пакетов — ЗАМЕЩЕНО более поздним
  решением владельца 2026-10-25 17:58/18:05, см. поправку 274.16 от
  2026-10-27. Действующие условия рядом: `nova build` через Карину на 118
  примерах и 9 пакетах (Windows+Clang) и exact-SHA CI; Linux, `nova test` и
  тесты пакетов — 0.4; тег — только после нового слова владельца «тег
  разрешён». Запись историческая, PASS не заявляется.)*
  Scoped commit, ordinary gate/std-check, CI и общая приёмка 092 ещё впереди;
  публикация, merge и tag не разрешены этим bookkeeping.

This draft makes no Linux, fixed-point, CI, examples, or packages PASS claim.
It is not a release announcement and must remain marked draft/unreleased until
separately accepted.
