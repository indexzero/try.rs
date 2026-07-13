# Releasing

One-time setup (owner; these are the credential-boundary steps the release
pipeline needs):

1. **Create the tap repo**: `gh repo create indexzero/homebrew-tap --public --add-readme`
   (`--add-readme` matters: the publish job checkouts the repo and pushes to its
   default branch, so it must have at least one commit).

   This is a **registry-style tap**: dist's bot commits a generated *binary*
   formula whose `url`s point at the GitHub Release artifacts — the tap never
   builds anything. Do **not** scaffold it with `brew tap-new`; that installs
   test-bot CI designed for build-from-source taps (PR → bottle → merge),
   which would fire on and fight every bot push. Naming rule: the repo slug is
   `indexzero/homebrew-tap` (what dist's `tap =` config wants), and brew's CLI
   addresses it as `indexzero/tap` — `user/xyz` expands to
   `github.com/user/homebrew-xyz`.
2. **Add the tap token**: create a fine-grained PAT with write access to
   `indexzero/homebrew-tap` and add it as the `HOMEBREW_TAP_TOKEN` actions
   secret on this repo.
3. **crates.io**: `cargo login` with an owner token.

Per release:

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
