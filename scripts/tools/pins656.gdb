set pagination off
frame function nova_supervised_run_impl
set var $q = q
printf "PINS count=%d\n", $q->ctx_pins_count
set var $i = 0
while $i < $q->ctx_pins_count
  set var $c = (NovaSpawnCtxBase*)$q->ctx_pins[$i]
  if $c != 0
    printf "p%03d pslot=%d wslot=%d fst=", $i, $c->_nova_parent_slot, $c->_nova_worker_slot
    output $c->_nova_fiber_state
    printf " pst="
    output $c->_nova_park_state
    printf "\n"
  else
    printf "p%03d NULL\n", $i
  end
  set var $i = $i + 1
end
