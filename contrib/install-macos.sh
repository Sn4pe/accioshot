#!/bin/sh
# Builds Accioshot.app, installs it and starts it now and at every login. Run it again to update.
set -eu

APP=/Applications/Accioshot.app
LABEL=com.sn4pe.accioshot
AGENT="$HOME/Library/LaunchAgents/$LABEL.plist"
DOMAIN="gui/$(id -u)"
IDENTITY="Accioshot Local Signing"

cd "$(dirname "$0")/.."
(cd editor/ui && npm ci --silent && npm run build --silent)
cargo build --release --workspace
TARGET="${CARGO_TARGET_DIR:-target}/release"

# Screen Recording follows the signature. Ad hoc signatures change with every build, this one stays.
if ! security find-certificate -c "$IDENTITY" >/dev/null 2>&1; then
    WORK=$(mktemp -d)
    trap 'rm -rf "$WORK"' EXIT
    PASSWORD=$(/usr/bin/openssl rand -hex 16)
    /usr/bin/openssl req -x509 -newkey rsa:2048 -nodes -days 3650 -subj "/CN=$IDENTITY" \
        -addext "basicConstraints=critical,CA:false" \
        -addext "keyUsage=critical,digitalSignature" \
        -addext "extendedKeyUsage=critical,codeSigning" \
        -keyout "$WORK/key.pem" -out "$WORK/cert.pem" 2>/dev/null
    /usr/bin/openssl pkcs12 -export -inkey "$WORK/key.pem" -in "$WORK/cert.pem" \
        -out "$WORK/identity.p12" -passout "pass:$PASSWORD"
    security import "$WORK/identity.p12" -P "$PASSWORD" -T /usr/bin/codesign >/dev/null
    # A grant for the previous signature looks enabled but does nothing.
    tccutil reset ScreenCapture "$LABEL" >/dev/null 2>&1 || true
fi

launchctl bootout "$DOMAIN/$LABEL" 2>/dev/null || true
# One opened from Finder is not launchd's, and it would keep the lock.
pkill -x accioshot 2>/dev/null || true
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS"
cp "$TARGET/accioshot" "$TARGET/accioshot-editor" "$APP/Contents/MacOS/"
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
    <key>NSHighResolutionCapable</key>
    <true/>
</dict>
</plist>
EOF
codesign --force --deep --sign "$IDENTITY" "$APP"

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
        <key>Crashed</key>
        <true/>
    </dict>
    <key>StandardErrorPath</key>
    <string>$HOME/Library/Logs/accioshot.log</string>
</dict>
</plist>
EOF
launchctl bootstrap "$DOMAIN" "$AGENT"
echo "Installed $APP. To stop it: launchctl bootout $DOMAIN/$LABEL"
