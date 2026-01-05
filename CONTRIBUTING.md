# Contributing to tylax

Thank you for your interest in contributing to tylax!

## Getting Started

1. Fork the repository
2. Clone your fork:
   ```bash
   git clone https://github.com/YOUR_USERNAME/tylax.git
   cd tylax
   ```
3. Create a new branch:
   ```bash
   git checkout -b feature/your-feature-name
   ```

## Development Setup

### Prerequisites

- Rust 1.70+ (install via [rustup](https://rustup.rs/))
- For WASM builds: `wasm-pack` (`cargo install wasm-pack`)

### Building

```bash
# Build CLI
cargo build --release --features cli

# Build WASM
wasm-pack build --target web --features wasm --no-default-features
```

### Testing

```bash
# Run all tests
cargo test --release

# Run specific tests
cargo test latex2typst
cargo test typst2latex
cargo test tikz
```

## Code Style

- Run `cargo fmt` before committing
- Run `cargo clippy --all-features` and fix all warnings
- Follow Rust naming conventions
- Add tests for new features

## Pull Request Process

1. Ensure all tests pass
2. Update documentation if needed
3. Add a clear description of your changes
4. Reference any related issues

## Reporting Issues

- Use the issue templates when available
- Include minimal reproduction steps
- Specify your environment (OS, Rust version)

## License

By contributing, you agree that your contributions will be licensed under the Apache-2.0 License.
