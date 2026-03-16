#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${REPO_DIR}"

APP_NAME="diske"
PROFILE="release"
ASSET_LABEL=""

usage() {
    cat <<'EOF'
Usage: ./scripts/package-release.sh --label <asset-label> [--debug|--release]

Options:
  --label     Required asset label suffix, e.g. `macos-aarch64`
  --debug     Package the debug build
  --release   Package the release build (default)
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --label)
            ASSET_LABEL="${2:-}"
            shift 2
            ;;
        --debug)
            PROFILE="debug"
            shift
            ;;
        --release)
            PROFILE="release"
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "Unknown option: $1" >&2
            usage >&2
            exit 1
            ;;
    esac
done

if [[ -z "${ASSET_LABEL}" ]]; then
    echo "Missing required option: --label" >&2
    usage >&2
    exit 1
fi

if [[ "$(uname -s)" != "Darwin" ]]; then
    echo "This packaging script currently supports macOS only." >&2
    exit 1
fi

VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)"
DIST_DIR="${REPO_DIR}/dist"
mkdir -p "${DIST_DIR}"
rm -f "${DIST_DIR}/${APP_NAME}-v${VERSION}-${ASSET_LABEL}.app.zip"
rm -f "${DIST_DIR}/${APP_NAME}-v${VERSION}-${ASSET_LABEL}.tar.gz"
rm -f "${DIST_DIR}/${APP_NAME}-v${VERSION}-${ASSET_LABEL}.build-info.txt"
rm -f "${DIST_DIR}/SHA256SUMS-${ASSET_LABEL}.txt"

if [[ "${PROFILE}" == "release" ]]; then
    ./bundle.sh --release
    BINARY_PATH="target/release/${APP_NAME}"
else
    ./bundle.sh --debug
    BINARY_PATH="target/debug/${APP_NAME}"
fi

APP_ZIP="${DIST_DIR}/${APP_NAME}-v${VERSION}-${ASSET_LABEL}.app.zip"
BIN_TGZ="${DIST_DIR}/${APP_NAME}-v${VERSION}-${ASSET_LABEL}.tar.gz"
BUILD_INFO_OUT="${DIST_DIR}/${APP_NAME}-v${VERSION}-${ASSET_LABEL}.build-info.txt"
CHECKSUMS="${DIST_DIR}/SHA256SUMS-${ASSET_LABEL}.txt"

ditto -c -k --sequesterRsrc --keepParent "target/${APP_NAME}.app" "${APP_ZIP}"
tar -C "$(dirname "${BINARY_PATH}")" -czf "${BIN_TGZ}" "${APP_NAME}"
cp "target/${APP_NAME}.app/Contents/Resources/build-info.txt" "${BUILD_INFO_OUT}"

shasum -a 256 \
    "${APP_ZIP}" \
    "${BIN_TGZ}" \
    "${BUILD_INFO_OUT}" > "${CHECKSUMS}"

echo "Packaged assets:"
printf ' - %s\n' "${APP_ZIP}" "${BIN_TGZ}" "${BUILD_INFO_OUT}" "${CHECKSUMS}"
