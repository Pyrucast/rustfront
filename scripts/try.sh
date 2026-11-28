#!/usr/bin/env bash
# Try rustfront on the test behaviours, then compare mtest-rs with MTest on
# the cases of mtest-rs/cases. Needs mfront and mtest (TFEL 5.1.0 or later),
# cargo and python3 in the PATH. Everything is written in target/try.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
work="$root/target/try"

missing=0
for tool in mfront mtest cargo python3; do
    if ! command -v "$tool" > /dev/null; then
        echo "missing: $tool"
        missing=1
    fi
done
if [ "$missing" = 1 ]; then
    cat << 'EOF'
mfront and mtest come with TFEL. When TFEL is installed in ~/.local/tfel-5.1.0:
  export PATH="$HOME/.local/tfel-5.1.0/bin:$PATH"
  export LD_LIBRARY_PATH="$HOME/.local/tfel-5.1.0/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
EOF
    exit 1
fi

rm -rf "$work"
mkdir -p "$work"
cd "$work"

echo "== Compiling the test behaviours with mfront"
if ! mfront --obuild --interface=generic "$root"/tests/behaviours/*.mfront > mfront.log 2>&1; then
    cat mfront.log
    exit 1
fi

echo "== The minimal example"
cargo run --quiet --manifest-path "$root/Cargo.toml" --example minimal -- "$work/src/libBehaviour.so"

echo "== Building mtest-rs"
cargo build --quiet --release --manifest-path "$root/Cargo.toml" -p mtest-rs
mtest_rs="${CARGO_TARGET_DIR:-$root/target}/release/mtest-rs"

failed=()
for case in "$root"/mtest-rs/cases/*.mtest; do
    name=$(basename "$case" .mtest)
    echo
    echo "== $name: MTest, then mtest-rs"
    cp "$case" .
    if ! mtest "$name.mtest" > "$name.mtest.log" 2>&1; then
        echo "mtest failed, see $work/$name.mtest.log"
        failed+=("$name")
        continue
    fi
    if ! "$mtest_rs" "$name.mtest" --output "$name-rs.res" > "$name.mtest-rs.log" 2>&1; then
        echo "mtest-rs failed, see $work/$name.mtest-rs.log"
        failed+=("$name")
        continue
    fi
    if ! python3 "$root/scripts/compare.py" "$name.res" "$name-rs.res"; then
        failed+=("$name")
        continue
    fi
    # Both follow the same algorithm: the same number of Newton iterations at
    # every step, and none at all for a point-wise model (grep then finds
    # nothing, which must not stop the script).
    mtest_iterations=$({ grep -o 'after [0-9]* iterations' "$name.mtest.log" || true; } | awk '{print $2}' | xargs)
    rs_iterations=$({ grep -o ': [0-9]* iteration' "$name.mtest-rs.log" || true; } | awk '{print $2}' | xargs)
    if [ "$mtest_iterations" = "$rs_iterations" ]; then
        echo "same iterations at every step: ${rs_iterations:-none, nothing to solve}"
    else
        echo "iterations differ: MTest $mtest_iterations, mtest-rs $rs_iterations"
        failed+=("$name")
    fi
done

echo
if [ "${#failed[@]}" = 0 ]; then
    echo "All the cases agree with MTest. Results in $work."
else
    echo "Cases that differ from MTest: ${failed[*]}. Results in $work."
    exit 1
fi
