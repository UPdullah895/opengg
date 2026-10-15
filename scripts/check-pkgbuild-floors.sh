#!/usr/bin/env bash
# Fail if packaging/aur/PKGBUILD declares lower Qt / glibc versions than the
# release binaries actually need.
#
# Why: the Arch tarball is built against whatever Qt archlinux:latest has. A
# binary built on Qt 6.12 imports the `Qt_6.12` symbol version and will not
# load on 6.11 ("version `Qt_6.12' not found"), but with unversioned depends
# pacman installs it on a not-yet-upgraded system anyway — which is exactly how
# v0.2.3 failed for CachyOS users still on Qt 6.11. This check makes the CI
# build fail instead of a user's launch.
#
# Usage: scripts/check-pkgbuild-floors.sh PKGBUILD BINARY...
set -euo pipefail

pkgbuild="$1"; shift
[ $# -gt 0 ] || { echo "usage: $0 PKGBUILD BINARY..." >&2; exit 2; }

syms="$(for b in "$@"; do objdump -T "$b"; done)"

# Highest versioned tag of a family, e.g. "6.12" from Qt_6.12, "2.43" from GLIBC_2.43.
max_tag() {
    { grep -oE "\\b$1[0-9]+(\\.[0-9]+)*\\b" <<<"$syms" || true; } | sed "s/^$1//" | sort -uV | tail -n1
}
# The ">=X" floor PKGBUILD declares for a package, or empty. The `|| true`
# matters: under pipefail a no-match grep would abort the script mid-report.
floor() {
    { grep -oE "'$1>=[0-9.]+'" "$pkgbuild" || true; } | head -n1 | sed -E "s/.*>=([0-9.]+)'/\\1/"
}
# version_ge A B: true when A >= B.
version_ge() { [ "$(printf '%s\n%s\n' "$1" "$2" | sort -V | head -n1)" = "$2" ]; }

fail=0

# Private Qt API pins the binary to one exact Qt build, which no ">=" floor can
# express. qt-shell/build.rs's bytecode-only qmlcachegen shim keeps it out; if
# this fires, the shim fell back (look for its cargo:warning in the build log).
if grep -q 'Qt_6_PRIVATE_API' <<<"$syms"; then
    echo "✗ binary imports Qt_6_PRIVATE_API — it will only run on the exact Qt it was built with" >&2
    fail=1
fi

# "6" alone is the unversioned Qt_6 node every Qt 6 build has; only a "6.N"
# tag (from qt_version_tag) sets a real floor.
qt_need="$(max_tag 'Qt_')"
if [[ "$qt_need" == 6.* ]]; then
    for p in qt6-base qt6-declarative qt6-multimedia qt6-shadertools; do
        have="$(floor "$p")"
        if [ -z "$have" ] || ! version_ge "$have" "$qt_need"; then
            echo "✗ binary needs Qt >= $qt_need but PKGBUILD has '${p}${have:+>=$have}'" >&2
            fail=1
        fi
    done
fi

glibc_need="$(max_tag 'GLIBC_')"
if [ -n "$glibc_need" ]; then
    have="$(floor glibc)"
    if [ -z "$have" ] || ! version_ge "$have" "$glibc_need"; then
        echo "✗ binary needs glibc >= $glibc_need but PKGBUILD has 'glibc${have:+>=$have}'" >&2
        fail=1
    fi
fi

if [ "$fail" = 0 ]; then
    echo "✓ PKGBUILD floors cover the binaries (Qt ${qt_need:-any}, glibc ${glibc_need:-any})"
fi
exit "$fail"
