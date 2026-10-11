<!-- SPDX-License-Identifier: CC-BY-4.0 -->
# Nova Carina 0.2.0

Release announcement draft (English/Russian; alpha pre-release).

Metadata for a future release: title `Nova Carina 0.2.0 (alpha)`, tag
`carina-v0.2.0`, pre-release. Planned asset names only:
`nova-carina-0.2.0-linux-x86_64.tar.gz`,
`nova-carina-0.2.0-windows-x86_64.zip`,
`nova-carina-0.2.0-src.tar.gz`, and `SHA256SUMS`.
These are plans, not existing or current CI artifacts.

## EN (GitHub Release / nv-lang.org)

**Nova Carina 0.2.0 (alpha)**

Carina is the Nova compiler written in Nova. This alpha marks an accepted
self-compilation milestone: at source SHA
`99756a8867f913bd6156ed10accf8a20edea00d1`, the recorded A → B → C bootstrap
produced byte-identical B and C executables and byte-identical B.c and C.c;
CI was green on that same SHA. Stage A comes from the Oracle release.

This demonstrates the accepted 0.2 self-compilation step, not readiness of the
wider package ecosystem or examples: compiling those is a 0.3 criterion.

This is an alpha: compiler behavior and interfaces may change, and existing
programs or packages may break; compatibility and release-asset availability
are not guaranteed.

See the [Carina 0.2.0 release notes](carina-release-notes-v0.2.0.md) for scope
and limitations. The separately maintained [Oracle 0.1.0 announcement
draft](oracle-release-announcement-v0.1.0.md) covers the preceding Oracle
release; Oracle is planned to be announced first.

## RU (nv-lang.ru)

**Nova Carina 0.2.0 (alpha)**

Carina — компилятор Nova, написанный на Nova. Эта альфа отмечает принятый
этап самокомпиляции: на исходном SHA
`99756a8867f913bd6156ed10accf8a20edea00d1` зафиксирован bootstrap A → B → C,
в котором исполняемые файлы B и C побайтно совпали, как и B.c с C.c; CI был
зелёным на том же SHA. Стадия A берётся из релиза Oracle.

Это подтверждает принятый этап самокомпиляции 0.2, но не готовность широкой
экосистемы пакетов и примеров: их компиляция — критерий версии 0.3.

Это альфа: поведение компилятора и интерфейсы могут измениться, существующие
программы и пакеты могут перестать работать; совместимость и наличие артефактов
релиза не гарантируются.

Область релиза и ограничения описаны в [заметках о выпуске Carina
0.2.0](carina-release-notes-v0.2.0.md). Отдельный [черновик анонса Oracle
0.1.0](oracle-release-announcement-v0.1.0.md) относится к предыдущему релизу
Oracle; сначала планируется анонс Oracle.
