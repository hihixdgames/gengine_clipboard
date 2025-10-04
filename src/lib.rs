mod clipboard_data;
mod clipboard_error;

pub use clipboard_data::*;
pub use clipboard_error::*;
use raw_window_handle::HasDisplayHandle;

#[cfg(not(target_arch = "wasm32"))]
pub trait WasmOrSend: Send + 'static {}
#[cfg(not(target_arch = "wasm32"))]
impl<T: Send + 'static> WasmOrSend for T {}

#[cfg(target_arch = "wasm32")]
pub trait WasmOrSend {}
#[cfg(target_arch = "wasm32")]
impl<T> WasmOrSend for T {}

trait ClipboardCallback: FnMut(ClipboardEvent) + WasmOrSend + 'static {}

impl<F: FnMut(ClipboardEvent) + WasmOrSend + 'static> ClipboardCallback for F {}

#[derive(Debug)]
pub enum ClipboardEvent {
	StartedPasteHandling,
	FailedPasteHandling(ClipboardError),
	Paste(ClipboardData, Option<ClipboardError>),
}

trait InternalClipboard {
	fn new<F: FnMut(ClipboardEvent) + WasmOrSend + 'static>(
		display_handle: &dyn HasDisplayHandle,
		callback: F,
	) -> Self;

	#[cfg(not(target_arch = "wasm32"))]
	fn request_data(&self);

	#[cfg(feature = "unstable_write")]
	fn write(&self, data: ClipboardData);
}

#[cfg_attr(target_os = "linux", path = "linux/mod.rs")]
#[cfg_attr(target_os = "windows", path = "windows/mod.rs")]
#[cfg_attr(target_arch = "wasm32", path = "wasm.rs")]
mod platform;

pub struct Clipboard {
	internal: platform::Clipboard,
}

impl Clipboard {
	pub fn new<F: FnMut(ClipboardEvent) + WasmOrSend + 'static>(
		display_handle: &dyn HasDisplayHandle,
		callback: F,
	) -> Self {
		let internal = platform::Clipboard::new(display_handle, callback);
		Self { internal }
	}

	#[cfg(not(target_arch = "wasm32"))]
	pub fn request_data(&self) {
		self.internal.request_data();
	}

	#[cfg(feature = "unstable-write")]
	pub fn write(&self, data: ClipboardData) {
		self.internal.write(data);
	}
}
