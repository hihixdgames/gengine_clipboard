use gengine_clipboard::{Clipboard, ClipboardData, ClipboardEvent, Image, Text};
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use web_sys::{HtmlImageElement, console, window};

#[cfg(not(target_arch = "wasm32"))]
fn main() {
	panic!("Only works on wasm32 target");
}

fn image_bytes(image: &Image) -> &[u8] {
	match image {
		Image::PNG(data) => data,
		Image::JPEG(data) => data,
		Image::GIF(data) => data,
		Image::BMP(data) => data,
		Image::WEBP(data) => data,
		Image::ICO(data) => data,
		Image::TIFF(data) => data,
	}
}

#[cfg(target_arch = "wasm32")]
fn main() {
	let _clipboard = Clipboard::new(move |event| match event {
		ClipboardEvent::Paste(ClipboardData::Image(image), _) => {
			let bytes = image_bytes(&image);

			let uint8_array = js_sys::Uint8Array::new_with_length(bytes.len() as u32);
			uint8_array.copy_from(bytes);

			let parts = js_sys::Array::new();
			parts.push(&uint8_array);

			let blob = web_sys::Blob::new_with_u8_array_sequence(&parts).unwrap();

			let url = web_sys::Url::create_object_url_with_blob(&blob).unwrap();

			let document = window().unwrap().document().unwrap();
			let body = document.body().unwrap();

			let img = document
				.create_element("img")
				.unwrap()
				.dyn_into::<HtmlImageElement>()
				.unwrap();

			let img_clone = img.clone();

			let onload = Closure::once(Box::new(move || {
				let width = img_clone.natural_width();
				let height = img_clone.natural_height();
				img_clone.set_width(width);
				img_clone.set_height(height);
				web_sys::Url::revoke_object_url(&img_clone.src()).unwrap_or(());
			}));

			img.set_onload(Some(onload.as_ref().unchecked_ref()));
			onload.forget();

			img.set_src(&url);

			body.append_child(&img).unwrap();

			console::log_1(&"Image appended".into());
		}
		ClipboardEvent::Paste(ClipboardData::Text(Text::Plain(text)), _) => {
			console::log_1(&format!("Clipboard text: {}", text).into());
		}
		ClipboardEvent::Paste(ClipboardData::Text(Text::HTML(html)), _) => {
			console::log_1(&format!("Clipboard HTML: {}", html).into());
		}
		ClipboardEvent::FailedPasteHandling(err) => {
			console::log_1(&format!("Paste failed: {:?}", err).into());
		}
		_ => {
			console::log_1(&"Other clipboard event".into());
		}
	});
}
