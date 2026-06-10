# Security Policy

## Reporting a vulnerability

Please **do not** open a public GitHub issue for security vulnerabilities.

Report vulnerabilities privately via
[GitHub's private vulnerability reporting](https://github.com/gitignore-in/gitignore-in/security/advisories/new)
or by email to `kitsuyui+security@kitsuyui.com`.

Include:
- A description of the vulnerability and its potential impact.
- Steps to reproduce or a proof-of-concept.
- The version(s) affected, if known.

## Scope

This project uses the following components that may be relevant for vulnerability tracking:

- **OpenSSL** (vendored via `openssl` crate with `vendored = true`): TLS for HTTPS requests.
- **reqwest**: HTTP client library.

## Response

We aim to acknowledge reports within 7 days and provide a fix timeline within 30 days
for confirmed vulnerabilities.
