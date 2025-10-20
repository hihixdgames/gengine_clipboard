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
	fn get_data(&mut self, mime_type: &str) -> Result<Vec<u8>, ClipboardError>;
}

/// This indicates the source from which a ClipboardEvent originates from.
///
/// These can be compared to check if two events come from the same source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClipboardEventSource {
	pub(crate) value: usize,
}

pub trait ClipboardConfig: WasmOrSend + Sized + 'static {
	type ClipboardData: WasmOrSend;

	fn callback(&mut self, event: ClipboardEvent<Self::ClipboardData>);

	fn resolve_paste_data(
		mime_types: Vec<String>,
		data_access: &mut impl PasteDataAccess,
	) -> Result<Self::ClipboardData, ClipboardError>;
}

#[derive(Debug)]
pub enum ClipboardEvent<T: WasmOrSend> {
	StartedPasteHandling {
		source: ClipboardEventSource,
	},
	FailedPasteHandling {
		source: ClipboardEventSource,
		error: ClipboardError,
	},
	PasteResult {
		source: ClipboardEventSource,
		data: T,
	},
}

trait InternalClipboard<T: ClipboardConfig> {
	fn new(display_handle: &dyn HasDisplayHandle, config: T) -> Self;

	#[cfg(not(target_arch = "wasm32"))]
	fn request_data(&self);

	#[cfg(feature = "unstable_write")]
	fn write(&self, data: ClipboardData);
}

#[cfg_attr(target_os = "linux", path = "linux/mod.rs")]
#[cfg_attr(target_os = "windows", path = "windows/mod.rs")]
#[cfg_attr(target_arch = "wasm32", path = "wasm/mod.rs")]
mod platform;

pub struct Clipboard<T: ClipboardConfig> {
	#[cfg_attr(
		all(target_arch = "wasm32", not(feature = "unstable_write")),
		allow(dead_code)
	)]
	internal: platform::Clipboard<T>,
}

impl<T: ClipboardConfig> Clipboard<T> {
	pub fn new(display_handle: &dyn HasDisplayHandle, config: T) -> Self {
		let internal = platform::Clipboard::new(display_handle, config);
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
