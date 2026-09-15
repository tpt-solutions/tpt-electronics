# Security Policy

## Supported versions

| Version | Supported |
|---|---|
| 0.1.x   | ✅ |

## Reporting a vulnerability

**Do not open a public GitHub issue for security reports.**

Email **security@tpt.solutions** with:

- A description of the issue and its impact.
- Steps to reproduce or a proof-of-concept.
- Affected crate(s) and version(s).
- Any known mitigations.

You will receive an acknowledgment within **48 hours** and a status update at
least every 7 days until resolution.

## Disclosure policy

1. Report received and acknowledged (48 h).
2. Issue triaged; severity assessed (CVSS).
3. Fix developed in a private fork; embargoed.
4. Coordinated release: patched version + advisory (GHSA) published together.
5. Reporter credited (unless anonymity is requested).

We target **90 days** from report to coordinated public disclosure.

## Scope

- Memory safety violations (the workspace forbids `unsafe`; `unsafe` in a
  released crate is a security bug by definition).
- Incorrect numerical results that could lead to unsafe hardware decisions
  (e.g., under-predicting junction temperature).
- Dependency-chain license violations that would silently break the MIT chain.

## Preferred languages

Rust. Please include a failing test if possible.
