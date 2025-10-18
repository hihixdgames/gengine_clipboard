use std::time::{Duration, Instant};

use x11rb::{
	CURRENT_TIME,
	connection::Connection,
	protocol::{
		Event,
		xproto::{
			self, Atom, ConnectionExt, CreateWindowAux, Property, Window, WindowClass,
			create_window,
		},
	},
	rust_connection::RustConnection,
};

use crate::{ClipboardError, PasteDataAccess, platform::x11::atoms::AtomManager};

const TIMEOUT_LIMIT: Duration = Duration::from_secs(2);

pub struct X11DataAcess {
	conn: RustConnection,
	window: Window,
	atoms: AtomManager,
	property: Atom,
}

impl X11DataAcess {
	pub fn new() -> Self {
		let (conn, screen) = RustConnection::connect(None).unwrap();
		let screen = conn.setup().roots.get(screen).unwrap();

		let window = conn.generate_id().unwrap();

		let _ = create_window(
			&conn,
			x11rb::COPY_DEPTH_FROM_PARENT,
			window,
			screen.root,
			0,
			0,
			1,
			1,
			0,
			WindowClass::INPUT_OUTPUT,
			screen.root_visual,
			&CreateWindowAux::new().event_mask(xproto::EventMask::PROPERTY_CHANGE),
		);

		Self {
			window,
			atoms: AtomManager::new(&conn).unwrap(),
			property: AtomManager::get_atom(&conn, b"GENGINE CLIPBOARD RECEIVER").unwrap(),
			conn,
		}
	}

	fn get_selection(&self, mut target: Atom) -> Result<Vec<u8>, ClipboardError> {
		self.conn
			.convert_selection(
				self.window,
				self.atoms.clipboard,
				target,
				self.property,
				CURRENT_TIME,
			)
			.unwrap()
			.check()
			.unwrap();

		self.conn.flush().unwrap();

		// When requesting targets, we get a list of atoms
		if target == self.atoms.targets {
			target = self.atoms.atom
		}

		let mut last_event = Instant::now();
		let mut data = Vec::new();
		let mut incr = false;

		loop {
			if Instant::now() - last_event > TIMEOUT_LIMIT {
				return Err(ClipboardError::Timeout);
			}

			let event = self.conn.wait_for_event().unwrap();

			match event {
				Event::SelectionNotify(event) => {
					if event.selection != self.atoms.clipboard {
						continue;
					}

					let reply = self
						.conn
						.get_property(false, self.window, event.property, target, 0, 0)
						.unwrap()
						.reply()
						.unwrap();

					if reply.type_ == self.atoms.incr {
						incr = true;

						if let Some(Some(size)) = reply.value32().map(|mut values| values.next()) {
							data.reserve(size as usize);
						}

						self.conn
							.delete_property(self.window, event.property)
							.unwrap()
							.check()
							.unwrap();

						last_event = Instant::now();
						continue;
					} else if reply.type_ != target {
						return Err(ClipboardError::ForeignClipboardError);
					}

					let data_reply = self
						.conn
						.get_property(
							false,
							self.window,
							event.property,
							target,
							0,
							reply.bytes_after.div_ceil(4),
						)
						.unwrap()
						.reply()
						.unwrap();

					data.extend_from_slice(&data_reply.value);
					break;
				}
				Event::PropertyNotify(event) if incr => {
					if event.state != Property::NEW_VALUE {
						continue;
					}

					let reply = self
						.conn
						.get_property(
							true,
							self.window,
							self.property,
							target,
							0,
							// There seem to be problems, if we first ask for empty property to get byte count.
							// Therefore, we have the MAX value here.
							u32::MAX,
						)
						.unwrap()
						.reply()
						.unwrap();

					if reply.value.is_empty() {
						self.conn
							.delete_property(self.window, self.property)
							.unwrap()
							.check()
							.unwrap();
						break;
					}

					data.extend_from_slice(&reply.value);
					last_event = Instant::now();
				}
				_ => {}
			}
		}

		Ok(data)
	}

	pub fn get_mime_types(&self) -> Result<Vec<String>, ClipboardError> {
		let bytes = self.get_selection(self.atoms.targets)?;
		let (atoms, remainder) = bytes.as_chunks::<4>();
		if !remainder.is_empty() {
			return Err(ClipboardError::ForeignClipboardError);
		}

		let mut names = Vec::new();
		for atom in atoms {
			let atom = u32::from_ne_bytes(*atom);
			names.push(AtomManager::get_name(&self.conn, &atom).unwrap());
		}

		Ok(names)
	}
}

impl PasteDataAccess for X11DataAcess {
	fn get_data(&mut self, mime_type: &str) -> Result<Vec<u8>, ClipboardError> {
		let target = AtomManager::get_atom(&self.conn, mime_type.as_bytes()).unwrap();
		self.get_selection(target)
	}
}
