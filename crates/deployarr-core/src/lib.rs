//! The task sequence model that the server, the agent, and the web UI share.
//!
//! The crate does no I/O. It is `no_std` with `alloc`, so library code that
//! names `std` fails to compile, and it forbids unsafe code, so it cannot reach
//! the operating system through `extern` blocks. Unit tests get `std` from the
//! `cfg_attr` below. Never add `extern crate std`.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

pub mod action;
pub mod condition;
pub mod sequence;
pub mod template;
pub mod variable;

mod rfc3339;
mod validated_string;
