//! Evaluate [MFront] material behaviours (constitutive laws) at a material
//! point, from Rust.
//!
//! MFront compiles material behaviours (elasticity, plasticity, creep, …) into
//! shared libraries. For behaviours generated with MFront's `generic`
//! interface, this crate will load the library, read the description the
//! behaviour exports (gradients, material properties, internal state
//! variables, …) and integrate it at one material point. It talks to the
//! library directly, in pure Rust: no [MGIS], no C or C++ build dependency.
//!
//! **Status:** early development, nothing usable yet.
//!
//! [MFront]: https://thelfer.github.io/tfel/web/index.html
//! [MGIS]: https://github.com/thelfer/MFrontGenericInterfaceSupport
