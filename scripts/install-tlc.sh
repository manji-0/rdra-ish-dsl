#!/usr/bin/env bash
# Install a PATH wrapper for TLC using the official tla2tools.jar.
set -euo pipefail

VERSION="${TLA_VERSION:-v1.8.0}"
JAR_URL="https://github.com/tlaplus/tlaplus/releases/download/${VERSION}/tla2tools.jar"
INSTALL_DIR="${1:-${RUNNER_TEMP:-/tmp}/tla-tools}"
mkdir -p "$INSTALL_DIR"
JAR="$INSTALL_DIR/tla2tools.jar"

if [[ ! -f "$JAR" ]]; then
  curl -fsSL -o "$JAR" "$JAR_URL"
fi

cat >"$INSTALL_DIR/tlc" <<EOF
#!/usr/bin/env bash
exec java -XX:+UseParallelGC -cp "$JAR" tlc2.TLC "\$@"
EOF
chmod +x "$INSTALL_DIR/tlc"
echo "$INSTALL_DIR"
