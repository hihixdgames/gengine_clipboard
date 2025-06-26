# gengine_clipboard
A multiplatform library for handling clipboard events.

### Wasm currently does not support copying images

# Wasm Development
Remember to windows target with
```
rustup target add wasm32-unknown-unknown
```
and to uncomment the wasm32-unknown-unknown target in .cargo/config for development tools know that we work on wasm.

To run the wasm example use:
```
cargo run-wasm --example wasm_test
```
and then open the link given in the terminal. It often makes sense to use a private/icognito window to prevent some cache problems.
