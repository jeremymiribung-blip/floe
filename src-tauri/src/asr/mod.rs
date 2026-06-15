#![allow(dead_code)]

pub mod adapters;
pub mod backend;
pub mod error;
pub mod fallback;
pub mod policy;
pub mod registry;
pub mod traits;
pub mod types;

#[cfg(test)]
pub mod tests;
