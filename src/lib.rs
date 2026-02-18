mod execution_context;

pub mod apply;
pub mod changeset;
pub mod cli;
pub mod error;
pub mod grammar;
pub mod handle;
pub mod hash;
pub mod hashline;
mod patch;
pub mod provider;
pub mod selector;
pub mod transform;

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
