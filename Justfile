lint:
  cargo fmt --check && cargo clippy --all-targets --all-features -- -D warnings

test:
  cargo test --all-targets --all-features

build:
  cargo build

update-submodules:
  git submodule update --remote --merge
  git add examples/fcl_site examples/rheo_docs
  git commit -m "Updates git submodules to latest"
  jj git import
  git submodule status
  @echo ""
  @echo "Submodule references updated and committed. Ready for 'jj git push'."

install:
  cargo install --path . --locked

watch:
  cargo watch -x "build --profile local-dev"