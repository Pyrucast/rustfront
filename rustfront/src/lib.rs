//! Safe Rust interface to [MFront] material behaviours.
//!
//! MFront generates material behaviours (elasticity, plasticity, creep, …) as
//! shared libraries. This crate loads them and integrates them from Rust code,
//! such as a finite-element solver, through [MGIS] and the raw bindings of
//! [`rustfront_sys`].
//!
//! **Status:** empty for now, the API is yet to be written.
//!
//! [MFront]: https://thelfer.github.io/tfel/web/index.html
//! [MGIS]: https://github.com/thelfer/MFrontGenericInterfaceSupport

// Tant que l'API n'existe pas, rien n'appelle `rustfront_sys`, et cargo signale
// la dépendance comme inutilisée (`cargo::unused_dependencies`). Cette ligne la
// marque comme voulue ; elle disparaîtra avec le premier appel réel.
use rustfront_sys as _;
