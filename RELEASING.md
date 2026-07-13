# Releasing

Preconditions (assert, don't set up here): the `indexzero/homebrew-tap` repo
exists with a default branch, the `HOMEBREW_TAP_TOKEN` actions secret is set
on this repo, and you are logged in to crates.io. If any of these fail, see
the provisioning notes in the PR that introduced the release pipeline.

1. Roll the changelog and review it:
   `mise run changelog && git diff CHANGELOG.md`
2. Bump `[workspace.package] version` in `Cargo.toml` if not already done;
   commit both on a PR and merge.
3. Publish to crates.io (order matters — the lib first):
   `cargo publish -p try-me-maybe-core && cargo publish -p try-me-maybe`
4. Tag and push — this triggers the dist pipeline (build, attest, GitHub
   Release, tap formula):
   `git tag v$(dist plan -o json | python3 -c 'import json,sys; print(json.load(sys.stdin)["releases"][0]["app_version"])') && git push --tags`
5. Verify from a clean machine: `brew install indexzero/tap/try-me-maybe`,
   `cargo binstall try-me-maybe`, `mise use -g cargo:try-me-maybe`, and
   `gh attestation verify <artifact> --owner indexzero`.
