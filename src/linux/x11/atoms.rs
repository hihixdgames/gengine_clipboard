use std::borrow::Cow;

use x11rb::{
	errors::ReplyError,
	protocol::xproto::{Atom, ConnectionExt},
	rust_connection::RustConnection,
};

#[allow(unused)]
pub struct AtomManager {
	/// "ATOM"
	pub atom: Atom,
	/// "CLIPBOARD"
	pub clipboard: Atom,
	/// "TARGETS"
	pub targets: Atom,
	/// "INCR" (for incremental clipboard transfers)
	pub incr: Atom,
}

impl AtomManager {
	pub fn new(conn: &RustConnection) -> Result<Self, ReplyError> {
		Ok(Self {
			atom: Self::get_atom(conn, b"ATOM")?,
			clipboard: Self::get_atom(conn, b"CLIPBOARD")?,
			targets: Self::get_atom(conn, b"TARGETS")?,
			incr: Self::get_atom(conn, b"INCR")?,
		})
	}

	pub fn get_atom(conn: &RustConnection, name: &[u8]) -> Result<Atom, ReplyError> {
		match conn.intern_atom(false, name) {
			Ok(atom) => Ok(atom.reply()?.atom),
			Err(error) => Err(ReplyError::ConnectionError(error)),
		}
	}

	pub fn get_name(conn: &RustConnection, atom: &Atom) -> Result<String, ReplyError> {
		let data = conn.get_atom_name(*atom)?;
		let reply = data.reply()?;
		if let Cow::Owned(string) = String::from_utf8_lossy(&reply.name) {
			Ok(string)
		} else {
			// Not owned means that it is valid.
			Ok(String::from_utf8(reply.name).unwrap())
		}
	}
}
