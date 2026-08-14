set pagination off
frame function nova_supervised_run_impl
set var $q = q
set var $c = (NovaSpawnCtxBase*)$q->ctx_pins[331]
printf "c330 ctx=%p\n", $c
printf "parent_scope=%p (q=%p) MATCH=%d\n", $c->_nova_parent_scope, $q, ($c->_nova_parent_scope == $q)
printf "fiber_scope=%p\n", $c->_nova_fiber_scope
set var $ws = $c->_nova_fiber_scope
printf "fiber_scope.dispatch_ready=%p sched_state=%p\n", $ws->dispatch_ready, $ws->sched_state
set var $st = $ws->sched_state
if $st != 0
  printf "worker-scope parked[72]="
  output $st->parked[72]
  printf " parked_co[72]=%p\n", $st->parked_co[72]
end
