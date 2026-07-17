#!/usr/bin/env bash
# Builds the full bilingual site into _site/:
#   English at /  ·  Portuguese at /pt/
set -euo pipefail
cd "$(dirname "$0")"

QUARTO="${QUARTO:-quarto}"
command -v "$QUARTO" >/dev/null || QUARTO="$HOME/.local/bin/quarto"

"$QUARTO" render --profile en
"$QUARTO" render --profile pt

rm -rf _site/pt
mkdir -p _site/pt
cp -R _site-pt/pt/. _site/pt/

echo "Done. Bilingual site in _site/ (English at /, Portuguese at /pt/)."
