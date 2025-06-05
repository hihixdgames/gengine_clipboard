use gengine_clipboard::{Clipboard, ClipboardData, ClipboardEvent, Image, Text};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

fn main() {
	let pair = Arc::new((Mutex::new(false), Condvar::new()));
	let pair_cb = Arc::clone(&pair);

	let _clipboard = Clipboard::new(move |event| {
		if let ClipboardEvent::Paste(data, _) = event {
			match data {
				ClipboardData::Text(Text::Plain(s)) => {
					println!("Text (plain): {}", s);
				}
				ClipboardData::Text(Text::HTML(html)) => {
					println!("Text (HTML): {}", html);
				}
				ClipboardData::Image(image) => {
					let (name, bytes) = match image {
						Image::PNG(d) => ("png.png", d),
						Image::JPEG(d) => ("jpeg.jpeg", d),
						Image::GIF(d) => ("gif.gif", d),
						Image::WEBP(d) => ("webp.webp", d),
						Image::BMP(d) => ("bmp.bmp", d),
						Image::ICO(d) => ("ico.ico", d),
						Image::TIFF(d) => ("tiff.tiff", d),
					};
					println!("Image detected: {}", name);
					let _ = std::fs::write(name, &bytes);
				}
			}

			let (lock, cvar) = &*pair_cb;
			let mut done = lock.lock().unwrap();
			*done = true;
			cvar.notify_one();
		}
	});
	println!("Watching clipboard...");

	_clipboard.get_data();

	let (lock, cvar) = &*pair;
	let mut done = lock.lock().unwrap();
	let timeout = Duration::from_secs(6);
	let start = Instant::now();

	while !*done {
		let elapsed = start.elapsed();
		if elapsed >= timeout {
			eprintln!("Timeout waiting for clipboard data.");
			break;
		}
		let remaining = timeout - elapsed;
		let (d, wait_result) = cvar.wait_timeout(done, remaining).unwrap();
		done = d;
		if wait_result.timed_out() && !*done {
			eprintln!("Timeout waiting for clipboard data.");
			break;
		}
	}

	if *done {
		println!("Clipboard data processed.");
	} else {
		eprintln!("No clipboard data received.");
	}
}
