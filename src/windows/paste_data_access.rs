use std::time::{Duration, Instant};

use windows::Win32::{
	Foundation::HGLOBAL,
	System::{
		DataExchange::{
			CloseClipboard, CountClipboardFormats, EnumClipboardFormats, GetClipboardData,
			OpenClipboard,
		},
		Memory::{GlobalLock, GlobalSize, GlobalUnlock},
	},
};

use crate::{
	ClipboardError,
	internal::InternalDataAccess,
	platform::format_conversion::{get_format_code, get_format_name},
};

const TIMEOUT_LIMIT: Duration = Duration::from_secs(2);

pub struct WindowsDataAccess {
	mime_types: Vec<String>,
}

impl WindowsDataAccess {
	pub fn new() -> Result<Self, ClipboardError> {
		let start_time = Instant::now();
		loop {
			if let Ok(()) = unsafe { OpenClipboard(None) } {
				break;
			}

			if Instant::now() - start_time > TIMEOUT_LIMIT {
				return Err(ClipboardError::Timeout);
			}
		}

		let mime_types = Self::get_mime_types();
		Ok(WindowsDataAccess { mime_types })
	}

	/// # Warning
	///
	/// Only use when clipboard is opened.
	fn get_mime_types() -> Vec<String> {
		let n_types = unsafe { CountClipboardFormats() };

		let mut formats: Vec<u32> = Vec::new();
		loop {
			let previous = formats.last().copied().unwrap_or(0);
			let next = unsafe { EnumClipboardFormats(previous) };
			if next != 0 {
				formats.push(next);
			} else {
				break;
			}

			if formats.len() as i32 == n_types {
				break;
			}
		}

		formats
			.iter()
			.map(|format| get_format_name(*format))
			.collect()
	}
}

impl InternalDataAccess for WindowsDataAccess {
	fn mime_types(&self) -> &[String] {
		&self.mime_types
	}

	fn get_raw_data(&self, mime_type: &str) -> Result<Vec<u8>, ClipboardError> {
		let format = get_format_code(mime_type);
		let handle = match unsafe { GetClipboardData(format) } {
			Ok(handle) => handle,
			Err(error) => {
				return Err(ClipboardError::Unknown(format!(
					"Windows error code {} with message {}.",
					error.code(),
					error.message()
				)));
			}
		};

		// Not sure if this can happen
		if handle.is_invalid() {
			return Err(ClipboardError::ClipboardDataUnavailable);
		}

		let global = HGLOBAL(handle.0);
		let lock_ptr = unsafe { GlobalLock(global) };
		if lock_ptr.is_null() {
			return Err(ClipboardError::ClipboardDataUnavailable);
		}

		let size = unsafe { GlobalSize(global) };
		if size == 0 {
			return Err(ClipboardError::ClipboardDataUnavailable);
		}

		let data = unsafe { std::slice::from_raw_parts(lock_ptr as *const u8, size) };
		let data = data.to_vec();
		let _ = unsafe { GlobalUnlock(global) };

		Ok(data)
	}
}

impl Drop for WindowsDataAccess {
	fn drop(&mut self) {
		unsafe {
			let _ = CloseClipboard();
		}
	}
}
