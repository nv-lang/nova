<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Инвентарь несведённых веток — сырые факты (реестр 221.1)

Составлено автоматическим перечислением 2026-08-08. Только факты: объём, возраст, сливаемость, следы в планах. Решение по каждой ветке (слить / доделать / удалить) принимает владелец — в этом файле его НЕТ намеренно.

| Ветка | Коммитов | Последний | Отведена от | Отстаёт | Файлов | Сливается | Worktree | В планах | Заголовок последнего коммита |
|---|---|---|---|---|---|---|---|---|---|
| p34-arity-overload | 301 | 2026-07-25 | 2026-07-22 | 2081 | 294 | нет: 68 конфликтующих файлов | нет | не упоминается | fix(checker,codegen [M-concrete-instance-arity-overload-mangle]): concrete overload dispatch survive |
| p88-structured-receiver | 298 | 2026-07-25 | 2026-07-22 | 2081 | 293 | нет: 68 конфликтующих файлов | нет | не упоминается | fix(checker, [M-structured-receiver-generic-not-enforced]): структурные generic-receivers — энфорс + |
| p95-iflet-enforce | 295 | 2026-07-25 | 2026-07-22 | 2081 | 288 | нет: 68 конфликтующих файлов | нет | не упоминается | fix(parser, [M-if-let-retraction-not-enforced]): парсер-энфорс ретракции if let/while let (реестр 22 |
| p99-entry-embed-order | 292 | 2026-07-25 | 2026-07-22 | 2081 | 284 | нет: 65 конфликтующих файлов | нет | не упоминается | fix(imports, [M-entry-value-embed-forward-decl-order]): entry-folder-module self-siblings merge alph |
| p228-fnnt-channel | 287 | 2026-07-25 | 2026-07-22 | 2081 | 282 | нет: 66 конфликтующих файлов | нет | не упоминается | fix(228, Ф.4, реестр 221.1 №97): fn-newtype receiver bare-@ C-type — split-brain value/type (два окн |
| p94-consolidation-196 | 279 | 2026-07-25 | 2026-07-22 | 2081 | 273 | нет: 66 конфликтующих файлов | нет | не упоминается | test(196-consolidation): регресс-пин №96 [M-closure-ctx-freefn-callee-unresolved-fnnt] — CC-FAIL, из |
| p92-samename-recv | 273 | 2026-07-25 | 2026-07-22 | 2081 | 272 | нет: 66 конфликтующих файлов | нет | не упоминается | fix(codegen, [M-samename-extension-method-recv-type-collision]): channel-first checker fix for №92 s |
| p90-method-return-fnnt | 269 | 2026-07-25 | 2026-07-22 | 2081 | 271 | нет: 65 конфликтующих файлов | нет | не упоминается | test(conformance, [M-nested-fn-newtype-bind-then-call-broken] №90): фикстура метод-возврата вложенно |
| p2141-fix-structured | 261 | 2026-07-24 | 2026-07-22 | 2081 | 270 | нет: 64 конфликтующих файлов | нет | не упоминается | fix(214.1, [M-coerce-generic-structured-receiver]): generic #coerce shape-gate reads receiver_ty, no |
| p214-1-generic-coerce | 256 | 2026-07-24 | 2026-07-22 | 2081 | 269 | нет: 64 конфликтующих файлов | нет | не упоминается | test(214.1): generic #coerce — pos/neg targeted фикстуры (spec_tests, standalone/neg) |
| p81-p67-singlefile | 250 | 2026-07-24 | 2026-07-22 | 2081 | 264 | нет: 64 конфликтующих файлов | нет | не упоминается | fix(std/net, [M-p81-unknown-static-receiver-silent-p67]): stress_test.nv missing import std.time.dur |
| p-mvinfer | 246 | 2026-07-24 | 2026-07-22 | 2081 | 262 | нет: 64 конфликтующих файлов | нет | не упоминается | fix(codegen, [M-method-value-arg-in-generic-combinator-infer]): infer method-level type-param from u |
| p80-into-split | 244 | 2026-07-24 | 2026-07-22 | 2081 | 257 | нет: 64 конфликтующих файлов | нет | не упоминается | refactor(net): rename TcpStream.split → into_split [M-tcp-split-consume-naked-name] |
| p-vela-naming | 240 | 2026-07-24 | 2026-07-22 | 2081 | 253 | нет: 60 конфликтующих файлов | нет | docs/plans/224-vela-runtime-naming.md; | Merge branch 'p-fmtdirect' (окно fmt): [M-fmt-rich-spec-primitive-fresh-sb-redundant] — девиртуализа |
| p-fmtdirect | 237 | 2026-07-24 | 2026-07-22 | 2081 | 253 | нет: 60 конфликтующих файлов | нет | docs/plans/backlog-followups.md; | docs(backlog-followups): close [M-fmt-rich-spec-primitive-fresh-sb-redundant] |
| p-lint-consume-into | 232 | 2026-07-24 | 2026-07-22 | 2081 | 252 | нет: 60 конфликтующих файлов | нет | не упоминается | doc(nv-coding-style): §1а — голое имя-вид запрещено на consume-receiver'е |
| p78-nested-fn-newtype | 228 | 2026-07-24 | 2026-07-22 | 2081 | 252 | нет: 60 конфликтующих файлов | нет | docs/plans/221.1-bug-sweep.md; | fix([M-nested-fn-newtype-bind-then-call-broken], реестр 221.1 №78): вложенный newtype-над-fn — bind- |
| p47-vec-fn-newtype | 224 | 2026-07-24 | 2026-07-22 | 2081 | 250 | нет: 58 конфликтующих файлов | нет | docs/plans/221.1-bug-sweep.md; | test(conformance, [M-vec-of-fn-newtype-codegen], реестр 221.1 №47): фикстура Vec[fn-newtype]/Vec[bar |
| p73-assoc-const-chain | 219 | 2026-07-24 | 2026-07-22 | 2081 | 249 | нет: 58 конфликтующих файлов | нет | не упоминается | docs(spec, [M-assoc-const-chained-method-call-p67], окно №73): implementation-note к D200 — цепной m |
| p-fullstack-value | 215 | 2026-07-24 | 2026-07-22 | 2081 | 248 | нет: 58 конфликтующих файлов | нет | docs/plans/226-ro-launder-l1-coercion.md;docs/plans/backlog-followups.md;docs/plans/221.1-bug-sweep.md; | docs(plans, №72): зафиксировать закрытие [M-ro-launder-fullstack-value-exemption] |
| p-oob-assoc-const | 212 | 2026-07-24 | 2026-07-22 | 2081 | 245 | нет: 59 конфликтующих файлов | нет | docs/plans/backlog-followups.md; | feat(parser, [M-assoc-const-out-of-body-syntax]): out-of-body `const Type.NAME` (D200 AMEND, окно №6 |
| p-consume-soundness | 208 | 2026-07-24 | 2026-07-22 | 2081 | 241 | нет: 58 конфликтующих файлов | нет | не упоминается | fix(consume-checker, D131/D133-класс): №55 [M-consume-fn-value-call-arg-not-tracked] закрыт — consum |
| p-openrange | 196 | 2026-07-24 | 2026-07-22 | 2081 | 235 | нет: 59 конфликтующих файлов | нет | не упоминается | fix([M-open-range-len-source-hardcoded]): убрать глобальную str-схему — регрессия str.from(5) (D410) |
| p-val-research | 180 | 2026-07-24 | 2026-07-22 | 2081 | 214 | нет: 58 конфликтующих файлов | нет | docs/plans/222-http-framework.md;docs/plans/221.1-bug-sweep.md; | probe(p-val-research): validate/serde/flatten/colocation research probes (lab, not for merge) |
| p-coalesce | 177 | 2026-07-24 | 2026-07-22 | 2081 | 231 | нет: 59 конфликтующих файлов | нет | docs/plans/backlog-followups.md; | docs(coalesce): close [M-coalesce-return-fallback-unparsed] and [M-manual-coalesce-lint-missing] |
| p-fix-n60-assoc-const | 171 | 2026-07-24 | 2026-07-22 | 2081 | 209 | нет: 58 конфликтующих файлов | нет | docs/plans/backlog-followups.md; | fix(types, [M-d200-assoc-const-composite-value]): D200 assoc-const codegen до составных (record-лите |
| p-fix-n53-fnret | 164 | 2026-07-23 | 2026-07-22 | 2081 | 206 | нет: 55 конфликтующих файлов | нет | docs/plans/backlog-followups.md; | fix(fntypes, [M-fn-newtype-return-position-broken], №53): resolve_fn_typeref теперь покрывает резуль |
| p-vela-f2 | 158 | 2026-07-24 | 2026-07-22 | 2081 | 200 | нет: 59 конфликтующих файлов | нет | не упоминается | docs(224 Ф.2, Vela): бренд-имя M:N-движка в шапках nova_rt scheduler-файлов |
| p225-blank | 157 | 2026-07-23 | 2026-07-22 | 2081 | 197 | нет: 52 конфликтующих файлов | нет | не упоминается | style(225): пустая строка между top-level декларациями — 28 вставок в 15 файлах (правило §1, только- |
| p224-vela | 157 | 2026-07-23 | 2026-07-22 | 2081 | 197 | нет: 55 конфликтующих файлов | нет | не упоминается | docs(224): Vela — имя M:N-рантайма в mn-conventions/debugging-races/runtime-tuning/channels(+ru) (Ф. |
| p-ro-launder | 144 | 2026-07-23 | 2026-07-22 | 2081 | 186 | нет: 46 конфликтующих файлов | нет | docs/plans/backlog-followups.md; | docs(224, [M-ro-launder-via-mut-binding]): финальный статус — норма закрыта для std/examples/spec_te |
| p-fix-n38-workertls | 119 | 2026-07-23 | 2026-07-22 | 2081 | 142 | нет: 36 конфликтующих файлов | C:/Users/Public/nova-n38 | docs/plans/221.1-bug-sweep.md; | docs(221.1 #38): РЕШЕНО — трасса-доказательство арм-пул-наследования deadline в w->scope; резолюция  |
| p-okno5-fntypes | 113 | 2026-07-23 | 2026-07-22 | 2081 | 140 | нет: 34 конфликтующих файлов | нет | docs/plans/222.3-extractors.md;docs/plans/backlog-followups.md; | feat(fntypes, [M-newtype-over-fn-type-unsupported][M-alias-of-fn-type-not-callable]): newtype/alias  |
| p-okno4 | 108 | 2026-07-23 | 2026-07-22 | 2081 | 137 | нет: 34 конфликтующих файлов | нет | docs/plans/backlog-followups.md;docs/plans/221.1-bug-sweep.md; | fix(codegen, [221.1 #37 guard]): honest E_METHOD_VALUE_STATIC_UNSUPPORTED instead of a silent broken |
| p-okno3-bugsweep | 86 | 2026-07-23 | 2026-07-22 | 2081 | 124 | нет: 32 конфликтующих файлов | нет | не упоминается | fix(test_runner, 221.1 №18): именованы + частично починены 3 незалипших cargo-test FAIL на main |
| p-okno2-derive-seed-223 | 81 | 2026-07-23 | 2026-07-22 | 2081 | 116 | нет: 30 конфликтующих файлов | нет | docs/plans/STATUS.md;docs/plans/223-src-transparency-entry-mode.md;docs/plans/221.1-bug-sweep.md; | docs(223): mark plan status IMPLEMENTED |
| p221-q4-notes-rev | 65 | 2026-07-23 | 2026-07-22 | 2081 | 82 | нет: 27 конфликтующих файлов | нет | не упоминается | docs(release-notes): A-Q4 ревизия под состояние 2026-07-23 — typed effects, serde-атрибуты, Router/e |
| p-fix-net2stream-imports | 51 | 2026-07-23 | 2026-07-22 | 2081 | 88 | нет: 27 конфликтующих файлов | нет | не упоминается | fix(emit_c, [M-generic-static-method-value-arg-addr-mismatch]): four parallel generic mono-instance  |
| p-fix-2227-blockers | 30 | 2026-07-23 | 2026-07-22 | 2081 | 66 | нет: 15 конфликтующих файлов | нет | docs/plans/221.1-bug-sweep.md; | fix(codegen, [M-nv-spawn-ctx-capture-mut-param-ptr-mismatch]+[M-http-props-mut-chain-argpos-value-pt |
| p222-flagship-router | 29 | 2026-07-23 | 2026-07-22 | 2081 | 63 | нет: 14 конфликтующих файлов | нет | не упоминается | migrate(222): флагман aggregator — ServeMux -> Router (nova-http server_router.nv, Plan 222.1) |
| p180-serde-field-attrs | 23 | 2026-07-22 | 2026-07-22 | 2081 | 58 | нет: 12 конфликтующих файлов | нет | docs/plans/180.1-serde-parity-and-beyond.md;docs/plans/222-http-framework.md;docs/plans/backlog-followups.md; | Merge branch 'main' into p180-serde-field-attrs |
| p-fix-bytes-coerce-gap | 19 | 2026-07-22 | 2026-07-22 | 2084 | 41 | нет: 12 конфликтующих файлов | нет | docs/plans/221.1-bug-sweep.md; | Merge branch 'main' into p-fix-bytes-coerce-gap |
| p424-d310-amendment | 7 | 2026-08-07 | 2026-08-07 | 180 | 13 | да | D:/Sources/nv-lang/nova-p424 | не упоминается | docs(p424): PROGRESS-p424.md — приёмка D310-амендмента (8/8 пунктов, нулевой греп, карта спеки) |
| p408-enforce-audit | 6 | 2026-08-07 | 2026-08-07 | 286 | 4 | да | D:/Sources/nv-lang/nova-p408 | не упоминается | triage(231.1): ревизия интегратора — вердикты A от haiku-батча ненадёжны (5 из 10 сомнительны, 2 про |
| p-stability | 4 | 2026-08-08 | 2026-08-07 | 175 | 1 | да | D:/Sources/nv-lang/nova-pstab | docs/plans/221.1-bug-sweep.md;docs/plans/250-vela-state-consolidation.md; | docs(p-stability): явно — шаг 3 (якорь §431) не брал, остаётся как у p431 (управляемый exit, не abor |
| p217-1-cleanup-rollout | 4 | 2026-07-22 | 2026-07-22 | 2092 | 10 | нет: 7 конфликтующих файлов | нет | docs/plans/221.1-bug-sweep.md; | docs(std,spec): Plan 217.1 — обоснование исключений из раскатки авто-@cleanup |
| bisect-cu-crash | 3 | 2026-07-30 | 2026-07-29 | 1523 | 3 | да | D:/Sources/nv-lang/nova-bisect | не упоминается | bisect: confirm bug alive after compiler update, document 2-file combos |
| pchan244 | 2 | 2026-08-03 | 2026-08-03 | 925 | 7 | да | нет | docs/plans/221.1-bug-sweep.md; | plan(244) Ф.1: Chan[T] reimplemented on Nova (ChanV2/ChanWriterV2/ChanReaderV2) |
| p-audit-enforcement | 2 | 2026-08-06 | 2026-08-06 | 346 | 18 | да | D:/Sources/nv-lang/nova-audit | docs/plans/wip/PROGRESS-p383-bounds.md;docs/plans/221.1-bug-sweep.md; | docs(p-audit-enforcement): пункт 2 (атрибуты типов) и пункт 3 (частично, consume/priv/pub_to) — #imp |
| p454-flagship-green | 2 | 2026-08-08 | 2026-08-08 | 45 | 3 | да | D:/Sources/nv-lang/nova-p454 | docs/plans/221.1-bug-sweep.md; | gate(№454): нова test флагман+regressions в gate.sh, --skip src/main |
| p238-paths | 2 | 2026-08-04 | 2026-08-04 | 582 | 13 | да | нет | docs/plans/wip/PROGRESS-p238-review.md;docs/plans/wip/p238-review-probes/pB_mutex_hashmap_return_escape/probe.nv;docs/plans/wip/p238-review-probes/pC_iflet_closure_bind/probe.nv; | docs(238, p238-paths): +путь №11 — тэйнтованное поле, переданное аргументом свободной функции |
| p238-form | 2 | 2026-08-05 | 2026-08-05 | 577 | 18 | да | нет | docs/plans/221.1-bug-sweep.md;docs/plans/238-fiber-memory-model.md; | p238-form: вердикты и рекомендация по форме записи требования безопасности на границе (Ф.6) |
| p235-bigint | 2 | 2026-07-29 | 2026-07-29 | 1530 | 3 | да | нет | не упоминается | chore(phase0): add operator desugar repro fixture (Ф.0) |
| p150-race-repro | 2 | 2026-07-30 | 2026-07-30 | 1517 | 5 | да | нет | docs/plans/wip/PROGRESS-p238-f3.md;docs/plans/238-fiber-memory-model.md; | docs(p150): заполнить хеш коммита в RACE150_FINDINGS.md |
| fix-high-conc-wedge | 2 | 2026-07-16 | 2026-07-15 | 2981 | 9 | нет: 4 конфликтующих файлов | нет | docs/plans/211-park-join-research.md; | 187: [M-187-high-concurrency-connection-wedge] park-join WIP — эскалация, НЕ мёржить |
| vrec-claims-repro | 1 | 2026-07-30 | 2026-07-30 | 1514 | 8 | да | нет | docs/plans/221.1-bug-sweep.md; | measure(vrec): repro трёх заявок codegen вокруг value-record |
| pguard290 | 1 | 2026-08-04 | 2026-08-03 | 956 | 3 | да | нет | не упоминается | guard(242/№290): устойчивый разбор ratchet-ключа доведён до конца — неоднозначность (дубль/коллизия) |
| p-fix-mn-crash | 1 | 2026-07-22 | 2026-07-22 | 2091 | 2 | нет: 1 конфликтующих файлов | нет | docs/plans/wip/mncrash-notes.md;docs/plans/backlog-followups.md; | docs([M-conformance-megacu-intermittent-run-crash]): маркер → ЗАКРЫТО — корень emit_detach by-ref mu |
| p-fix-lsp-lifecycle | 1 | 2026-07-22 | 2026-07-22 | 2092 | 2 | да | нет | docs/plans/backlog-followups.md; | fix(vscode-lsp-client): guard document-sync notifications against dead connection |
| pcdoors-recon | 1 | 2026-08-08 | 2026-08-08 | 52 | 1 | да | D:/Sources/nv-lang/nova-pcdoors | не упоминается | recon(contracts): двери в системе контрактов — invariant проверяется ТОЛЬКО на именованном RecordLit |
| p452-stack-overflow | 1 | 2026-08-08 | 2026-08-08 | 55 | 2 | да | D:/Sources/nv-lang/nova-p452 | не упоминается | диагноз(221.1 №452): fiber stack overflow slot 501 — не воспроизведён; A/B против пре-№446-фикса ука |
| p2gap2-static-generic-dispatch | 1 | 2026-07-26 | 2026-07-26 | 1654 | 2 | да | нет | не упоминается | wip(222.3 Гэп-2, opus, checkpoint): overload_applicability pruning частично помогает — компилируется |
| p248-recon | 1 | 2026-08-05 | 2026-08-05 | 578 | 1 | да | нет | docs/plans/221.1-bug-sweep.md;docs/plans/248-shared-handles-linearity.md; | docs(p248-recon): Ф.1 разведка — заражение линейностью 6 типов (5 после устранимого переноса), проба |
| p248-nocopy | 1 | 2026-08-05 | 2026-08-05 | 567 | 8 | да | нет | docs/plans/wip/PROGRESS-p248-nocopy2.md;docs/plans/wip/PROGRESS-p248-sharedcell.md;docs/plans/wip/PROGRESS-p248-mech.md; | docs(p248-nocopy): разведка форм записи «нельзя копировать» — пробы компилятора |
| p248-copyflip | 1 | 2026-08-05 | 2026-08-05 | 547 | 1 | да | нет | docs/plans/221.1-bug-sweep.md;docs/plans/248-shared-handles-linearity.md; | docs(p248-copyflip): исследование поворота умолчания копирования — вердикт против унификации |
| p2223-extractors | 1 | 2026-07-31 | 2026-07-31 | 1364 | 2 | нет: 1 конфликтующих файлов | нет | не упоминается | docs(222.3 Ф.0, чекпоинт): №140 реконфирмирован — механизм и связь с №129/№170, фикс не сделан |
| p196-hardcode-detector | 1 | 2026-07-22 | 2026-07-22 | 2092 | 1 | да | нет | не упоминается | tooling(196): scripts/hardcode-audit.sh — детектор хардкода 7 категорий + tripwire-режим |
| p196-gs-spike | 1 | 2026-07-30 | 2026-07-30 | 1502 | 2 | да | нет | docs/plans/wip/196-gs-spike.md;docs/plans/196-one-truth-closeout.md; | spike(196): GS_SPIKE.md — разведка миграции gs под баунды |
| modconst-repro | 1 | 2026-07-30 | 2026-07-29 | 1523 | 6 | да | нет | docs/plans/221.1-bug-sweep.md; | test(modconst): репро кросс-модульного CC-FAIL при одноимённых export const разных типов |
| fix-privfile-fn-scope | 1 | 2026-07-14 | 2026-07-14 | 3179 | 1 | да | нет | docs/plans/wip/196-facetB-privfile-notes.md;docs/plans/221.1-bug-sweep.md; | docs(196-B): priv(file) free-fn bleed — диагноз до 3-го сайта (generic-mono dispatch), СТОП по прото |
| aq3-isolate | 1 | 2026-07-29 | 2026-07-29 | 1530 | 2 | да | нет | не упоминается | test(aq3): isolation validation for a_q3_println_debug_record fallback |

## Сводка

- всего локальных веток: 118
- уже влитых в main (0 коммитов): 46
- с несведённой работой: 71
- из них сливаются без конфликта: 25
- из них упоминаются в планах: 36
- суммарно несведённых коммитов: 7850

Примечания к методике:
- «Отведена от» — дата коммита, на который указывает `git merge-base main <ветка>` (без `--all`); у части веток (например `p34-arity-overload`) история криссчросс и существует несколько merge-base — взята одна точка, как предписано методикой шага 2.
- «Сливается» проверено `git merge-tree --write-tree main <ветка>` (git version 2.55.0.windows.3, команда поддерживается) — только чтение, рабочее дерево не тронуто.
- «В планах» — `grep -rl "<имя-ветки>" docs/plans/`, до трёх файлов (без учёта самого файла инвентаря).
