lint:
  cargo fmt --check && cargo clippy --all-targets --all-features -- -D warnings

test:
  cargo test --all-targets --all-features

build:
  cargo build

install:
  cargo install --path . --locked

watch:
  cargo watch -x "build --profile local-dev"