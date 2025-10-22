mod clipboard_error;

pub use clipboard_error::*;
use raw_window_handle::HasDisplayHandle;

#[cfg(not(target_arch = "wasm32"))]
pub trait WasmOrSend: Send {}

#[cfg(not(target_arch = "wasm32"))]
impl<T: Send> WasmOrSend for T {}

#[cfg(target_arch = "wasm32")]
pub trait WasmOrSend {}

#[cfg(target_arch = "wasm32")]
impl<T> WasmOrSend for T {}

pub trait PasteDataAccess {
	fn mime_types(&self) -> &[String];

	fn get_data(&mut self, mime_type: &str) -> Result<Vec<u8>, ClipboardError>;
}

/// This indicates the source from which a ClipboardEvent originates from.
///
/// These can be compared to check if two events come from the same source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClipboardEventSource {
	pub(crate) value: usize,
}

pub enum ClipboardEvent<'a> {
	StartedPasteHandling {
		source: ClipboardEventSource,
	},
	FailedPasteHandling {
		error: ClipboardError,
		source: ClipboardEventSource,
	},
	PasteResult {
		data: &'a mut dyn PasteDataAccess,
		source: ClipboardEventSource,
	},
}

pub trait ClipboardHandler: WasmOrSend + Sized + 'static {
	fn handle_event(&mut self, event: ClipboardEvent<'_>);
}

trait InternalClipboard {
	fn new<T: ClipboardHandler>(display_handle: &dyn HasDisplayHandle, handler: T) -> Self;

	#[cfg(not(target_arch = "wasm32"))]
	fn request_data(&self);

	#[cfg(feature = "unstable_write")]
	fn write(&self, data: ClipboardData);
}

#[cfg_attr(target_os = "linux", path = "linux/mod.rs")]
#[cfg_attr(target_os = "windows", path = "windows/mod.rs")]
#[cfg_attr(target_arch = "wasm32", path = "wasm/mod.rs")]
mod platform;

pub struct Clipboard {
	#[cfg_attr(
		all(target_arch = "wasm32", not(feature = "unstable_write")),
		allow(dead_code)
	)]
	internal: platform::Clipboard,
}

impl Clipboard {
	pub fn new<T: ClipboardHandler>(display_handle: &dyn HasDisplayHandle, handler: T) -> Self {
		let internal = platform::Clipboard::new(display_handle, handler);
		Self { internal }
	}

	#[cfg(not(target_arch = "wasm32"))]
	pub fn request_data(&self) {
		self.internal.request_data();
	}

	#[cfg(feature = "unstable_write")]
	pub fn write(&self, data: ClipboardData) {
		self.internal.write(data);
	}
}
