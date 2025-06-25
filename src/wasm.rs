use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::{JsCast, prelude::*};
use web_sys::{ClipboardEvent as WebClipboardEvent, FileReader, Response, window};

use crate::{ClipboardData, ClipboardEvent, InternalClipboard, Text, WasmOrSend};

pub struct WasmClipboard;

#[cfg(feature = "follow_html_img")]
fn try_extract_online_image_url(html: &str) -> Option<&str> {
	html.find("src=\"").and_then(|start| {
		let after = &html[start + 5..];
		after.find('"').map(|end| &after[..end])
	})
}

#[cfg(feature = "follow_html_img")]
fn strip_html_tags(html: &str) -> String {
	let mut output = String::new();
	let mut in_tag = false;
	for c in html.chars() {
		match c {
			'<' => in_tag = true,
			'>' => in_tag = false,
			_ if !in_tag => output.push(c),
			_ => (),
		}
	}
	output
}

impl WasmClipboard {
	fn setup_clipboard_listener(callback: &Rc<RefCell<dyn FnMut(ClipboardEvent)>>) {
		let callback_clone = callback.clone();

		let closure =
			Closure::<dyn FnMut(WebClipboardEvent)>::new(move |event: WebClipboardEvent| {
				if let Some(cd) = event.clipboard_data() {
					#[cfg(feature = "follow_html_img")]
					{
						if let Ok(html) = cd.get_data("text/html") {
							if let Some(url) = try_extract_online_image_url(&html) {
								let cb_clone = callback_clone.clone();
								let promise = window().unwrap().fetch_with_str(url);
								let future = wasm_bindgen_futures::JsFuture::from(promise);

								let task = async move {
									if let Ok(resp_val) = future.await {
										if let Ok(resp) = resp_val.dyn_into::<Response>() {
											if let Ok(Some(content_type)) =
												resp.headers().get("Content-Type")
											{
												if content_type.starts_with("image/") {
													if let Ok(buffer_promise) = resp.array_buffer()
													{
														if let Ok(buffer_js) =
															wasm_bindgen_futures::JsFuture::from(
																buffer_promise,
															)
															.await
														{
															let u8_array =
																js_sys::Uint8Array::new(&buffer_js);
															let mut data =
																vec![0; u8_array.length() as usize];
															u8_array.copy_to(&mut data[..]);

															let image = ClipboardData::Image(
																crate::Image::PNG(data),
															);
															(cb_clone.borrow_mut())(
																ClipboardEvent::Paste(image, None),
															);
															return;
														}
													}
												}
											}
										}
									}
									let plain = strip_html_tags(&html);
									(cb_clone.borrow_mut())(ClipboardEvent::Paste(
										ClipboardData::Text(Text::Plain(plain.trim().to_string())),
										None,
									));
								};

								wasm_bindgen_futures::spawn_local(task);
								return;
							}
						}
					}

					let items = cd.items();
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
										let text_trimmed = text_str.trim().to_string();

										#[cfg(feature = "follow_links")]
										if text_trimmed.starts_with("http://") || text_trimmed.starts_with("https://") {
											let cb_clone2 = cb_clone.clone();
											let url = text_trimmed.clone();
											let promise = window().unwrap().fetch_with_str(&url);
											let future = wasm_bindgen_futures::JsFuture::from(promise);

											wasm_bindgen_futures::spawn_local(async move {
												if let Ok(resp_val) = future.await {
													if let Ok(resp) = resp_val.dyn_into::<web_sys::Response>() {
														if let Ok(Some(content_type)) = resp.headers().get("Content-Type") {
															if content_type.starts_with("image/") {
																if let Ok(buffer_promise) = resp.array_buffer() {
																	if let Ok(buffer_js) = wasm_bindgen_futures::JsFuture::from(buffer_promise).await {
																		let u8_array = js_sys::Uint8Array::new(&buffer_js);
																		let mut data = vec![0; u8_array.length() as usize];
																		u8_array.copy_to(&mut data[..]);

																		let image = ClipboardData::Image(crate::Image::PNG(data));
																		(cb_clone2.borrow_mut())(ClipboardEvent::Paste(image, None));
																		return;
																	}
																}
															}
														}
													}
												}

												(cb_clone2.borrow_mut())(ClipboardEvent::Paste(
													ClipboardData::Text(Text::Plain(text_trimmed)),
													None,
												));
											});
										} else {
											(cb_clone.borrow_mut())(ClipboardEvent::Paste(
												ClipboardData::Text(Text::Plain(text_trimmed)),
												None,
											));
										}

										#[cfg(not(feature = "follow_links"))]
										(cb_clone.borrow_mut())(ClipboardEvent::Paste(
											ClipboardData::Text(Text::Plain(text_trimmed)),
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
