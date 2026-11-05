#!/bin/sh
# Builds Accioshot.app, installs it and starts it now and at every login. Run it again to update.
set -eu

APP=/Applications/Accioshot.app
LABEL=com.sn4pe.accioshot
AGENT="$HOME/Library/LaunchAgents/$LABEL.plist"
DOMAIN="gui/$(id -u)"

cd "$(dirname "$0")/.."
(cd editor/ui && npm ci --silent && npm run build --silent)
cargo build --release --workspace

launchctl bootout "$DOMAIN/$LABEL" 2>/dev/null || true
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS"
cp target/release/accioshot target/release/accioshot-editor "$APP/Contents/MacOS/"
cat > "$APP/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key>
    <string>accioshot</string>
    <key>CFBundleIdentifier</key>
    <string>$LABEL</string>
    <key>CFBundleName</key>
    <string>Accioshot</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)</string>
    <key>LSUIElement</key>
    <true/>
</dict>
</plist>
EOF
codesign --force --deep --sign - "$APP"
# Each build is a new app to the system, and a stale grant looks enabled but does nothing.
tccutil reset ScreenCapture "$LABEL" >/dev/null 2>&1 || true

mkdir -p "$(dirname "$AGENT")" "$HOME/Library/Logs"
cat > "$AGENT" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>$LABEL</string>
    <key>ProgramArguments</key>
    <array>
        <string>$APP/Contents/MacOS/accioshot</string>
        <string>daemon</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <dict>
        <key>SuccessfulExit</key>
        <false/>
    </dict>
    <key>StandardErrorPath</key>
    <string>$HOME/Library/Logs/accioshot.log</string>
</dict>
</plist>
EOF
launchctl bootstrap "$DOMAIN" "$AGENT"
echo "Installed $APP. To stop it: launchctl bootout $DOMAIN/$LABEL"
