#!/bin/bash
# submit githem-cli to aur
set -euo pipefail

# check if in correct directory
if [ ! -f PKGBUILD ]; then
    echo "error: PKGBUILD not found in current directory"
    exit 1
fi

# get version - from PKGBUILD in CI, ask user if manual
if [ -t 0 ]; then
    # running manually - ask for version
    read -p "enter version to submit: " VERSION
else
    # running in CI - extract from PKGBUILD
    VERSION=$(grep '^pkgver=' PKGBUILD | cut -d'=' -f2)
fi

# regenerate .SRCINFO
makepkg --printsrcinfo > .SRCINFO

# initialize git repo if needed
if [ ! -d .git ]; then
    git init
    git remote add origin ssh://aur@aur.archlinux.org/githem-cli.git
fi

# add files
git add PKGBUILD .SRCINFO
git commit -m "update to version ${VERSION}"

# push to aur
git push -u origin master
echo "package submitted to AUR"
