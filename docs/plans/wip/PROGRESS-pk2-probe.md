# pk2-probe: проба актуальности К2-очереди — итог

Модель: **sonnet**. Задание: НИЧЕГО не чинить, только пробы (repro в
изолированных каталогах, свежий `nova.exe` от 2026-08-05 20:52) и вердикты.
Worktree: `d:/Sources/nv-lang/nova-pk2` (ветка `pk2-probe` от `main`,
коммит `e2bad2f42`). Пробы лежат в `probes-pk2/pNNN*/` этого worktree.

Окружение для сборки/теста пакетных реп (path-депы вне git-репы):
`NOVA_CG_INCLUDE`/`NOVA_RT_DIR`/`NOVA_STD_PATH`/`NOVA_GC_LIB_DIR`/
`NOVA_GC_INCLUDE_DIR` указаны на `d:/Sources/nv-lang/nova/compiler-codegen`
(main repo) — иначе `nova test`/`nova build` в чужой репе не линкуется
(нет `libnova_rt`/`gc.lib` рядом).

## Таблица вердиктов (сперва воспроизводящиеся)

| № | Вердикт | Рекомендация |
|---|---|---|
| 36 | ВОСПРОИЗВОДИТСЯ | чинить окном |
| 39 | ВОСПРОИЗВОДИТСЯ (диагноз в реестре шире факта — см. ниже) | чинить окном, уточнить диагноз |
| 126 | ВОСПРОИЗВОДИТСЯ | чинить окном |
| 147 (блокер №4) | ВОСПРОИЗВОДИТСЯ (через №4) | чинить окном (сперва №4) |
| 166 | ВОСПРОИЗВОДИТСЯ | чинить окном |
| 317 | ПОДТВЕРЖДЁН (код-инспекция, не крэш) | чинить окном — как и стоит в очереди |
| 121 | ПОДТВЕРЖДЁН (код-инспекция, не крэш) | чинить окном — как и стоит в очереди |
| 135 | ЧАСТИЧНО УСТАРЕЛ — половина уже пофикшена | переклассифицировать (сузить) |
| 19 | НЕ ВОСПРОИЗВОДИТСЯ КАК ОПИСАНО — вытеснен №166 | закрыть как дубликат №166 |
| 172 | НЕ ВОСПРОИЗВОДИТСЯ — уже пофикшено | закрыть |
| 139 | НЕ ВОСПРОИЗВОДИТСЯ — уже пофикшено (Раунд 3 влит) | закрыть |

---

## №36 — `[M-fluent-value-init-local-deref]` — ВОСПРОИЗВОДИТСЯ

Реальный контекст (`nova-polaris/src/middleware/compress.nv:56-62`) уже несёт
workaround (statement-форма), поэтому пакет polaris не даёт репро напрямую.
Собран самодостаточный минимальный репро 1:1 по форме `ServerResponse`
(`value` + `mut @body(data []u8) -> @`, `probes-pk2/p36_fluent_deref/main.nv`):

Ломающаяся форма (`mut out = resp0.body(gz)` — init mut-локали напрямую из
fluent-сеттера):
```
error: compiler error:
...\main.c:5167:25: error: initializing 'NovaValue_Nzp36Wrap' (aka 'struct NovaValue_Nzp36Wrap') with an expression of incompatible type 'NovaValue_Nzp36Wrap *' (aka 'struct NovaValue_Nzp36Wrap *'); dereference with *
 5167 |     NovaValue_Nzp36Wrap _nv_tmp_439 = nzp36_out;
      |                         ^             ~~~~~~~~~
      |                                       *
1 error generated.
```

Рабочая соседняя форма (`probes-pk2/p36b_working_only/main.nv`: сначала
копия `mut out = resp0`, потом `out.body(...)` отдельным statement)
собирается и работает:
```
built: ...\p36b_working_only\main.exe
$ ./main.exe
working split ok, body.len=3
```

Вывод: дефект семьи №9/№10 (CC-FAIL «assigning T from T*») жив ровно в
описанной форме — init mut-локали напрямую из результата fluent-сеттера на
value-типе. compress.nv-workaround остаётся нужен.

---

## №39 — generic value-record по эффект-типовому параметру — ВОСПРОИЗВОДИТСЯ (диагноз реестра шире факта)

Примечание: `Handler[E, IRT]` — зарезервированное отозванное имя (D61/D87,
переименовано в `Effect[E, IRT]`), в репро это ПОЛЬЗОВАТЕЛЬСКИЙ generic
value-тип с тем же именем `Handler` — не встроенный.

Репро (`probes-pk2/p39_effect_generic_value/main.nv`): `Nzp39Handler[E] value`
инстанцирован ДВУМЯ эффект-типами (`Nzp39Db`, `Nzp39Log`) в одном CU:
```
error: compiler error:
...\main.c:1569:66: error: unknown type name 'NovaValue_Nzp39Handler____Nova_Nzp39Db_p'
...\main.c:1570:67: error: unknown type name 'NovaValue_Nzp39Handler____Nova_Nzp39Log_p'
...
18 errors generated.
```
Дословно совпадает с записью реестра (`unknown type name
NovaValue_Handler____Nova_Db_p`).

**Уточнение диагноза** (сузили пробой): реестр формулирует условие как
«Handler[Db]+Handler[Log] в одном CU» (коллизия ДВУХ инстансов). Проба
`probes-pk2/p39b_single_effect_generic/main.nv` — тот же тип, но
инстанцирован ОДНИМ эффект-типом (`Nzp39bHandler[Nzp39bDb]`, без второго
рядом) — падает С ТОЙ ЖЕ ошибкой:
```
error: unknown type name 'NovaValue_Nzp39bHandler____Nova_Nzp39bDb_p'
...
9 errors generated.
```
Контрольная проба `probes-pk2/p39c_plain_type_generic/main.nv` — тот же
generic value-тип, инстанцирован ОБЫЧНЫМ (не эффектным) типом-параметром
(`Nzp39cHandler[int]`) — собирается и работает:
```
built: ...\p39c_plain_type_generic\main.exe
$ ./main.exe
plain
```
Вывод: реальное условие — «generic value-record инстанцирован ЛЮБЫМ
эффект-типовым параметром» (mono не материализует инстанс для
effect-kind аргумента), а не «два эффект-инстанса в одном CU». Второй
инстанс не обязателен для репро — реестр описывает более узкое условие,
чем есть на самом деле.

---

## №126 — `[M-static-generic-method-path-call-p67-panic]` — ВОСПРОИЗВОДИТСЯ

Репро (`probes-pk2/p126_static_generic_path/main.nv`): статик-generic метод
вызван Path-формой `Type.method[T](x)`:
```
nova: internal error at D:\Sources\nv-lang\nova\compiler-codegen\src\codegen\emit_c.rs:59667: [P67-LEGACY] Path call return type unknown for method=show — checker must annotate (compiler-conventions.md §0)
This is a bug in nova. Please report it.
```
Дословно совпадает с записью реестра.

Рабочая соседняя форма (`probes-pk2/p126b_instance_generic_ok/main.nv`):
ТОТ ЖЕ generic-метод, но вызван через инстанс (`x.show[T]()`, не через
статик-Path) — собирается и работает:
```
built: ...\p126b_instance_generic_ok\main.exe
$ ./main.exe
show = 42
```
Вывод: дефект живёт РОВНО на границе «статик Path-форма против
инстанс-формы» generic-вызова, как и записано.

---

## №147 (через блокер №4 `[M-d39-embed-delegation-dispatch-noop]`) — ВОСПРОИЗВОДИТСЯ

№147 сам по себе — эргономика (не крэш), полностью зависит от №4. Проверен
№4 напрямую: `use name Type` (D39 embed) — авто-прокси полей/методов.

Репро (`probes-pk2/p147_embed_delegation/main.nv`): `Nzp147Audited` embed'ит
`Nzp147Account` (`use nzp147_acc Nzp147Account`), прямой доступ
`a.balance` / `a.balance_pct(...)`:
```
error: compiler error:
...\main.c:5141:98: error: no member named 'balance' in 'struct NovaValue_Nzp147Audited'
...
...\main.c:5165:50: error: passing 'NovaValue_Nzp147Account' ... to parameter of incompatible type 'NovaValue_Nzp147Account *' ...; take the address with &
 5165 |     return Nova_Nzp147Account_method_balance_pct(nova_self->nzp147_acc, arg0);
      |                                                  ^~~~~~~~~~~~~~~~~~~~~
      |                                                  &
5 errors generated.
```
Два независимых симптома диспетчер-no-op: (а) прокси-поле НЕ генерируется
вообще (`a.balance` — «no member»), (б) прокси-метод генерируется, но
codegen забывает взять адрес получателя (передаёт структуру по значению
там, где метод ждёт указатель). №4 воспроизводится дословно по сути
(«диспетчеризация через embed = no-op»); значит и блокировка №147 —
актуальна, не устарела.

---

## №166 — `[M-io-write-all-tcpstream-mono-cc-fail]` — ВОСПРОИЗВОДИТСЯ

`nova test std/src/net` (worktree nova-pk2, env на main repo rt/gc):
```
Toolchain: clang, mode=Dev, jobs=16, paths=[D:\Sources\nv-lang\nova-pk2\std/src/net]
SKIP           std/src/net/neg/double_close_neg  # compile-error lane — requires --full
SKIP           std/src/net/neg/host_str_removed_neg  # compile-error lane — requires --full
SKIP           std/src/net/neg/split_after_use_neg  # compile-error lane — requires --full
CC-FAIL        std/src/net/addr  # D:\Sources\nv-lang\nova-pk2\std/src/net\addr.c:20041:19: error: initializing 'nova_unit' with an expression of incompatible type 'NovaRes_nova_int_NovaValue_IoError *' (aka 'struct NovaRes_nova_int_NovaValue_IoError *') | D:\Sources\nv-lang\nova-pk2\std/src/net\addr.c:20044:49: error: member reference type 'nova_unit' is not a pointer; did you mean to use '.'? | D:\Sources\nv-lang\nova-pk2\std/src/net\addr.c:20044:51: error: no member named 'tag' in 'nova_unit'

===== SUMMARY =====
CC-FAIL        std/src/net/addr  # (тот же текст)
PASS: 0  FAIL: 1  SKIP: 3 (skipped)
```
Дословно совпадает с описанием реестра. Открыт, компиляторная очередь —
подтверждено.

---

## №317 — долг: `narrow_by_param_mode`/`var_consume` в легаси emit_c.rs — ПОДТВЕРЖДЁН (не крэш-репро, код-инспекция)

Это не поведенческий баг, а архитектурный долг (место фикса, не
наблюдаемое поведение) — крэш-репро тут неприменимо по своей природе.
Проверено грепом по коду worktree (тот же код, что и в main на момент
ветвления):
```
$ grep -n "var_consume\|is_consume_eligible_arg\|narrow_by_param_mode" compiler-codegen/src/codegen/emit_c.rs
18680:    fn narrow_by_param_mode(&self, pool: Vec<MethodSig>, args: &[CallArg]) -> Vec<MethodSig> {
38938:                            let matches_v = self.narrow_by_param_mode(type_matches, args);
42838:                                    self.narrow_by_param_mode(pool, args)
```
Механизм по-прежнему в легаси-слое `emit_c.rs`, не в чекер-канале. Запись
реестра актуальна («долг открыт, работа влита с обязательством, задача — в
очередь до тега»). Реклассификация не нужна.

---

## №121 — polaris server-TLS не подключён — ПОДТВЕРЖДЁН (не крэш-репро, код-инспекция + read-only на пакетных репах)

Архитектурный/wiring-гэп, не крэш — согласно заданию, проверено чтением
кода `nova-polaris`/`nova-tls` (правки не вносились):

Конвейер по-прежнему жёстко `TcpStream`-типизирован:
```
$ grep -n "fn serve_connection\|fn read_one_request\|fn run_request" nova-polaris/src/**/*.nv
net/policy.nv:426:export fn read_one_request(mut s TcpStream, policy ServerPolicy, served_count int) Net Time -> GovernedRead {
net/serve.nv:183:fn run_request(mut s TcpStream, handler fn([]u8) -> ServerResponse, raw []u8, policy ServerPolicy, keep_alive bool) Net Time -> ConnStep {
net/serve.nv:351:export fn serve_connection(consume stream TcpStream, handler fn([]u8) -> ServerResponse, policy ServerPolicy) Net Time -> Result[(), NetError] {
```
`TlsListener`/`ConnStream`/`serve_router_tls` — НЕ найдены нигде в
`nova-polaris/src` (grep пуст). Серверный TLS-примитив в `nova-tls`
по-прежнему готов:
```
$ grep -n "fn TlsStream.accept" nova-tls/src/server.nv
53:export fn TlsStream.accept(consume stream TcpStream, config ServerConfig) Net -> Result[TlsStream, TlsError] {
```
Запись реестра актуальна дословно — гэп никуда не делся, план (Go-модель →
Hyper-модель) остаётся тем же следующим шагом.

---

## №135 — path-dep warning не подключён в check/deps — ЧАСТИЧНО УСТАРЕЛ

Реестр утверждает ДВЕ дыры: (1) `manifest_warnings()` не вызывается из
`nova check` вовсе; (2) проверяется только корневой манифест, не манифесты
зависимостей.

**Часть (1) — ОПРОВЕРГНУТА пробой, уже пофикшено.** `nova check` СЕЙЧАС
зовёт `manifest_warnings()` и печатает предупреждение для СВОЕГО прямого
голого `path`-депа:
```
$ nova check main.nv        # (probes-pk2/p172_contract_div, bignum = { path = "..." })
  warning: зависимость `bignum` объявлена голым `path` в [dependencies] (nova.toml) — путь выходит за границу git-репозитория, нет публикуемого источника (версия/git)
    подсказка: ... [W_DEP_PATH_NO_RELEASE]
ok: main.nv
```
Также обнаружено попутно: пример реестра «`nova check src` на
nova-polaris молчит, т.к. манифест несёт `http = { path = "../nova-http" }`»
устарел вдвойне — `nova-polaris/nova.toml` СЕЙЧАС несёт `http` ЧЕРЕЗ
`git+version` (A-V7-миграция, path-форма снята), не через голый `path`.

**Часть (2) — ПОДТВЕРЖДЕНА, всё ещё открыта.** Построен двухуровневый
репро: `probes-pk2/p135_leaf` (манифест несёт голый `path`-деп на
`nova-bignum`, за границей git-репы) + `probes-pk2/p135_consumer`
(зависит от `p135_leaf` по `path` ВНУТРИ той же git-репы — своя ссылка не
предупреждает). `nova check` потребителя:
```
$ nova check main.nv        # (probes-pk2/p135_consumer)
  warning: зависимость `p135_leaf` объявлена голым `path` в [dependencies] (nova.toml) — путь выходит за границу git-репозитория, нет публикуемого источника (версия/git)
    ...
ok: main.nv
```
Предупреждён ТОЛЬКО про собственный `p135_leaf`-деп потребителя.
Транзитивный `bignum`-path-деп, объявленный ВНУТРИ манифеста
`p135_leaf`, НИКАК не всплыл — подтверждает «только корневой манифест».

Вывод: реестр смешивает пофикшенную и живую половины одной записи.
Рекомендация — переклассифицировать/сузить запись до части (2)
(«манифесты зависимостей не проверяются транзитивно»), пункты (в)/(г)/(д)/(е)
реестра (эргономика/громкое оповещение об override/поднятие до ошибки) не
пробовались — не крэш-репро, дизайн-задачи вне охвата этого окна.

---

## №19 — ICE P67-LEGACY для std/net в изоляции — НЕ ВОСПРОИЗВОДИТСЯ КАК ОПИСАНО (вытеснен №166)

`nova test std/src/net` (worktree nova-pk2, folder-CU тянет `stress_test.nv`
с `.now()`, как и описано) даёт НЕ ICE `[P67-LEGACY] ... method=now`, а
CC-FAIL `std/src/net/addr` — БАЙТ-В-БАЙТ ту же ошибку, что и №166 (см.
выше: `nova_unit` vs `NovaRes_nova_int_NovaValue_IoError*`). ICE-путь
`P67-LEGACY` для `method=now` в коде компилятора всё ещё существует
(`grep` в `emit_c.rs`/`types/mod.rs` находит текст паники), но в этом
сценарии не достигается — тест падает раньше, на другой стадии
(`addr.c`), с другой ошибкой.

Вывод: №19, как записан («ICE P67-LEGACY method=now»), больше НЕ
воспроизводится — вытеснен тем же провалом, что и №166 (возможно,
общий корень, но текущий наблюдаемый симптом идентичен №166, не своей
собственной формулировке). Рекомендация: закрыть №19 как дубликат/устаревшую
формулировку №166 (при повторном всплытии — переоткрыть с новым текстом).

---

## №172 — requires с вызовом метода в условии не enforce'ится — НЕ ВОСПРОИЗВОДИТСЯ (уже пофикшено)

`nova-bignum/src/bigdecimal/core.nv:407-408`:
```nova
export fn BigDecimal @div(other BigDecimal, mc MathContext) -> BigDecimal
    requires !other.mant.is_zero()
```
(не `#unverified`, как названо в записи реестра — атрибут отсутствует у
текущего `@div`; возможно, запись описывала более раннюю редакцию).

Существующий тест пакета УЖЕ проверяет ровно это (`core_test.nv:427`,
`panics "requires failed"`) и **проходит**:
```
$ nova test src/bigdecimal        # (nova-bignum, env на main repo rt/gc)
PASS           src/bigdecimal/core
PASS: 1  FAIL: 0
```
Комментарий в тесте (`core_test.nv:421-426`) сам содержит СМЕШАННУЮ
формулировку — первая половина описывает СТАРЫЙ дефект (тот же текст, что
и в реестре 221.1), вторая половина дописана позже: «Contract-first:
`requires !other.mant.is_zero()` now fires at the call boundary (runtime
contract check), int-parity (D423) preserved» — это и есть фикс.

Дополнительно построен изолированный репро
(`probes-pk2/p172_contract_div/main.nv`, `a.div(BigDecimal.zero(), mc)`):
```
$ ./main.exe
panic: main.nv:14: requires failed: !other.mant.is_zero()
```
Паника идёт РОВНО через `requires`-механизм (текст «requires failed:
<условие>», а не через внутренний `?? panic("@div: ...")` на строке 419
core.nv, у которого другой текст). Контракт enforce'ится в рантайме на
границе вызова, как и должно быть по доке. Рекомендация: закрыть №172,
запись реестра — устаревший диагноз (аналог №34/№122/№138/№51 из брифа).

---

## №139 — user generic value-тип как поле структуры — НЕ ВОСПРОИЗВОДИТСЯ (Раунд 3 уже влит)

Реестр помечает запись «🔨 В РАБОТЕ (окно p139, sonnet, РАУНД 3)» с
архитектурной задачей (единая топологически отсортированная секция
value-типов). Проверено — Раунд 3 уже присутствует в коде:
```
$ grep -n "topo\|topological" compiler-codegen/src/codegen/emit_c.rs
663:    /// [реестр 221.1 №139 Round 3 — unified value-type topo-sort] One entry
669:    /// `render_unified_value_types`, which topologically sorts by by-value
...
8552:        // ONE topologically-sorted section covering user value-records,
```
Дословный 8-строчный репро из записи реестра (`type Wrap[T] value { priv
data T }` + `Bundle value { w Wrap[int], n int }`) СЕЙЧАС собирается и
работает:
```
$ nova build main.nv        # (probes-pk2/p139_generic_value_field)
built: ...\p139_generic_value_field\main.exe
$ ./main.exe
ok, n=1
```
Вывод: базовый кейс закрыт, механизм (`render_unified_value_types`) в коде.
Статус «🔨 В РАБОТЕ РАУНД 3» устарел — влито. Рекомендация: закрыть запись
(или перевести в ✅ с ссылкой на коммит, если интегратор хочет сохранить
историю), разблокировать зависимые пункты 222.8 (бандл-экстракторы),
которые упомянуты как ждущие фикса. Регрессии флагмана
(`examples/flagship/aggregator`, `NovaValue_HeaderName`) этим окном НЕ
перепроверялись целиком (вне мандата «только пробы, без полных гейтов») —
рекомендую короткую точечную сверку флагмана перед официальным закрытием.

---

## Итоговая рекомендация по очереди

- **Чинить окном** (актуальны, дословно воспроизведены): №36, №39
  (уточнённый диагноз), №126, №147/№4, №166, №317 (долг), №121 (wiring-гэп).
- **Закрыть**: №172, №139 (оба — задокументированы как «уже пофикшено»,
  живое доказательство приложено).
- **Переклассифицировать**: №135 (сузить до части «манифесты зависимостей
  не проверяются транзитивно»), №19 (закрыть как устаревшую формулировку/
  дубликат №166 — при желании сохранить как «известный сопутствующий
  симптом» пометкой-ссылкой на №166).
