#!/usr/bin/env bash
# Compila o painel e instala "Painel do Site.app" em ~/Applications.
set -euo pipefail
cd "$(dirname "$0")"
export PATH="/opt/homebrew/opt/rustup/bin:$HOME/.cargo/bin:$PATH"

cargo build --release

APP="$HOME/Applications/Painel do Site.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp target/release/painel-site "$APP/Contents/MacOS/painel-site"

# Ícone: icone/icone-1024.png (gerado por icone/desenhar-icone.swift) → .icns
ICONSET="target/Painel.iconset"
rm -rf "$ICONSET" && mkdir -p "$ICONSET"
for s in 16 32 128 256 512; do
  sips -z $s $s icone/icone-1024.png --out "$ICONSET/icon_${s}x${s}.png" >/dev/null
  sips -z $((s*2)) $((s*2)) icone/icone-1024.png --out "$ICONSET/icon_${s}x${s}@2x.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/Painel.icns"
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Painel do Site</string>
  <key>CFBundleDisplayName</key><string>Painel do Site</string>
  <key>CFBundleIdentifier</key><string>net.gakusai.painel-site</string>
  <key>CFBundleExecutable</key><string>painel-site</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleIconFile</key><string>Painel</string>
  <key>CFBundleShortVersionString</key><string>$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>LSApplicationCategoryType</key><string>public.app-category.productivity</string>
</dict>
</plist>
PLIST
codesign --force --deep -s - "$APP" >/dev/null 2>&1 || true
touch "$APP"  # faz o Finder/Dock recarregar o ícone
echo "Instalado: $APP"
