use clap::{Parser, Subcommand};

pub mod apply;
pub mod changeset;
pub mod edit;
pub mod grammar;
pub mod hashline;
pub mod merge;
pub mod patch;
pub mod read;
pub mod select;
pub mod transform;

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
    #[command(name = "select", hide = true)]
    LegacySelect(select::SelectArgs),
    #[command(name = "transform", hide = true)]
    LegacyTransform(transform::TransformArgs),
    #[command(name = "changeset", hide = true)]
    LegacyChangeset(changeset::ChangesetArgs),
    #[command(name = "hashline", hide = true)]
    LegacyHashline(Box<hashline::HashlineArgs>),
}
