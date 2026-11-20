use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Component, Path};
use std::process::Command;

use crate::error::IdenteditError;

use super::catalog::{InstallSource, PinnedSource};

pub(super) fn acquire_source(
    source: InstallSource<'_>,
    workspace: &Path,
    destination: &Path,
) -> Result<(), IdenteditError> {
    match source {
        InstallSource::Explicit(repo) => clone_repo(repo, destination),
        InstallSource::Pinned(pin) => acquire_pin(pin, workspace, destination),
    }
}

fn clone_repo(repo: &str, destination: &Path) -> Result<(), IdenteditError> {
    let output = clone_repo_command(repo, destination)
        .output()
        .map_err(|error| failure("acquisition", error))?;
    if !output.status.success() {
        return Err(failure(
            "acquisition",
            String::from_utf8_lossy(&output.stderr).trim(),
        ));
    }
    Ok(())
}

fn clone_repo_command(repo: &str, destination: &Path) -> Command {
    let mut command = Command::new("git");
    command
        .args(["clone", "--depth", "1", "--", repo])
        .arg(destination);
    command
}

fn acquire_pin(
    pin: PinnedSource<'_>,
    workspace: &Path,
    destination: &Path,
) -> Result<(), IdenteditError> {
    if !is_git_object_id(pin.commit) {
        return Err(failure(
            "verification",
            "catalog pin is not a full commit ID",
        ));
    }

    let objects = workspace.join("objects.git");
    fs::create_dir(&objects).map_err(|error| failure("acquisition", error))?;
    git(
        &objects,
        "acquisition",
        &["init", "--bare", "--template=", "--object-format=sha1"],
    )?;
    git(
        &objects,
        "acquisition",
        &[
            "-c",
            "fetch.fsckObjects=true",
            "fetch",
            "--depth=1",
            "--no-tags",
            "--no-recurse-submodules",
            "--",
            pin.repo,
            pin.commit,
        ],
    )?;
    let fetched = git(
        &objects,
        "verification",
        &["rev-parse", "--verify", "FETCH_HEAD^{commit}"],
    )?;
    verify_commit(pin.commit, &fetched)?;

    export_sources(&objects, pin.commit, destination)
}

fn verify_commit(expected: &str, fetched: &[u8]) -> Result<(), IdenteditError> {
    if fetched.trim_ascii_end() != expected.as_bytes() {
        return Err(failure(
            "verification",
            format!(
                "expected commit {expected}, got {}",
                String::from_utf8_lossy(fetched).trim()
            ),
        ));
    }
    Ok(())
}

fn export_sources(objects: &Path, commit: &str, destination: &Path) -> Result<(), IdenteditError> {
    let entries = git(
        objects,
        "materialization",
        &["ls-tree", "-r", "-z", commit, "--", "src"],
    )?;
    let entries =
        std::str::from_utf8(&entries).map_err(|error| failure("materialization", error))?;
    let entries = entries
        .strip_suffix('\0')
        .ok_or_else(|| failure("materialization", "missing source files under src/"))?;

    // Read blobs directly so checkout transformations cannot change the compiler inputs.
    for record in entries.split('\0') {
        let (path, blob) = source_blob(record)?;
        let content = git(objects, "materialization", &["cat-file", "blob", blob])?;
        let target = destination.join(path);
        fs::create_dir_all(target.parent().expect("source path has a parent"))
            .map_err(|error| failure("materialization", error))?;
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)
            .and_then(|mut file| file.write_all(&content))
            .map_err(|error| failure("materialization", error))?;
    }
    Ok(())
}

fn source_blob(record: &str) -> Result<(&Path, &str), IdenteditError> {
    if let Some((metadata, path)) = record.split_once('\t') {
        let path = Path::new(path);
        let mut components = path.components();
        let mut fields = metadata.split(' ');
        if components.next() == Some(Component::Normal("src".as_ref()))
            && components.clone().next().is_some()
            && components.all(|component| matches!(component, Component::Normal(_)))
            && matches!(fields.next(), Some("100644" | "100755"))
            && fields.next() == Some("blob")
            && let Some(blob) = fields.next()
            && is_git_object_id(blob)
            && fields.next().is_none()
        {
            return Ok((path, blob));
        }
    }
    Err(failure(
        "materialization",
        "expected a regular tracked file under src/",
    ))
}

fn is_git_object_id(id: &str) -> bool {
    id.len() == 40 && id.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn git(directory: &Path, stage: &str, args: &[&str]) -> Result<Vec<u8>, IdenteditError> {
    let output = Command::new("git")
        .current_dir(directory)
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .args(args)
        .output()
        .map_err(|error| failure(stage, error))?;
    if !output.status.success() {
        return Err(failure(
            stage,
            String::from_utf8_lossy(&output.stderr).trim(),
        ));
    }
    Ok(output.stdout)
}

fn failure(stage: &str, detail: impl std::fmt::Display) -> IdenteditError {
    IdenteditError::GrammarInstall {
        message: format!("{stage}: {detail}"),
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;

    use super::*;

    #[test]
    fn clone_repo_passes_repository_as_one_operand_after_options() {
        let destination = Path::new("install workspace").join("source");
        for repo in ["relative grammar", "--example-repository"] {
            let command = clone_repo_command(repo, &destination);
            assert_eq!(command.get_program(), "git");
            assert!(command.get_args().eq([
                OsStr::new("clone"),
                OsStr::new("--depth"),
                OsStr::new("1"),
                OsStr::new("--"),
                OsStr::new(repo),
                destination.as_os_str(),
            ]));
        }
    }

    #[test]
    fn rejects_a_fetched_commit_other_than_the_pin() {
        let pin = "fedcba9876543210fedcba9876543210fedcba98";
        let other = "0123456789abcdef0123456789abcdef01234567\n";
        assert!(verify_commit(pin, other.as_bytes()).is_err());
        assert!(verify_commit(pin, format!("{pin}\n").as_bytes()).is_ok());
    }

    #[test]
    fn materializes_the_pinned_files_instead_of_the_current_checkout() {
        let repository = tempfile::tempdir().unwrap();
        let path = repository.path();
        if Command::new("git").arg("--version").output().is_err() {
            return;
        }
        git(
            path,
            "fixture",
            &["init", "--template=", "--object-format=sha1"],
        )
        .unwrap();
        fs::create_dir(path.join("src")).unwrap();
        fs::write(path.join(".gitattributes"), "*.c text eol=crlf\n").unwrap();
        let parser = path.join("src/parser.c");
        let original = b"/* original grammar */\n";
        fs::write(&parser, original).unwrap();

        let commit = |message| {
            git(path, "fixture", &["add", "."]).unwrap();
            git(
                path,
                "fixture",
                &[
                    "-c",
                    "user.name=identedit-tests",
                    "-c",
                    "user.email=identedit-tests@example.com",
                    "commit",
                    "--no-gpg-sign",
                    "-m",
                    message,
                ],
            )
            .unwrap();
            String::from_utf8(git(path, "fixture", &["rev-parse", "HEAD"]).unwrap()).unwrap()
        };
        let pinned = commit("original");
        fs::write(&parser, b"/* newer grammar */\n").unwrap();
        commit("update grammar");
        let repo = path.to_str().unwrap();

        let workspace = tempfile::tempdir().unwrap();
        let destination = workspace.path().join("grammar");
        acquire_pin(
            PinnedSource {
                repo,
                commit: pinned.trim_end(),
            },
            workspace.path(),
            &destination,
        )
        .unwrap();
        assert_eq!(
            fs::read(destination.join("src/parser.c")).unwrap(),
            original
        );
        assert!(!destination.join(".git").exists());

        let unavailable = tempfile::tempdir().unwrap();
        let destination = unavailable.path().join("grammar");
        let result = acquire_pin(
            PinnedSource {
                repo,
                commit: "fedcba9876543210fedcba9876543210fedcba98",
            },
            unavailable.path(),
            &destination,
        );
        assert!(result.is_err());
        assert!(!destination.exists());
    }
}
