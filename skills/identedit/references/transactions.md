# Transactions and Error Reference

## Multi-File Transactions

Use `edit` to compile a changeset first, then apply it atomically.

```bash
# request.json contains the edit request, either single-file or files[] batch shape.
identedit edit --json < request.json > changeset.json

# Validate without writing.
identedit apply --dry-run changeset.json

# Commit from plan file.
identedit apply changeset.json
```

`apply --dry-run` and `patch --dry-run` (including `--diff`) take apply locks when they reach apply validation. Direct line-target `patch` no-ops skip that stage, but `apply --dry-run` still locks unchanged planned edits. Overlapping identedit previews or apply runs on the same file can return `resource_busy`; preview locks are released before the command returns and do not reserve a later write.

Apply validation opens the file for both reading and writing, so even previews need write access.

On Unix, locks are generally advisory and do not protect against programs that ignore them. On Windows, locks can also deny other programs read/write access.

Wrapped stdin mode:

```bash
jq -n --slurpfile plan changeset.json '{
  command: "apply",
  changeset: $plan[0]
}' | identedit apply --json
```

`apply --json` expects a compiled changeset, i.e. the output of `identedit edit --json`, not a raw edit request.

If any file fails, all files are rolled back to their original state.

## Staging-Only Rollback Drill

```bash
IDENTEDIT_EXPERIMENTAL=1 identedit apply --inject-failure-after-writes 1 changeset.json
```

Use this only for operational drills. It injects a deterministic commit-stage failure before write `N+1`; for `N=1`, one write commits, then rollback is exercised.

## Error Recovery Table

| Error | Meaning | Action |
|---|---|---|
| `precondition_failed` | File changed since read | Re-run read, rebuild edit request, retry once |
| `target_missing` | Structure no longer exists | Re-run read to discover current state |
| `ambiguous_target` | Multiple matches for a selector | Inspect candidates and any `candidate_summary`; for omitted targets, use `read --json` without `--symbol` or filters, adding `--verbose` for bodies. Use the chosen guards and JSON `span_hint` for duplicate identities; see the [candidate contract](protocol.md#ambiguity-candidate-details) |
| `path_changed` | File modified during apply | Re-run full pipeline (`read`, `edit`, `apply`) |
| `resource_busy` | Apply lock held, e.g. by another identedit preview or apply | Wait briefly, then retry once within the [shared target limit](../SKILL.md#recovery-and-output-contracts) |
| `rollback_failed` | Apply failed and rollback incomplete | Inspect files manually, then re-run pipeline |
| `parse_failure`, `error.stage: "source"` | Provider rejected the original source | Repair the source with direct editing, then retry |
| `parse_failure`, `error.stage: "proposed_result"` | Provider rejected the proposed file; no transaction writes started | Correct replacement text or other edits, then retry once with `--dry-run` |
| Other `parse_failure` | Provider setup failed or stage is unspecified | Inspect diagnostics; do not infer that the original file is invalid |
| `no_provider` | Unsupported file type | Use direct editing instead |

Use `error.stage` for this distinction, not message wording or the presence of `suggestion`. See [node-result parsing](protocol.md#node-result-parsing) for the validation boundary.
