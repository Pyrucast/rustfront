# rustfront

Use [MFront] material behaviours from Rust.

MFront generates material behaviours (elasticity, plasticity, creep, …) as
shared libraries. `rustfront` loads and integrates them from Rust code, such as
the [pyrucast] finite-element library, through the C API of [MGIS]
(MFrontGenericInterfaceSupport).

> **Status:** early development, nothing usable yet.

## Crates

| Crate | Role |
|---|---|
| [`rustfront`](rustfront) | Safe interface, the one solvers use |
| [`rustfront-sys`](rustfront-sys) | Raw `unsafe` bindings to the C API of MGIS |

## Requirements

MGIS must be installed, with its C bindings (`libMFrontGenericInterface-c`).
`rustfront-sys` will link to it dynamically by default.

Rust 1.85 or later.

## Upstream

Rust bindings are also on the MGIS roadmap:
[thelfer/MFrontGenericInterfaceSupport#376](https://github.com/thelfer/MFrontGenericInterfaceSupport/issues/376).

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
  <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or
  <https://opensource.org/licenses/MIT>)

at your option.

MGIS itself is distributed under the LGPL v3 or the CeCILL-C licence, and
programs that link to it must respect its terms. Linking dynamically, the
default, keeps those obligations minimal.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in this work by you, as defined in the Apache-2.0 license, shall
be dual licensed as above, without any additional terms or conditions.

[MFront]: https://thelfer.github.io/tfel/web/index.html
[MGIS]: https://github.com/thelfer/MFrontGenericInterfaceSupport
[pyrucast]: https://github.com/Pyrucast/pyrucast
