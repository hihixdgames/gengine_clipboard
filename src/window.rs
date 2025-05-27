use std::{
	sync::mpsc::{self, Receiver, Sender},
	thread::{self, JoinHandle},
	time::Duration,
};

use crate::{ClipboardData, ClipboardError, ClipboardEvent, InternalClipboard};

const N_RETRIES: usize = 5;
const TIME_BETWEEN_RETRIES: Duration = Duration::from_millis(100);

enum ThreadCommand {
	GetData,
	Write(ClipboardData),
	Exit,
}

pub struct WindowsClipboard {
	sender: Sender<ThreadCommand>,
	join_handle: Option<JoinHandle<()>>,
}

impl InternalClipboard for WindowsClipboard {
	fn new<F: FnMut(crate::ClipboardEvent) + crate::WasmOrSend>(callback: F) -> Self {
		let (sender, receiver) = mpsc::channel();
		let join_handle = Some(spawn_thread(receiver, callback));
		Self {
			sender,
			join_handle,
		}
	}

	fn get_data(&self) {
		let _ = self.sender.send(ThreadCommand::GetData);
	}

	fn write(&self, data: ClipboardData) {
		let _ = self.sender.send(ThreadCommand::Write(data));
	}
}

fn spawn_thread<F: FnMut(crate::ClipboardEvent) + crate::WasmOrSend>(
	receiver: Receiver<ThreadCommand>,
	mut callback: F,
) -> JoinHandle<()> {
	thread::spawn(move || {
		for command in receiver {
			match command {
				ThreadCommand::GetData => {
					// In the line below try to get clipboard (I think this will be a result instead
					// of option)
					let mut clipboard: Option<()> = None;
					for _ in 0..N_RETRIES {
						thread::sleep(TIME_BETWEEN_RETRIES);
						// Retry to get clipboard and set to variable
					}

					// Will probably be is_err
					if clipboard.is_none() {
						callback(ClipboardEvent::FailedPasteHandling(ClipboardError::InUse));
					}

					// get data from clipboard and send via callback
				}
				ThreadCommand::Write(_data) => {
					todo!()
				}
				ThreadCommand::Exit => {
					return;
				}
			}
		}
	})
}

impl Drop for WindowsClipboard {
	fn drop(&mut self) {
		if let Ok(()) = self.sender.send(ThreadCommand::Exit) {
			let _ = self.join_handle.take().unwrap().join();
		}
	}
}
