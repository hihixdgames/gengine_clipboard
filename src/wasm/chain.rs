use js_sys::{Array, Function, Math::log, Uint8Array};
use wasm_bindgen::{JsCast, JsValue, prelude::Closure};
use web_sys::{DataTransfer, Event, File, FileReader};

fn chain(
	name: String,
	file_reader: FileReader,
	mut raw: Vec<File>,
	mut handled: Vec<(String, Uint8Array)>,
	source: usize,
	closure: Function,
) {
	let read_closure = closure.clone();
	let on_file_read = Closure::once_into_js(move |event: Event| {
		let file_reader: FileReader = event.target().unwrap().dyn_into().unwrap();
		let file = file_reader.result().unwrap();
		let data = js_sys::Uint8Array::new(&file);
		handled.push((name, data));

		match raw.pop() {
			Some(file) => {
				let file_reader = web_sys::FileReader::new().unwrap();
				file_reader.read_as_array_buffer(&file).unwrap();

				chain(
					file.type_(),
					file_reader,
					raw,
					handled,
					source,
					read_closure,
				);
			}
			_ => {
				let array = Array::new();
				for (name, data) in handled {
					array.push(&name.into());
					array.push(&data.into());
				}
				let _ = read_closure.call2(
					&read_closure.clone().into(),
					&array,
					&(source as f64).into(),
				);
			}
		}
	});

	file_reader.set_onload(Some(on_file_read.as_ref().unchecked_ref()));
}

pub fn start(data: DataTransfer, source: usize, closure: Function) {
	let mut itemlist = Vec::new();

	let items = data.items();
	for i in 0..items.length() {
		if let Some(item) = items.get(i)
			&& let Ok(Some(item)) = item.get_as_file()
		{
			itemlist.push(item);
		}

		let wtf = Closure::once_into_js(|string: JsValue| {
			let a = string.as_string().unwrap();
			log::warn!("We got: {}", a);
		});

		let _ = items
			.get(i)
			.unwrap()
			.get_as_string(Some(wtf.as_ref().unchecked_ref()));
	}

	log::warn!("len {}", items.length());

	match itemlist.pop() {
		Some(file) => {
			let file_reader = web_sys::FileReader::new().unwrap();
			file_reader.read_as_array_buffer(&file).unwrap();

			chain(
				file.type_(),
				file_reader,
				itemlist,
				Vec::new(),
				source,
				closure,
			);
		}
		_ => {
			let _ = closure.call2(&(&closure).into(), &Array::new(), &(source as f64).into());
		}
	}
}
