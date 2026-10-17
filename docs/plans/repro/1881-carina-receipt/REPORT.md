# №1881 — provenance receipt Carina 0.2.0: независимый разбор

Задача #98. Read-only разбор опубликованных release assets `carina-v0.2.0` по реестру
[221.1 №1881](../../221.1-bug-sweep.md). Никакие опубликованные releases/assets/body не
изменялись; все проверки — скачивание публичных bytes, GitHub API read-only запросы, хеширование
в session scratchpad. Локальные scratch-пути в тексте намеренно не приводятся.

## 1. Объекты и их идентификаторы

Release `carina-v0.2.0` (release id 409764429, `prerelease: true`, `draft: false`):
создан 2026-10-10T19:30:03Z, опубликован 2026-10-11T16:52:20Z. Тег указывает на коммит
`99756a8867f913bd6156ed10accf8a20edea00d1` (`git refs/tags/carina-v0.2.0`, type commit) — это
совпадает с accepted SHA из [274.11 §0](../../274.11-carina-release.md) и с release body.

Актуальный список assets (GitHub API `repos/nv-lang/nova/releases/tags/carina-v0.2.0`,
снят 2026-10-17):

| asset | id | size (API) | created_at | updated_at |
|---|---|---|---|---|
| `nova-carina-0.2.0-windows-x86_64.zip` | 631171872 | 2 402 373 | 2026-10-11T16:04:46Z | 2026-10-11T16:05:00Z |
| `nova-carina-0.2.0-src.tar.gz` | 631172559 | 36 462 159 | 2026-10-11T16:05:03Z | 2026-10-11T16:08:08Z |
| `SHA256SUMS` | 631180044 | 293 | 2026-10-11T16:08:26Z | 2026-10-11T16:08:27Z |
| `proof-and-accept-receipt.txt` (текущий) | 635759780 | 7 232 | 2026-10-13T03:26:50Z | 2026-10-13T03:26:50Z |

Исторический исходный receipt asset id `631179653` в списке отсутствует (см. §4).

## 2. Независимая byte-level сверка (скачано и хешировано заново в этой задаче)

Опубликованный `SHA256SUMS` (asset 631180044), 293 bytes, SHA-256
`b760af266e464d93651ad993780c3c30e53608f6b17eb6130b189c4746a46596`, содержит три строки:

```
f9f973e2c83a930a68053b4952da47f4b72f8d50624fc6e86742da22fc4e12bf *nova-carina-0.2.0-windows-x86_64.zip
dd286cf2c661f42028222212565272b42f7ce9395b61ea615150d9a6ef735c36 *nova-carina-0.2.0-src.tar.gz
9e3f509d21a8fb1c708cf1e42c67fa53415cff5b9fbd091212ad42832168a90e *proof-and-accept-receipt.txt
```

Результаты сверки скачанных bytes:

| asset | скачанный размер | SHA-256 скачанного | строка manifest | вердикт |
|---|---|---|---|---|
| zip | 2 402 373 | `f9f973e2c83a930a68053b4952da47f4b72f8d50624fc6e86742da22fc4e12bf` | та же | **MATCH** |
| src.tar.gz | 36 462 159 | `dd286cf2c661f42028222212565272b42f7ce9395b61ea615150d9a6ef735c36` | тот же | **MATCH** |
| receipt | 7 232 | `cb3466a2067914eadcee0fcf3487b18b6951e4740370909bb3f6099a0b34566d` | `9e3f509d…` | **MISMATCH** |

Размеры совпали с полем `size` из API по всем трём позициям. Mismatch receipt независимо
воспроизведён: значения идентичны записанным в реестре №1881 (проверки #93 и приёмщика).

Дополнительные сверки содержимого:

- `SOURCE-SHA` внутри zip = `99756a8867f913bd6156ed10accf8a20edea00d1`; `SOURCE-SHA` внутри
  src.tar.gz = тот же. Оба совпадают с целью тега и с `source_revision` из
  [docs/dev/carina-seeds/windows-x86_64.json](../../../dev/carina-seeds/windows-x86_64.json).
- `novac.exe` внутри zip: 3 876 352 bytes, SHA-256
  `b3df460726e92eca612d7df3497c1a6184c07f415bf58f80001717cb2cf8536f` — совпадает с hash
  accepted stock B/C executable, задокументированным в 274.11 §0 (документальная сверка; сам
  локальный evidence-дом задачи #55 недоступен, см. §4). Это подтверждает claim release body
  «The Windows ZIP contains the saved stock B `novac.exe` from task #55» на уровне digest.

## 3. Текущий receipt (635759780): что это за байты

Скачанный receipt 7 232 bytes, SHA-256 `cb3466a2…`, побайтно совпадает с репо-файлом
[docs/dev/release-artifacts/carina-v0.2.0/proof-and-accept-receipt.txt](../../../dev/release-artifacts/carina-v0.2.0/proof-and-accept-receipt.txt)
(коммит `f978ae36a2c4eaf987c20672171cb833f05eb0bf` «docs(release): add sanitized Carina
receipt candidate», 2026-10-13 05:49:37 +0300) после нормализации CRLF→LF: репо-копия
7 358 bytes содержит ровно 126 байт CR (по одному на каждую из 126 строк); удаление CR даёт
7 232 bytes и digest `cb3466a2…` в точности. Загрузка asset (2026-10-13T03:26:50Z =
06:26:50 +0300) произошла через ~37 минут после этого коммита.

Содержимое опубликованного receipt:

- Называет себя «replacement candidate … not yet uploaded» — то есть сам asset отрицает, что
  он загружен, будучи загруженным.
- Сообщает, что исходный receipt (asset 631179653, SHA-256 `9e3f509d…`) говорил «Reused
  private Rust oracle» с private-oracle SHA-256
  `7c830cc31b94f25fd24827be0248c3a319feb25d4b5e78464a9cfdcaae916eee`, и что та proof «did not
  use the released Oracle binary».
- Описывает отдельную reproduction из опубликованного Oracle asset с конкретными digest входов
  (см. §6) и результатами B ≡ C: B/C executables 3 874 304 bytes, SHA-256 `f2b70da8…`;
  B.c/C.c 13 250 302 bytes, SHA-256 `de274aa1…`.

Важно: числа candidate-прогона (B exe 3 874 304 / `f2b70da8…`) **не совпадают** с accepted
stock evidence (B/C exe 3 876 352 / `b3df4607…` по 274.11 §0) и с B0 внутри zip. Candidate —
это отдельная reproduction в другой среде (clang 22.1.5), он сам так и заявляет; выводить из
него provenance исходного proof нельзя.

## 4. Восстановление исходного receipt asset 631179653: каналы и результат

Проверенные каналы:

1. **GitHub API** `repos/nv-lang/nova/releases/assets/631179653` → **404 Not Found**.
   Удалённый asset по API не отдаётся; публичной истории удаления/замены assets GitHub не
   предоставляет. Идентификатор 631179653 лежит между src (631172559) и SHA256SUMS (631180044),
   что согласуется с загрузкой исходного receipt в той же сессии 2026-10-11 ~16:05–16:08Z.
2. **Локальный evidence-дом приёмки #55** (`<worktree>/nova-opencode-55-final-997/target/
   acceptance-evidence/`, назван в 274.11 §0): worktree существует, но `target/` пуст —
   `proof-and-accept-receipt.txt`, `bootstrap-stock.log`, sabotage-логи и
   `bootstrap-stock-artifacts/` локально недоступны.
3. **Session scratchpads предыдущих задач**: найдены только две копии текущего replacement
   (7 232 bytes, `cb3466a2…`), скачанные задачей #93. Поиск по содержимому («Reused private»)
   находит лишь цитату внутри самого replacement, ни одной копии исходных bytes.
4. **Wayback Machine CDX API** для URL скачивания receipt: обращения 2026-10-17 дважды
   вернули HTTP 429 (rate limit); наличие снимка **не проверено** — это единственный
   оставшийся публичный канал, который в этой задаче закрыть не удалось.

**Вердикт: исходные bytes receipt 631179653 восстановить не удалось.** Строка
`9e3f509d… *proof-and-accept-receipt.txt` в опубликованном SHA256SUMS — это digest без
доступного содержимого: она доказывает только, что 2026-10-11 ~16:08Z под этим именем был
загружен некий файл с этим digest. Что именно он доказывал (и доказывал ли A→B→C), по
публичным артефактам установить нельзя. Единственное свидетельство о его содержимом — цитата
внутри replacement candidate («Reused private Rust oracle», private-oracle `7c830cc3…`),
single-source и непроверяемая; если она точна, исходный receipt фиксировал A→B→C со stage A
НЕ из опубликованного Oracle asset, что противоречило бы норме release notes «stage A must
come from the Oracle release». Это противоречие уровня claims, а не установленный факт.

## 5. Release body против содержимого asset

Текущий release body (API, идентичен
[docs/dev/carina-release-notes-v0.2.0.md](../../../dev/carina-release-notes-v0.2.0.md) с
переписанными ссылками) утверждает: «The acceptance receipt records the double-build sequence
A → B → C … The saved receipt is `proof-and-accept-receipt.txt:1–15` …». Имя совпадает с
именем опубликованного asset, поэтому читатель неизбежно относит это утверждение к asset.
Фактические bytes asset: «replacement candidate … not yet uploaded … it does not
retroactively change the provenance of the original proof. The prior receipt's provenance
must not be attributed to this reproduction.» — то есть asset сам запрещает приписывать себе
доказательную силу исходного proof. Примечательно, что ссылка `:1–15` в body указывает на
**локальный** receipt из evidence-дома #55 (см. 274.11 §0), а не на release asset; но эта
тонкость неотличима для внешнего читателя, а сам evidence-дом недоступен (§4.2).

## 6. Что из заявленной цепочки публично проверяется сегодня

Подтверждено независимой проверкой в этой задаче:

- Manifest → bytes для zip и src.tar.gz (§2); SOURCE-SHA обоих архивов = цель тега =
  accepted SHA `99756a88…`.
- B0 executable внутри zip имеет тот же digest, что задокументирован для accepted stock B/C
  (3 876 352 bytes, `b3df4607…`).
- CI на том же SHA: run
  [38080300506](https://github.com/nv-lang/nova/actions/runs/38080300506) — `head_sha`
  `99756a88…`, `conclusion: success`, все 8 jobs success, включая `novac-gate`
  (перепроверено через API).
- Все проверяемые утверждения replacement candidate о его **входах** верны: Oracle release
  assets (ids 631158204 / 631163540 / 631148753) существуют с указанными размерами; Oracle
  SHA256SUMS (302 bytes) содержит ровно те digest, что процитированы
  (`54093317…` windows zip, `0ac9b18a…` src); скачанный Oracle windows zip совпал с manifest,
  `nova.exe` внутри него — member архива с SHA-256
  `ebe1dc72c52317a9f7f9dc649ecc1c10eb49f759ade458259c47b10f7761a41e`, как записано в candidate.

НЕ подтверждено / непроверяемо публично:

- Сам факт A→B→C double build на accepted SHA: единственная публичная «расписка» об этом —
  asset, отрицающий, что он расписка. Результаты candidate-прогона (digest B/C `f2b70da8…`,
  smoke, negative-input, sabotage) в этой задаче не переисполнялись и ничем публичным не
  подкреплены — это claims документа, загруженного после замены.
- Цитата о содержимом исходного receipt («Reused private Rust oracle») — single-source.
- Связь digest `9e3f509d…` с любым содержимым — bytes недоступны (§4).

Вывод: опубликованный bundle сегодня доказывает целостность zip/src и их привязку к accepted
SHA и к зелёному CI того же SHA, но **не доказывает bootstrap chain A→B→C**. Из replacement
asset нельзя выводить A→B→C исходного proof — это запрещено и самим документом.

## 7. Негативная проба (sabotage) — дословный вывод

На копии скачанного receipt в session scratchpad; контрольный manifest содержит истинный
digest скачанного файла (`cb3466a2…`). Команды: `sha256sum -c` → инверсия байта 101 копии →
`sha256sum -c` → восстановление → `sha256sum -c`; затем проверка опубликованной строки
manifest против опубликованного receipt.

```
== 1. GREEN control: true digest of downloaded receipt
probe.txt: OK
rc=0
== 2. SABOTAGE: flip byte 101 of the copy
probe.txt: FAILED
rc=1 (expect non-zero = RED)
== 3. RESTORE: pristine copy again
probe.txt: OK
rc=0
== 4. PUBLISHED MANIFEST vs PUBLISHED RECEIPT (the finding itself)
receipt-published.txt: FAILED
rc=1 (expect non-zero = RED: published SHA256SUMS does not match published receipt)
sha256sum: WARNING: 1 computed checksum did NOT match
```

Шаг 4 — сам дефект №1881, воспроизведённый инструментом верификации; шаги 1–3 доказывают,
что метод различает RED/GREEN на этом материале.

## 8. Класс и поиск других носителей

Класс дефекта (по реестру №1881): опубликованная immutable release evidence не привязана к
checksum manifest / manifest не соответствует bytes. Проверены все релизы репозитория — их
ровно два:

- `oracle-v0.1.0`: все три бинарных asset скачаны и сверены с его SHA256SUMS — windows zip
  16 073 849 bytes `54093317…` **MATCH**, linux tar.gz 16 633 733 bytes `08257eb1…` **MATCH**,
  src tar.gz 36 482 965 bytes `0ac9b18a…` **MATCH**. Receipt-asset у Oracle нет.
- `carina-v0.2.0`: mismatch только у receipt (§2).

Носитель класса ровно один; расширять запись не потребовалось. Новых дефектов разбор не
выявил: различие CRLF/LF между репо-копией и asset объясняет разницу digest репо-файла и
asset, но не является дефектом (загружен LF-вариант того же текста).

## 9. Предлагаемый remediation — ТОЛЬКО ПЛАН для owner gate, без публикации

По умолчанию NO PUBLIC ACTION. Если владелец решит исправлять, форма, согласованная с его же
решением 2026-10-14 (274.11 §0: «опубликованный asset immutable, исправление публикуется под
новым отличимым именем»):

1. **Additive correction asset под уникальным именем**, например
   `bootstrap-repro-receipt-0.2.0-2026-10-13.txt` (НЕ `proof-and-accept-receipt.txt`): имя
   обязано отличаться, чтобы старый pin `9e3f509d…` в старом manifest остался осмысленным и
   ничто не притворялось исходным proof. Содержание: точный source SHA и цель тега; asset
   ids/sizes/digests всех входов; среда; команды; наблюдаемые выходы; негативная проба;
   явный статус «это reproduction от <дата>, не исходный acceptance proof 2026-10-11».
2. **Correction manifest под новым именем** (например `SHA256SUMS-0.2.0-correction-2026-…`),
   покрывающий новый asset; старый `SHA256SUMS` не трогать.
3. **Перед загрузкой — exact-SHA CI** на кандидате и независимый reviewer, как требует сам
   candidate («must not be replaced until independent candidate CI and explicit
   release-asset replacement verification are complete»).
4. Альтернатива или дополнение: **correction release 0.2.1** со свежей end-to-end распиской
   (это уже решение владельца о выпуске; здесь не предлагается к исполнению).
5. **Release body erratum** (body мутабелен без замены assets): датированная пометка, что
   receipt asset заменён 2026-10-13, не совпадает с опубликованным SHA256SUMS и не является
   доказательством исходного A→B→C; ссылка на этот отчёт. Тоже только по решению владельца.

Чего план НЕ включает: перезапись/удаление существующих assets, правку старого SHA256SUMS
«под текущие bytes» (это уничтожило бы последнюю публичную зацепку о digest исходного
receipt), любые заявления B≡C для Linux, любые действия по #92/#94 (HOLD).

## 10. Неопределённости и пределы этого разбора

- Wayback Machine не проверен (HTTP 429 оба обращения) — возможный след исходных bytes.
- История удаления asset 631179653 и авторство замены публично недоступны; выводы о
  хронологии — по asset ids и timestamps API.
- Документальные сверки с 274.11 §0 (stock B/C digest, A check 214/214) — сверки с текстом
  плана, не с первичными evidence-файлами: они локальны и недоступны (§4.2).
- Candidate-прогон (clang 22.1.5, double-build, smoke, negative-input) не переисполнялся —
  это вне рамок read-only разбора.
- Источник `57a46857…`, фигурирующий в #92/#94 и №1882, к provenance опубликованных assets
  0.2.0 отношения не имеет (ни один asset его не содержит) и здесь не исследовался.

## 11. Приёмочный чеклист задачи #98

- **criteria:** выполнены дословно — published bytes скачаны и сверены по asset ids/URL и
  SHA256SUMS/body (§2, §5), сравнение с документами и source SHA (§2, §6), независимая
  воспроизводимая проверка hash/size и негативная проба (§7), восстановимость 631179653 и
  предел (§4), отделение подтверждённого proof от claims (§6), план отличимого immutable
  remediation без публикации (§9).
- **fixture:** не применимо — правка только документальная; наблюдаемые утверждения отчёта
  (digest/size/строки manifest) воспроизводятся командами §2/§7 и дают конкретные значения.
- **sabotage:** §7, вывод дословно в обе стороны.
- **class:** §8 — проверены оба релиза; носитель один; новых записей в реестр не требуется,
  №1881 уже заведена задачей #93.
- **spec:** не применимо — язык не меняется, D-номер не брался.
- **std-check:** не применимо — правка не затрагивает компилятор и std.
- **invariant:** новых инвариантов/стражей не вводится.
- **gate / ci:** вердикт гейта — на кандидате integrate/t<N> стороны приёмщика; локально
  `scripts/gate.sh` и `scripts/gate-novac.sh` не запускались (слово владельца 2026-10-07).
- **commits:** коммит на английском, с `Signed-off-by`, без `Co-Authored-By`; add по имени,
  коммит с `--only --`.
- **report:** сдача — `crew_task submit` с формой по window-brief-template §6.
