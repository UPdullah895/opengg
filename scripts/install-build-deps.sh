#!/usr/bin/env bash
# Install everything needed to BUILD OpenGG (daemon + Qt6/QML shell) inside a
# distro build container. Package names mirror dev.sh's pkg_for() (verified by
# building in clean containers). Used by .github/workflows/release.yml and by
# local packaging verification. Must run as root.
#
# Usage: scripts/install-build-deps.sh [debian|fedora|suse]   (default: detect)
set -euo pipefail

family="${1:-}"
if [ -z "$family" ]; then
    ids=" $(. /etc/os-release; echo "${ID:-} ${ID_LIKE:-}") "
    case "$ids" in
        *" fedora "*|*" rhel "*) family=fedora ;;
        *" debian "*|*" ubuntu "*) family=debian ;;
        *suse*) family=suse ;;
        *) echo "unsupported distro: $ids" >&2; exit 1 ;;
    esac
fi

case "$family" in
    debian)
        export DEBIAN_FRONTEND=noninteractive
        apt-get update
        apt-get install -y --no-install-recommends \
            ca-certificates curl git xz-utils build-essential cmake ninja-build \
            clang libclang-dev pkg-config \
            qt6-base-dev qt6-declarative-dev qt6-multimedia-dev qt6-shadertools-dev \
            libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev \
            libsqlite3-dev libgl1-mesa-dev
        ;;
    fedora)
        dnf install -y \
            ca-certificates curl git xz tar gzip gcc gcc-c++ make cmake ninja-build \
            clang clang-devel pkgconf-pkg-config \
            qt6-qtbase-devel qt6-qtdeclarative-devel qt6-qtmultimedia-devel qt6-qtshadertools-devel \
            gstreamer1-devel gstreamer1-plugins-base-devel \
            sqlite-devel mesa-libGL-devel
        ;;
    suse)
        zypper --non-interactive --gpg-auto-import-keys refresh
        zypper --non-interactive install --no-recommends \
            ca-certificates curl git xz tar gzip gcc gcc-c++ make cmake ninja \
            clang clang-devel pkgconf-pkg-config \
            qt6-base-devel qt6-declarative-devel qt6-multimedia-devel qt6-shadertools-devel \
            gstreamer-devel gstreamer-plugins-base-devel \
            sqlite3-devel Mesa-libGL-devel
        ;;
    *) echo "unknown family: $family" >&2; exit 1 ;;
esac
