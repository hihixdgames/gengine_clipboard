use raw_window_handle::HasDisplayHandle;

use crate::{ClipboardError, ClipboardHandler};

pub(crate) trait InternalClipboard {
	fn new<T: ClipboardHandler>(display_handle: &dyn HasDisplayHandle, handler: T) -> Self;

	#[cfg(not(target_arch = "wasm32"))]
	fn request_data(&self);

	#[cfg(feature = "unstable_write")]
	fn write<T: WriteToClipboard>(&self, data: T);
}

pub(crate) trait InternalDataAccess {
	fn mime_types(&self) -> &[String];

	fn get_raw_data(&self, mime_type: &str) -> Result<Vec<u8>, ClipboardError>;
}
