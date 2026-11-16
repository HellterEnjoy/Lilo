#!/usr/bin/env bash
# Builds the release binary and packages it as dist/Lilo-<version>-<platform>-x86_64.tar.gz
# with a matching .sha256 file. Requires VERSION and PLATFORM (for example ubuntu-22.04 or arch).
set -euo pipefail

: "${VERSION:?VERSION is required}"
: "${PLATFORM:?PLATFORM is required}"

cd "$(dirname "$0")/.."
cargo build --release --locked

package="Lilo-${VERSION}-${PLATFORM}-x86_64"
rm -rf "dist/$package"
mkdir -p "dist/$package"
install -m 755 target/release/Lilo "dist/$package/Lilo"
for document in README.md ROADMAP.md CHANGELOG.md RELEASE.md PRIVACY.md LICENSE; do
  install -m 644 "$document" "dist/$package/$document"
done
tar -C dist -czf "dist/$package.tar.gz" "$package"
(cd dist && sha256sum "$package.tar.gz" > "$package.tar.gz.sha256")
rm -rf "dist/$package"
echo "Packaged dist/$package.tar.gz"
