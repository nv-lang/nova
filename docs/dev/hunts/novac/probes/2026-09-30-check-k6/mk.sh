#!/bin/sh
# mk.sh <name> <desc> : creates <name>/cmd.sh (probe.nv must be written first), runs it, writes run.out
n=$1; desc=$2
dir=/d/Temp/hunt-check-k6/$n
[ -f "$dir/probe.nv" ] || { echo "MISSING probe $dir/probe.nv"; exit 2; }
cat > "$dir/cmd.sh" <<EOC
#!/bin/sh
# Probe $n: $desc
P="\$(cd "\$(dirname "\$0")" && pwd)/probe.nv"
export P
. "\$(dirname "\$0")/../run-probe.sh"
EOC
bash "$dir/cmd.sh" > "$dir/run.out" 2>&1
[ -s "$dir/run.out" ] || { echo "EMPTY run.out for $n"; exit 3; }
echo "- $n: $desc (run.out $(wc -l < "$dir/run.out") lines)" >> /d/Temp/hunt-check-k6/PROGRESS.md
cat "$dir/run.out"
