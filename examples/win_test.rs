use gengine_clipboard::{Clipboard, ClipboardEvent};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

fn main() {
	let pair = Arc::new((Mutex::new(false), Condvar::new()));
	let pair_cb = Arc::clone(&pair);

	let _clipboard = Clipboard::new(move |event| match event {
		ClipboardEvent::Paste(_, _) => {
			println!("Got paste: {:?}", event);
			let (lock, cvar) = &*pair_cb;
			let mut success = lock.lock().unwrap();
			*success = true;
			cvar.notify_one();
		}
		ClipboardEvent::FailedPasteHandling(err) => {
			eprintln!("Error: {:?}", err);
		}
		_ => {}
	});

	println!("Watching clipboard...");

	_clipboard.get_data();

	let (lock, cvar) = &*pair;
	let mut success = lock.lock().unwrap();
	let timeout = Duration::from_secs(6);
	let start = Instant::now();

	while !*success {
		let elapsed = start.elapsed();
		if elapsed >= timeout {
			eprintln!("Timeout waiting for clipboard data.");
			break;
		}
		let remaining = timeout - elapsed;
		let (s, wait_result) = cvar.wait_timeout(success, remaining).unwrap();
		success = s;
		if wait_result.timed_out() && !*success {
			eprintln!("Timeout waiting for clipboard data.");
			break;
		}
	}

	if *success {
		println!("Clipboard data successfully handled.");
	} else {
		eprintln!("Clipboard data was not received.");
	}
}
