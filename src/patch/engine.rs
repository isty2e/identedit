use crate::error::IdenteditError;

/// Run patch execution in canonical stage order.
///
/// Stages:
/// 1) resolve target-specific input into canonical internal form
/// 2) verify preconditions against that canonical form
/// 3) apply verified changes
pub(crate) fn run_resolve_verify_apply<Resolved, Verified, Output, Resolve, Verify, Apply>(
    resolve: Resolve,
    verify: Verify,
    apply: Apply,
) -> Result<Output, IdenteditError>
where
    Resolve: FnOnce() -> Result<Resolved, IdenteditError>,
    Verify: FnOnce(Resolved) -> Result<Verified, IdenteditError>,
    Apply: FnOnce(Verified) -> Result<Output, IdenteditError>,
{
    let resolved = resolve()?;
    let verified = verify(resolved)?;
    apply(verified)
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::run_resolve_verify_apply;
    use crate::error::IdenteditError;

    #[test]
    fn stage_runner_executes_in_resolve_verify_apply_order() {
        let stages = RefCell::new(Vec::new());

        let output = run_resolve_verify_apply(
            || {
                stages.borrow_mut().push("resolve");
                Ok::<usize, IdenteditError>(7)
            },
            |resolved| {
                stages.borrow_mut().push("verify");
                Ok::<usize, IdenteditError>(resolved + 1)
            },
            |verified| {
                stages.borrow_mut().push("apply");
                Ok::<usize, IdenteditError>(verified + 1)
            },
        )
        .expect("stage runner should succeed");

        assert_eq!(output, 9);
        assert_eq!(*stages.borrow(), vec!["resolve", "verify", "apply"]);
    }

    #[test]
    fn resolve_error_short_circuits_verify_and_apply() {
        let stages = RefCell::new(Vec::new());

        let error = run_resolve_verify_apply(
            || {
                stages.borrow_mut().push("resolve");
                Err(IdenteditError::InvalidRequest {
                    message: "resolve failed".to_string(),
                })
            },
            |_: usize| {
                stages.borrow_mut().push("verify");
                Ok::<usize, IdenteditError>(1)
            },
            |_: usize| {
                stages.borrow_mut().push("apply");
                Ok::<usize, IdenteditError>(2)
            },
        )
        .expect_err("resolve error should propagate");

        assert!(
            matches!(error, IdenteditError::InvalidRequest { .. }),
            "expected invalid request from resolve stage"
        );
        assert_eq!(*stages.borrow(), vec!["resolve"]);
    }

    #[test]
    fn verify_error_short_circuits_apply() {
        let stages = RefCell::new(Vec::new());

        let error = run_resolve_verify_apply(
            || {
                stages.borrow_mut().push("resolve");
                Ok::<usize, IdenteditError>(42)
            },
            |_: usize| {
                stages.borrow_mut().push("verify");
                Err(IdenteditError::InvalidRequest {
                    message: "verify failed".to_string(),
                })
            },
            |_: usize| {
                stages.borrow_mut().push("apply");
                Ok::<usize, IdenteditError>(99)
            },
        )
        .expect_err("verify error should propagate");

        assert!(
            matches!(error, IdenteditError::InvalidRequest { .. }),
            "expected invalid request from verify stage"
        );
        assert_eq!(*stages.borrow(), vec!["resolve", "verify"]);
    }
}
