set pagination off
frame function nova_supervised_run_impl
set var $q = q
printf "SCOPE children=%d pins=%d\n", $q->child_count, $q->ctx_pins_count
set var $i = 0
while $i < $q->child_count
  set var $c = (NovaSpawnCtxBase*)$q->child_ctx[$i]
  if $c != 0
    printf "c%03d slot=%d fst=", $i, $c->_nova_worker_slot
    output $c->_nova_fiber_state
    printf " pst="
    output $c->_nova_park_state
    printf "\n"
  else
    printf "c%03d NULLCTX\n", $i
  end
  set var $i = $i + 1
end
set var $w = 0
while $w < _n_workers
  printf "W%d yielded_count=%d yielded_head=%d runnext=%p\n", $w, _workers[$w].yielded_count, _workers[$w].yielded_head, _workers[$w].runnext
  set var $w = $w + 1
end
