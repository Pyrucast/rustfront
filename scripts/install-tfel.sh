#!/usr/bin/env bash
# Builds TFEL (mfront, mtest) from its tag on GitHub and installs it.
#
# usage: scripts/install-tfel.sh <version> <prefix>
#   scripts/install-tfel.sh 5.1.0 ~/.local/tfel-5.1.0
#
# Then put <prefix>/bin in the PATH and <prefix>/lib in LD_LIBRARY_PATH. The
# CI builds TFEL with this script, then caches the installation.
set -euo pipefail

if [ $# != 2 ]; then
    echo "usage: $0 <version> <prefix>" >&2
    exit 2
fi
version=$1
prefix=$(realpath -m "$2")
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

git clone --quiet --depth 1 --branch "TFEL-$version" https://github.com/thelfer/tfel.git "$work/tfel"
# Neither the tests nor the website: only the libraries and the tools.
cmake -S "$work/tfel" -B "$work/build" \
    -DCMAKE_BUILD_TYPE=Release \
    -DCMAKE_INSTALL_PREFIX="$prefix" \
    -Denable-testing=OFF -Denable-website=OFF -Denable-mfront-quantity-tests=OFF \
    > "$work/configure.log" || { cat "$work/configure.log"; exit 1; }
cmake --build "$work/build" --parallel "$(nproc)"
cmake --install "$work/build" > /dev/null
echo "TFEL $version installed in $prefix"
