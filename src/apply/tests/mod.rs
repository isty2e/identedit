use std::io;
use std::path::{Path, PathBuf};

use super::AtomicWritePhase;

mod apply_guards;
mod atomic_write;
mod locking;
mod preflight;
mod properties;
mod summary;
mod symlink;

fn fail_on_phase(target_phase: AtomicWritePhase) -> impl FnMut(AtomicWritePhase) -> io::Result<()> {
    move |phase| {
        if phase == target_phase {
            Err(io::Error::other("injected atomic-write failure"))
        } else {
            Ok(())
        }
    }
}

fn create_python_target(directory: &Path) -> PathBuf {
    let file_path = directory.join("target.py");
    std::fs::write(
        &file_path,
        "def process_data(value):\n    result = value + 1\n    return result\n\n\ndef helper():\n    return \"helper\"\n",
    )
    .expect("fixture write should succeed");
    file_path
}
