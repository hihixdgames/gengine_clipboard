use js_sys::Uint8Array;

use crate::{ClipboardError, internal::InternalDataAccess};

pub struct WasmDataAccess {
	mime_types: Vec<String>,
	data: Vec<(String, Uint8Array)>,
}

impl WasmDataAccess {
	pub fn new(mime_types: Vec<String>, data: Vec<(String, Uint8Array)>) -> Self {
		Self { mime_types, data }
	}
}

impl InternalDataAccess for WasmDataAccess {
	fn mime_types(&self) -> &[String] {
		&self.mime_types
	}

	fn get_raw_data(&self, mime_type: &str) -> Result<Vec<u8>, crate::ClipboardError> {
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
