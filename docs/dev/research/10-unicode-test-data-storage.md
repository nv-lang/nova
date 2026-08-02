<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->
# Как хранят тяжёлые Unicode-тест-данные: Go / Rust / TS / Kotlin-Java + референсы (ICU / CPython)

> **Дата:** 2026-06-14. **Тип:** research / cross-ecosystem survey.
> **Метод:** multi-agent web-верификация по первоисточникам (GitHub Contents API, репозитории, гайды) —
> 7 параллельных исследователей (Go, Rust, TS/Node, Kotlin-Java, ICU/CLDR, CPython + механизмы) + синтез.
> **Повод:** Plan 156 наш slow-lane закоммитил ~23 МБ полных conformance-корпусов (`*_conformance_slow.nv`);
> вопрос — нужна ли отдельная репа / git-lfs, и как сделать «не хуже / лучше» мейнстрима.
> **Принятое решение:** Option E/F — **регенерировать on-demand** (см. ниже). Owns: `[M-test-runner-large-test-lane]`.

## TL;DR

- **Никто не коммитит полный conformance-корпус как «истину» без альтернативы.** Доминирует двухуровневость: **быстрый дефолт (мелкий сэмпл) + тяжёлый opt-in**.
- **git-lfs для текстовых фикстур не использует НИКТО** (только ICU, и только для opaque-бинарей `.dat`/`.jar`/`.zip`). → нам lfs не нужен.
- **Отдельная репа для тестов оправдана только при огромных/общих данных** (tc39/test262, llvm-test-suite на 756 МБ, unicode-org/cldr). Для соло-монорепо на 23 МБ — оверкилл.
- **Сгенерированные property-таблицы — коммитят** (Go, Rust, CPython, ICU, OpenJDK-в-gensrc). Сырой UCD — обычно НЕ коммитят (качают в gitignored-кэш). Nova так и делает (`*_data.nv` + `--ucd-dir`). ✅
- **Главная ось разногласий:** полный корпус **коммитят** (Rust/OpenJDK/ICU) или **качают/регенерят on-demand** (Go, CPython — у обоих есть генератор, как у нас).
- **Рекомендация для Nova:** регенерировать on-demand (как Go/CPython) — у нас уже есть байт-идентичный генератор, поэтому коммит 23 МБ почти ничего не даёт сверх генератора, но навсегда утяжеляет историю.

## Сравнительная таблица

| Экосистема | Property-таблицы (cp→prop) | Полные conformance-данные | Размер в репе | Отд. репа? | git-lfs? |
|---|---|---|---|---|---|
| **Go** (stdlib `unicode` + x/text) | генерят из UCD → **коммитят `.go`** (`tables.go` 257 КБ; x/text держит 2 версии ~400 КБ). Сырой UCD → gitignored `/DATA` | **download-on-demand + кэш, по умолчанию SKIP** (`-long`/`-local`/`UNICODE_DIR`) + мелкий сэмпл вшит в код | **0** (NormalizationTest/CollationTest/allkeys не коммитят) | нет (де-факто unicode.org) | нет |
| **Rust** unicode-normalization | генерят → **коммитят `.rs`** (`tables.rs` 612 КБ) | **запекают полный файл в `.rs`-const** (`normalization_tests.rs` 4.07 МБ) | ~4.07 МБ | нет | нет |
| **Rust** icu4x | **коммитят baked `*.rs.data`** (per-component, `linguist-generated`) | source-инпуты **качают** (`cargo make download-repo-sources`); кросс-impl — **отд. репо** (unicode-org/conformance) | subset | да (conformance/cldr) | нет |
| **TS/Node** (Node + V8) | **бинарный ICU-блоб** `icudt##l.dat.bz2` (сжат, ~1.5 МБ small / ~25–30 МБ full), собран upstream ICU | JS/Intl conformance = **отд. репо tc39/test262** (pinned: DEPS у V8, submodule у Node WPT) | ~0 текста (данные — ICU-блоб) | **да** (test262) | нет (сжатый блоб) |
| **Java** OpenJDK | сырой UCD **коммитят** (`src/.../unicodedata/`, UnicodeData.txt 2.2 МБ) → таблицы **генерят при сборке** в gensrc (не коммитят) | **коммитят `.txt` в репе** (NormalizationTest.txt 2.83 МБ, GraphemeBreakTest 127 КБ), читают на тесте | ~3 МБ+ | нет | нет |
| **Java** ICU4J | offline-генерят → **коммитят бинарь** (~31.6 МБ ресурсов, в ~14 МБ jar) | **коммитят `.txt`** (~33.8 МБ) + ship **SHORT-сэмплы** коллации (2.2–2.5 МБ вместо полного UCA) | ~33.8 МБ | source = cldr | нет |
| **ICU** (эталон C/C++) | сырой UCD коммитят (`ppucd.txt`) + **бинарь-таблицы коммитят** (`source/data/in/*.icu`/`*.nrm` ~0.9 МБ); регген только на version-bump | **коммитят `.txt`** (BidiTest 7.96 МБ, LineBreak 3.17 МБ) + **SHORT-коллация** 2.2–2.5 МБ; новый кросс-impl JSON — **gitignore, генерят** | ~30 МБ+ | cldr (source); conformance (cross-impl) | **да** — только opaque `.dat`/`.jar`/`.zip` |
| **CPython** | генерят → **коммитят C** (`unicodedata_db.h` 612 КБ); сырой UCD → gitignored-кэш | **download-on-demand + кэш, по умолчанию SKIP** (`-u network,cpu`, **skip-never-fail** offline) + 1 мелкий frozen-сэмпл (NormalizationTest-3.2.0.txt 2.03 МБ) | 2.03 МБ (1 frozen) | нет | нет |
| **NOVA (до решения)** | генерят → коммитят `std/unicode/*_data.nv` (~0.9 МБ) ✅ | двухуровнево, **ОБА коммичены**: сэмпл 1.64 МБ + полные slow **21.9 МБ** | **~23.5 МБ** | нет | нет |

## По экосистемам (детали + источники)

### Go (stdlib `unicode` + golang.org/x/text)
- **Таблицы:** генерятся из UCD скриптами x/text (`internal/export/unicode/gen.go`, `unicode/norm/maketables.go`, `//go:build ignore`), результат **коммитится** как `.go` (`src/unicode/tables.go` = 256 788 B; `tables15.0.0.go`/`tables17.0.0.go` ~400 КБ — держат две версии). Сырой UCD качается в `/DATA` (в `.gitignore`).
- **Conformance:** **download-on-demand** через `internal/testtext` `SkipIfNotLong` — NormalizationTest.txt, UCA `CollationTest.zip` + `allkeys.txt` (multi-MB) качаются с `unicode.org/Public/...` на тесте, по умолчанию **SKIP** (нужен `-long`/`-local`/`UNICODE_DIR`). Плюс мелкий сэмпл вшит в исходник (`const txt_canon`).
- **Почему:** Go-модули не дружат с lfs/submodule; коммит сгенерённого `.go` = герметичная offline-сборка; большие фикстуры не раздувают историю.
- Источники: github.com/golang/text (`internal/gen/gen.go`, `.gitignore`, `internal/testtext/flag.go`, `unicode/norm/{maketables,ucd_test,normalize_test}.go`, `collate/reg_test.go`); github.com/golang/go `src/unicode/tables.go`.

### Rust (три модели)
- **unicode-normalization:** `scripts/unicode.py` качает UCD на лету → **коммитят** `src/tables.rs` (625 711 B) И запекают NormalizationTest.txt в **коммиченный** `tests/data/normalization_tests.rs` (4 075 100 B ≈ 4.07 МБ) как Rust-const. Тест итерирует const, ничего не качает.
- **ucd-generate (BurntSushi):** CLI, юзер сам качает UCD → коммитят сгенерённые `.rs` (regex-syntax/bstr). Сырой UCD не в репе.
- **icu4x:** **коммитят** baked `provider/data/**/*.rs.data` (norm: nfd 86 КБ, nfkd 121 КБ, uts46 138 КБ, …; `.gitattributes linguist-generated`, **без lfs**); datagen-инпуты качают (`cargo make download-repo-sources`); кросс-impl conformance — **отдельная репа** unicode-org/conformance.
- Источники: unicode-rs/unicode-normalization (`scripts/unicode.py`, API sizes); BurntSushi/ucd-generate; unicode-org/icu4x (`CONTRIBUTING.md`, `provider/data/.gitattributes`, API sizes); unicode-org/conformance.

### TypeScript / Node (Node + V8)
- **Данные:** делегированы ICU — единый бинарь `icudt##l.dat` (small ~1.5 МБ / full ~25 МБ). В nodejs/node коммитится **сжатым**: `deps/icu-small/source/data/in/icudt78l.dat.bz2`, тримится `tools/icu/shrink-icu-src.py`. С Node 13 дефолт = full-icu. Сырой UCD не коммитят (ICU генерит upstream).
- **Conformance:** JS/Intl = **отдельная репа** tc39/test262 (50 000+ файлов), вендорится pinned: V8 через Chromium DEPS + `gclient sync` в `test/test262/data`; Node берёт WPT как git-submodule. В test262 нет multi-MB Unicode `.txt`.
- **Грабли:** small-icu (English-only) удивлял пользователей → перешли на full-icu по умолчанию.
- Источники: nodejs/node `doc/contributing/maintaining/maintaining-icu.md`, `deps/icu-small/...`, PR #29522; tc39/test262; v8 `test/test262/`.

### Kotlin / Java / JVM (OpenJDK + ICU4J)
- **OpenJDK:** сырой UCD **коммитят** (`src/java.base/share/data/unicodedata/`: UnicodeData.txt 2.2 МБ, DerivedCoreProperties 1.13 МБ, …). Таблицы **генерят при сборке** (`GenerateCharacter.java` + `*.java.template` → gensrc, **не коммитят**). Conformance `.txt` **коммитят рядом** (NormalizationTest.txt 2.83 МБ, GraphemeBreakTest 127 КБ).
- **ICU4J:** offline-генерят → **коммитят бинарь-ресурсы** (~31.6 МБ) → в jar (icu4j-77.1.jar = 14.6 МБ). Conformance `.txt` **коммитят** (~33.8 МБ); коллация — **SHORT-сэмплы** (CollationTest_SHIFTED_SHORT.txt 2.48 МБ) вместо полного UCA. Source локалей = отдельная репа unicode-org/cldr.
- Kotlin своих таблиц не имеет — делегирует JDK/ICU4J.
- Источники: openjdk/jdk (API trees + sizes; `GensrcCharacterData.gmk`); unicode-org/icu `icu4j/...` (API sizes); Maven Central (HEAD jar sizes); `tools/cldr/cldr-to-icu`.

### ICU (эталон Unicode Consortium) + CLDR
- **Таблицы:** сырой UCD коммитят как `ppucd.txt`; бинарь-таблицы **коммитят** (`source/data/in/uprops.icu` 170 КБ, `unames.icu` 342 КБ, `*.nrm` 36–61 КБ); регген только при version-bump (`preparseucd.py`/`genprops`), не при обычной сборке.
- **Conformance:** классические фикстуры **коммитят** `.txt` (BidiTest 7.96 МБ, LineBreak 3.17 МБ, CollationTest_*_**SHORT** 2.2–2.5 МБ). Новый кросс-impl unicode-org/conformance — JSON **генерят on-demand, gitignore**.
- **git-lfs:** **только** для opaque-бинарей — `.gitattributes` гонит `*.jar *.dat *.zip *.gz *.bz2 *.gif` через `filter=lfs`; `.icu`/`.nrm`/`.txt` — обычные.
- **Отдельные репы:** unicode-org/cldr (source данных), unicode-org/conformance (cross-impl).
- Источники: unicode-org/icu (`icu4c/source/{test/testdata,data/in}`, `.gitattributes`, userguide), unicode-org/cldr `common/uca`, unicode-org/conformance.

### CPython + сквозные механизмы
- **Таблицы:** `Tools/unicode/makeunicodedata.py` качает UCD в gitignored-кэш → **коммитят** C-хедеры (`unicodedata_db.h` 612 КБ). Рантайм не трогает сеть.
- **Conformance:** NormalizationTest.txt (2.83 МБ, 17.0) **не коммитят** — `download_test_data_file` с pythontest.net-зеркала (по версии), кэш в `TEST_DATA_DIR`, гейт `@requires_resource('network','cpu')` → по умолчанию **SKIP**, при ошибке → **SkipTest (never fail)** offline. Плюс 1 frozen-сэмпл `Lib/test/NormalizationTest-3.2.0.txt` (2.03 МБ) **коммичен**.
- **Меню механизмов (pro/con):**
  - **git-lfs** — free-tier лимиты (1 ГиБ/мес), ломает shallow/partial-clone, public-fork billing, доп. зависимость. Использует только ICU и только для бинарей.
  - **git-submodule** — detached-HEAD / GC'нутый pinned-commit ломают CI; слабая эргономика (особенно exFAT/Windows).
  - **separate test-data repo** — оправдан при ОГРОМНЫХ (llvm-test-suite 756 МБ) или ОБЩИХ (test262) данных.
  - **download-on-demand + cache** (Go/CPython) — самые лёгкие репы; нужна сеть/зеркало + skip-never-fail.
  - **commit-sampled-subset** — мелкий сэмпл в репе, полное opt-in.
- Источники: python/cpython (`Tools/unicode/makeunicodedata.py`, `Lib/test/test_unicodedata.py`, `Lib/test/support/__init__.py`, frozen fixture); github docs (LFS quotas); llvm TestSuite docs.

## Анализ

### Консенсус — Nova уже соответствует ✅
1. Коммитить сгенерированные property-таблицы; не парсить UCD при сборке (`*_data.nv` ✅).
2. Сырой UCD не в репе (`--ucd-dir`-mirror = модель Go/CPython ✅).
3. Двухуровнево: быстрый дефолт + тяжёлый opt-in (Plan 156 slow-lane ✅).
4. git-lfs для текстовых фикстур не использует никто → нам не нужен.

### Ось разногласий — что выбрать
Полный корпус **коммитят** (Rust 4 МБ, OpenJDK 2.8 МБ, ICU/ICU4J 8+ МБ — но крупнейшие ставят **SHORT-сэмплы**) **или качают/регенерят** (Go, CPython — у обоих генератор, как у нас).

Наши **15.5 МБ collation** — больше, чем у кого-либо в репе (ICU держит 2.5 МБ SHORT). На каждый Unicode-bump это **+~16 МБ в историю навсегда** (227 800 переставленных строк git не дельтит).

## Опции для Nova

- **A — оставить in-repo как есть.** Pro: offline-герметично, уже сделано, = Rust/OpenJDK/ICU. Con: +23.5 МБ в истории, +16 МБ/bump навсегда; крупнейший in-repo корпус.
- **B — git-lfs для slow.** ❌ Reject: никто не делает для фикстур; ломает shallow-clone; плохо на exFAT/Windows; bandwidth-локауты.
- **C — отдельная тест-репа.** Premature: оправдано при огромных/общих данных; мы соло, 23 МБ.
- **D — git-submodule.** ❌ Reject: detached-HEAD CI-грабли; слабо на exFAT; 16 worktree × init.
- **E/F — download/regenerate-on-demand + cache (рекомендовано).** Убрать slow из репы; регенерить из pinned UCD в gitignored-кэш для `--slow-only`, skip-never-fail; коммитить только сэмпл. = Go/CPython (самые лёгкие репы).

## Рекомендация и принятое решение: **Option E/F (regenerate-on-demand)**

Почему именно нам это лучше «оставить как есть»:
1. У Nova **уже есть байт-идентичный детерминированный генератор** (`nova-codegen unicode --emit-conformance --conformance-full --ucd-dir <UCD>`). Главный аргумент за коммит — воспроизводимость — и так гарантирован. Go и CPython, у которых **тоже** есть генератор, **оба** выбрали download-on-demand ровно поэтому. Следуем экосистемам, разделяющим наше ключевое свойство (генератор), а не тем (Rust/OpenJDK/ICU), кто коммитит из-за тяжёлого/внешнего тулчейна.
2. Вес истории: collation 15.5 МБ / 227 800 пар, +~16 МБ/bump навсегда (не дельтится). При соло + ~16 worktree (общий `.git`) + быстрый main держать регенерируемый build-output вне истории — выше по гигиене.
3. exFAT/Windows убивает lfs/submodule (нет symlink). download+cache им не нужен.
4. Точнее ложится в требование «малый сэмпл в регрессе, полное opt-in/out-of-band» — где «out-of-band» = регенерируется-не-в-истории.

**Честный runner-up:** Option A легитимен (уже сделан, корректен, offline-герметичен, = Rust/OpenJDK/ICU; дефолт-регресс и так быстр — тела slow-файлов не читаются при discovery). Выбор — чисто про «мешают ли ~23 МБ (и +16 МБ/bump) регенерируемого build-output в истории навсегда». Для нашего профиля (соло + 16 worktree + быстрый main + exFAT) — E/F.

**❌ git-lfs (B) и submodule (D) — не использовать ни при каком раскладе** (хуже на exFAT/Windows, непрецедентно для фикстур кроме ICU-бинарей).

Это и есть ответ на исходный вопрос: **отдельная репа не нужна, git-lfs не нужен; оптимум — регенерация on-demand, что ставит нас наравне с самыми лёгкими репами (Go/CPython) — т.е. «лучше», чем у большинства.**

## Миграция (выполняется на ветке plan-156 ДО мержа)
Ключевой тайминг: slow-файлы сейчас на `plan-156`, но **ещё не в main**. Чтобы 23 МБ не попали в постоянную историю main — **выкинуть Populate-коммит целиком** (rebase --onto, drop), а не `git rm` сверху. Затем: `.gitignore` для `*_conformance_slow.nv`; оставить коммиченным сэмпл `*_conformance.nv`; контракт «full регенерируется on-demand из pinned UCD, `--slow-only` находит после регена / 0 (skip) если отсутствуют»; обновить spec D277 / Plan 156 / test-conventions / simplifications / логи.
