# Contributing to tpt-electronics

Thank you for helping build a fully open-source electronics simulation engine.
This project is **CLA-free** — contributions are accepted under the
[Developer Certificate of Origin](https://developercertificate.org/) (DCO).

## Developer Certificate of Origin

Every commit must be signed off. Use `git commit -s` (or add the trailer
manually):

```text
Signed-off-by: Jane Doe <jane.doe@example.com>
```

The sign-off certifies that you wrote the patch or have the right to pass it on
under the project's `MIT OR Apache-2.0` license. CI rejects PRs whose commits
lack a `Signed-off-by` trailer.

## Contribution workflow

1. Fork the repository.
2. Create a feature branch: `feature/my-new-crate`.
3. Write code + tests (every public API needs rustdoc and an SPDX header).
4. Run the local gate:

   ```console
   $ cargo fmt --all
   $ cargo clippy --workspace --all-targets
   $ cargo test --workspace
   $ cargo deny check licenses
   ```

5. Submit the PR with DCO sign-off.
6. RFC discussion for new crates (below).
7. Merge after 2 approvals.

## Source file header

Every Rust source file starts with the SPDX header (copy
`docs/templates/source-header.rs`):

```rust
// SPDX-License-Identifier: MIT OR Apache-2.0
```

## RFC process

New crates, new public APIs on existing crates, or breaking changes require an
RFC in `rfcs/` (template: `.github/ISSUE_TEMPLATE/rfc.md`):

```text
rfcs/0001-thermal-fem.md
rfcs/0002-spice-mna.md
rfcs/0003-si-eye-diagram.md
```

RFCs move through `Draft → Accepted → Implemented → Final`. Acceptance requires
consensus with the Benevolent Dictator as tie-breaker.

## Release cadence

- **Minor releases:** every 6 weeks (feature train).
- **Patch releases:** as needed for regressions/security.
- **Major releases:** when breaking changes accumulate; each is preceded by a
  deprecation window of at least one minor release.

Versioning is strict SemVer. The CHANGELOG is updated with every PR.

## Code of conduct

See [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md). Be excellent to each other.

## Reporting issues

- Bugs: use the bug report template.
- Security: **do not** open public issues — see [SECURITY.md](SECURITY.md).
- Ideas: feature request template or an RFC for substantial designs.
