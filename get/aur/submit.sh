#!/bin/bash
# submit githem-cli to aur
set -euo pipefail

# check if in correct directory
if [ ! -f PKGBUILD ]; then
    echo "error: PKGBUILD not found in current directory"
    exit 1
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
git commit -m "update to version 0.3.2"

# push to aur
git push -u origin master

echo "package submitted to AUR"
