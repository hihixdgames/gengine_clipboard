pub mod atoms;
pub mod paste_data_access;

use std::sync::mpsc::{self, Sender};
use std::thread::{self, JoinHandle};

use crate::platform::x11::paste_data_access::ConnectionHandler;
use crate::{ClipboardEvent, ClipboardEventSource, ClipboardHandler};

use raw_window_handle::HasDisplayHandle;
#[allow(unused_imports)]
use x11rb::protocol::xproto::{
	Atom, ConnectionExt, CreateWindowAux, EventMask, GetPropertyReply, Window, WindowClass,
	create_window, get_selection_owner, intern_atom,
};

use crate::InternalClipboard;

enum ThreadCommand {
	GetData,
	Exit,
}

pub struct X11Clipboard {
	sender: Sender<ThreadCommand>,
	join_handle: Option<JoinHandle<()>>,
}

impl InternalClipboard for X11Clipboard {
	fn new<T: ClipboardHandler>(_display_handle: &dyn HasDisplayHandle, mut handler: T) -> Self {
		let (sender, receiver) = mpsc::channel();
		let join_handle = Some(thread::spawn(move || {
			let connection = ConnectionHandler::new();
			let mut event_conut = 0;

			for command in receiver {
				match command {
					ThreadCommand::GetData => {
						let source = ClipboardEventSource { value: event_conut };
						event_conut += 1;

						handler.handle_event(ClipboardEvent::StartedPasteHandling { source });

						let mut data = match connection.get_data_access() {
							Ok(data_access) => data_access,
							Err(error) => {
								handler.handle_event(ClipboardEvent::FailedPasteHandling {
									source,
									error,
								});
								continue;
							}
						};

						handler.handle_event(ClipboardEvent::PasteResult {
							source,
							data: &mut data,
						});
					}
					ThreadCommand::Exit => break,
				}
			}
		}));

		X11Clipboard {
			sender,
			join_handle,
		}
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
