use x11rb::{
	errors::ReplyError,
	protocol::xproto::{Atom, ConnectionExt},
	rust_connection::RustConnection,
};

#[allow(unused)]
pub struct AtomHolder {
	/// "ATOM"
	pub atom: Atom,
	/// "CLIPBOARD"
	pub clipboard: Atom,
	/// "image/gif"
	pub gif: Atom,
	/// "image/bmp"
	/// "image/png"
	/// "image/ico"
	/// "image/jpeg"
	/// "image/tiff"
	/// "image/webp",
	pub image: [Atom; 6],
	/// text/html
	pub html: Atom,
	/// "text/uri-list"
	pub path: Atom,
	/// "UTF8_STRING"
	pub text: Atom,
	/// "TARGETS"
	pub targets: Atom,
	/// "INCR" (for incremental clipboard transfers)
	pub incr: Atom,
}

pub fn get_atom(conn: &RustConnection, name: &[u8]) -> Result<Atom, ReplyError> {
	match conn.intern_atom(false, name) {
		Ok(atom) => Ok(atom.reply()?.atom),
		Err(error) => Err(ReplyError::ConnectionError(error)),
	}
}

impl AtomHolder {
	pub fn new(conn: &RustConnection) -> Result<Self, ReplyError> {
		Ok(Self {
			atom: get_atom(conn, b"ATOM")?,
			clipboard: get_atom(conn, b"CLIPBOARD")?,
			gif: get_atom(conn, b"image/gif")?,
			image: [
				get_atom(conn, b"image/bmp")?,
				get_atom(conn, b"image/png")?,
				get_atom(conn, b"image/ico")?,
				get_atom(conn, b"image/jpeg")?,
				get_atom(conn, b"image/tiff")?,
				get_atom(conn, b"image/webp")?,
			],
			html: get_atom(conn, b"text/html")?,
			path: get_atom(conn, b"text/uri-list")?,
			text: get_atom(conn, b"UTF8_STRING")?,
			targets: get_atom(conn, b"TARGETS")?,
			incr: get_atom(conn, b"INCR")?,
		})
	}

	pub fn is_image(&self, type_atoms: &[u32]) -> Option<Atom> {
		self.image
			.iter()
			.find(|a| type_atoms.contains(&(**a)))
			.copied()
	}
}
