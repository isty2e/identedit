mod args;
mod handoff;
mod model;
mod parse;
mod target;
#[cfg(test)]
mod tests;
mod text;

pub(crate) use args::EditIntentArgs;
pub(crate) use handoff::{FailedDiffResponse, prepare_failed_diff_handoff};
pub(crate) use model::{NodeEditIntent, PreparedEditIntent};
pub(crate) use parse::parse_flag_edit_intent;
