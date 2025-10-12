pub mod atoms;

use std::fs;
use std::path::Path;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};

use crate::{ClipboardBehaviour, ClipboardEvent};

use atoms::{AtomHolder, get_atom};
use image::{EncodableLayout, GenericImageView};
use raw_window_handle::HasDisplayHandle;
use x11rb::CURRENT_TIME;
use x11rb::connection::Connection;
use x11rb::protocol::Event;
#[allow(unused_imports)]
use x11rb::protocol::xproto::{
	Atom, ConnectionExt, CreateWindowAux, EventMask, GetPropertyReply, Window, WindowClass,
	create_window, get_selection_owner, intern_atom,
};
use x11rb::rust_connection::RustConnection;

use crate::{ClipboardData, ClipboardError, Image, InternalClipboard};

#[allow(non_upper_case_globals)]
const GIF87a: [u8; 6] = [0x47, 0x49, 0x46, 0x38, 0x37, 0x61];
#[allow(non_upper_case_globals)]
const GIF89a: [u8; 6] = [0x47, 0x49, 0x46, 0x38, 0x39, 0x61];

enum ThreadCommand {
	GetData,
	Write(ClipboardData),
	Exit,
}

struct ConnectionHandler {
	conn: RustConnection,
	window: Window,
	atoms: AtomHolder,
	property: Atom,
}

impl ConnectionHandler {
	pub fn new(name: &[u8]) -> ConnectionHandler {
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
			&CreateWindowAux::new(),
		);

		ConnectionHandler {
			property: get_atom(&conn, name).unwrap(),
			atoms: AtomHolder::new(&conn).unwrap(),
			conn,
			window,
		}
	}

	fn convert_selection(&mut self, type_: Atom) {
		self.conn
			.convert_selection(
				self.window,
				self.atoms.clipboard,
				type_,
				self.property,
				CURRENT_TIME,
			)
			.unwrap()
			.check()
			.unwrap();

		let _ = self.conn.flush();
		loop {
			let event = self.conn.wait_for_event().unwrap();
			if let Event::SelectionNotify(_) = event {
				return;
			}
		}
	}

	fn get_property(&mut self, type_: Atom) -> GetPropertyReply {
		use x11rb::protocol::xproto::AtomEnum;
		let initial = self
			.conn
			.get_property(false, self.window, self.property, type_, 0, 0)
			.unwrap()
			.reply()
			.unwrap();

		let incr_atom = self.atoms.incr;
		if initial.type_ == incr_atom {
			let _incr_size = u32::from_ne_bytes([
				initial.value.get(0).copied().unwrap_or(0),
				initial.value.get(1).copied().unwrap_or(0),
				initial.value.get(2).copied().unwrap_or(0),
				initial.value.get(3).copied().unwrap_or(0),
			]);
			println!("[X11Clipboard] INCR protocol detected: starting incremental transfer...");
			self.conn
				.delete_property(self.window, self.property)
				.unwrap();
			let mut data = Vec::new();
			loop {
				let event = self.conn.wait_for_event().unwrap();
				if let Event::PropertyNotify(e) = event {
					if e.state == x11rb::protocol::xproto::Property::NEW_VALUE {
						let chunk = self
							.conn
							.get_property(true, self.window, self.property, type_, 0, std::u32::MAX)
							.unwrap()
							.reply()
							.unwrap();
						if chunk.value.is_empty() {
							break;
						}
						data.extend_from_slice(&chunk.value);
					}
				}
			}
			println!(
				"[X11Clipboard] INCR transfer complete: received {} bytes",
				data.len()
			);
			GetPropertyReply {
				format: initial.format,
				sequence: initial.sequence,
				length: initial.length,
				type_,
				bytes_after: 0,
				value_len: data.len() as u32,
				value: data,
			}
		} else {
			let len = initial.bytes_after;
			self.conn
				.get_property(false, self.window, self.property, type_, 0, len)
				.unwrap()
				.reply()
				.unwrap()
		}
	}

	pub fn get_content(&mut self) -> ClipboardData {
		// Type Selection
		self.convert_selection(self.atoms.targets);
		let type_atoms = self.get_property(self.atoms.atom);
		let type_atoms: Vec<u32> = type_atoms.value32().unwrap().collect();

		#[cfg(feature = "debug")]
		{
			println!("Found TARGETS:");
			for atom in type_atoms.iter() {
				let name = self
					.conn
					.get_atom_name(*atom)
					.unwrap()
					.reply()
					.unwrap()
					.name;
				println!("    {}", String::from_utf8(name).unwrap());
			}
			println!()
		}

		if type_atoms.contains(&self.atoms.gif) {
			self.convert_selection(self.atoms.gif);
			let data = self.get_property(self.atoms.gif);
			ClipboardData::Image(Image::GIF(data.value))
		} else if let Some(image_type) = self.atoms.is_image(&type_atoms) {
			#[cfg(feature = "follow_html_img")]
			{
				if type_atoms.contains(&self.atoms.html) {
					self.convert_selection(self.atoms.html);
					let html_data = self.get_property(self.atoms.html);
					let html_data = String::from_utf8(html_data.value).unwrap_or_default();
					let image_link = {
						let start = match html_data.find("src=\"") {
							Some(start) => &html_data[(start + 5)..],
							None => {
								return self
									.return_image(image_type, Some(ClipboardError::ReadFailed));
							}
						};
						match start.find('"') {
							Some(end) => &start[..end],
							None => {
								return self
									.return_image(image_type, Some(ClipboardError::ReadFailed));
							}
						}
					};

					let data = match reqwest::blocking::get(image_link) {
						Ok(result) => match result.bytes() {
							Ok(bytes) => bytes,
							Err(_) => {
								return self
									.return_image(image_type, Some(ClipboardError::ReadFailed));
							}
						},
						Err(_) => {
							return self.return_image(image_type, Some(ClipboardError::ReadFailed));
						}
					};

					if data.starts_with(&GIF87a) || data.starts_with(&GIF89a) {
						ClipboardData::Image(Image::GIF(data.as_bytes().to_vec()))
					} else {
						match image::load_from_memory(&data) {
							Ok(image) => {
								let (_width, _height) = image.dimensions();
								let _rgba8 = image.to_rgba8().to_vec();
								ClipboardData::Image(Image::PNG(data.to_vec()))
							}
							Err(_) => {
								self.return_image(image_type, Some(ClipboardError::ReadFailed))
							}
						}
					}
				} else {
					self.return_image(image_type, None)
				}
			}
			#[cfg(not(feature = "follow_html_img"))]
			{
				self.return_image(image_type, None)
			}
		} else if type_atoms.contains(&self.atoms.path) {
			self.convert_selection(self.atoms.path);
			let data = self.get_property(self.atoms.path);
			let data = String::from_utf8(data.value).unwrap_or_default();
			let file_uri = match data.split_once(char::is_whitespace) {
				Some((file_uri, _)) => file_uri,
				None => &data,
			};
			if file_uri.starts_with("file://") {
				let data = match fs::read(Path::new(file_uri.strip_prefix("file://").unwrap())) {
					Ok(data) => data,
					Err(_) => {
						return ClipboardData::Text(crate::Text::Plain(format!(
							"{:?}",
							ClipboardError::ReadFailed
						)));
					}
				};

				return if data.starts_with(&GIF87a) || data.starts_with(&GIF89a) {
					ClipboardData::Image(Image::GIF(data))
				} else {
					match image::load_from_memory(&data) {
						Ok(image) => {
							let (width, height) = image.dimensions();
							let rgba8 = image.to_rgba8().to_vec();
							ClipboardData::Image(Image::PNG(data.to_vec()))
						}
						Err(_) => ClipboardData::Text(crate::Text::Plain(format!(
							"{:?}",
							ClipboardError::FormatNotAvailable
						))),
					}
				};
			}

			#[cfg(feature = "follow_links")]
			if file_uri.starts_with("https://") || file_uri.starts_with("http://") {
				let data = match reqwest::blocking::get(file_uri) {
					Ok(result) => match result.bytes() {
						Ok(bytes) => bytes,
						Err(_) => {
							return ClipboardData::Text(crate::Text::Plain(format!(
								"{:?}",
								ClipboardError::ReadFailed
							)));
						}
					},
					Err(_) => {
						return ClipboardData::Text(crate::Text::Plain(format!(
							"{:?}",
							ClipboardError::ReadFailed
						)));
					}
				};

				return if data.starts_with(&GIF87a) || data.starts_with(&GIF89a) {
					ClipboardData::Image(Image::GIF(data.as_bytes().to_vec()))
				} else {
					match image::load_from_memory(&data) {
						Ok(_image) => ClipboardData::Image(Image::PNG(data.to_vec())),
						Err(_) => ClipboardData::Text(crate::Text::Plain(format!(
							"{:?}",
							ClipboardError::FormatNotAvailable
						))),
					}
				};
			}

			return {
				ClipboardData::Text(crate::Text::Plain(format!(
					"{:?}",
					ClipboardError::FormatNotAvailable
				)))
			};
		} else if type_atoms.contains(&self.atoms.text) {
			self.convert_selection(self.atoms.text);
			let data = self.get_property(self.atoms.text);
			let string = match String::from_utf8(data.value) {
				Ok(text) => text,
				Err(_) => {
					return ClipboardData::Text(crate::Text::Plain(format!(
						"{:?}",
						ClipboardError::Utf16ConversionFailed
					)));
				}
			};

			#[cfg(all(feature = "x11", feature = "follow_links"))]
			{
				if string.starts_with("https://") || string.starts_with("http://") {
					let data = match reqwest::blocking::get(&string) {
						Ok(result) => match result.bytes() {
							Ok(bytes) => bytes,
							Err(_) => {
								return ClipboardData::Text(crate::Text::Plain(format!(
									"{:?}",
									ClipboardError::ReadFailed
								)));
							}
						},
						Err(_) => {
							return ClipboardData::Text(crate::Text::Plain(format!(
								"{:?}",
								ClipboardError::ReadFailed
							)));
						}
					};

					if data.starts_with(&GIF87a) || data.starts_with(&GIF89a) {
						return ClipboardData::Image(Image::GIF(data.as_bytes().to_vec()));
					} else {
						match image::load_from_memory(&data) {
							Ok(image) => {
								let (width, height) = image.dimensions();
								let rgba8 = image.to_rgba8().to_vec();
								return ClipboardData::Image(Image::PNG(data.to_vec()));
							}
							Err(_) => {
								return ClipboardData::Text(crate::Text::Plain(format!(
									"{:?}",
									ClipboardError::ReadFailed
								)));
							}
						}
					}
				}
			}
			ClipboardData::Text(crate::Text::Plain(string))
		} else {
			ClipboardData::Text(crate::Text::Plain(format!(
				"{:?}",
				ClipboardError::FormatNotAvailable
			)))
		}
	}

	fn return_image(&mut self, image_type: Atom, _err: Option<ClipboardError>) -> ClipboardData {
		self.convert_selection(image_type);
		let data = self.get_property(image_type);
		match image::load_from_memory(&data.value) {
			Ok(_image) => ClipboardData::Image(Image::PNG(data.value)),
			Err(_) => ClipboardData::Text(crate::Text::Plain(format!(
				"{:?}",
				ClipboardError::FormatNotAvailable
			))),
		}
	}
}

fn spawn_thread<F: FnMut(ClipboardEvent) + Send + 'static>(
	receiver: Receiver<ThreadCommand>,
	mut callback: F,
) -> JoinHandle<()> {
	thread::spawn(move || {
		for command in receiver {
			match command {
				ThreadCommand::GetData => {
					let mut handler = ConnectionHandler::new(b"GENGINEL_CLIPBOARD");
					let data = handler.get_content();
					callback(ClipboardEvent::Paste(data, None));
				}
				ThreadCommand::Exit => break,
				_ => {}
			}
		}
	})
}

pub struct X11Clipboard {
	sender: Sender<ThreadCommand>,
	join_handle: Option<JoinHandle<()>>,
}

impl X11Clipboard {
	pub fn new<F: FnMut(ClipboardEvent) + Send + 'static>(callback: F) -> Self {
		let (sender, receiver) = mpsc::channel();
		let join_handle = Some(spawn_thread(receiver, callback));
		X11Clipboard {
			sender,
			join_handle,
		}
	}

	pub fn get_data(&self) {
		let _ = self.sender.send(ThreadCommand::GetData);
	}
}

impl<T: ClipboardBehaviour> InternalClipboard<T> for X11Clipboard {
	fn new(display_handle: &dyn HasDisplayHandle, behaviour: T) -> Self {
		todo!()
		//X11Clipboard::new(callback)
	}

	fn request_data(&self) {
		let _ = self.sender.send(ThreadCommand::GetData);
	}

	#[cfg(feature = "unstable_write")]
	fn write(&self, _data: ClipboardData) {
		unimplemented!("Clipboard write not implemented yet.");
	}
}

impl Drop for X11Clipboard {
	fn drop(&mut self) {
		let _ = self.sender.send(ThreadCommand::Exit);
		if let Some(handle) = self.join_handle.take() {
			let _ = handle.join();
		}
	}
}
