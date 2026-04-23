mod execution_context;

mod apply;
pub mod changeset;
pub mod cli;
pub mod error;
mod grammar;
mod handle;
pub mod hash;
pub mod hashline;
mod patch;
mod provider;
mod selector;
mod transform;

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
