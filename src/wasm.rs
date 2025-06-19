use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::{JsCast, prelude::*};
use web_sys::{ClipboardEvent as WebClipboardEvent, FileReader, window};

use crate::{ClipboardData, ClipboardEvent, InternalClipboard, WasmOrSend};

pub struct WasmClipboard;

impl WasmClipboard {
	fn setup_clipboard_listener(callback: &Rc<RefCell<dyn FnMut(ClipboardEvent)>>) {
		let callback_clone = callback.clone();

		let closure =
			Closure::<dyn FnMut(WebClipboardEvent)>::new(move |event: WebClipboardEvent| {
				if let Some(items) = event.clipboard_data().map(|cd| cd.items()) {
					let length = items.length();
					for i in 0..length {
						if let Some(item) = items.get(i) {
							let kind = item.kind();
							let mime = item.type_();
							if kind == "string" && mime == "text/plain" {
								let cb_clone = callback_clone.clone();
								let _ = item.get_as_string(Some(
									Closure::once_into_js(move |text: JsValue| {
										if let Some(text_str) = text.as_string() {
											(cb_clone.borrow_mut())(ClipboardEvent::Paste(
												ClipboardData::Text(crate::Text::Plain(text_str)),
												None,
											));
										}
									})
									.unchecked_ref(),
								));
							} else if kind == "file" && mime.starts_with("image/") {
								if let Ok(Some(blob)) = item.get_as_file() {
									let fr = FileReader::new().unwrap();
									let fr_clone = fr.clone();
									let cb_clone2 = callback_clone.clone();

									let onload =
										Closure::once(Box::new(move |_e: web_sys::Event| {
											let array_buffer = fr_clone.result().unwrap();
											let uint8_array =
												js_sys::Uint8Array::new(&array_buffer);
											let mut data = vec![0; uint8_array.length() as usize];
											uint8_array.copy_to(&mut data[..]);

											let image_data =
												ClipboardData::Image(crate::Image::PNG(data));

											(cb_clone2.borrow_mut())(ClipboardEvent::Paste(
												image_data, None,
											));
										}));

									fr.set_onload(Some(onload.as_ref().unchecked_ref()));
									fr.read_as_array_buffer(&blob).unwrap();
									onload.forget();
								}
							}
						}
					}
				}
			});

		let document = window().unwrap().document().unwrap();
		document
			.add_event_listener_with_callback("paste", closure.as_ref().unchecked_ref())
			.unwrap();
		closure.forget();
	}
}

impl InternalClipboard for WasmClipboard {
	fn new<F: FnMut(ClipboardEvent) + WasmOrSend + 'static>(callback: F) -> Self {
		let callback_obj: Rc<RefCell<dyn FnMut(ClipboardEvent)>> = Rc::new(RefCell::new(callback));
		Self::setup_clipboard_listener(&callback_obj);
		(callback_obj.borrow_mut())(ClipboardEvent::StartedPasteHandling);
		WasmClipboard
	}

	fn write(&self, _data: ClipboardData) {}
}
