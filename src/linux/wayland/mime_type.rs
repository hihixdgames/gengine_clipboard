#[derive(Debug, Clone, Copy)]
pub enum MimeType {
	/// image/gif
	ImageGif,
	/// image/png
	ImagePng,
	/// image/bmp
	ImageBmp,
	/// image/ico
	ImageIco,
	/// image/jpeg
	ImageJpeg,
	/// image/tiff
	ImageTiff,
	/// image/webp
	ImageWebp,
	/// text/html
	TexteHtml,
	/// text/uri-list
	TextUriList,
	/// text/plain;charset=utf-8
	TextPlainUtf8,
	/// UTF8_STRING
	Utf8String,
	/// text/plain
	TextPlain,
}

impl MimeType {
	pub const DEFAULT_TARGETS: &'static [MimeType; 11] = &[
		Self::ImageGif,
		Self::ImagePng,
		Self::ImageJpeg,
		Self::ImageWebp,
		Self::ImageBmp,
		Self::ImageTiff,
		Self::ImageIco,
		Self::TextUriList,
		Self::TextPlainUtf8,
		Self::Utf8String,
		Self::TextPlain,
	];

	pub fn as_str(&self) -> &str {
		match self {
			Self::ImageGif => "image/gif",
			Self::ImagePng => "image/png",
			Self::ImageBmp => "image/bmp",
			Self::ImageIco => "image/ico",
			Self::ImageJpeg => "image/jpeg",
			Self::ImageTiff => "image/tiff",
			Self::ImageWebp => "image/webp",
			Self::TexteHtml => "text/html",
			Self::TextUriList => "text/uri-list",
			Self::TextPlainUtf8 => "text/plain;charset=utf-8",
			Self::Utf8String => "UTF8_STRING",
			Self::TextPlain => "text/plain",
		}
	}

	pub fn select(targets: &[MimeType], offers: &[String]) -> Option<MimeType> {
		for target in targets {
			for offer in offers {
				if target.as_str() == offer {
					return Some(*target);
				}
			}
		}

		None
	}

	pub fn is_string(&self) -> bool {
		matches!(
			self,
			Self::TextPlainUtf8 | Self::Utf8String | Self::TextPlain
		)
	}
}
