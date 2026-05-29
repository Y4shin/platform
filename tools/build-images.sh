#!/usr/bin/env bash
# Build the M24 precompiled images (full / backend / frontend).
#
# Strategy: produce the binaries + FE bundle via `nix build` against the
# flake derivations in `flake.nix`, dereference the resulting nix-store
# symlinks into `docker/staging/` (docker COPY doesn't follow symlinks
# pointing outside the build context), then `docker build` each image
# off those staged artifacts. CI calls `stage` to get the staged
# artifacts and runs its own `docker/build-push-action` for the
# ghcr.io + cache integration; local devs run with no subcommand to
# get the staged artifacts AND a tagged local image in one go.
#
# Usage:
#   tools/build-images.sh                 # stage + build all three
#   tools/build-images.sh backend         # stage + build just backend
#   tools/build-images.sh stage           # stage all (no docker build)
#   tools/build-images.sh stage frontend  # stage just frontend
#   tools/build-images.sh image full      # build the docker image
#                                         #   (assumes staging done)
#
# Output tags: `junius-<variant>:local` (override via `TAG_PREFIX` /
# `TAG_SUFFIX` env). Re-tag for `docker push` as needed.

set -euo pipefail

# Repo root regardless of where the script was invoked from.
REPO_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

STAGING="docker/staging"
TAG_PREFIX="${TAG_PREFIX:-junius-}"
TAG_SUFFIX="${TAG_SUFFIX:-:local}"

mode="full"   # full = stage + image
case "${1:-}" in
    stage|image) mode="$1"; shift ;;
esac

variants=("$@")
if [[ ${#variants[@]} -eq 0 ]]; then
    variants=(backend full frontend)
fi

# Validate.
for v in "${variants[@]}"; do
    case "$v" in
        backend|full|frontend) ;;
        *) echo "unknown variant: $v (expected backend|full|frontend)" >&2; exit 2 ;;
    esac
done

mkdir -p "$STAGING"

nix_build() {
    nix --extra-experimental-features 'nix-command flakes' \
        build "$@"
}

stage() {
    # What needs `nix build`-ing depends on which variants were requested.
    local need_juniusd_full=0
    local need_juniusd_headless=0
    local need_junius=0
    local need_frontend=0
    for v in "${variants[@]}"; do
        case "$v" in
            full)     need_juniusd_full=1; need_junius=1 ;;
            backend)  need_juniusd_headless=1; need_junius=1 ;;
            frontend) need_frontend=1 ;;
        esac
    done

    echo "==> nix build"
    if (( need_juniusd_full )); then
        nix_build .#juniusd-static -o result-juniusd
        cp -L --no-preserve=mode result-juniusd/bin/juniusd "$STAGING/juniusd-full"
    fi
    if (( need_juniusd_headless )); then
        nix_build .#juniusd-headless-static -o result-juniusd-headless
        cp -L --no-preserve=mode result-juniusd-headless/bin/juniusd "$STAGING/juniusd-headless"
    fi
    if (( need_junius )); then
        nix_build .#junius-static -o result-junius
        cp -L --no-preserve=mode result-junius/bin/junius "$STAGING/junius"
    fi
    if (( need_frontend )); then
        nix_build .#frontend-ssr-bundle -o result-frontend
        rm -rf "$STAGING/frontend-dist"
        cp -rL result-frontend/dist "$STAGING/frontend-dist"
        chmod -R u+w "$STAGING/frontend-dist"
    fi
}

build_image() {
    local v="$1"
    local tag="${TAG_PREFIX}${v}${TAG_SUFFIX}"
    local -a args=()
    case "$v" in
        full)
            args=(
                --build-arg "JUNIUSD_PATH=$STAGING/juniusd-full"
                --build-arg "JUNIUS_PATH=$STAGING/junius"
            ) ;;
        backend)
            args=(
                --build-arg "JUNIUSD_PATH=$STAGING/juniusd-headless"
                --build-arg "JUNIUS_PATH=$STAGING/junius"
            ) ;;
        frontend)
            args=(
                --build-arg "BUNDLE_PATH=$STAGING/frontend-dist"
            ) ;;
    esac
    echo "    $v -> $tag"
    docker build -f "docker/$v.Dockerfile" "${args[@]}" -t "$tag" .
}

if [[ "$mode" == "stage" || "$mode" == "full" ]]; then
    stage
fi

if [[ "$mode" == "image" || "$mode" == "full" ]]; then
    echo "==> docker build"
    for v in "${variants[@]}"; do
        build_image "$v"
    done
fi

if [[ "$mode" == "full" ]]; then
    echo "==> done"
    docker images --format 'table {{.Repository}}:{{.Tag}}\t{{.Size}}' \
        | grep -E "^${TAG_PREFIX}(backend|full|frontend)" || true
fi
