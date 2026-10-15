#!/usr/bin/env bash
# Fail unless every place that carries the app version agrees.
#
#   scripts/check-versions.sh [vX.Y.Z]
#
# Compared: daemon/, core/ and qt-shell/ Cargo.toml `version`, and the AUR
# PKGBUILD `pkgver`. With a tag argument (the release workflow passes
# $GITHUB_REF_NAME) the tag must equal them too.
#
# Why: the packages are named from the tag, but the binary reports
# CARGO_PKG_VERSION. Tagging v0.2.4 with Cargo.toml still at 0.2.3 shipped an
# app whose About page says 0.2.3 inside packages named 0.2.4, and an AUR
# pkgver pointing at a tarball that does not exist. scripts/bump-version.sh
# sets all of them; this catches a tag pushed without running it.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
tag="${1:-}"

crate_version() { sed -n 's/^version *= *"\(.*\)"/\1/p' "$ROOT/$1/Cargo.toml" | head -n1; }

declare -A v=(
    [daemon/Cargo.toml]="$(crate_version daemon)"
    [core/Cargo.toml]="$(crate_version core)"
    [qt-shell/Cargo.toml]="$(crate_version qt-shell)"
    [packaging/aur/PKGBUILD]="$(sed -n 's/^pkgver=//p' "$ROOT/packaging/aur/PKGBUILD" | head -n1)"
)
[ -n "$tag" ] && v[tag]="${tag#v}"

fail=0
ref="${v[qt-shell/Cargo.toml]}"
[ -n "$ref" ] || { echo "::error::cannot read the qt-shell version" >&2; exit 1; }
for k in "${!v[@]}"; do
    if [ "${v[$k]}" != "$ref" ]; then
        echo "::error::version mismatch: $k is '${v[$k]}' but qt-shell/Cargo.toml is '$ref'" >&2
        fail=1
    fi
done
for k in $(printf '%s\n' "${!v[@]}" | sort); do printf '  %-24s %s\n' "$k" "${v[$k]}"; done
[ "$fail" = 0 ] && echo "ok  all versions are $ref" || echo "Run scripts/bump-version.sh <version> (or fix the tag)." >&2
exit "$fail"
