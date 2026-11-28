# rustfront: notes for contributors

`rustfront` calls MFront behaviours from Rust, in pure Rust: it opens a library
generated with MFront's `generic` interface, reads the description the behaviour
exports, and integrates it at one material point. `mtest-rs` is an unofficial,
independent reimplementation of a subset of MTest built on it.

## Layout

- `src/`: the `rustfront` crate.
  - `ffi.rs`: the C structures of the `generic` interface (TFEL 5.1 headers
    `MFront/GenericBehaviour/State.h` and `BehaviourData.h`).
  - `behaviour.rs`: `Behaviour::load` reads and checks the exported symbols
    (`<behaviour>_<hypothesis>_<name>`, else `<behaviour>_<name>`);
    `Behaviour::integrate` checks every array length, then calls the behaviour.
  - `material_property.rs`: `MaterialProperty::load` and `evaluate`, for the
    material properties (`@DSL MaterialLaw`), whose C interface differs.
  - `symbols.rs`: the private `Symbols` helper shared by both loaders: the
    only place that looks symbols up, with typed reads, each `unsafe` block
    justified once.
  - `rotation.rs`: `Rotation` and `Frame`, for the rotations of orthotropic
    behaviours, which `Behaviour` calls (`_rotateArrayOf…`, in place).
  - `voigt.rs`: conversions between TFEL's storage of symmetric tensors
    (`xx, yy, zz, √2·xy, √2·xz, √2·yz`) and Voigt notation.
- `mtest-rs/`: the command line tool; it follows MTest's default algorithm
  exactly (Lagrange multipliers, convergence criteria, sub-steps), with the
  finite strain residual of TFEL's master branch (see `scripts/try.sh`).
- `tests/behaviours/*.mfront`: test behaviours written for this repository.
  Do not copy TFEL's own test behaviours: their licence is not ours.
- `scripts/`: `check.sh` (the checks of the CI), `try.sh` (mtest-rs against
  MTest), `compare.py`, `install-tfel.sh`, `tfel-suite.py` (mtest-rs against
  MTest on the tests of TFEL's generic interface, read from a checkout of the
  TFEL sources: nothing of TFEL is copied here).

Both crates share one version, declared in the root `Cargo.toml`
(`[workspace.package]` and the `rustfront` entry of `[workspace.dependencies]`).

## Conventions

- Code, comments, documentation and commit messages in English.
- No `Result` that cannot fail, no `Option` whose `None` only stands for a
  default value (`clippy::unnecessary_wraps` is on, extended to the public API).
- Simple, flat code: no function of one or two lines, no function called only
  once or twice; a local closure is fine when it is reused many times.
- One dependency, `libloading`. `mtest-rs` depends on nothing but `rustfront`:
  its parser, expression evaluator and linear solver are written by hand.
- Only `Behaviour::load` is `unsafe`; every `unsafe` block has a `SAFETY:`
  comment.
- The interface is generic: nothing in the code, comments or commits is
  specific to one solver (the README may cite users).
- Discuss a new public API before implementing it.

## Checks

The tests compile the test behaviours with `mfront` and fail without it; the
comparison needs `mtest`. Both come with TFEL 5.1.0 or later:

```bash
export PATH="$HOME/.local/tfel-5.1.0/bin:$PATH"
export LD_LIBRARY_PATH="$HOME/.local/tfel-5.1.0/lib"
./scripts/check.sh            # everything the CI checks
./scripts/check.sh test       # or one check: fmt clippy test doc compare package msrv
./scripts/install-tfel.sh 5.2.0 ~/.local/tfel-5.2.0   # another TFEL version
```

`msrv` needs the toolchain of `rust-version` (`rustup toolchain install 1.88`).
The CI runs the same script, for TFEL 5.1.0 and the latest stable TFEL.

A new check counts only once it has been seen failing: break it on purpose
once, and check the exit code.

## Releasing

See "Releasing" in `README.md`: change the two version lines of `Cargo.toml`,
commit, push a tag `vX.Y.Z`; `.github/workflows/release.yml` publishes both
crates through crates.io's Trusted Publishing.

`Cargo.toml` carries the version of the next release: right after a release,
move to the next version. Otherwise `check.sh package` verifies `mtest-rs`
against the `rustfront` already published under that number, not against the
code of the repository.
