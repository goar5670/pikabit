#![feature(is_some_and)]
#![feature(let_chains)]

pub mod bitfield;
pub mod client;
pub mod common;
pub mod conc;
pub mod constants;
pub mod error;
pub mod peer_protocol;
pub mod stats;
pub mod tracker_protocol;

// todo: add crate-level checks for correctness | priority: med
// todo: implement error handling | priority: med
// todo: add crate-level doc | priority: low
