use std::collections::HashMap;

use js_sys::{Array, Function, Uint8Array};
use wasm_bindgen::{JsCast, JsValue, prelude::Closure};

use crate::{
	ClipboardError, ClipboardEvent, ClipboardEventSource, ClipboardHandler,
	platform::pasta_data_access::WasmDataAccess,
};

#[derive(Default)]
struct DataStorage {
	size: Option<usize>,
	data: Vec<(String, Uint8Array)>,
}

pub struct CollectorHandle {
	_callback: Closure<dyn FnMut(JsValue, JsValue, JsValue)>,
}

#[derive(Clone)]
pub struct Collector {
	function: Function,
}

impl Collector {
	pub fn new<T: ClipboardHandler>(mut handler: T) -> (CollectorHandle, Collector) {
		let mut storage: HashMap<usize, DataStorage> = HashMap::new();

		let callback = Closure::<dyn FnMut(_, _, _)>::new(
			move |command: JsValue, data: JsValue, source: JsValue| {
				let source = source.as_f64().unwrap();
				let source = ClipboardEventSource {
					value: source as usize,
				};

				match command.as_string().unwrap().as_str() {
					"data" => {
						let data: Array = data.dyn_into().unwrap();
						let mime_type = data.get(0).as_string().unwrap();
						let array: Uint8Array = data.get(1).dyn_into().unwrap();
						storage
							.entry(source.value)
							.or_default()
							.data
							.push((mime_type, array));
					}
					"start" => {
						let size = data.as_f64().unwrap() as usize;

						storage.entry(source.value).or_default().size = Some(size);

						handler.handle_event(ClipboardEvent::StartedPasteHandling { source });
					}
					"error" => {
						let code = data.as_f64().unwrap() as u32;
						let error = ClipboardError::try_from(code).unwrap();
						return handler
							.handle_event(ClipboardEvent::FailedPasteHandling { source, error });
					}
					_ => unreachable!(),
				}

				let collected = storage.get(&source.value).unwrap();
				if collected.size.is_some() && collected.size.unwrap() > collected.data.len() {
					return;
				}

				let collected = storage.remove(&source.value).unwrap();

				if collected.data.is_empty() {
					return handler.handle_event(ClipboardEvent::FailedPasteHandling {
						source,
						error: ClipboardError::Empty,
					});
				}

				let mime_types: Vec<String> = collected
					.data
					.iter()
					.map(|(mime_type, _)| mime_type.clone())
					.collect();
				let mut data_access = WasmDataAccess::new(mime_types, collected.data);

				handler.handle_event(ClipboardEvent::PasteResult {
					data: &mut data_access,
					source,
				});
			},
		);

		let function: &Function = callback.as_ref().unchecked_ref();
		let function = function.clone();

		let handle = CollectorHandle {
			_callback: callback,
		};

		let collector = Collector { function };

		(handle, collector)
	}

	pub fn insert_data(&self, mime_type: String, array: Uint8Array, source: usize) {
		let data = Array::new();
		data.push(&mime_type.into());
		data.push(&array.into());
		let command = JsValue::from_str("data");

		let _ = self
			.function
			.call3(&self.function, &command, &data, &(source as f64).into());
	}

	pub fn start_paste_handling(&self, size: usize, source: usize) {
		let command = JsValue::from_str("start");
		let _ = self.function.call3(
			&self.function,
			&command,
			&(size as f64).into(),
			&(source as f64).into(),
		);
	}

	pub fn send_error(&self, error: ClipboardError, source: usize) {
		let code: u32 = error.into();
		let command = JsValue::from_str("error");
		let _ = self.function.call3(
			&self.function,
			&command,
			&(code as f64).into(),
			&(source as f64).into(),
		);
	}
}
