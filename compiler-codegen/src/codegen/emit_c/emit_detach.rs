//! №240 [M-detach-box-while-loop-read-after]: `detach { body }` codegen,
//! split out of `emit_c.rs` (git mv equivalent — moved wholesale, only the
//! box-creation branch changed) so the box-hoist fix below could be added
//! without growing the ratcheted file (`scripts/guards/arch-ratchet.sh`
//! only measures `emit_c.rs` itself). Child module of `codegen::emit_c` —
//! sees `CEmitter`'s private fields/methods per Rust's ancestor-module
//! privacy rule (a child module sees its ancestors' private items).

use super::CEmitter;
use crate::ast::{Block, ExprKind, Stmt};
use std::collections::HashSet;
use std::fmt::Write as FmtWrite;

impl CEmitter {
    /// №379 fix (D415 §4): `spawn consume a[, b, …] { body }` / `detach
    /// consume a[, b, …] { body }` desugar to nested `Stmt::ConsumeScope`
    /// layers (`re_consume: false`, "already-bound" form — `parse_spawn` /
    /// `parse_detach` / `parse_spawn_detach_consume_multivar`), each layer
    /// transferring ownership of an OUTER binding into the child fiber. If
    /// that outer binding was ALSO a bare auto-cleanup-eligible `consume x
    /// = e;` (registered in `auto_cleanup_active` by `enter_defer_scope`),
    /// the outer entry has no idea the fiber now owns cleanup and stays
    /// armed — `emit_spawn`/`emit_detach` then swap `self.out` to the
    /// child fiber's OWN C function before emitting `body`, so the generic
    /// `disarm_auto_cleanup_receiver_call` (consume-param call-arg /
    /// consuming-receiver-call disarm, `emit_c.rs`) can find the stale
    /// outer entry from INSIDE the fiber body and emit a disarm assignment
    /// targeting a C local declared in the PARENT function — "use of
    /// undeclared identifier" (multi-var repro:
    /// `spawn_detach_consume_multivar_ok.nv` two-binding form; the same
    /// class is latent, untriggered, for the single-var form too — no
    /// existing fixture called a consume-param helper on the captured
    /// binding from inside a single-var fiber body). Leaving the entry
    /// armed is ALSO a double-cleanup risk symmetric to the `if
    /// *re_consume` outer-disarm branch in `Stmt::ConsumeScope` codegen
    /// (`emit_c.rs`, folder-CU `d188_reconsume_block.nv` regression) — the
    /// outer scope's own exit would fire `@cleanup` a second time. Fix:
    /// walk the desugar's nested-ConsumeScope spine BEFORE the `self.out`
    /// swap (still in the PARENT function), disarming + dropping each
    /// already-bound outer entry exactly once — mirrors that branch's own
    /// outer-disarm but unconditional on `re_consume` (both forms move
    /// ownership out permanently). Called from `emit_spawn`/`emit_detach`.
    pub(super) fn disarm_outer_auto_cleanup_for_fiber_body(&mut self, block: &Block) {
        let mut cur = block;
        loop {
            let (binding, init, re_consume, inner) = match cur.stmts.first() {
                Some(Stmt::ConsumeScope { binding, init, re_consume, body, .. }) => {
                    (binding, init, *re_consume, body)
                }
                _ => break,
            };
            if re_consume { break; }
            if !matches!(&init.kind, ExprKind::Ident(n) if n == binding) { break; }
            if let Some(var) = self.disarm_var_for(binding) {
                self.line(&format!(
                    "{} = 0;  /* №379: spawn/detach-consume — outer auto-cleanup дизармлен перед файбером */",
                    var));
            }
            // План 253.4 Ф.1: реестра `auto_cleanup_active` больше нет —
            // владение помечается ушедшим прямо в записи, которая владеет
            // флагом и его `defer`'ом (см. `drop_disarm_binding`).
            self.drop_disarm_binding(binding);
            let single_stmt_no_trailing = cur.stmts.len() == 1 && cur.trailing.is_none();
            if !single_stmt_no_trailing { break; }
            cur = inner;
        }
    }

    /// Emit `detach { body }` — D50 fire-and-forget primitive.
    /// Bootstrap default handler is SyncDetach: body executes inline in the caller's
    /// stack, no fiber, no scheduler. Production runtime would route to a global
    /// supervisor on a separate OS thread (with LogAndDrop default panic policy).
    /// Plan 83.4.5.2 Ф.2 (2026-05-23): `detach { body }` — production-grade
    /// fire-and-forget на orphan fiber (D50 AsyncDetach default).
    ///
    /// Архитектура (паритет Go `go fn()` / tokio::spawn без JoinHandle):
    /// 1. Capture-анализ (как у emit_spawn): immutable captures by-value;
    ///    mutable captures — HEAP-BOX (НЕ `&stack_local` как в emit_spawn:
    ///    орфан переживает кадр вызывающего —
    ///    [M-conformance-megacu-intermittent-run-crash], см. capture-setup
    ///    ниже).
    /// 2. Ctx-struct с NovaSpawnCtxBase prefix + capture fields →
    ///    `lambda_forward_decls` (file scope).
    /// 3. Entry function `_nova_detach_N(mco_coro*)` — body wrapped в
    ///    LogAndDrop fail-frame (errors logged to stderr, не propagate'ятся).
    /// 4. На call site: heap-alloc ctx (GC-tracked), set captures, set
    ///    _nova_parent_scope=NULL (orphan!), captures init_snapshot для
    ///    handler inheritance, вызов `nova_runtime_spawn_orphan(entry, ctx)`.
    /// 5. Возвращается NOVA_UNIT мгновенно — caller не ждёт.
    ///
    /// Runtime routing (см. runtime.c::nova_runtime_spawn_orphan):
    ///   - armed → push в worker deque (parent_scope=NULL → LogAndDrop в
    ///     fiber's fail-handler без scope.report_error).
    ///   - bootstrap → cooperative spawn в global `_nova_orphan_scope`,
    ///     drained on atexit либо explicit `runtime.drain_orphans()`.
    ///
    /// Cross-runtime parity:
    ///   - Go `go fn()` — runtime.newproc, fiber goes to scheduler runq,
    ///     orphan'е errors → goroutine panic propagate process-wide
    ///     (Nova: LogAndDrop вместо panic, D50 spec).
    ///   - tokio `tokio::spawn(future)` без JoinHandle — multi-thread
    ///     executor, error в task — implicit drop.
    ///   - Kotlin `GlobalScope.launch { … }` — truly detached coroutine.
    ///   - Node `setImmediate(cb)` — single-thread event-loop queue.
    pub(super) fn emit_detach(&mut self, body: &Block) -> Result<String, String> {
        let detach_id = format!("_nova_detach_{}", self.detach_counter);
        self.detach_counter += 1;

        // ── Capture-анализ (по образцу emit_spawn) ──
        // [M-parfor-loopvar-nonscalar-byref-capture] (2026-07-13): by-value gate
        // widened to ANY immutable capture (was scalar-only) — see emit_spawn's
        // capture-analysis comment for the full rationale (loop-variable aliasing
        // of non-scalar types, e.g. `str`, was captured by-reference and observed
        // stale/duplicate values).
        let mut refs: Vec<String> = Vec::new();
        Self::collect_idents_block(body, &mut refs);
        refs.sort();
        refs.dedup();
        let mut bound: HashSet<String> = HashSet::new();
        Self::collect_bound_names_block(body, &mut bound);

        // [M-parfor-capture-callee-name-collides-std-local] -- THE SAME SKIP
        // `emit_spawn` HAS, MISSING HERE UNTIL 2026-08-23 (registry #534).
        // A name the checker resolved to a real module-fn/method callee is not
        // a variable, so `var_types` must not be consulted for it: that map is
        // flat and never scoped per function, so a same-named LOCAL left by an
        // unrelated function elsewhere in the CU turned the CALL into a ctx
        // capture field and the emitted C referenced a name it never declared.
        // This file was written from `emit_spawn` and carries its other two
        // capture markers verbatim; this one did not come along.
        let mut resolved_fn_call_names: HashSet<String> = HashSet::new();
        self.collect_resolved_call_target_names_block(body, &mut resolved_fn_call_names);

        let mut captures: Vec<(String, String, bool)> = Vec::new();
        for name in refs {
            if bound.contains(&name) { continue; }
            if std::env::var("NOVA_KILL_DETACH_CALLEE_SKIP").as_deref() != Ok("1")
                && resolved_fn_call_names.contains(&name)
            {
                continue;
            }
            // [M-spawn-module-const-capture]: module-level const — resolves to
            // its mangled file-scope global, never a capture (see emit_spawn).
            if self.private_const_c_names
                .contains_key(&(body.span.file_id, name.clone()))
            {
                continue;
            }
            if let Some(ty) = self.var_types.get(&name).cloned() {
                // Plan 248 (wave 3, third mega-CU regression,
                // [M-detach-capture-mut-param-not-in-var-mutable]):
                // `var_mutable` tracks only `let mut` LOCALS, never a `mut x
                // T` PARAMETER of the enclosing fn (see `mut_param_names`'s
                // own doc — deliberately a separate set, not folded into
                // `var_mutable`). A captured `mut` PARAM must count as a
                // real mutating capture (`by_value = false`, boxed/pointer-
                // forwarded below) exactly like a captured `mut` local —
                // found via `detach_mut_capture_outlives_frame.nv`'s
                // `dmc_kick(mut n AtomicInt)`: without this, `n` was
                // misclassified as a read-only capture and the orphan
                // mutated a throwaway BY-VALUE ctx-struct snapshot, never
                // touching the caller's real `n`.
                let is_mut = self.var_mutable.contains(&name)
                    || self.mut_param_names.contains(&name);
                let by_value = !is_mut;
                captures.push((name, ty, by_value));
            }
        }

        let ctx_ty  = format!("NovaDetachCtx_{}", &detach_id[1..]);
        let ctx_var = format!("{}_ctx", detach_id);

        // ── Ctx-struct typedef + entry forward-decl → lambda_forward_decls ──
        // NovaSpawnCtxBase prefix (6 fields) — required so worker loop в
        // runtime.c корректно cast'ает user_data к NovaSpawnCtxBase* и
        // обрабатывает handler-snapshot adopt (Plan 83.4.5.4).
        let _ = writeln!(self.lambda_forward_decls, "typedef struct {{");
        let _ = writeln!(self.lambda_forward_decls,
            "    NovaFiberQueue* _nova_parent_scope;");
        // Plan 173.0 Ф.2 (A2.2): must mirror NovaSpawnCtxBase field order
        // exactly (see emit_spawn's identical field for the full rationale).
        // Detach/orphan fibers never participate in Ф.2 retention (LogAndDrop
        // has no supervising parent to retain errors for) — stays -1 always,
        // but the field must exist for common-initial-sequence layout parity.
        let _ = writeln!(self.lambda_forward_decls,
            "    int _nova_parent_slot;");
        let _ = writeln!(self.lambda_forward_decls,
            "    int _nova_worker_slot;");
        let _ = writeln!(self.lambda_forward_decls,
            "    NovaFailFrame* _nova_saved_fail_top;");
        let _ = writeln!(self.lambda_forward_decls,
            "    NovaInterruptFrame* _nova_saved_interrupt_top;");
        let _ = writeln!(self.lambda_forward_decls,
            "    NovaFiberQueue* _nova_fiber_scope;");
        let _ = writeln!(self.lambda_forward_decls,
            "    NovaEffectSnapshot* _nova_init_snapshot;");
        // Plan 83.4.5.7 (2026-05-23): atomic fiber state machine — see
        // NovaSpawnCtxBase в fibers.h. MUST match layout exactly.
        let _ = writeln!(self.lambda_forward_decls,
            "    nova_atomic_int _nova_fiber_state;");
        // Plan 83.6 (2026-05-24): allocation size — used by free path.
        let _ = writeln!(self.lambda_forward_decls,
            "    size_t _nova_pool_size;");
        // Plan 110.2.1.a (D188 R3) [M-110.x-cleanup-shield-deadline-underflow]
        // supervised(cancel:) fix (2026-06-05): cancel-shield mask + deadline
        // fields — same as NovaSpawnCtx layout. Без них runtime reads past
        // struct → garbage mask > 0 triggers bogus watchdog-варн (было:
        // bogus CleanupTimeoutError — D192-ретракт, Plan 173 Ф.5 п.2).
        let _ = writeln!(self.lambda_forward_decls,
            "    nova_atomic_int _nova_cancel_mask_count;");
        let _ = writeln!(self.lambda_forward_decls,
            "    int64_t _nova_cancel_deadline_ns;");
        // Plan 83-go-cmn Ф.2: gopark/goready 4-state park-latch — mirrors
        // NovaSpawnCtxBase._nova_park_state (fibers.h). MUST be BEFORE schedlink
        // (which stays second-to-last, ahead of the #431 fiber-anchor). Same
        // FATAL as emit_spawn if omitted.
        let _ = writeln!(self.lambda_forward_decls,
            "    nova_atomic_int _nova_park_state;");
        // Plan 83-go-cmn Ф.1: intrusive overflow link — LAST base field,
        // mirrors NovaSpawnCtxBase.schedlink (fibers.h). See emit_spawn note.
        let _ = writeln!(self.lambda_forward_decls,
            "    mco_coro* schedlink;");
        // [221.1 №431 остаток] Fiber-exit anchor — LAST base field. Same
        // FATAL-if-omitted property as schedlink; see emit_spawn's helper.
        self.emit_spawn_ctx_anchor_field();
        for (cap, ty, by_value) in &captures {
            if *by_value {
                let _ = writeln!(self.lambda_forward_decls, "    {} {};", ty, cap);
            } else {
                let _ = writeln!(self.lambda_forward_decls, "    {}* {};", ty, cap);
            }
        }
        let _ = writeln!(self.lambda_forward_decls, "}} {};", ctx_ty);
        let _ = writeln!(self.lambda_forward_decls,
            "{}void {}(mco_coro* _co);", self.top_level_storage(), detach_id);

        // №379 fix (mirrors emit_spawn's identical fix): disarm any outer
        // bare-auto-cleanup entries the `detach consume a[, b, …] { … }`
        // desugar's nested ConsumeScope spine takes ownership of — MUST run
        // in THIS (parent) function, before the `self.out` swap below moves
        // emission into the orphan fiber function (see
        // `disarm_outer_auto_cleanup_for_fiber_body`).
        self.disarm_outer_auto_cleanup_for_fiber_body(body);

        // ── Entry function body в deferred_impls ──
        let saved_out = std::mem::take(&mut self.out);
        let saved_indent = self.indent;
        self.indent = 0;
        // Plan 175 Ф.2-v2 ([M-spawn-var-boxed-leak] class, mirrors emit_spawn's
        // identical fix): isolate `var_boxed` for this detach-body's own
        // scope — captures here are resolved via `current_spawn_captures`
        // (`_c->name`), not `var_boxed`; a stale outer entry would shadow it.
        let saved_var_boxed_detach = std::mem::take(&mut self.var_boxed);

        self.line(&format!("{}void {}(mco_coro* _co) {{", self.top_level_storage(), detach_id));
        self.indent += 1;
        self.line(&format!("{ctx}* _c = ({ctx}*)mco_get_user_data(_co);", ctx = ctx_ty));
        // [221.1 №431 остаток] Arm this fiber's exit anchor as the entry's
        // first act — same protocol and rationale as emit_spawn's identical
        // call (a late cancel that finds no fail-frame retires THIS orphan
        // fiber instead of ending the process).
        self.emit_fiber_anchor_arm();

        // Worker preamble (M:N path): alloc home scope slot + adopt init_snapshot.
        // Single-thread path: parent_scope == NULL → skip; orphan fiber's
        // home scope (для cooperative) — global _nova_orphan_scope, который
        // nova_fiber_spawn_into добавил в подложку queue.
        self.line("if (_c->_nova_parent_scope) {");
        self.indent += 1;
        self.line("_nova_active_slot = nova_scope_alloc_slot(_nova_active_scope, _co);");
        self.line("_c->_nova_worker_slot = _nova_active_slot;");
        self.line("_c->_nova_fiber_scope = _nova_active_scope;");
        self.line("if (_c->_nova_init_snapshot && _nova_active_slot >= 0) {");
        self.indent += 1;
        self.line("_nova_active_scope->fiber_effect_snapshot[_nova_active_slot] = _c->_nova_init_snapshot;");
        self.line("_c->_nova_init_snapshot = NULL;");
        self.indent -= 1;
        self.line("}");
        self.indent -= 1;
        self.line("} else {");
        self.indent += 1;
        self.line("_c->_nova_worker_slot = -1;");
        self.indent -= 1;
        self.line("}");

        // Capture rewriting context — ExprKind::Ident → `(*_c->name)` / `_c->name`.
        let mut cap_set: HashSet<String> = HashSet::new();
        let mut cap_by_value: HashSet<String> = HashSet::new();
        for (cap, _, by_value) in &captures {
            cap_set.insert(cap.clone());
            if *by_value { cap_by_value.insert(cap.clone()); }
        }
        let prev_caps = std::mem::replace(&mut self.current_spawn_captures, Some(cap_set));
        let prev_by_value = std::mem::replace(
            &mut self.current_spawn_capture_by_value, Some(cap_by_value));

        // LogAndDrop fail-frame: D50 detach errors → log to stderr, не propagate.
        // (vs emit_spawn где error report'ится в parent scope для scope-wide
        // first_error+kind propagation — orphan не имеет parent scope.)
        self.line("NovaFailFrame _ff;");
        self.line("nova_fail_push(&_ff);");
        self.line("if (setjmp(_ff.jmp) == 0) {");
        self.indent += 1;

        let block_id = self.enter_defer_scope(body, false);
        for stmt in &body.stmts {
            self.emit_stmt(stmt)?;
        }
        if let Some(trailing) = &body.trailing {
            let v = self.emit_expr(trailing)?;
            self.line(&format!("(void)({});", v));
        }
        self.leave_defer_scope(block_id);

        self.line("nova_fail_pop();");
        self.indent -= 1;
        self.line("} else {");
        self.indent += 1;
        self.line("nova_fail_pop();");
        // LogAndDrop: print error to stderr (per D50), fiber exits cleanly.
        // Не вызываем nova_fiber_report_error — orphan нет parent scope для
        // propagation; errors не должны abort'ить процесс (другие orphans
        // / main flow продолжают).
        self.line("fprintf(stderr, \"nova: detach orphan fiber error (LogAndDrop): %.*s\\n\", (int)_ff.error_msg.len, _ff.error_msg.ptr ? _ff.error_msg.ptr : \"<no message>\");");
        self.indent -= 1;
        self.line("}");
        // [221.1 №431 остаток] Close the anchor region + disarm before the
        // shared epilogue — identical protocol to emit_spawn.
        self.emit_fiber_anchor_close();

        // Plan 83.4.5.8 (2026-05-24): orphan epilogue — dec pending_remote
        // + signal_main + free uncollectable slot (если был). Под armed
        // detach'ы tracked через _nova_orphan_scope.pending_remote чтобы
        // runtime.drain_orphans() мог wait их завершения (как
        // supervised_run_impl wait wait для children). Под bootstrap
        // parent_scope = NULL → этот блок пропускается; cooperative
        // queue (orphan_scope.fibers[]) drain'ится через
        // nova_supervised_drain_main_scope.
        self.line("if (_c->_nova_parent_scope) {");
        self.indent += 1;
        self.line("if (_c->_nova_worker_slot >= 0) {");
        self.indent += 1;
        self.line("nova_scope_free_slot(_nova_active_scope, _c->_nova_worker_slot);");
        self.line("_nova_active_slot = -1;");
        self.indent -= 1;
        self.line("}");
        // [196.6 / D466 §6 class]: pending_sweeps++ strictly BEFORE the
        // pending_remote release-decrement (same thread, program order) —
        // the scope owner that observes pending_remote==0 therefore also
        // observes pending_sweeps>0 until the worker's post-mortem sweep
        // (nova_scope_sweep_dead_child) release-decrements it. Closes the
        // stack-scope use-after-return in the sweep (Plan 198 floating AV;
        // docs/plans/196.6-race-state-dump-notes.md).
        self.line("(void)nova_aint_inc(&_c->_nova_parent_scope->pending_sweeps);");
        self.line("(void)nova_aint_fetch_sub_release(&_c->_nova_parent_scope->pending_remote);");
        self.line("nova_runtime_signal_main();");
        self.indent -= 1;
        self.line("}");

        self.indent -= 1;
        self.line("}");

        self.current_spawn_captures = prev_caps;
        self.current_spawn_capture_by_value = prev_by_value;

        // Move entry-fn body to deferred_impls, restore main out.
        let entry_fn_text = std::mem::take(&mut self.out);
        self.indent = saved_indent;
        self.out = saved_out;
        self.var_boxed = saved_var_boxed_detach;
        self.deferred_impls.push_str(&entry_fn_text);

        // ── Call site: heap-alloc ctx, fill captures, spawn_orphan ──
        // ctx должен пережить caller — heap (GC-tracked). Plan 82 fiber arena
        // независим от user-managed heap.
        //
        // Plan 83.4.5.8 (2026-05-24): conditional allocation. Под armed M:N
        // используем nova_alloc_uncollectable (см. emit_spawn для rationale —
        // worker-side reading ctx fields через mco_get_user_data видит zeros
        // если ctx становится GC-unreachable между main's write и worker's
        // read). Под bootstrap (`_armed == false`) — regular nova_alloc;
        // orphan scope's q->fiber_ctx[] держит ctx reachable.
        self.line(&format!(
            "nova_bool _nova_is_init_detach_{ctr} = nova_runtime_is_initialized();",
            ctr = self.detach_counter - 1));
        // Plan 83.6 (2026-05-24): per-worker SpawnCtx pool — см. emit_spawn.
        self.line(&format!(
            "{ctx_ty}* {ctx_var} = ({ctx_ty}*)(_nova_is_init_detach_{ctr} ? nova_spawn_pool_acquire(sizeof({ctx_ty})) : nova_alloc(sizeof({ctx_ty})));",
            ctr = self.detach_counter - 1));
        // Plan 83.4.5.8 (2026-05-24): под armed M:N orphan tracked через
        // _nova_orphan_scope.pending_remote — drain_orphans ждёт worker-pool
        // orphan'ов. Под bootstrap: parent_scope = NULL (orphan_scope handles
        // через nova_fiber_spawn_into → q->fibers[] queue). Init orphan scope
        // явно чтобы получить valid pointer.
        self.line(&format!(
            "if (_nova_is_init_detach_{ctr}) {{", ctr = self.detach_counter - 1));
        self.indent += 1;
        self.line("nova_runtime_orphan_scope_init();");
        self.line(&format!("{ctx_var}->_nova_parent_scope = nova_runtime_orphan_scope();"));
        /* Inc pending_remote BEFORE spawn (consistency w/ emit_spawn). */
        self.line(&format!("nova_aint_inc(&{ctx_var}->_nova_parent_scope->pending_remote);"));
        self.indent -= 1;
        self.line("} else {");
        self.indent += 1;
        self.line(&format!("{ctx_var}->_nova_parent_scope = NULL;"));  // orphan!
        self.indent -= 1;
        self.line("}");
        // Plan 173.0 Ф.2 (A2.2): detach/orphan fibers never allocate a
        // retention slot (no supervising parent — LogAndDrop) — always -1.
        self.line(&format!("{ctx_var}->_nova_parent_slot = -1;"));
        self.line(&format!("{ctx_var}->_nova_worker_slot = -1;"));
        self.line(&format!("{ctx_var}->_nova_saved_fail_top = NULL;"));
        self.line(&format!("{ctx_var}->_nova_saved_interrupt_top = NULL;"));
        self.line(&format!("{ctx_var}->_nova_fiber_scope = NULL;"));

        // Plan 83.4.5.4: capture parent TLS snapshot for handler inheritance.
        // Под bootstrap nova_fiber_spawn_into внутри spawn_orphan тоже save'ит
        // snapshot — но codegen-init его ноль'ит, чтобы избежать double-save.
        // Plan 83.4.5.8: snapshot тоже uncollectable под armed.
        // Plan 83.4.5.8: snapshot — collectable, reachable через ctx
        // (uncollectable, scanned) и scope's fiber_effect_snapshot[].
        self.line(&format!(
            "if (_nova_is_init_detach_{ctr}) {{", ctr = self.detach_counter - 1));
        self.indent += 1;
        self.line(&format!("{ctx_var}->_nova_init_snapshot = (NovaEffectSnapshot*)nova_alloc(sizeof(NovaEffectSnapshot));"));
        self.line(&format!("nova_effect_snapshot_save({ctx_var}->_nova_init_snapshot);"));
        self.indent -= 1;
        self.line("} else {");
        self.indent += 1;
        self.line(&format!("{ctx_var}->_nova_init_snapshot = NULL;"));
        self.indent -= 1;
        self.line("}");

        // Capture setup (как у emit_spawn — handles nested-capture rewriting).
        //
        // [M-conformance-megacu-intermittent-run-crash] (2026-07-22): mutable
        // captures are HEAP-BOXED here, NOT taken by `&stack_local` as
        // emit_spawn does. emit_spawn's by-ref capture is sound because a
        // supervised parent JOINS its children before the enclosing frame
        // pops; a detach orphan is fire-and-forget and ROUTINELY outlives the
        // caller frame — `ctx->cap = &local` was a use-after-return that hit
        // as a stochastic AV (frame[1] = `_nova_detach_1` →
        // `Nova_AtomicInt_method_fetch_sub_int` on a garbage handle read from
        // the dead frame) whenever the orphan's worker pickup was delayed past
        // the caller's return under CPU contention (~15%/run at 4-way load on
        // the conformance mega-CU; the «~1 of 8 gate runs» silent mid-run
        // death of `a_q3_println_debug_record`). Fix mirrors the established
        // escaping-handler idiom (emit_effect_handler_literal case (a)):
        // lazily heap-promote the var into a GC box, register it in
        // `var_boxed` so textually-later reads/writes in the enclosing fn
        // transparently deref the box (keeps the D50 §3.1 canonical pattern
        // `mut x = 0; detach { x = 42 }; runtime.drain_orphans();
        // assert(x == 42)` working), and store the BOX pointer in the ctx —
        // the ctx field type (`T*`) and the orphan body's `(*_c->cap)` access
        // are unchanged; only the pointee moves stack → GC heap. The box is
        // collectable (`nova_alloc`) and stays reachable through the scanned
        // ctx for the orphan's whole life. D415 §2 already restricts mut
        // captures across a detach boundary to `#share` types
        // (AtomicInt/Mutex/#share records), for which a boxed handle copy
        // preserves shared-object mutation exactly.
        //
        // №240 [M-detach-box-while-loop-read-after] (2026-08-05): the box
        // pointer's DECLARATION used to be emitted right here too, via a
        // plain `self.line(...)` at the CURRENT C position. That is exactly
        // the call site of `detach{}` — if it sits inside a `while`/`if`/
        // match-arm C block, the declared pointer is block-scoped: the
        // NAME disappears once that block's `}` closes, even though the
        // heap cell it points to is still alive (`nova_alloc`, GC-scanned
        // via the ctx). `var_boxed` (Rust-side, survives past the C block)
        // kept rewriting every LATER read of the captured name to that now-
        // invisible identifier → `use of undeclared identifier
        // '_nova_detach_N_box_<cap>'` for any read after the enclosing loop
        // (minimal repro: `mut x = 0; while cond { detach { x = 1 }; cond =
        // false }; ro y = x`). Fix: `hoist_box_decl` retroactively inserts
        // ONLY the bare declaration (`T* bv;`, no initializer) at the
        // enclosing top-level C function's own top scope (anchor set by
        // `emit_block_stmts`) — a scope that by construction dominates both
        // this capture point and every later read in the same function,
        // since Nova requires a captured `mut` local's OWN declaration to
        // already be in scope before ANY capture of it. The heap-alloc
        // ASSIGNMENT (still exactly-once, same control flow as before)
        // stays right here.
        //
        // Known accepted limits (same class as the escaping-handler box,
        // [M-175-handler-lit-boxed-var-c-scope-leak]): (a) box reuse across
        // TWO detach sites capturing the same var relies on the first box's
        // C declaration still being in scope; (b) rebinding-visibility of a
        // scalar capture read BEFORE the detach line inside a loop follows
        // emission order, not iteration order. Neither shape exists in the
        // corpus; both degrade to stale reads, never to UB.
        for (cap, ty, by_value) in &captures {
            let is_outer_cap = self.current_spawn_captures.as_ref()
                .map(|s| s.contains(cap)).unwrap_or(false);
            let outer_by_value = self.current_spawn_capture_by_value.as_ref()
                .map(|s| s.contains(cap)).unwrap_or(false);
            // [M-detach-ctx-capture-after-ro-call-value-ptr-mismatch] fix
            // (221.1 Ф.2 #16, mirrors emit_spawn's identical
            // [M-nv-spawn-ctx-capture-mut-param-ptr-mismatch] fix, same
            // file's `emit_spawn` capture loop above): a captured name that
            // is NOT itself an outer-fiber
            // capture (`is_outer_cap` false) is not necessarily a plain C
            // value either — a `mut T` in-out param (Plan 184 R10) OR a
            // large `ro` value-struct param passed by-ref for efficiency
            // (Plan 172.14 Ф.1, free fn or method) is ALREADY `T*` in C
            // (`self.ref_params`, populated once per enclosing fn — see
            // emit_fn's param-classification pass). This detach capture-
            // populate loop only ever checked `is_outer_cap`, never
            // `ref_params` — so capturing such a parameter into `detach {}`
            // (with NO intervening spawn) fell through to the bare
            // `cap.clone()` "ordinary local" arm, which assumes `cap`'s C
            // storage IS the value: `ctx->field = cap;` (by_value) then
            // assigned a `T*` into a `T` field (clang: "assigning to 'T'
            // from incompatible type 'T *'"). Live repro: a bare (no `mut`/
            // `consume`) multi-field value-record parameter (nova-http's
            // `ServerPolicy`, 8 fields) captured into `detach { ... }` —
            // reproduced independent of any earlier method call on it (the
            // "after an earlier ro-call" framing was circumstantial: any
            // multi-field value-struct param triggers `free_fn_byref_flag`/
            // `method_byref_flag` and lands in `ref_params` regardless).
            let outer_is_ref_param = self.ref_params.contains(cap);
            let access_outer = if is_outer_cap {
                if outer_by_value { format!("_c->{}", cap) }
                else { format!("(*_c->{})", cap) }
            } else if outer_is_ref_param {
                format!("(*{})", cap)
            } else { cap.clone() };
            // Plan 248 (wave 3, second mega-CU regression,
            // [M-detach-box-mut-param-value-copy-diverges]): is the capture's
            // SOURCE already a pointer to storage whose lifetime is someone
            // ELSE's guarantee — an outer fiber's own capture slot (`_c->cap`,
            // already either a box or a P10 pointer set up by whoever emitted
            // THAT capture) or a P10 `mut x T` in-out PARAMETER (`ref_params`,
            // pointing at the CALLER's frame — which may well still be alive,
            // e.g. a test body that calls a helper taking `mut n AtomicInt`,
            // detaches inside it, and reads `n` back itself after
            // `drain_orphans()`) — as opposed to a genuine LOCAL declared in
            // THIS function's own body (`mut n = AtomicInt.new(0)` right
            // here), whose storage really IS this frame's own stack and
            // really does need a fresh heap box (see the box branch below,
            // and its own №240 doc, for THAT case).
            let source_is_already_ptr = (is_outer_cap && !outer_by_value) || outer_is_ref_param;
            if *by_value {
                self.line(&format!("{ctx_var}->{cap} = {access_outer};"));
            } else if source_is_already_ptr {
                // `access_outer` above DEREFERENCES the source pointer down
                // to a VALUE (right for the `*by_value` read-only-snapshot
                // arm just above). For a MUTABLE capture whose source is
                // ALREADY a pointer, copying that VALUE into a NEW heap box
                // (the branch below) would SEVER the alias: the box becomes
                // an independent cell, and neither the caller's own storage
                // nor any sibling capture of the SAME source ever observes
                // the orphan's mutation. Invisible for the old pointer-
                // newtype shapes — dereferencing there landed on ANOTHER
                // pointer (`T*` all the way down), still aliasing the one
                // shared heap object even after boxing. For a value-inside
                // `#share` type the dereference lands on the REAL value, and
                // boxing IT is a genuine, diverging copy. Found via
                // `detach_mut_capture_outlives_frame.nv`'s `dmc_kick(mut n
                // AtomicInt)` — `n` is a P10 pointer into the TEST's own
                // (still-alive) frame; forward it AS-IS, no allocation at
                // all — the pointee's lifetime is already someone else's
                // guarantee, exactly the property `#share` types exist for.
                // Ctx field type is already `T*` for non-by-value captures
                // (see `lambda_forward_decls` above); no `var_boxed` entry
                // either — there is no box, later reads of `cap` in THIS
                // function's own body are untouched (still plain `n`/
                // `(*n)`, unaffected by anything a `detach{}` did with it).
                let ptr_expr = if is_outer_cap { format!("_c->{}", cap) } else { cap.clone() };
                self.line(&format!("{ctx_var}->{cap} = {ptr_expr};"));
            } else {
                let box_ptr = if let Some(existing) = self.var_boxed.get(cap) {
                    // Var already heap-promoted by an earlier closure/handler/
                    // detach in this enclosing fn — share the same box so all
                    // parties observe the same cell.
                    existing.clone()
                } else {
                    let bv = format!("{}_box_{}", detach_id, cap);
                    // №240: bare declaration hoisted (see fn-doc above);
                    // only the assignment stays at this (possibly nested)
                    // call site.
                    //
                    // Plan 248 (wave 3 fallout, real bug found by the
                    // integrator's mega-CU gate, [M-detach-box-value-inside-
                    // reboxing-loses-aggregation]): the alloc+copy below sits
                    // at the (possibly LOOPED) call site of `detach{}` — a
                    // single AST node compiled ONCE, but when that C position
                    // is inside a `while`/`for` body it EXECUTES every
                    // iteration. For a POINTER-kind captured type (the only
                    // shape this mechanism ever boxed before this plan —
                    // Mutex/Condvar/the old Atomic* newtypes) re-running
                    // `*bv = counter` every iteration only re-copies the SAME
                    // pointer VALUE — wasteful (a fresh heap cell per
                    // iteration) but harmless, since dereferencing ANY of
                    // those cells still reaches the ONE shared pointee. For a
                    // value-inside `#no_copy` `#share` type (Plan 248 wave 3
                    // — `AtomicInt` etc. now hold their state INLINE, no
                    // indirection) `*bv = counter` COPIES THE CURRENT VALUE —
                    // every iteration silently starts a BRAND NEW,
                    // independent counter at whatever `counter` (never
                    // written back from any box) currently reads, and every
                    // earlier iteration's fibers end up mutating an orphaned,
                    // no-longer-referenced box. `runtime.drain_orphans()` +
                    // read-after only ever observes the LAST iteration's
                    // fresh, near-zero box — found via `standalone/
                    // m240_detach_box_while_loop_read_after.nv`'s value-
                    // checked asserts going from `n == 3`/`n == 6` to wrong
                    // numbers once `AtomicInt` moved off the pointer-newtype
                    // shape (byte-identical C position/shape either way —
                    // this bug always existed in the GENERATED C, masked
                    // purely by the old representation's copy-is-cheap-alias
                    // property, same class as the `compare_exchange`-through-
                    // `callnorm.rs`-hoist bug found earlier in this wave).
                    // Fix: guard the alloc+copy so it runs at most ONCE per
                    // enclosing-function CALL (the hoisted declaration is a
                    // plain C local, freshly `NULL` on every fresh stack
                    // frame) — every LATER iteration (and every OTHER
                    // `detach{}` site sharing this box via the `var_boxed`
                    // reuse branch above) reuses the SAME heap cell, matching
                    // the pointer-kind behavior exactly and restoring correct
                    // cross-iteration/cross-site aggregation for value-inside
                    // types without changing anything for pointer-kind ones.
                    self.hoist_box_decl(ty, &bv);
                    self.line(&format!("if (!{bv}) {{", bv = bv));
                    self.indent += 1;
                    self.line(&format!(
                        "{bv} = ({ty}*)nova_alloc(sizeof({ty}));",
                        ty = ty, bv = bv));
                    self.line(&format!("*{bv} = {access_outer};",
                        bv = bv, access_outer = access_outer));
                    self.indent -= 1;
                    self.line("}");
                    self.var_boxed.insert(cap.clone(), bv.clone());
                    bv
                };
                self.line(&format!("{ctx_var}->{cap} = {box_ptr};"));
            }
        }

        // Fire-and-forget — caller continues without waiting.
        self.line(&format!("nova_runtime_spawn_orphan({detach_id}, {ctx_var});"));

        Ok("NOVA_UNIT".to_string())
    }

    /// №240 [M-detach-box-while-loop-read-after]: hoist a bare box-pointer
    /// **declaration** (`T* bv;`, no initializer) to the enclosing top-level
    /// C function's own scope — see the long rationale comment at this
    /// function's only call site (`emit_detach`'s capture-setup loop) for
    /// the full root-cause story. `self.detach_box_hoist` is the insertion
    /// anchor: `(byte-offset into self.out, indent level)`, set once per
    /// top-level C function by `emit_c.rs::emit_block_stmts`. Each hoist
    /// inserts right after the previous one (offset advances by the
    /// inserted text's own length), so multiple boxes in the same function
    /// stay in declaration order. No anchor (defensive fallback — should
    /// not happen in practice, every `detach{}`-containing body routes
    /// through `emit_block_stmts` first) falls back to the old inline
    /// declare-at-call-site behavior.
    ///
    /// Plan 248 (wave 3 fallout): initialized to `NULL` — the call site's
    /// alloc+copy is now guarded by `if (!bv)` (see the capture-setup loop
    /// above) so a re-executed (looped) `detach{}` position boxes AT MOST
    /// ONCE per enclosing-function call, instead of re-allocating (and, for
    /// value-inside captured types, re-SNAPSHOTTING) a fresh cell on every
    /// iteration.
    fn hoist_box_decl(&mut self, ty: &str, bv: &str) {
        if let Some((offset, indent)) = self.detach_box_hoist {
            let text = format!("{}{}* {} = NULL;\n", "    ".repeat(indent), ty, bv);
            self.out.insert_str(offset, &text);
            self.detach_box_hoist = Some((offset + text.len(), indent));
        } else {
            self.line(&format!("{}* {} = NULL;", ty, bv));
        }
    }
}
