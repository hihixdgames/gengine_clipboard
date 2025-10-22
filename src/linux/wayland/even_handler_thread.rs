use std::{
	sync::mpsc::{self, Sender},
	thread::{self, JoinHandle},
};

use crate::{
	ClipboardError, ClipboardEvent, ClipboardEventSource, ClipboardHandler,
	platform::wayland::paste_data_access::WaylandPasteDataAccess,
};

enum HandlerThreadCommand {
	StartedPasteHandling {
		source: ClipboardEventSource,
	},
	FailedPasteHandling {
		source: ClipboardEventSource,
		error: ClipboardError,
	},
	PasteResult {
		source: ClipboardEventSource,
		data: WaylandPasteDataAccess,
	},
	Exit,
}

pub struct HandlerThread {
	handle: Option<JoinHandle<()>>,
	sender: Sender<HandlerThreadCommand>,
}

impl HandlerThread {
	pub fn new<T: ClipboardHandler>(mut handler: T) -> Self {
		let (sender, receiver) = mpsc::channel();
		let handle = thread::spawn(move || {
			for event in receiver {
				use HandlerThreadCommand::*;
				match event {
					StartedPasteHandling { source } => {
						handler.handle_event(ClipboardEvent::StartedPasteHandling { source })
					}
					FailedPasteHandling { source, error } => {
						handler.handle_event(ClipboardEvent::FailedPasteHandling { source, error })
					}
					PasteResult { source, mut data } => {
						handler.handle_event(ClipboardEvent::PasteResult {
							source,
							data: &mut data,
						});
					}
					Exit => {
						return;
					}
				}
			}
		});

		Self {
			handle: Some(handle),
			sender,
		}
	}

	pub fn started_paste_handling(&self, source: ClipboardEventSource) {
		let _ = self
			.sender
			.send(HandlerThreadCommand::StartedPasteHandling { source });
	}

	pub fn failed_paste_handling(&self, error: ClipboardError, source: ClipboardEventSource) {
		let _ = self
			.sender
			.send(HandlerThreadCommand::FailedPasteHandling { source, error });
	}

	pub fn paste_result(&self, data: WaylandPasteDataAccess, source: ClipboardEventSource) {
		let _ = self
			.sender
			.send(HandlerThreadCommand::PasteResult { source, data });
	}
}

impl Drop for HandlerThread {
	fn drop(&mut self) {
		let _ = self.sender.send(HandlerThreadCommand::Exit);
		if let Some(handle) = self.handle.take() {
			let _ = handle.join();
		}
	}
}
