#[cfg(all(target_os = "linux", feature = "x11"))]
use gengine_clipboard::X11Clipboard;
#[cfg(all(target_os = "linux", feature = "x11"))]
use gengine_clipboard::{ClipboardData, ClipboardEvent};
#[cfg(all(target_os = "linux", feature = "x11"))]
use std::sync::{Arc, Mutex};
#[cfg(all(target_os = "linux", feature = "x11"))]
use std::thread;
#[cfg(all(target_os = "linux", feature = "x11"))]
use std::time::Duration;

#[cfg(all(target_os = "linux", feature = "x11"))]
fn main() {
	let clipboard_data = Arc::new(Mutex::new(None));

	let clipboard_data_clone = clipboard_data.clone();

	let clipboard = X11Clipboard::new(move |event| {
		if let ClipboardEvent::Paste(data, _) = event {
			println!("Clipboard data received!");

			let mut lock = clipboard_data_clone.lock().unwrap();
			*lock = Some(data);
		}
	});

	clipboard.get_data();

	thread::sleep(Duration::from_secs(2));

	let lock = clipboard_data.lock().unwrap();
	if let Some(data) = &*lock {
		match data {
			ClipboardData::Text(text) => println!("Text clipboard: {:?}", text),
			ClipboardData::Image(image) => match image {
				gengine_clipboard::Image::GIF(data) => {
					println!("Clipboard contains a GIF image ({} bytes)", data.len());
					std::fs::write("gif.gif", data).expect("Failed to save GIF image");
				}
				gengine_clipboard::Image::PNG(data) => {
					println!("Clipboard contains a PNG image ({} bytes)", data.len());
					std::fs::write("png.png", data).expect("Failed to save PNG image");
				}
				gengine_clipboard::Image::JPEG(data) => {
					println!("Clipboard contains a JPEG image ({} bytes)", data.len());
					std::fs::write("jpeg.jpeg", data).expect("Failed to save JPEG image");
				}
				gengine_clipboard::Image::WEBP(data) => {
					println!("Clipboard contains a WEBP image ({} bytes)", data.len());
					std::fs::write("webp.webp", data).expect("Failed to save WEBP image");
				}
				gengine_clipboard::Image::BMP(data) => {
					println!("Clipboard contains a BMP image ({} bytes)", data.len());
					std::fs::write("bmp.bmp", data).expect("Failed to save BMP image");
				}
				gengine_clipboard::Image::ICO(data) => {
					println!("Clipboard contains an ICO image ({} bytes)", data.len());
					std::fs::write("ico.ico", data).expect("Failed to save ICO image");
				}
				gengine_clipboard::Image::TIFF(data) => {
					println!("Clipboard contains a TIFF image ({} bytes)", data.len());
					std::fs::write("tiff.tiff", data).expect("Failed to save TIFF image");
				}
			},
		}
	} else {
		println!("No clipboard data received.");
	}
}

#[cfg(not(all(target_os = "linux", feature = "x11")))]
fn main() {
	eprintln!("This example requires the x11 feature and Linux.");
}
