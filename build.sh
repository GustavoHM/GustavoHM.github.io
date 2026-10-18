#!/usr/bin/env bash
# Builds the full multilingual site into _site/:
#   the default profile (English) at /  ·  every other _quarto-<code>.yml at /<code>/
set -euo pipefail
cd "$(dirname "$0")"

QUARTO="${QUARTO:-quarto}"
command -v "$QUARTO" >/dev/null || QUARTO="$HOME/.local/bin/quarto"

ROOT=$(awk '/^profile:/{p=1} p && /default:/{print $2; exit}' _quarto.yml)
"$QUARTO" render --profile "$ROOT"

for f in _quarto-*.yml; do
  code="${f#_quarto-}"
  code="${code%.yml}"
  [ "$code" = "$ROOT" ] && continue
  "$QUARTO" render --profile "$code"
  rm -rf "_site/$code"
  mkdir -p "_site/$code"
  cp -R "_site-$code/$code/." "_site/$code/"
done

echo "Done. Site in _site/ ($ROOT at /, other languages at /<code>/)."
