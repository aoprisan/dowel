//! Derive macro for the `hewn` dependency-wiring convention.
//!
//! See the `hewn` crate for the documented expansion. This crate is an
//! implementation detail; depend on `hewn`, not on `hewn-macros`.

use proc_macro::TokenStream;

/// Stub — real expansion implemented in the next step.
#[proc_macro_derive(Wire, attributes(wire))]
pub fn derive_wire(_input: TokenStream) -> TokenStream {
    TokenStream::new()
}
