#!/usr/bin/env bash
# Type-checks and lints the Windows-only Rust code (`#[cfg(windows)]`) from a Linux or macOS
# box, without a Windows C toolchain (#196).
#
#   scripts/check-windows.sh                  # cargo clippy -p git-core --all-targets -D warnings
#   scripts/check-windows.sh -p git-core --lib # any arguments replace those defaults
#
# git-core holds all of the workspace's `cfg(windows)` code; the other crates are the same on
# every platform. CI's windows-latest `rust` job runs the real clippy and the tests.
#
# Needs only the Rust target, run from the repository (it is added to the pinned toolchain):
# `rustup target add x86_64-pc-windows-gnu`. Nothing is linked,
# so no MinGW or MSVC linker is needed. The -sys crates' build scripts (libgit2-sys,
# libssh2-sys, libz-sys) still compile C for the target; this script points them at a stub
# compiler that writes empty object files, which is enough for `cargo check`/`clippy`, since
# the C code is only needed when linking. The output therefore can't run: for tests, push and
# read CI's windows-latest jobs.
#
# Its build goes to target/windows-check, so it never mixes the stub objects into a real build.
set -euo pipefail

TARGET=x86_64-pc-windows-gnu
root=$(cd "$(dirname "$0")/.." && pwd)

if ! rustup target list --installed | grep -qx "$TARGET"; then
    echo "check-windows: the $TARGET target is missing; run: rustup target add $TARGET" >&2
    exit 2
fi

stub="$root/target/windows-check/stub-cc"
mkdir -p "$stub"
cat >"$stub/cc" <<'EOF'
#!/bin/sh
# Stub C compiler for scripts/check-windows.sh: writes an empty file wherever -o points.
out=
while [ $# -gt 0 ]; do
    case "$1" in
    # cc-rs tells the compiler family by preprocessing a probe file and by trying MSVC's `-?`:
    # answer as GCC (MinGW).
    -E) echo '#pragma message "gcc"'; exit 0 ;;
    -\?) exit 1 ;;
    -o) out=$2; shift ;;
    -o*) out=${1#-o} ;;
    esac
    shift
done
if [ -n "$out" ]; then : >"$out"; fi
exit 0
EOF
chmod +x "$stub/cc"

if [ $# -eq 0 ]; then
    set -- -p git-core --all-targets -- -D warnings
fi

export CC_x86_64_pc_windows_gnu="$stub/cc"
# Archives the empty objects; any ar can write the archive format.
export AR_x86_64_pc_windows_gnu="${AR:-ar}"
export CARGO_TARGET_DIR="$root/target/windows-check"

cd "$root"
exec cargo clippy --target "$TARGET" "$@"
