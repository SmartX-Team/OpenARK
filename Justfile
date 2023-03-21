default:
  @just run

build:
  cargo build --all

clippy:
  cargo clippy --all

fmt:
  cargo fmt --all

test: fmt clippy
  cargo test --all

run:
  cargo run --package 'dash-actor-cli' --release
