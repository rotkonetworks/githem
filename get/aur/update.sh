#!/bin/bash
# update githem-cli aur package to new version
set -euo pipefail

if [ $# -ne 1 ]; then
    echo "usage: $0 <new_version>"
    exit 1
fi

NEW_VERSION="$1"
echo "updating to version $NEW_VERSION..."

# fetch new checksums
echo "fetching checksums for x64..."
X64_SHA=$(curl -sSL "https://github.com/rotkonetworks/githem/releases/download/v${NEW_VERSION}/githem-linux-x64" | sha256sum | cut -d' ' -f1)
X64_SHA512=$(curl -sSL "https://github.com/rotkonetworks/githem/releases/download/v${NEW_VERSION}/githem-linux-x64.sha512" | sha256sum | cut -d' ' -f1)

echo "fetching checksums for arm64..."
ARM64_SHA=$(curl -sSL "https://github.com/rotkonetworks/githem/releases/download/v${NEW_VERSION}/githem-linux-arm64" | sha256sum | cut -d' ' -f1)
ARM64_SHA512=$(curl -sSL "https://github.com/rotkonetworks/githem/releases/download/v${NEW_VERSION}/githem-linux-arm64.sha512" | sha256sum | cut -d' ' -f1)

# update PKGBUILD
sed -i "s/^pkgver=.*/pkgver=${NEW_VERSION}/" PKGBUILD
sed -i "s/^pkgrel=.*/pkgrel=1/" PKGBUILD

# update checksums
sed -i "/^sha256sums_x86_64=/,/)/ {
    s/'[a-f0-9]\{64\}'/'${X64_SHA}'/1
    s/'[a-f0-9]\{64\}'/'${X64_SHA512}'/2
}" PKGBUILD

sed -i "/^sha256sums_aarch64=/,/)/ {
    s/'[a-f0-9]\{64\}'/'${ARM64_SHA}'/1
    s/'[a-f0-9]\{64\}'/'${ARM64_SHA512}'/2
}" PKGBUILD

# regenerate .SRCINFO
makepkg --printsrcinfo > .SRCINFO

echo "updated to version $NEW_VERSION"
echo "review changes and run ./submit.sh to push to AUR"
