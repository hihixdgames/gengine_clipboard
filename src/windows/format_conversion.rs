use windows::{
	Win32::System::DataExchange::{GetClipboardFormatNameW, RegisterClipboardFormatW},
	core::PCWSTR,
};

pub(super) fn get_format_name(format: u32) -> String {
	// These are the standart formats from:
	// https://learn.microsoft.com/en-us/windows/win32/dataxchg/standard-clipboard-formats
	match format {
		2 => "CF_BITMAP",
		8 => "CF_DIB",
		17 => "CF_DIBV5",
		5 => "CF_DIF",
		0x0082 => "CF_DSPBITMAP",
		0x008E => "CF_DSPENHMETAFILE",
		0x0083 => "CF_DSPMETAFILEPICT",
		0x0081 => "CF_DSPTEXT",
		14 => "CF_ENHMETAFILE",
		0x0300 => "CF_GDIOBJFIRST",
		0x03FF => "CF_GDIOBJLAST",
		15 => "CF_HDROP",
		16 => "CF_LOCALE",
		3 => "CF_METAFILEPICT",
		7 => "CF_OEMTEXT",
		0x0080 => "CF_OWNERDISPLAY",
		9 => "CF_PALETTE",
		10 => "CF_PENDATA",
		0x0200 => "CF_PRIVATEFIRST",
		0x02FF => "CF_PRIVATELAST",
		11 => "CF_RIFF",
		4 => "CF_SYLK",
		1 => "CF_TEXT",
		6 => "CF_TIFF",
		13 => "CF_UNICODETEXT",
		12 => "CF_WAVE",
		_ => {
			// This is not a standard format so we need to get the custom name.
			// Yes windows is a bit stupid, we just assume that this is enough.
			let mut data: [u16; 80] = [0; 80];
			let size = unsafe { GetClipboardFormatNameW(format, &mut data) };

			let mime_type = if size == 0 {
				String::from("unknown")
			} else {
				String::from_utf16_lossy(&data[0..(size as usize)])
			};

			return mime_type;
		}
	}
	.to_string()
}

pub(super) fn get_format_code(name: &str) -> u32 {
	// These are the standart formats from:
	// https://learn.microsoft.com/en-us/windows/win32/dataxchg/standard-clipboard-formats
	match name {
		"CF_BITMAP" => 2,
		"CF_DIB" => 8,
		"CF_DIBV5" => 17,
		"CF_DIF" => 5,
		"CF_DSPBITMAP" => 0x0082,
		"CF_DSPENHMETAFILE" => 0x008E,
		"CF_DSPMETAFILEPICT" => 0x0083,
		"CF_DSPTEXT" => 0x0081,
		"CF_ENHMETAFILE" => 14,
		"CF_GDIOBJFIRST" => 0x0300,
		"CF_GDIOBJLAST" => 0x03FF,
		"CF_HDROP" => 15,
		"CF_LOCALE" => 16,
		"CF_METAFILEPICT" => 3,
		"CF_OEMTEXT" => 7,
		"CF_OWNERDISPLAY" => 0x0080,
		"CF_PALETTE" => 9,
		"CF_PENDATA" => 10,
		"CF_PRIVATEFIRST" => 0x0200,
		"CF_PRIVATELAST" => 0x02FF,
		"CF_RIFF" => 11,
		"CF_SYLK" => 4,
		"CF_TEXT" => 1,
		"CF_TIFF" => 6,
		"CF_UNICODETEXT" => 13,
		"CF_WAVE" => 12,
		_ => {
			let mut utf16: Vec<u16> = name.encode_utf16().collect();
			utf16.push(0_u16);
			unsafe { RegisterClipboardFormatW(PCWSTR(utf16.as_ptr())) }
		}
	}
}
