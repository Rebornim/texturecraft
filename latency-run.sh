#!/bin/bash
# Latency benches on GitHub: build HEAD (and BASE), then run each bench alternately BASE, HEAD, BASE, HEAD so runner noise cancels
# usage: latency-run.sh <base-ref|none> <rounds> <bench names...>   e.g. latency-run.sh origin/perf-harness/main 2 visibility strokes
set -eo pipefail
BASE=$1 ROUNDS=$2; shift 2
mkdir -p out
trees=(new)
if [ "$BASE" != none ]; then git worktree add -q ../base "$BASE"; trees=(base new); fi
dir() { [ "$1" = base ] && echo ../base || echo .; }
for tree in "${trees[@]}"; do
  (cd "$(dir $tree)" && git log --oneline -1 | tee -a "$OLDPWD/out/trees.txt" && CARGO_TARGET_DIR=$PWD/target cargo test --release -p texturecraft-engine --test it --no-run 2>&1 | tail -1)
done
for round in $(seq "$ROUNDS"); do
  for bench in "$@"; do
    for tree in "${trees[@]}"; do
      echo "=== $tree round $round latency_$bench  load $(cut -d' ' -f1-3 /proc/loadavg)" | tee -a out/$tree.log
      start=$(date +%s)
      (cd "$(dir $tree)" && CARGO_TARGET_DIR=$PWD/target cargo test -q --release -p texturecraft-engine --test it "latency::latency_$bench" -- --ignored --nocapture --test-threads=1 2>&1) | grep -E " \| |panicked|error" | sed "s/^/$tree r$round | /" | tee -a out/$tree.log
      echo "=== took $(( $(date +%s) - start )) s" | tee -a out/$tree.log
    done
  done
done
