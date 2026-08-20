---
name: feedback-rustc-as-reference
description: "УПРАВЛЯЮЩИЙ принцип — при архитектурных решениях по типам/резолву/mono/IR сверяться с rustc как ЭТАЛОНОМ; отклонения только как явно-помеченные компромиссы, не «наша норма»"
metadata: 
  node_type: memory
  type: feedback
  originSessionId: a48a9f3a-0403-4a44-a6e3-8894781d4b88
---

Владелец (2026-07-12): «смотреть на раст-реализацию как на эталон». При ЛЮБОМ архитектурном решении по
типам/резолву/mono/IR — сверяться с тем, КАК это сделано в rustc, прежде чем проектировать своё.

**Why:** rustc — зрелый корректный референс. Наш компилятор молод: AST — единственный IR (нет typed MIR),
поэтому codegen ПЕРЕвыводит типы/резолв («второе окно» — семейство ~20+ `infer_*`/`resolve_*` в emit_c.rs;
`resolved_types` = side-table-заплатка, симулирующая typed IR, оттого неполна). Изобретать своё вопреки rustc
= риск ещё одного «второго окна». Урок сессии: я назвал mono-машинерию «законно живёт в codegen» — владелец:
«как в раст?» — в rustc mono = ОТДЕЛЬНАЯ ФАЗА (collector), а не codegen-side-effect; мой дизайн был размыт.

**rustc-модель (эталон):** typeck (типы ОДИН раз → `TyCtxt`) → HIR → THIR → **MIR** (типизированный CFG) →
**mono-collector** (`rustc_monomorphize`, отдельная фаза → worklist `Instance`) → **symbol-mangling**
(отдельная фаза) → codegen (`rustc_codegen_ssa/llvm`) ЧИТАЕТ typed MIR + подставляет known substs, НИКОГДА
не инферит. LSP тоже читает `TyCtxt`.

**How to apply:** проектируешь фазу типов/резолва/mono → сначала «как в rustc?» → повторяй модель. Наши
отклонения (напр. Plan 196 Stage-1 без MIR: одно окно симулируется side-table `resolved_types`) — ЯВНО
помечать «сознательный компромисс Stage-1, не идеал», а не закреплять как норму. Полная rustc-модель (typed
MIR + mono-фаза) = Stage-2. Связано: [[feedback-maximize-nv-sourcing]] (maximize .nv-источник — тоже про
«не хардкодить своё»), Plan 196 (матрица «одного окна» + целевая архитектура + Rust-сверка).
