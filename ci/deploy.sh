#!/usr/bin/env bash
set -euo pipefail

# Script to build .deb package for git-hierarchy

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

CARGO_CMD="cargo"

echo "Building release binaries with $CARGO_CMD..."
$CARGO_CMD build --release

PACKAGE_NAME="git-hierarchy"
VERSION="$(grep -m1 '^version =' Cargo.toml | sed -E 's/version = "([^"]+)"/\1/')"
ARCH="$(dpkg --print-architecture 2>/dev/null || echo "amd64")"

BUILD_DIR="target/debian-pkg/${PACKAGE_NAME}_${VERSION}_${ARCH}"
echo "Assembling package in $BUILD_DIR..."

rm -rf "$BUILD_DIR"
mkdir -p "$BUILD_DIR/DEBIAN"
mkdir -p "$BUILD_DIR/usr/bin"
mkdir -p "$BUILD_DIR/usr/share/doc/$PACKAGE_NAME"

# Copy binaries
BINARIES=(
    "git-walk-down"
    "git-rebase-poset"
    "git-rebase-segment"
    "git-segment"
    "git-sum"
)

for bin in "${BINARIES[@]}"; do
    if [ -f "target/release/$bin" ]; then
        cp "target/release/$bin" "$BUILD_DIR/usr/bin/"
        chmod 0755 "$BUILD_DIR/usr/bin/$bin"
    else
        echo "Error: Binary target/release/$bin not found!" >&2
        exit 1
    fi
done

# Copy docs
if [ -f "readme.md" ]; then
    cp readme.md "$BUILD_DIR/usr/share/doc/$PACKAGE_NAME/README"
fi
if [ -f "debian/copyright" ]; then
    cp debian/copyright "$BUILD_DIR/usr/share/doc/$PACKAGE_NAME/copyright"
fi
if [ -f "debian/changelog" ]; then
    cp debian/changelog "$BUILD_DIR/usr/share/doc/$PACKAGE_NAME/changelog"
    gzip -n -9 "$BUILD_DIR/usr/share/doc/$PACKAGE_NAME/changelog"
fi

# Generate DEBIAN/control
cat <<EOF > "$BUILD_DIR/DEBIAN/control"
Package: $PACKAGE_NAME
Version: $VERSION
Architecture: $ARCH
Maintainer: Michal Maruska <mmaruska@gmail.com>
Depends: git
Section: utils
Priority: optional
Homepage: https://github.com/MichalMaruska/git-hierarchy-rust
Description: Tool to rebase a hierarchy of local development commits
 git-hierarchy provides a suite of tools (git-walk-down, git-rebase-poset,
 git-rebase-segment, git-segment, git-sum) to manipulate and rebase a
 hierarchy of local git development commits.
EOF

# Build .deb package
DEB_FILE="target/${PACKAGE_NAME}_${VERSION}_${ARCH}.deb"
echo "Building debian package $DEB_FILE..."
dpkg-deb --build "$BUILD_DIR" "$DEB_FILE"

echo "Package created successfully: $DEB_FILE"
