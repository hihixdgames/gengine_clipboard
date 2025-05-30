use gengine_clipboard::{Clipboard, ClipboardEvent};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

fn main() {
	let success_flag = Arc::new(Mutex::new(false));
	let success_flag_cb = Arc::clone(&success_flag);

	let clipboard = Clipboard::new(move |event| match event {
		ClipboardEvent::Paste(_, _) => {
			println!("Got paste: {:?}", event);
			let mut flag = success_flag_cb.lock().unwrap();
			*flag = true;
		}
		ClipboardEvent::FailedPasteHandling(err) => {
			eprintln!("Error: {:?}", err);
		}
		_ => {}
	});

	println!("Watching clipboard...");

	{
		let mut flag = success_flag.lock().unwrap();
		*flag = false;
	}
	println!("Calling copy() (internal retry up to 5 times)");
	clipboard.get_data();

	for _ in 0..6 {
		thread::sleep(Duration::from_secs(1));
		if *success_flag.lock().unwrap() {
			break;
		}
	}
}
