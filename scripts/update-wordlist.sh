#!/usr/bin/env bash
# Rebuilds crates/deckscribe-core/data/english-words.txt from SCOWL (nixpkgs
# `scowl`): sizes 10-50 of the English and American lists, lowercase entries
# only (proper nouns are left out on purpose), no possessives, letters only.
# The list tells ordinary English words apart from names when correcting
# mishearings. SCOWL's copyright notice is kept in data/SCOWL-COPYRIGHT.
set -euo pipefail
cd "$(dirname "$0")/.."
scowl=${SCOWL:-$(nix build --no-link --print-out-paths nixpkgs#scowl)}
out=crates/deckscribe-core/data
for size in 10 20 35 40 50; do
  cat "$scowl/share/scowl/english-words.$size" "$scowl/share/scowl/american-words.$size"
done | iconv -f ISO-8859-1 -t UTF-8 | grep -E '^[a-z]+$' | LC_ALL=C sort -u > "$out/english-words.txt"
cp "$scowl/lib/aspell/Copyright" "$out/SCOWL-COPYRIGHT"
chmod u+w "$out/SCOWL-COPYRIGHT"
wc -l "$out/english-words.txt"
