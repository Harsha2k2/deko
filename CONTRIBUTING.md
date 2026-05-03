# Contributing to Deko

Thank you for your interest in contributing to Deko! This document provides guidelines for contributing to the project.

## Getting Started

1. Fork the repository
2. Clone your fork: `git clone git@github.com:your-username/deko.git`
3. Create a feature branch: `git checkout -b feature/my-feature`
4. Make your changes
5. Run tests: `cargo test`
6. Run clippy: `cargo clippy -- -D warnings`
7. Format code: `cargo fmt`
8. Commit and push: `git push origin feature/my-feature`
9. Open a Pull Request

## Development Requirements

- Rust 1.75+
- SQLite 3.x
- At least one LLM API key (Gemini or OpenAI)

## Local Development

```bash
# Copy and configure environment
cp .env.example .env

# Run the server
cargo run

# Run tests
cargo test

# Run integration tests
cargo test --test integration

# Check code quality
cargo clippy -- -D warnings
cargo fmt --check
```

## Project Structure

```
deko/
├── src/
│   ├── main.rs          # Entry point
│   ├── config.rs        # Configuration & tracing
│   ├── db.rs            # Database setup & migrations
│   ├── error.rs         # Error types
│   ├── lib.rs           # Library exports
│   ├── middleware/      # Auth middleware
│   ├── models/          # Data models & enums
│   ├── routes/          # API endpoints
│   └── services/        # Business logic (LLM, verdict, webhook, processor)
├── templates/           # Askama HTML templates
├── static/              # CSS & static assets
├── migrations/          # SQLx migrations
└── tests/               # Integration tests
```

## Adding Features

1. Update `FEATURES.md` with the new feature
2. Implement the code following existing patterns
3. Write tests (unit + integration as appropriate)
4. Update API documentation if adding endpoints
5. Ensure clippy passes with no warnings

## Code Style

- Follow Rust idioms and conventions
- Use `thiserror` for error types
- Use `serde` for serialization
- Use `utoipa` for OpenAPI documentation
- Keep functions focused and well-named
- No comments unless explaining non-obvious logic

## Pull Request Guidelines

- One feature/fix per PR
- Include tests for new functionality
- Update documentation as needed
- Ensure CI passes (tests + clippy + fmt)
- Write a clear PR description explaining the changes

## Reporting Issues

- Use the GitHub issue tracker
- Include steps to reproduce for bugs
- Specify the environment (OS, Rust version, etc.)
- For feature requests, describe the use case

## License

By contributing, you agree that your contributions will be licensed under the project's license.
