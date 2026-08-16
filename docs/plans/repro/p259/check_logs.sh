#!/bin/bash
cd /home/craft/nova-p259/scratch259/par_logs
for f in b12_5.log b14_0.log b15_6.log b19_1.log b19_4.log; do
  echo "== $f =="
  cat "$f" 2>&1
done
