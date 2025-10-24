mod wayland;
mod x11;

use std::rc::Rc;

use raw_window_handle::{HasDisplayHandle, RawDisplayHandle};

use crate::{
	ClipboardHandler, InternalClipboard,
	internal::InternalDataAccess,
	platform::{
		wayland::{WaylandClipboard, WaylandPasteDataAccess},
		x11::{ConnectionHandler, X11Clipboard},
	},
};

pub enum DataAccess {
	X11 {
		conn: Rc<ConnectionHandler>,
		mime_types: Vec<String>,
	},
	Wayland(WaylandPasteDataAccess),
}

impl InternalDataAccess for DataAccess {
	fn mime_types(&self) -> &[String] {
		match self {
			DataAccess::X11 { mime_types, .. } => mime_types,
			DataAccess::Wayland(data_access) => data_access.mime_types(),
		}
	}

	fn get_raw_data(&self, mime_type: &str) -> Result<Vec<u8>, crate::ClipboardError> {
		match self {
			DataAccess::X11 { conn, .. } => conn.get_raw_data(mime_type),
			DataAccess::Wayland(data_access) => data_access.get_raw_data(mime_type),
		}
	}
}

pub enum Internal {
	X11(X11Clipboard),
	Wayland(WaylandClipboard),
}

pub struct Clipboard {
	internal: Internal,
}

impl InternalClipboard for Clipboard {
	fn new<T: ClipboardHandler>(display_handle: &dyn HasDisplayHandle, handler: T) -> Self {
		let handle = display_handle.display_handle().unwrap();
		match handle.as_raw() {
			RawDisplayHandle::Xlib(_) | RawDisplayHandle::Xcb(_) => {
				println!("Using X11");
				Clipboard {
					internal: Internal::X11(X11Clipboard::new(display_handle, handler)),
				}
			}
			RawDisplayHandle::Wayland(_) => {
				println!("Using wayland!");
				Clipboard {
					internal: Internal::Wayland(WaylandClipboard::new(display_handle, handler)),
				}
			}
			_ => panic!(),
		}
	}

	fn request_data(&self) {
		match &self.internal {
			Internal::X11(internal) => {
				internal.request_data();
			}
			Internal::Wayland(internal) => {
				internal.request_data();
			}
		}
	}

	#[cfg(feature = "unstable_write")]
	fn write(&self, data: crate::ClipboardData) {
		todo!()
	}
}
