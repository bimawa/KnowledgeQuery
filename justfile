# kq development commands

set shell := ["bash", "-uc", "unset CARGO_TARGET_DIR; eval $@", "bash"]

# Build workspace (debug)
build:
    cargo build --workspace

# Build workspace (release)
release:
    cargo build --release --workspace

# Run all tests
test:
    cargo test --workspace

# Run clippy with deny warnings
lint:
    cargo clippy --workspace -- -D warnings

# Check formatting
fmt:
    cargo fmt --check

# Fix formatting
fmt-fix:
    cargo fmt

# Full CI check: build + lint + test + fmt
check: build lint test fmt

# Remove build artifacts
clean:
    cargo clean

# Show help
help:
    @echo "Available recipes:"
    @echo "  build     - Build workspace (debug)"
    @echo "  release   - Build workspace (release)"
    @echo "  test      - Run all tests"
    @echo "  lint      - Run clippy with deny warnings"
    @echo "  fmt       - Check formatting"
    @echo "  fmt-fix   - Fix formatting"
    @echo "  check     - Build + lint + test + fmt (CI equivalent)"
    @echo "  clean     - Remove build artifacts"
    @echo "  help      - Show this help"
