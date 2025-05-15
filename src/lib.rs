mod clipboard_data;
mod clipboard_error;

pub use clipboard_data::*;
pub use clipboard_error::*;

#[cfg(not(target_arch = "wasm32"))]
pub trait WasmOrSync: Sync {}
#[cfg(not(target_arch = "wasm32"))]
impl<T: Sync> WasmOrSync for T {}

#[cfg(target_arch = "wasm32")]
pub trait WasmOrSync {}
#[cfg(target_arch = "wasm32")]
impl<T> WasmOrSync for T {}

pub enum ClipboardEvent {
	StartedPasteHandling,
	FailedPasteHandling(ClipboardError),
	Paste(ClipboardData, Option<ClipboardError>),
}

pub struct Clipboard;

impl Clipboard {
	pub fn new<F: FnMut(ClipboardEvent) + WasmOrSync>(f: F) -> Self {
		todo!()
	}

	pub fn copy() {
		todo!()
	}

	pub fn write(&self, data: ClipboardData) {
		todo!()
	}
}
