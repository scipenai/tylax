# Security Policy

## Supported Versions

| Version | Supported          |
| ------- | ------------------ |
| 0.1.x   | :white_check_mark: |

## Reporting a Vulnerability

If you discover a security vulnerability in tylax, please report it responsibly:

1. **Do NOT** open a public GitHub issue for security vulnerabilities
2. Email the maintainers directly or use GitHub's private vulnerability reporting feature
3. Include as much detail as possible:
   - Description of the vulnerability
   - Steps to reproduce
   - Potential impact
   - Any suggested fixes (if available)

## Response Timeline

- **Acknowledgment**: Within 48 hours
- **Initial Assessment**: Within 7 days
- **Resolution Target**: Within 30 days for critical issues

## Security Considerations

This project is a text processing tool that converts between LaTeX and Typst formats. While it does not handle sensitive data directly, we take the following precautions:

- **Input Validation**: All input is parsed through established parsers (MiTeX, typst-syntax)
- **Recursion Limits**: Macro expansion has depth limits (`MAX_EXPANSION_DEPTH = 100`) to prevent stack overflow
- **No Network Access**: The library operates entirely offline
- **No File System Access**: The core library does not access the file system (CLI does for I/O only)

## Dependency Security

We regularly update dependencies to address known vulnerabilities. You can audit dependencies using:

```bash
cargo audit
```

## Thank You

We appreciate security researchers who help keep this project safe. Contributors who responsibly disclose vulnerabilities will be acknowledged in our release notes (with permission).

