#!/usr/bin/env bash
# The checks of the CI, to run locally: the CI calls this script, one check
# per step, so that both cannot drift apart.
#
# usage: scripts/check.sh [fmt|clippy|test|doc|compare|package|msrv|all]
#
# `all` (the default) runs every check. `test`, `compare` and `package` need
# mfront and mtest (TFEL 5.1.0 or later) in the PATH; `msrv` needs the Rust
# toolchain of the `rust-version` of Cargo.toml (rustup toolchain install 1.88).
set -euo pipefail
cd "$(dirname "$0")/.."

check=${1:-all}
case "$check" in
    fmt) cargo fmt --all --check ;;
    clippy) cargo clippy --workspace --all-targets -- -D warnings ;;
    test) cargo test --workspace ;;
    doc) RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps ;;
    # mtest-rs against MTest, on the cases of mtest-rs/cases.
    compare) ./scripts/try.sh ;;
    # Packages both crates and compiles them as crates.io will, mtest-rs
    # against the packaged rustfront. A dry run: uncommitted changes allowed.
    package) cargo publish --workspace --dry-run --allow-dirty ;;
    # The minimum Rust version is read where it is declared.
    msrv)
        version=$(sed -n '/^\[workspace.package\]/,/^\[/s/^rust-version = "\(.*\)"/\1/p' Cargo.toml)
        cargo "+$version" check --workspace --all-targets
        ;;
    all)
        for c in fmt clippy test doc compare package msrv; do
            echo "== $c"
            "$0" "$c"
        done
        ;;
    *)
        echo "usage: $0 [fmt|clippy|test|doc|compare|package|msrv|all]" >&2
        exit 2
        ;;
esac
