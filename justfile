# kqs development commands

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

# Publish dry-run. Leaf crates verify fully; dependent crates (kq-core, kqs)
# cannot resolve their unpublished path deps locally, so they are audited via
# `cargo package --list` and verified in CI after the leaves are live.
publish-check:
    cargo publish -p kq-config --dry-run --allow-dirty
    cargo publish -p kq-llm --dry-run --allow-dirty
    cargo publish -p kq-embeddings --dry-run --allow-dirty
    cargo package -p kq-core --list --allow-dirty
    cargo package -p kqs --list --allow-dirty

# Bump workspace version (single source of truth in Cargo.toml)
bump-version VERSION:
    sed -i.bak -E 's/^version = "[0-9]+\.[0-9]+\.[0-9]+"$$/version = "{{VERSION}}"/' Cargo.toml
    rm -f Cargo.toml.bak

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
    @echo "  publish-check - Publish dry-run (leaves) + package audit (dependents)"
    @echo "  bump-version VERSION - Bump workspace version in Cargo.toml"
    @echo "  clean     - Remove build artifacts"
    @echo "  help      - Show this help"
