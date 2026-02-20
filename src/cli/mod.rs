use clap::{Parser, Subcommand};

pub mod apply;
mod merge_plan;
mod line_patch;
pub mod edit;
pub mod grammar;
pub mod merge;
pub mod patch;
pub mod read;
mod read_select;
mod edit_build;

#[derive(Debug, Parser)]
#[command(name = "identedit")]
#[command(about = "Agent-oriented editing engine")]
#[command(
    long_about = "Agent-oriented structural and line-based editing engine. Canonical flow: read -> edit -> apply, with patch for one-shot edits."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    #[command(about = "Read file structure/content with node or line identities")]
    Read(read::ReadArgs),
    #[command(about = "Build an edit plan from canonical targets")]
    Edit(edit::EditArgs),
    #[command(about = "Commit a prepared edit plan to one or more files")]
    Apply(apply::ApplyArgs),
    #[command(about = "Merge multiple edit plans with strict conflict checks")]
    Merge(merge::MergeArgs),
    #[command(about = "Install dynamic tree-sitter grammars")]
    Grammar(grammar::GrammarArgs),
    #[command(about = "One-shot single-target patch (build + apply)")]
    Patch(Box<patch::PatchArgs>),
}
