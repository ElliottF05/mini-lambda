set -euo pipefail
cargo build --release --target wasm32-wasip1 --target-dir ./target
cp ./target/wasm32-wasip1/release/test-wasm.wasm ./test-wasm.wasm