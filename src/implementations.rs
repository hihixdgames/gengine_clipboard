use std::borrow::Cow;

use crate::ReadFromClipboard;

// const a: HashSet<String> = a(vec!["a", "b"]);

const TEXT_TYPES: [&str; 4] = [
	"text/plain;charset=utf-8",
	"UTF8_STRING",
	"text/plain",
	"CF_UNICODETEXT",
];

fn to_string(bytes: Vec<u8>) -> String {
	if let Cow::Owned(string) = String::from_utf8_lossy(&bytes) {
		string
	} else {
		// Not owned means that it is valid.
		String::from_utf8(bytes).unwrap()
	}
}

impl ReadFromClipboard for String {
	fn is_available(mime_types: &[&str]) -> bool {
		mime_types
			.iter()
			.any(|mime_type| TEXT_TYPES.contains(mime_type))
	}

	fn read(data: &crate::DataAccess) -> Option<Self> {
		if data.raw_types().contains(&TEXT_TYPES[0])
			&& let Ok(bytes) = data.get_raw_data(TEXT_TYPES[0])
		{
			return Some(to_string(bytes));
		}

		if let Some(bytes) = data.get_first_success(&TEXT_TYPES[1..]) {
			#[cfg(not(target_os = "windows"))]
			return Some(to_string(bytes));

			#[cfg(target_os = "windows")]
			{
				let data: Vec<u16> = bytes
					.chunks(2)
					.map(|v| ((v[1] as u16) << 8) | v[0] as u16)
					.collect();
				return Some(String::from_utf16_lossy(&data));
			}
		}

		None
	}
}
