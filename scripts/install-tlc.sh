#!/usr/bin/env bash
# Install a PATH wrapper for TLC using the official tla2tools.jar.
set -euo pipefail

VERSION="${TLA_VERSION:-v1.8.0}"
JAR_URL="https://github.com/tlaplus/tlaplus/releases/download/${VERSION}/tla2tools.jar"
# Pin integrity for the default VERSION. Override with TLA_JAR_SHA256 when using another release.
EXPECTED_SHA256="${TLA_JAR_SHA256:-cc4803dce2a8ffaf0f5920a9dc39df4b5ee34ab4cb53fb58ac557277a7e516b3}"
INSTALL_DIR="${1:-${RUNNER_TEMP:-/tmp}/tla-tools}"
mkdir -p "$INSTALL_DIR"
JAR="$INSTALL_DIR/tla2tools.jar"

verify_sha256() {
  local file="$1"
  local expected="$2"
  local actual
  if command -v sha256sum >/dev/null 2>&1; then
    actual=$(sha256sum "$file" | awk '{print $1}')
  else
    actual=$(shasum -a 256 "$file" | awk '{print $1}')
  fi
  if [[ "$actual" != "$expected" ]]; then
    echo "error: tla2tools.jar SHA256 mismatch (got $actual, expected $expected)" >&2
    rm -f "$file"
    exit 1
  fi
}

if [[ ! -f "$JAR" ]]; then
  curl -fsSL -o "$JAR" "$JAR_URL"
  if [[ "$VERSION" == "v1.8.0" || -n "${TLA_JAR_SHA256:-}" ]]; then
    verify_sha256 "$JAR" "$EXPECTED_SHA256"
  else
    echo "warning: skipping SHA256 check for VERSION=$VERSION (set TLA_JAR_SHA256 to enforce)" >&2
  fi
elif [[ "$VERSION" == "v1.8.0" || -n "${TLA_JAR_SHA256:-}" ]]; then
  verify_sha256 "$JAR" "$EXPECTED_SHA256"
fi

cat >"$INSTALL_DIR/tlc" <<EOF
#!/usr/bin/env bash
exec java -XX:+UseParallelGC -cp "$JAR" tlc2.TLC "\$@"
EOF
chmod +x "$INSTALL_DIR/tlc"
echo "$INSTALL_DIR"
