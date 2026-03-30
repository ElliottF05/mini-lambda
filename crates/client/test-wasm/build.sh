set -euo pipefail
cd "$(dirname "$0")"
cargo build --release --target wasm32-wasip2 --target-dir ./target
cp ./target/wasm32-wasip2/release/test-wasm.wasm ./test-wasm.wasm