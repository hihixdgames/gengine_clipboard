#[cfg(not(target_arch = "wasm32"))]
fn main() {
	panic!("Only works on wasm32 target")
}

#[cfg(target_arch = "wasm32")]
fn main() {
	use gengine_clipboard::Clipboard;

	Clipboard::new(|_| {});
}
