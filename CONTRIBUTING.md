# Contributing to tpt-electronics

**This project does not accept pull requests.** The repository is maintained
by its author; external code changes are not merged. There is no CLA process and
no Developer Certificate of Origin (DCO) sign-off requirement, because there is
no contribution queue to sign off on.

The supported way to help is to **open an issue**. Issues are how bugs get
reported, how features get requested, and how design changes get discussed.

## Reporting a bug

Use the [bug report template](.github/ISSUE_TEMPLATE/bug_report.md). The most
useful reports include:

- The exact command, test, or API call that reproduces the problem.
- The input data (a minimal Gerber/Excellon file, netlist, Touchstone fixture,
  or a few lines of code) — see `test-data/` for the formats already supported.
- Your platform, toolchain version (`rustc --version`), and crate version.
- Expected versus actual output, including the full error text.

Security problems are **not** public issues — see [SECURITY.md](SECURITY.md).

## Requesting a feature

Use the [feature request template](.github/ISSUE_TEMPLATE/feature_request.md).
Describe the physics or engineering problem you are trying to solve and the
accuracy or throughput you need, rather than a specific API shape. That makes it
much easier to judge whether the feature belongs in an existing crate or needs a
new one.

## Proposing a design change

New crates, new public APIs, and breaking changes go through the RFC process
using the [RFC template](.github/ISSUE_TEMPLATE/rfc.md). Write the RFC as an
issue; there is no PR to attach it to. RFCs live in `rfcs/` once accepted and
move through `Draft → Accepted → Implemented → Final`, with the maintainer as
tie-breaker.

## What is not accepted

- Pull requests, forks submitting patches, or patch emails.
- CLA requests or sign-off enforcement. There is no CLA and never will be one.

## Code of conduct

Participation in issues and discussions is governed by
[CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md). Be excellent to each other.

## License

The source is dual-licensed under `MIT OR Apache-2.0`. The absence of a
contribution process does not change the license: the code remains open source
and reusable under those terms.

