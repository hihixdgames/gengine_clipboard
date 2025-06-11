use gengine_clipboard::{Clipboard, ClipboardData, ClipboardEvent, Image, Text};
use std::sync::{Arc, Condvar, Mutex};

fn main() {
	let pair = Arc::new((Mutex::new(None), Condvar::new()));
	let pair_cb = Arc::clone(&pair);

	let clipboard = Arc::new(Clipboard::new(move |event| {
		let (lock, cvar) = &*pair_cb;
		let mut done = lock.lock().unwrap();
		*done = Some(event);
		cvar.notify_one();
	}));

	println!("Reading clipboard...");
	clipboard.get_data();

	let (lock, cvar) = &*pair;
	let mut done = lock.lock().unwrap();
	while done.is_none() {
		done = cvar.wait(done).unwrap();
	}

	match done.take().unwrap() {
		ClipboardEvent::Paste(data, _) => match data {
			ClipboardData::Text(Text::Plain(s)) => println!("Text (plain): {}", s),
			ClipboardData::Text(Text::HTML(html)) => println!("Text (HTML): {}", html),
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
		},
		ClipboardEvent::FailedPasteHandling(e) => {
			println!("Clipboard error: {:?}", e);
		}
		_ => {}
	}
}
