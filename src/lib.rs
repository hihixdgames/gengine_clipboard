mod clipboard_error;
mod internal;

pub use clipboard_error::*;
use internal::{InternalClipboard, InternalDataAccess};
use raw_window_handle::HasDisplayHandle;

#[cfg(not(target_arch = "wasm32"))]
pub trait WasmOrSend: Send {}

#[cfg(not(target_arch = "wasm32"))]
impl<T: Send> WasmOrSend for T {}

#[cfg(target_arch = "wasm32")]
pub trait WasmOrSend {}

#[cfg(target_arch = "wasm32")]
impl<T> WasmOrSend for T {}

pub trait WriteToClipboard {
	fn viable_conversions(&self) -> Vec<String>;

	fn convert_to(&self, mime_type: &str) -> Option<Vec<u8>>;
}

pub trait ReadFromClipboard: Sized {
	fn is_available(mime_types: &[String]) -> bool;

	fn read(data: &mut DataAccess) -> Option<Self>;
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
		data: &'a DataAccess,
		source: ClipboardEventSource,
	},
}

pub trait ClipboardHandler: WasmOrSend + Sized + 'static {
	fn handle_event(&mut self, event: ClipboardEvent<'_>);
}

#[cfg_attr(target_os = "linux", path = "linux/mod.rs")]
#[cfg_attr(target_os = "windows", path = "windows/mod.rs")]
#[cfg_attr(target_arch = "wasm32", path = "wasm/mod.rs")]
mod platform;

pub struct DataAccess {
	internal: platform::DataAccess,
}

impl DataAccess {
	pub fn raw_types(&self) -> &[String] {
		<platform::DataAccess as InternalDataAccess>::mime_types(&self.internal)
	}

	pub fn get_raw_data(&self, mime_type: &str) -> Result<Vec<u8>, ClipboardError> {
		<platform::DataAccess as InternalDataAccess>::get_raw_data(&self.internal, mime_type)
	}

	pub fn is_available<T: ReadFromClipboard>(&self) -> bool {
		T::is_available(self.raw_types())
	}

	pub fn get_data<T: ReadFromClipboard>(&mut self) -> Option<T> {
		T::read(self)
	}
}

pub struct Clipboard {
	#[cfg_attr(
		all(target_arch = "wasm32", not(feature = "unstable_write")),
		allow(dead_code)
	)]
	internal: platform::Clipboard,
}

impl Clipboard {
	pub fn new<T: ClipboardHandler>(display_handle: &dyn HasDisplayHandle, handler: T) -> Self {
		let internal = <platform::Clipboard as InternalClipboard>::new(display_handle, handler);
		Self { internal }
	}

	#[cfg(not(target_arch = "wasm32"))]
	pub fn request_data(&self) {
		<platform::Clipboard as InternalClipboard>::request_data(&self.internal);
	}

	#[cfg(feature = "unstable_write")]
	pub fn write_data<T: WriteToClipboard>(&self, data: T) {
		self.internal.write(data);
	}
}
