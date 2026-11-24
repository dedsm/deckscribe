#!/usr/bin/env bash
# usage: bench/run.sh RUN_NAME [extra deckscribe args...]
# Runs deckscribe on every downloaded sample video into $BENCH_DIR/runs/RUN_NAME/.
# BENCH_DIR holds the datasets and videos (see bench/README.md); BIN the binary.
set -u
name=$1; shift
: "${BENCH_DIR:?set BENCH_DIR to the folder with videos/ and the dataset archives}"
BIN=$(realpath "${BIN:-$(dirname "$0")/../target/release/deckscribe}")
cd "$BENCH_DIR"
for v in videos/lpm/*.mp4 videos/m3av/*.mp4; do
  [ -s "$v" ] || continue
  set_=$(basename "$(dirname "$v")"); stem=$(basename "$v" .mp4)
  out=runs/$name/$set_/$stem
  [ -s "$out/deck.json" ] && continue
  mkdir -p "$out"
  t0=$(date +%s)
  "$BIN" "$v" -o "$out" --debug "$@" > "$out/run.log" 2>&1
  echo "$set_/$stem exit=$? $(( $(date +%s)-t0 ))s $(grep -E '^found' "$out/run.log")"
done
