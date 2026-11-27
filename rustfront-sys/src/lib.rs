//! Raw FFI bindings to the C API of [MGIS], the MFrontGenericInterfaceSupport
//! library.
//!
//! This crate will only declare the C functions and types of MGIS (headers
//! `MGIS/Behaviour/*.h`, `MGIS/Model/*.h`, `MGIS/Status.h`) and link to
//! `libMFrontGenericInterface-c`. Calling them is `unsafe`: use the
//! [`rustfront`](https://docs.rs/rustfront) crate for a safe interface.
//!
//! **Status:** empty for now, the bindings are yet to be written.
//!
//! [MGIS]: https://github.com/thelfer/MFrontGenericInterfaceSupport
