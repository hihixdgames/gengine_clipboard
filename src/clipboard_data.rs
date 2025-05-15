pub enum ClipboardData {
	Text(Text),
	Image(Image),
}

pub enum Text {
	Plain(String),
	HTML(String),
}

pub enum Image {
	GIF(Vec<u8>),
	PNG(Vec<u8>),
	JPEG(Vec<u8>),
	BMP(Vec<u8>),
	WEBP(Vec<u8>),
	ICO(Vec<u8>),
	TIFF(Vec<u8>),
}

