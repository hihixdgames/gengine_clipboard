use sctk::data_device_manager::data_offer::SelectionOffer;
use std::io::Read;

use crate::{ClipboardError, PasteDataAccess};

pub(super) struct WaylandPasteDataAccess {
	selection: SelectionOffer,
}

impl WaylandPasteDataAccess {
	pub fn new(selection: SelectionOffer) -> Self {
		Self { selection }
	}
}

impl PasteDataAccess for WaylandPasteDataAccess {
	fn get_data(&mut self, mime_type: &str) -> Result<Vec<u8>, ClipboardError> {
		let mut read_pipe = match self.selection.receive(mime_type.to_string()) {
			Ok(read_pipe) => read_pipe,
			_ => {
				return Err(ClipboardError::Unknown(
					"selection does not want to give after offering wayland".to_string(),
				));
			}
		};

		let mut buffer = Vec::new();
		if read_pipe.read_to_end(&mut buffer).is_err() {
			return Err(ClipboardError::Unknown(
				"Failed to read clipboard content".to_string(),
			));
		}

		Ok(buffer)
	}
}
