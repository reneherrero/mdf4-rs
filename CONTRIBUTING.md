# Contributing to mdf4-rs

Thank you for your interest in contributing to mdf4-rs! This document provides guidelines and instructions for contributing to the project.

## Code of Conduct

This project adheres to a code of conduct that all contributors are expected to follow. Please be respectful and constructive in all interactions.

## Getting Started

### Prerequisites

- Familiarity with Rust and the MDF4 file format
- Latest stable release of Rust (MSRV: 1.85.0)
- Code coverage is done with `cargo-llvm-cov`

### Setting Up the Development Environment

### Code Coverage

Install `cargo-llvm-cov` using prebuilt binaries (recommended):

```bash
# Get your host target
host=$(rustc -vV | grep '^host:' | cut -d' ' -f2)

# Download and install prebuilt binary
curl --proto '=https' --tlsv1.2 -fsSL \
  "https://github.com/taiki-e/cargo-llvm-cov/releases/latest/download/cargo-llvm-cov-$host.tar.gz" \
  | tar xzf - -C "$HOME/.cargo/bin"
```

**Alternative methods:**

Using `cargo-binstall` (if installed):
```bash
cargo binstall cargo-llvm-cov
```

Using Homebrew (macOS/Linux):
```bash
brew install cargo-llvm-cov
```

### Git Pre-Commit Hook (recommended)

Install git hooks (recommended):
```bash
./setup-git-hooks.sh
```
This installs a pre-commit hook that automatically runs clippy and formatting checks before each commit.

## Development Workflow

### Basic Commands

```bash
# Build
cargo check --all-targets
cargo check --no-default-features --features alloc

# Test
cargo test --all-features

# Format
cargo fmt

# Lint
cargo clippy --all-features -- -D warnings
cargo clippy --no-default-features --features alloc --lib -- -D warnings
```

**Note**: The pre-commit hook automatically runs clippy and formatting checks.

### Testing and Quality Checks

**Quick reference:**
- **Tests**: Must pass with all features enabled
- **Compilation**: Must compile with `alloc` only (no_std compatible)
- **Coverage**: Minimum 80%
- **MSRV**: Must work with Rust 1.85.0

### Coding Standards

#### Code Style

- Follow the existing code style in the project
- Use `cargo fmt` to format your code (configuration is in `rustfmt.toml`)

#### Documentation

- **All public APIs must be documented** with doc comments (`///`)
- Use code examples in documentation when helpful
- Document error conditions and return values
- Follow Rust documentation conventions

#### Error Handling

- Use `Result<T>` for fallible operations
- Use appropriate error variants (`Error::IOError`, `Error::InvalidBlock`, etc.)

#### Testing

- Write tests for new functionality
- Include both positive and negative test cases
- Test edge cases and error conditions
- Ensure tests pass with all feature combinations

#### Safety

- **No `unsafe` code** - The project uses `#![forbid(unsafe_code)]` (see [ARCHITECTURE.md](ARCHITECTURE.md#design-principles))
- Avoid `unwrap()` and `expect()` in production code (tests are fine)
- Use proper error handling with the `Result` type

### Commit Messages

Write clear, descriptive commit messages:

```
Short summary (50 chars or less)

More detailed explanation if needed. Wrap at 72 characters. Explain:
- What changed and why
- Any breaking changes
- Related issues

Fixes #123
```

### Pull Requests

1. Update documentation if you're adding new features
2. Add tests for new functionality
3. Ensure all CI checks pass
4. Update `CHANGELOG.md`
5. Reference any related issues in your PR description

#### PR Checklist

- [ ] Code follows the project's guidelines
- [ ] All tests pass (`cargo test --all-features`)
- [ ] Clippy passes without warnings
- [ ] Code is formatted (`cargo fmt`)
- [ ] Documentation is updated

### CI/CD Workflows

The project uses GitHub Actions for continuous integration. Workflows automatically run on pushes and pull requests to the `main` branch.

- **mdf4-rs Workflow** (`.github/workflows/mdf4-rs.yml`): Tests library with `std`/`alloc`, MSRV, linting, formatting, and docs

**Best Practices:**
- Wait for CI checks to pass before merging PRs
- Fix CI failures locally before pushing
- Workflows use path-based triggers to reduce unnecessary runs

## Project Structure

See [ARCHITECTURE.md](ARCHITECTURE.md) for detailed module structure, design principles, and technical documentation.

## Areas for Contribution

### High Priority

- More comprehensive test coverage

### Medium Priority

- More example code
- Performance optimizations

### Low Priority

- Extended SI block parsing

## Questions?

If you have questions or need help:

- Open an issue on GitHub
- Check existing issues and discussions
- Review the documentation in the README files

## License

By contributing to mdf4-rs, you agree that your contributions will be licensed under the same license as the project (MIT OR Apache-2.0). See [LICENSING.md](LICENSING.md) for details.

Thank you for contributing!
