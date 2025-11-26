lint:
  cargo fmt --check && cargo clippy --all-targets --all-features -- -D warnings

test:
  cargo test --all-targets --all-features

build:
  cargo build
