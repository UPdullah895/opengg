# Security Policy

## Supported Versions

OpenGG follows semantic versioning. Security patches are applied to the latest stable release and the previous minor version series (if applicable).

| Version | Status | Support |
|---------|--------|---------|
| 0.1.x (latest) | Stable | Full support |
| 0.0.x | Obsolete | Security patches on request only |

## Reporting a Vulnerability

We take security seriously. If you discover a vulnerability in OpenGG, please report it privately using GitHub's Private Vulnerability Reporting feature rather than opening a public issue.

### How to Report

1. **GitHub Private Reporting** (Preferred)
   - Navigate to **Security** → **Report a Vulnerability** in the OpenGG repository
   - Provide details: affected component, impact, reproduction steps (if applicable), and proposed timeline

2. **Email** (Alternative)
   - Email `abdullahbomozh@gmail.com` with subject line `[SECURITY] OpenGG Vulnerability Report`
   - Include the same details as above

### What to Expect

- **Initial Response**: We aim to acknowledge reports within 48 hours
- **Assessment**: We'll evaluate the vulnerability's scope and severity
- **Patch Timeline**:
  - Critical (CVSS 9–10): Patch within 7 days
  - High (CVSS 7–8.9): Patch within 14 days
  - Medium & Below: Fixed in next scheduled release
- **Notification**: We'll provide a coordinated disclosure date and credit the reporter in release notes

## Scope

Security reports are in-scope for the following components:

- **Daemon** (`daemon/src/`) — D-Bus service, device access (ratbagd, OpenRGB SDK), audio routing (PipeWire), extension loading
- **Tauri Host** (`frontend/src-tauri/src/`) — IPC commands, file operations, media server (warp), system integrations, GPU access
- **Media Server** (`frontend/src-tauri/src/media_server.rs`) — HTTP video/thumbnail serving, path traversal, symlink handling
- **Extension System** (`daemon/src/extensions/`) — Plugin loading, sandboxing, manifest parsing

## Out of Scope

- Social engineering or phishing attacks
- Denial of service attacks on end-user machines (OpenGG is client-side only)
- Vulnerabilities in dependencies with no direct exploitation path in OpenGG itself
- Configuration mistakes by end users
- Third-party applications or services (e.g., PipeWire crashes, GPU driver bugs)

## Security Principles

### No Runtime Privilege Escalation

OpenGG operates entirely unprivileged. The daemon runs as the current user. Sensitive operations (udev rules, D-Bus activation, polkit) are configured at install time only and are managed by the system. At runtime, membership in standard Linux groups (`audio`, `input`, `video`) grants all necessary access — **no sudo is ever required or used**.

### Fixed Security Issues (Conceptual Examples)

Recent work has addressed:
- Media server symlink/path traversal hardening
- Extension manifest parsing and sandboxing design
- IPC boundary validation and capability separation

All future security patches will follow the same strict validation and least-privilege principles.

## CI Security Gates

The repository runs automated security scanning on every push and pull request:

- **Cargo Audit** — Checks both daemon and Tauri crates against RUSTSEC database
- **Cargo Deny** — Enforces license policies and bans vulnerable/yanked crates
- **npm Audit** — Scans frontend npm dependencies (high-severity failures block merges)
- **CodeQL** — Static analysis for Rust and JavaScript/TypeScript

## Contributing

When submitting PRs, please:

- Ensure `cargo clippy -- -D warnings` passes in both crates
- Run `npx vue-tsc --noEmit` in the frontend
- Avoid third-party services that require credentials or runtime escapes
- If adding new IPC endpoints, validate all user input and document threat model

## Questions?

For non-security questions, use the issue tracker. For security-related questions that aren't bug reports, feel free to reach out privately at `abdullahbomozh@gmail.com`.

---

Last updated: 2026-06-10
