# Пробы к №895 — статический метод протокола через параметр типа и статический set-blanket не резолвятся в кодогене

Заведено исследовательским окном (owner-research) 2026-09-04 по замечанию интегратора: строки реестра ссылались на пробы `examples/_pidxN/`, которых в дереве нет (правило плана 278 Ф.5/Ф.6 — находка без файла-пробы не доказана; `examples/` — поставляемый корпус, не место для улик). Пробы воспроизведены заново под именами предложения `docs/dev/research/2026-09-04-typed-index-vec.md` (`Ordinal` / `@ordinal()` / `.from_ordinal()`); цитаты в строках реестра были сняты под рабочими именами `Idx` / `@idx()` / `.from_idx()` — существо то же.

Чекер зелёный, падает `nova build`. Контроль `direct` — тот же статический вызов без параметра типа собирается и печатает.

## Как запускать

Суффикс `.nv.txt` — по правилу этого каталога (README, п. 2): улика вне раннера и стражей. Для прогона скопировать без `.txt` в каталог внутри репозитория (например, сюда же) и вызвать:

```sh
cp docs/plans/repro/895-static-via-type-param/<файл>.nv.txt docs/plans/repro/895-static-via-type-param/<файл>.nv
nova-cli/target/release/nova.exe check docs/plans/repro/895-static-via-type-param/<файл>.nv
nova-cli/target/release/nova.exe build docs/plans/repro/895-static-via-type-param/<файл>.nv -o <куда-нибудь>.exe
```

## Что замерено 2026-09-04 (owner-research, `nova-cli/target/release/nova.exe`)

| проба | ожидание | `nova check` | `nova build` |
|---|---|---|---|
| `newtype_bound.nv.txt` | build: E_UNKNOWN_STATIC_METHOD int.from_ordinal | `ok: docs/plans/repro/895-static-via-type-param/newtype_bound.nv` | `error: codegen error: [E_UNKNOWN_STATIC_METHOD] `int.from_ordinal(...)` — у примитива `int` нет статического метода `from_ordinal`. Валидные static-ме` |
| `set_blanket_static.nv.txt` | build: INTERNAL-PANIC E_CODEGEN_TYPE_UNKNOWN | `ok: docs/plans/repro/895-static-via-type-param/set_blanket_static.nv` | `error: codegen error: [INTERNAL-PANIC] [E_CODEGEN_TYPE_UNKNOWN] Path call return type unknown for method=from_ordinal` |
| `direct.nv.txt` | build ok, prints 42 | `ok: docs/plans/repro/895-static-via-type-param/direct.nv` | `built; stdout: 42` |

## Замер 2026-09-18 (интегратор): ось шире, чем говорила строка, и причина названа зондом

Четыре клетки добавлены в тот же каталог. Каждая меняет ОДИН фактор — иначе
«воспроизвелось» не отделить от «похоже».

| проба | что меняется | `check` | `build` |
|---|---|---|---|
| `b_record.nv.txt` | newtype над ЗАПИСЬЮ, а не над примитивом | ok | `lld-link: undefined symbol: Nova_Cell_static_from_ordinal` |
| `c_plain_record.nv.txt` | БЕЗ newtype: обычная запись | ok | собрано, печатает `42` |
| `d_instance_bound.nv.txt` | ИНСТАНСНАЯ половина через тот же `I` | ok | собрано, печатает `42` |
| `e_neg_int.nv.txt` | `I = int` без реализации протокола | `[E_BOUND_NOT_SATISFIED]` | — |

**Что каждая доказывает.** `b_record` — отказ НЕ про «ветку примитивов»: подставлено
представление `Cell` вместо newtype `Wrap`, и оттого отказ тише и позже (линковка, а
не диагностика). `c_plain_record` отделяет ось от параметра типа как такового.
`d_instance_bound` отделяет её от инстансной позиции. `e_neg_int` показывает, что
негативную половину приёмки чекер исполняет уже сегодня, — она контроль, а не работа.

**ПРИЧИНА (временный зонд в эмиттере, снят после замера):**

```
ZOND895 key="I" method="from_ordinal" subst_has=true rt=Some(Raw("nova_int"))
ZOND895 overload_keys_for_method=[("FnRow", "from_ordinal")]
```

Подстановка параметра типа хранит ЗАМОРОЖЕННУЮ C-СТРОКУ (`ResolvedType::Raw`), а не
тип: имя newtype'а стирается ДО эмиссии вызова. Таблица диспетчеризации при этом
права — в ней ровно `("FnRow", "from_ordinal")`. Значит чинить надо не диспетчер:
`Raw`/`lift_c_name` объявлены в самом эмиттере как transitional debt, снимаемый
шагом A1″ плана 172.12. И там же видно второе следствие: mono ключуется
C-представлением (`mk____nova_int`), то есть `mk[FnRow]` и `mk[int]` — один
экземпляр.

**Узкий фикс пробован и откачен:** помощник, спрашивающий подстановку вместо C-имени,
написан и прогнан — обе пробы остались красными без изменений, потому что спрашивать
уже нечего.
