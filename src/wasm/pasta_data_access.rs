use js_sys::Uint8Array;

use crate::{ClipboardError, PasteDataAccess};

pub struct WasmDataAccess {
	data: Vec<(String, Uint8Array)>,
}

impl WasmDataAccess {
	pub fn new(data: Vec<(String, Uint8Array)>) -> Self {
		Self { data }
	}
}

impl PasteDataAccess for WasmDataAccess {
	fn get_data(&mut self, mime_type: &str) -> Result<Vec<u8>, crate::ClipboardError> {
		for (mime, data) in self.data.iter() {
			if mime == mime_type {
				let mut raw = vec![0; data.length() as usize];
				data.copy_to(&mut raw);
				return Ok(raw);
			}
		}

		Err(ClipboardError::FormatNotAvailable)
	}
}
