mod format_conversion;
mod paste_data_access;

use crate::{
	ClipboardEvent, ClipboardEventSource, ClipboardHandler, InternalClipboard,
	platform::paste_data_access::WindowsDataAccess,
};
use std::{
	sync::mpsc::{self, Sender},
	thread::{self, JoinHandle},
};

enum ThreadCommand {
	GetData,
	Exit,
}

pub struct Clipboard {
	sender: Sender<ThreadCommand>,
	join_handle: Option<JoinHandle<()>>,
}

impl InternalClipboard for Clipboard {
	fn new<T: ClipboardHandler>(
		_display_handle: &dyn raw_window_handle::HasDisplayHandle,
		mut handler: T,
	) -> Self {
		let (sender, receiver) = mpsc::channel();
		let join_handle = Some(thread::spawn(move || {
			let mut event_conut = 0;

			for command in receiver {
				match command {
					ThreadCommand::GetData => {
						let source = ClipboardEventSource { value: event_conut };
						event_conut += 1;
						handler.handle_event(ClipboardEvent::StartedPasteHandling { source });

						let mut data_access = match WindowsDataAccess::new() {
							Ok(data_access) => data_access,
							Err(error) => {
								handler.handle_event(ClipboardEvent::FailedPasteHandling {
									source,
									error,
								});
								return;
							}
						};

						handler.handle_event(ClipboardEvent::PasteResult {
							data: &mut data_access,
							source,
						});
					}
					ThreadCommand::Exit => break,
				}
			}
		}));

		Self {
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

impl Drop for Clipboard {
	fn drop(&mut self) {
		let _ = self.sender.send(ThreadCommand::Exit);
		if let Some(handle) = self.join_handle.take() {
			let _ = handle.join();
		}
	}
}
