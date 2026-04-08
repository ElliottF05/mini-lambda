set -euo pipefail
cd "$(dirname "$0")"
cargo build --release --target wasm32-wasip2 --target-dir ./target
cp ./target/wasm32-wasip2/release/fib.wasm ./fib.wasm
cp ./target/wasm32-wasip2/release/sleep.wasm ./sleep.wasm
cp ./target/wasm32-wasip2/release/http.wasm ./http.wasm