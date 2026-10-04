#!/bin/sh
# SPDX-License-Identifier: GPL-2.0-or-later
# Run in Linux/WSL with host autotools and a MinGW-w64 x64 C toolchain.
set -eu
adapter_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
test "$#" -ge 1 && test "$#" -le 2 || {
    echo "Usage: build-windows.sh NEW_OUTPUT_DIRECTORY [SOURCE_CACHE]" >&2; exit 2;
}
output=$1
source_cache=${2:-}
test ! -e "$output" || { echo "Output already exists; choose a new directory" >&2; exit 2; }
cross=${OPENOCD_CROSS_PREFIX:-x86_64-w64-mingw32-}
for tool in git python3 gcc make aclocal autoconf autoheader automake libtoolize pkg-config \
    "${cross}gcc" "${cross}windres" "${cross}objdump" "${cross}strip"; do
    command -v "$tool" >/dev/null || { echo "Required build tool missing: $tool" >&2; exit 2; }
done
test "$("${cross}gcc" -dumpmachine)" = x86_64-w64-mingw32 || {
    echo "A native Windows x64 MinGW-w64 compiler is required" >&2; exit 2;
}
mkdir -p "$output"
output=$(CDPATH= cd -- "$output" && pwd)
if [ -n "$source_cache" ]; then source_cache=$(CDPATH= cd -- "$source_cache" && pwd); fi
export GIT_OPTIONAL_LOCKS=0
lock_value() {
    python3 - "$adapter_dir/$1" "$2" <<'PY'
import json, sys
value = json.load(open(sys.argv[1], encoding='utf-8'))
for key in sys.argv[2].split('.'):
    value = value[key]
print(value)
PY
}
clone_pin() {
    destination=$1
    upstream=$2
    revision=$3
    cached=$4
    if [ -n "$source_cache" ]; then
        test -d "$cached" || { echo "Pinned source cache missing: $cached" >&2; exit 2; }
        git clone --no-hardlinks --no-checkout "$cached" "$destination"
        git -C "$destination" remote set-url origin "$upstream"
    else
        git init "$destination"
        git -C "$destination" remote add origin "$upstream"
        git -C "$destination" fetch --depth=1 origin "$revision"
    fi
    git -C "$destination" config core.autocrlf false
    git -C "$destination" checkout --detach "$revision"
    test "$(git -C "$destination" rev-parse HEAD)" = "$revision"
}
source_dir="$output/source"
clone_pin "$source_dir" https://github.com/openocd-org/openocd.git \
    "$(lock_value source.lock.json revision)" "$source_cache/openocd-source"
clone_pin "$source_dir/jimtcl" https://github.com/msteveb/jimtcl.git \
    "$(lock_value source.lock.json jimtcl_revision)" "$source_cache/openocd-source/jimtcl"
clone_pin "$source_dir/src/jtag/drivers/libjaylink" \
    "$(lock_value windows-dependencies.lock.json libjaylink.source)" \
    "$(lock_value windows-dependencies.lock.json libjaylink.revision)" \
    "$source_cache/openocd-source/src/jtag/drivers/libjaylink"
clone_pin "$output/hidapi" "$(lock_value windows-dependencies.lock.json hidapi.source)" \
    "$(lock_value windows-dependencies.lock.json hidapi.revision)" "$source_cache/hidapi-0.15.0"
git -C "$source_dir" apply --check "$adapter_dir/0001-debugtui-armv8-register-adapter.patch"
git -C "$source_dir" apply "$adapter_dir/0001-debugtui-armv8-register-adapter.patch"
python3 "$adapter_dir/test.py" --source "$source_dir" --out "$output/host-tests"
python3 - "$adapter_dir/windows-dependencies.lock.json" "$output" "$source_cache" <<'PY'
import hashlib, json, pathlib, shutil, sys, tarfile, urllib.request
lock = json.load(open(sys.argv[1], encoding='utf-8'))['libusb']
output = pathlib.Path(sys.argv[2])
archive = output / ('libusb-' + lock['version'] + '.tar.bz2')
if sys.argv[3]:
    shutil.copyfile(pathlib.Path(sys.argv[3]) / archive.name, archive)
else:
    with urllib.request.urlopen(lock['url'], timeout=60) as response, archive.open('wb') as target:
        shutil.copyfileobj(response, target)
if hashlib.sha256(archive.read_bytes()).hexdigest() != lock['sha256']:
    raise SystemExit('libusb archive checksum mismatch')
with tarfile.open(archive, 'r:bz2') as bundle:
    bundle.extractall(output, filter='data')
PY
prefix="$output/deps"
export PKG_CONFIG_LIBDIR="$prefix/lib/pkgconfig"
export PKG_CONFIG_PATH=
export PKG_CONFIG_SYSROOT_DIR=
build_host=$(gcc -dumpmachine)
jobs=${OPENOCD_BUILD_JOBS:-4}
mkdir -p "$output/libusb-build" "$output/hidapi-build" "$output/build"
(cd "$output/libusb-build" && "$output/libusb-$(lock_value windows-dependencies.lock.json libusb.version)/configure" \
    --build="$build_host" --host=x86_64-w64-mingw32 --prefix="$prefix" --enable-shared --disable-static && \
    make -j"$jobs" && make install)
(cd "$output/hidapi-build" && \
    "${cross}windres" -I "$output/hidapi/hidapi" "$output/hidapi/windows/hidapi.rc" -O coff -o hidapi-resource.o && \
    "${cross}gcc" -O2 -g -shared -I "$output/hidapi/hidapi" -I "$output/hidapi/windows" \
    "$output/hidapi/windows/hid.c" hidapi-resource.o -Wl,--no-undefined \
    -Wl,--out-implib,"$prefix/lib/libhidapi.dll.a" -o "$prefix/bin/libhidapi-0.dll")
# hid.c includes hidapi_descriptor_reconstruct.c when used as an embedded source.
mkdir -p "$prefix/include/hidapi" "$prefix/lib/pkgconfig"
cp "$output/hidapi/hidapi/hidapi.h" "$output/hidapi/windows/hidapi_winapi.h" "$prefix/include/hidapi/"
sed -e "s|@prefix@|$prefix|g" -e 's|@exec_prefix@|${prefix}|g' \
    -e 's|@libdir@|${prefix}/lib|g' -e 's|@includedir@|${prefix}/include|g' \
    -e "s|@VERSION@|$(lock_value windows-dependencies.lock.json hidapi.version)|g" \
    "$output/hidapi/pc/hidapi.pc.in" > "$prefix/lib/pkgconfig/hidapi.pc"
(cd "$source_dir" && ./bootstrap nosubmodule)
(cd "$output/build" && "$source_dir/configure" --build="$build_host" --host=x86_64-w64-mingw32 \
    --prefix=/ --enable-internal-jimtcl --enable-internal-libjaylink --enable-jlink \
    --enable-cmsis-dap --enable-cmsis-dap-v2 --enable-stlink --enable-ftdi \
    --enable-dummy --enable-remote-bitbang --disable-doxygen-html && \
    make -j"$jobs" && make DESTDIR="$output/install" install)
cp "$prefix/bin/libusb-1.0.dll" "$prefix/bin/libhidapi-0.dll" "$output/install/bin/"
python3 "$adapter_dir/windows.py" stage --root "$output" --cross "$cross"
echo "Candidate and corresponding source retained in $output; native Windows verification is still required."
