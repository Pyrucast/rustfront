# rustfront

Evaluate [MFront] material behaviours (constitutive laws) at a material point,
in pure Rust.

MFront compiles material behaviours (elasticity, plasticity, creep, …) into
shared libraries. `rustfront` loads the libraries generated with MFront's
`generic` interface and integrates a behaviour at one material point, from
Rust code such as the [pyrucast] finite-element library.

It talks to the generated library directly: no [MGIS], no C or C++ build
dependency, nothing to compile but Rust.

> **Status:** early development, nothing usable yet.

## Requirements

- Behaviours compiled by MFront with the `generic` interface:
  `mfront --obuild --interface=generic Norton.mfront`.
- Rust 1.85 or later.

## Relation to MGIS

[MGIS] is the official library for calling MFront behaviours from a solver,
with C, Fortran, Python and Julia bindings; Rust bindings are on its roadmap
([thelfer/MFrontGenericInterfaceSupport#376](https://github.com/thelfer/MFrontGenericInterfaceSupport/issues/376)).
`rustfront` takes another route: it reads the description each behaviour
exports and calls the behaviour itself, without going through MGIS.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
  <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or
  <https://opensource.org/licenses/MIT>)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in this work by you, as defined in the Apache-2.0 license, shall
be dual licensed as above, without any additional terms or conditions.

[MFront]: https://thelfer.github.io/tfel/web/index.html
[MGIS]: https://github.com/thelfer/MFrontGenericInterfaceSupport
[pyrucast]: https://github.com/Pyrucast/pyrucast
