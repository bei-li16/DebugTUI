#!/bin/sh
# SPDX-License-Identifier: GPL-2.0-or-later
# Requires a Unix build shell, Git, GCC, make, autotools and pkg-config.
set -eu
adapter_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
test "$#" -ge 1 || { echo "Usage: build.sh NEW_OUTPUT_DIRECTORY [configure options]" >&2; exit 2; }
output=$1
shift
test ! -e "$output" || { echo "Output already exists; choose a new directory" >&2; exit 2; }
mkdir -p "$output"
output=$(CDPATH= cd -- "$output" && pwd)
export GIT_OPTIONAL_LOCKS=0
source_dir="$output/source"
revision=d3ebb8d2b9adbfd9a13072e8e446f424b5ff3c0e
git init "$source_dir"
git -C "$source_dir" config core.autocrlf false
git -C "$source_dir" remote add origin https://github.com/openocd-org/openocd.git
git -C "$source_dir" fetch --depth=1 origin "$revision"
git -C "$source_dir" checkout --detach FETCH_HEAD
git -C "$source_dir" submodule update --init --depth=1 jimtcl
git -C "$source_dir" apply --check "$adapter_dir/0001-debugtui-armv8-register-adapter.patch"
git -C "$source_dir" apply "$adapter_dir/0001-debugtui-armv8-register-adapter.patch"
(cd "$source_dir" && ./bootstrap nosubmodule)
mkdir "$output/build"
(cd "$output/build" && "$source_dir/configure" --prefix=/ \
    --enable-internal-jimtcl --enable-dummy --enable-remote-bitbang \
    --disable-internal-libjaylink --disable-jlink "$@" && make -j4 && make DESTDIR="$output/install" install)
cp "$source_dir/COPYING" "$output/install/COPYING.OpenOCD"
python3 "$adapter_dir/test.py" --source "$source_dir" --out "$output/tests" \
    --openocd "$output/install/bin/openocd"
echo "Patched source, installed backend and validation retained in $output"
