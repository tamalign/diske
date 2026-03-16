#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "${SCRIPT_DIR}"

APP_NAME="diske"
PROFILE="release"
OPEN_APP="false"

usage() {
    cat <<'EOF'
Usage: ./bundle.sh [--debug|--release] [--open]

Options:
  --debug     Build `target/debug/diske` into the app bundle
  --release   Build `target/release/diske` into the app bundle (default)
  --open      Open the generated `target/diske.app` after bundling
EOF
}

for arg in "$@"; do
    case "$arg" in
        --debug)
            PROFILE="debug"
            ;;
        --release)
            PROFILE="release"
            ;;
        --open)
            OPEN_APP="true"
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "Unknown option: $arg" >&2
            usage >&2
            exit 1
            ;;
    esac
done

if [[ "${PROFILE}" == "release" ]]; then
    CARGO_BUILD_CMD=(cargo build --release)
    BINARY_PATH="target/release/${APP_NAME}"
else
    CARGO_BUILD_CMD=(cargo build)
    BINARY_PATH="target/debug/${APP_NAME}"
fi

APP_DIR="target/${APP_NAME}.app"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)"
BUILD_TIME="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"

echo "Building ${PROFILE} binary..."
"${CARGO_BUILD_CMD[@]}"

if [[ ! -f "${BINARY_PATH}" ]]; then
    echo "Expected binary not found: ${BINARY_PATH}" >&2
    exit 1
fi

echo "Creating fresh app bundle at ${APP_DIR}..."
rm -rf "${APP_DIR}"
mkdir -p "${APP_DIR}/Contents/MacOS"
mkdir -p "${APP_DIR}/Contents/Resources"

cp "${BINARY_PATH}" "${APP_DIR}/Contents/MacOS/${APP_NAME}"

cat > "${APP_DIR}/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>
    <string>diske</string>
    <key>CFBundleDisplayName</key>
    <string>diske</string>
    <key>CFBundleIdentifier</key>
    <string>com.diske.app</string>
    <key>CFBundleVersion</key>
    <string>${VERSION}</string>
    <key>CFBundleShortVersionString</key>
    <string>${VERSION}</string>
    <key>CFBundleExecutable</key>
    <string>diske</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>LSMinimumSystemVersion</key>
    <string>11.0</string>
</dict>
</plist>
PLIST

cat > "${APP_DIR}/Contents/Resources/build-info.txt" <<EOF
app=${APP_NAME}
version=${VERSION}
profile=${PROFILE}
built_at_utc=${BUILD_TIME}
source_binary=${BINARY_PATH}
EOF

chmod +x "${APP_DIR}/Contents/MacOS/${APP_NAME}"

echo "Done!"
echo "App bundle:   ${APP_DIR}"
echo "Executable:   ${APP_DIR}/Contents/MacOS/${APP_NAME}"
echo "Build info:   ${APP_DIR}/Contents/Resources/build-info.txt"
echo "Open with:    open ${APP_DIR}"

if [[ "${OPEN_APP}" == "true" ]]; then
    echo "Opening ${APP_DIR}..."
    open "${APP_DIR}"
fi
