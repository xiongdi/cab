---
name: release
description: "Run CAB's complete release workflow: validate local CI, commit and tag changes, push to GitHub, monitor Actions, repair failures, and report the final release status."
---

# CAB Release

Use this skill only when the user asks to release CAB or explicitly invokes `$release`.

Work from the repository root. Preserve the repository's single-instance development rules in `AGENTS.md`; do not start a second dev server or change ports.

## Workflow

1. Inspect `git status`, the current branch, existing tags, and `origin`. Review the version in `package.json` and `Cargo.toml`. Do not overwrite unrelated user changes.
2. Run the local equivalent of `.github/workflows/ci.yml` before creating a commit:
   - `cargo fmt --all -- --check`
   - `cargo clippy --workspace --all-targets -- -D warnings`
   - `./scripts/run-tests.sh`
   - `vp install`, `npx svelte-kit sync`, `vp check`, `npx svelte-check --tsconfig ./tsconfig.json`, `vp test`, and `vp build`
     Use the repository's configured package manager when `vp` is unavailable, and report that deviation.
3. If local CI fails, diagnose and fix only failures caused by the current release changes. Re-run the failed checks and then the complete local CI. Never tag or push a failing tree.
4. Determine the release version automatically. Require `package.json` and `Cargo.toml` versions to agree; if they differ, treat the highest semver component as authoritative only when the difference is a stale generated/lockfile value, otherwise repair the metadata before continuing. If `v<version>` already exists, select the next patch version after the highest existing `vX.Y.Z` tag (bumping major/minor only when repository metadata already does so). Update all version metadata that is part of the release (`package.json`, `package-lock.json`, and Rust workspace/package manifests), then rerun the relevant checks. Never move or delete an existing tag.
5. Review the final diff, commit all intended release changes with a focused message, create the tag, and push the commit and tag to `origin`.
6. Monitor the GitHub Actions runs triggered by the push with `gh run watch` or equivalent. Inspect failed job logs with `gh run view --log-failed`.
7. For a failure caused by the release changes, fix it, rerun local CI, commit, and push a follow-up fix. Do not endlessly retry infrastructure failures: identify them, retry once when useful, then stop and report the run URL and logs.
8. Consider the release complete only when the relevant CI and release/build workflows succeed and GitHub shows the final release as published. Confirm the tag, commit, workflow conclusion, and release URL.

## Safety and reporting

- Tagging and pushing are part of this explicitly invoked release workflow. Before mutating, verify the exact branch, commit, tag, and remote locally; proceed automatically without asking for separate confirmation.
- Never use force-push, delete or move tags, skip CI, or publish a draft manually to bypass a failed workflow.
- Keep hard timeouts on polling commands and clean up any temporary processes.
- Final report must include local CI results, commit SHA, tag, pushed remote, GitHub Actions run URLs/conclusions, release URL, and any fixes made. If blocked, include the exact failing job and evidence.
