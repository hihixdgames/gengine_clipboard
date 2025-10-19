mod clipboard_error;

pub use clipboard_error::*;
use raw_window_handle::HasDisplayHandle;

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

#[cfg(not(target_arch = "wasm32"))]
pub trait ClipboardConfig: Send + Sized + 'static {
	type ClipboardData: Send;

	fn callback(&mut self, event: ClipboardEvent<Self::ClipboardData>);

	fn resolve_paste_data(
		mime_types: Vec<String>,
		data_access: &mut impl PasteDataAccess,
	) -> Result<Self::ClipboardData, ClipboardError>;
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug)]
pub enum ClipboardEvent<T: Send> {
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

#[cfg(target_arch = "wasm32")]
pub trait ClipboardConfig: Sized + 'static {
	fn callback(&mut self, event: ClipboardEvent<Self::ClipboardData>);

	fn resolve_paste_data(
		mime_types: Vec<String>,
		data_access: &mut impl PasteDataAccess,
	) -> Result<Self::ClipboardData, ClipboardError>;
}

#[cfg(target_arch = "wasm32")]
pub enum ClipboardEvent<T> {
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
#[cfg_attr(target_arch = "wasm32", path = "wasm.rs")]
mod platform;

pub struct Clipboard<T: ClipboardConfig> {
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
