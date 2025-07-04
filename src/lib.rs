mod clipboard_data;
mod clipboard_error;

pub use clipboard_data::*;
pub use clipboard_error::*;

#[cfg(not(target_arch = "wasm32"))]
pub trait WasmOrSend: Send + 'static {}
#[cfg(not(target_arch = "wasm32"))]
impl<T: Send + 'static> WasmOrSend for T {}

#[cfg(target_arch = "wasm32")]
pub trait WasmOrSend {}
#[cfg(target_arch = "wasm32")]
impl<T> WasmOrSend for T {}

#[derive(Debug)]
pub enum ClipboardEvent {
	StartedPasteHandling,
	FailedPasteHandling(ClipboardError),
	Paste(ClipboardData, Option<ClipboardError>),
}

trait InternalClipboard {
	fn new<F: FnMut(ClipboardEvent) + WasmOrSend + 'static>(callback: F) -> Self;

	#[cfg(not(target_arch = "wasm32"))]
	fn get_data(&self);

	fn write(&self, data: ClipboardData);
}

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
type Internal = windows::WindowsClipboard;

#[cfg(target_arch = "wasm32")]
mod wasm;
#[cfg(target_arch = "wasm32")]
type Internal = wasm::WasmClipboard;

pub struct Clipboard {
	internal: Internal,
}

impl Clipboard {
	pub fn new<F: FnMut(ClipboardEvent) + WasmOrSend + 'static>(callback: F) -> Self {
		Self {
			internal: Internal::new(callback),
		}
	}

	#[cfg(not(target_arch = "wasm32"))]
	pub fn get_data(&self) {
		self.internal.get_data();
	}

	pub fn write(&self, data: ClipboardData) {
		self.internal.write(data);
	}
}
