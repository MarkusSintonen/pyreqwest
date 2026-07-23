# Security Policy

## Supported Versions

Only the latest release of pyreqwest receives security fixes.

## Reporting a Vulnerability

**Do not open a public issue for security vulnerabilities.**

Use the [**Report a vulnerability**](https://github.com/MarkusSintonen/pyreqwest/security/advisories/new) button on the GitHub Security tab.

## Scope

pyreqwest is a Python binding over [reqwest](https://github.com/seanmonstar/reqwest) (Rust) using rustls for TLS. The security surface includes:

- **TLS/certificate validation** — handled by rustls; issues in rustls or webpki should be reported upstream
- **reqwest** — HTTP client vulnerabilities (redirects, header injection, SSRF) should be reported upstream to the [reqwest repository](https://github.com/seanmonstar/reqwest)
- **pyreqwest binding layer** — memory safety, Python/Rust boundary issues, incorrect exposure of reqwest behavior

## Out of Scope

- Vulnerabilities in Python itself or PyO3
- Issues in dependencies that have no published CVE or upstream fix
- Denial of service via resource exhaustion from user-controlled inputs (caller's responsibility)
