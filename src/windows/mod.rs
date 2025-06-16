#![allow(clippy::manual_c_str_literals)]

use std::ffi::OsStr;
use std::fmt;
use std::os::windows::ffi::OsStrExt;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::{
	BI_RGB, BITMAP, BITMAPFILEHEADER, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, GetDC,
	GetDIBits, GetObjectW, HBITMAP, RGBQUAD, ReleaseDC,
};
use windows::Win32::System::DataExchange::{
	CloseClipboard, EnumClipboardFormats, GetClipboardData, IsClipboardFormatAvailable,
	OpenClipboard, RegisterClipboardFormatA, SetClipboardData,
};
use windows::Win32::System::Memory::*;
use windows::core::PCSTR;

use crate::clipboard_data::{ClipboardData, Image, Text};
use crate::{ClipboardError, ClipboardEvent, InternalClipboard};

const CF_UNICODETEXT: u32 = 13;
const CF_BITMAP: u32 = 2;
const CF_DIB: u32 = 8;
const BF_TYPE_BM: u16 = 0x4D42;

const N_RETRIES: usize = 5;
const TIME_BETWEEN_RETRIES: u64 = 1000;

impl fmt::Display for Text {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Text::Plain(s) | Text::HTML(s) => write!(f, "{}", s),
		}
	}
}

pub fn try_read_clipboard_image_from_formats(
	formats: &[u32],
) -> Result<Option<Image>, ClipboardError> {
	unsafe {
		for &format in formats {
			if IsClipboardFormatAvailable(format).is_err() {
				continue;
			}

			let handle =
				GetClipboardData(format).map_err(|_| ClipboardError::FormatNotAvailable)?;
			if handle.0.is_null() {
				continue;
			}

			let hglobal = HGLOBAL(handle.0);
			let ptr = GlobalLock(hglobal);
			if ptr.is_null() {
				let _ = GlobalUnlock(hglobal);
				continue;
			}

			let size = GlobalSize(hglobal);
			if size == 0 {
				let _ = GlobalUnlock(hglobal);
				continue;
			}

			let slice = std::slice::from_raw_parts(ptr as *const u8, size);
			let data = slice.to_vec();

			let _ = GlobalUnlock(hglobal);

			if let Some(image) = detect_image_type(&data) {
				return Ok(Some(image));
			}
		}

		if let Ok(bitmap_data) = try_read_clipboard_bitmap() {
			if let Some(image) = detect_image_type(&bitmap_data) {
				return Ok(Some(image));
			}
		}
	}
	Ok(None)
}

pub fn try_read_clipboard_bitmap() -> Result<Vec<u8>, ClipboardError> {
	unsafe {
		let handle = GetClipboardData(CF_BITMAP).map_err(|_| ClipboardError::Empty)?;
		if handle.0.is_null() {
			return Err(ClipboardError::Empty);
		}

		let hbitmap = HBITMAP(handle.0);
		let mut bmp = BITMAP::default();
		if GetObjectW(
			hbitmap.into(),
			std::mem::size_of::<BITMAP>() as i32,
			Some(&mut bmp as *mut _ as *mut _),
		) == 0
		{
			return Err(ClipboardError::LockFailed);
		}

		let mut bmi = BITMAPINFO {
			bmiHeader: BITMAPINFOHEADER {
				biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
				biWidth: bmp.bmWidth,
				biHeight: bmp.bmHeight,
				biPlanes: 1,
				biBitCount: 32,
				biCompression: BI_RGB.0,
				..Default::default()
			},
			bmiColors: [RGBQUAD::default(); 1],
		};

		let width = bmp.bmWidth as usize;
		let height = bmp.bmHeight as usize;
		let pixel_data_size = width * height * 4;
		let mut buffer = vec![0u8; pixel_data_size];

		let hdc = GetDC(None);
		if hdc.0.is_null() {
			return Err(ClipboardError::LockFailed);
		}

		let scanlines = GetDIBits(
			hdc,
			hbitmap,
			0,
			height as u32,
			Some(buffer.as_mut_ptr() as *mut _),
			&mut bmi,
			DIB_RGB_COLORS,
		);

		ReleaseDC(None, hdc);

		if scanlines == 0 {
			return Err(ClipboardError::LockFailed);
		}

		let file_header_size = std::mem::size_of::<BITMAPFILEHEADER>();
		let info_header_size = std::mem::size_of::<BITMAPINFOHEADER>();
		let file_size = file_header_size + info_header_size + pixel_data_size;

		let file_header = BITMAPFILEHEADER {
			bfType: BF_TYPE_BM,
			bfSize: file_size as u32,
			bfReserved1: 0,
			bfReserved2: 0,
			bfOffBits: (file_header_size + info_header_size) as u32,
		};

		let mut bmp_data = Vec::with_capacity(file_size);

		bmp_data.extend_from_slice(std::slice::from_raw_parts(
			&file_header as *const _ as *const u8,
			file_header_size,
		));
		bmp_data.extend_from_slice(std::slice::from_raw_parts(
			&bmi.bmiHeader as *const _ as *const u8,
			info_header_size,
		));
		bmp_data.extend_from_slice(&buffer);

		Ok(bmp_data)
	}
}

pub fn detect_image_type(data: &[u8]) -> Option<Image> {
	if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") {
		Some(Image::GIF(data.to_vec()))
	} else if data.starts_with(b"\x89PNG\r\n\x1a\n") {
		Some(Image::PNG(data.to_vec()))
	} else if data.starts_with(b"\xFF\xD8\xFF") {
		Some(Image::JPEG(data.to_vec()))
	} else if data.len() > 12 && &data[0..4] == b"RIFF" && &data[8..12] == b"WEBP" {
		Some(Image::WEBP(data.to_vec()))
	} else {
		None
	}
}

#[cfg(feature = "follow_html_img")]
fn extract_html_fragment(raw_html: &str) -> Option<&str> {
	let start_tag = "StartFragment:";
	let end_tag = "EndFragment:";

	let start_pos = raw_html.find(start_tag)?;
	let end_pos = raw_html.find(end_tag)?;

	let start_num_line_end =
		raw_html[start_pos + start_tag.len()..].find('\n')? + start_pos + start_tag.len();
	let end_num_line_end =
		raw_html[end_pos + end_tag.len()..].find('\n')? + end_pos + end_tag.len();

	let start_num_str = &raw_html[start_pos + start_tag.len()..start_num_line_end].trim();
	let end_num_str = &raw_html[end_pos + end_tag.len()..end_num_line_end].trim();

	let start_idx = start_num_str.parse::<usize>().ok()?;
	let end_idx = end_num_str.parse::<usize>().ok()?;

	raw_html.get(start_idx..end_idx)
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

#[cfg(feature = "follow_links")]
fn try_image_from_link() -> Option<ClipboardData> {
	unsafe {
		let handle = GetClipboardData(CF_UNICODETEXT).ok()?;
		if handle.0.is_null() {
			return None;
		}
		let hglobal = HGLOBAL(handle.0);
		let ptr = GlobalLock(hglobal);
		if ptr.is_null() {
			return None;
		}
		let len = (0..)
			.take_while(|&i| *(ptr.add(i * 2) as *const u16) != 0)
			.count();
		let slice = std::slice::from_raw_parts(ptr as *const u16, len);
		let string = String::from_utf16(slice).ok()?;
		let _ = GlobalUnlock(hglobal);

		if string.starts_with("http://") || string.starts_with("https://") {
			if let Ok(resp) = reqwest::blocking::get(&string) {
				let content_type_opt = resp
					.headers()
					.get(reqwest::header::CONTENT_TYPE)
					.and_then(|v| v.to_str().ok())
					.map(|s| s.to_owned());

				if let Some(content_type) = content_type_opt {
					if content_type.starts_with("image/") {
						if let Ok(bytes) = resp.bytes() {
							let data = bytes.to_vec();
							if let Some(image) = detect_image_type(&data) {
								return Some(ClipboardData::Image(image));
							}
						}
					}
				}
			}
		}
		None
	}
}

pub struct ClipboardHandle;

impl ClipboardHandle {
	pub fn new() -> Result<Self, ClipboardError> {
		unsafe {
			OpenClipboard(None).map_err(|_| ClipboardError::OpenFailed)?;
		}
		Ok(Self {})
	}

	pub fn read_data(&self, format: u32) -> Result<ClipboardData, ClipboardError> {
		unsafe {
			let png_format = RegisterClipboardFormatA(PCSTR(b"PNG\0".as_ptr()));
			let jpeg_format = RegisterClipboardFormatA(PCSTR(b"JPEG\0".as_ptr()));
			let image_png_format = RegisterClipboardFormatA(PCSTR(b"image/png\0".as_ptr()));
			let image_jpeg_format = RegisterClipboardFormatA(PCSTR(b"image/jpeg\0".as_ptr()));
			let gif_format = RegisterClipboardFormatA(PCSTR(b"GIF\0".as_ptr()));
			let webp_format = RegisterClipboardFormatA(PCSTR(b"WEBP\0".as_ptr()));
			#[cfg(feature = "follow_html_img")]
			let html_format = RegisterClipboardFormatA(PCSTR(b"HTML Format\0".as_ptr()));

			#[cfg(feature = "follow_html_img")]
			fn try_extract_online_image_url(html: &str) -> Option<&str> {
				html.find("src=\"").and_then(|start_idx| {
					let rest = &html[start_idx + 5..];
					rest.find('"').map(|end_idx| &rest[..end_idx])
				})
			}

			#[cfg(feature = "follow_html_img")]
			if IsClipboardFormatAvailable(html_format).is_ok() {
				let handle = GetClipboardData(html_format)
					.map_err(|_| ClipboardError::FormatNotAvailable)?;
				if handle.0.is_null() {
					return Err(ClipboardError::Empty);
				}
				let hglobal = HGLOBAL(handle.0);
				let ptr = GlobalLock(hglobal);
				if ptr.is_null() {
					return Err(ClipboardError::LockFailed);
				}
				let size = GlobalSize(hglobal);
				let slice = std::slice::from_raw_parts(ptr as *const u8, size);
				let raw_html = String::from_utf8_lossy(slice).to_string();
				let _ = GlobalUnlock(hglobal);

				if let Some(url) = try_extract_online_image_url(&raw_html) {
					if let Ok(resp) = reqwest::blocking::get(url) {
						let content_type_opt = resp
							.headers()
							.get(reqwest::header::CONTENT_TYPE)
							.and_then(|v| v.to_str().ok())
							.map(|s| s.to_owned());

						if let Some(content_type) = content_type_opt {
							if content_type.starts_with("image/") {
								match resp.bytes() {
									Ok(bytes) => {
										let data = bytes.to_vec();
										if let Some(image) = detect_image_type(&data) {
											return Ok(ClipboardData::Image(image));
										}
									}
									Err(_) => {
										if let Ok(Some(image)) =
											try_read_clipboard_image_from_formats(&[
												png_format,
												image_png_format,
												jpeg_format,
												image_jpeg_format,
												gif_format,
												webp_format,
												CF_BITMAP,
											]) {
											return Ok(ClipboardData::Image(image));
										}
									}
								}
							} else if content_type.starts_with("text/html") {
								let text = resp.text().map_err(|e| {
									ClipboardError::Unknown(format!("Failed to read text: {}", e))
								})?;
								return Ok(ClipboardData::Text(Text::HTML(text)));
							}
						}

						if let Ok(Some(image)) = try_read_clipboard_image_from_formats(&[
							png_format,
							image_png_format,
							jpeg_format,
							image_jpeg_format,
							gif_format,
							webp_format,
							CF_BITMAP,
						]) {
							return Ok(ClipboardData::Image(image));
						}
					}
				}

				if let Some(fragment_html) = extract_html_fragment(&raw_html) {
					let plain_text = strip_html_tags(fragment_html);
					return Ok(ClipboardData::Text(Text::Plain(
						plain_text.trim().to_string(),
					)));
				} else {
					let plain_text = strip_html_tags(&raw_html);
					return Ok(ClipboardData::Text(Text::Plain(
						plain_text.trim().to_string(),
					)));
				}
			}

			if format == CF_UNICODETEXT {
				let handle =
					GetClipboardData(format).map_err(|_| ClipboardError::FormatNotAvailable)?;
				if handle.0.is_null() {
					return Err(ClipboardError::Empty);
				}
				let hglobal = HGLOBAL(handle.0);
				let ptr = GlobalLock(hglobal);
				if ptr.is_null() {
					return Err(ClipboardError::LockFailed);
				}
				let len = (0..)
					.take_while(|&i| *(ptr.add(i * 2) as *const u16) != 0)
					.count();
				let slice = std::slice::from_raw_parts(ptr as *const u16, len);
				let string =
					String::from_utf16(slice).map_err(|_| ClipboardError::Utf16ConversionFailed)?;
				let _ = GlobalUnlock(hglobal);
				return Ok(ClipboardData::Text(Text::Plain(string)));
			}

			if let Ok(Some(image)) = try_read_clipboard_image_from_formats(&[
				png_format,
				image_png_format,
				jpeg_format,
				image_jpeg_format,
				gif_format,
				webp_format,
				CF_BITMAP,
			]) {
				return Ok(ClipboardData::Image(image));
			}

			let handle =
				GetClipboardData(CF_UNICODETEXT).map_err(|_| ClipboardError::FormatNotAvailable)?;
			if handle.0.is_null() {
				return Err(ClipboardError::Empty);
			}
			let hglobal = HGLOBAL(handle.0);
			let ptr = GlobalLock(hglobal);
			if ptr.is_null() {
				return Err(ClipboardError::LockFailed);
			}
			let len = (0..)
				.take_while(|&i| *(ptr.add(i * 2) as *const u16) != 0)
				.count();
			let slice = std::slice::from_raw_parts(ptr as *const u16, len);
			let string =
				String::from_utf16(slice).map_err(|_| ClipboardError::Utf16ConversionFailed)?;
			let _ = GlobalUnlock(hglobal);
			Ok(ClipboardData::Text(Text::Plain(string)))
		}
	}

	pub fn write_data(&self, data: &ClipboardData) -> Result<(), ClipboardError> {
		unsafe {
			match data {
				ClipboardData::Text(text) => {
					let s = match text {
						Text::Plain(plain) => plain.as_str(),
						Text::HTML(html) => html.as_str(),
					};

					let wide: Vec<u16> = OsStr::new(s)
						.encode_wide()
						.chain(std::iter::once(0))
						.collect();

					let size = wide.len() * std::mem::size_of::<u16>();

					let hglobal = GlobalAlloc(GMEM_MOVEABLE, size)
						.map_err(|_| ClipboardError::AllocationFailed)?;

					if hglobal.is_invalid() {
						return Err(ClipboardError::AllocationFailed);
					}

					let ptr = GlobalLock(hglobal);
					if ptr.is_null() {
						let _ = GlobalUnlock(hglobal);
						return Err(ClipboardError::LockFailed);
					}

					std::ptr::copy_nonoverlapping(wide.as_ptr() as *const u8, ptr as *mut u8, size);
					let _ = GlobalUnlock(hglobal);

					SetClipboardData(CF_UNICODETEXT, Some(HANDLE(hglobal.0)))
						.map_err(|_| ClipboardError::SetFailed)?;
				}

				ClipboardData::Image(image) => match image {
					Image::PNG(data) => {
						let png_format = RegisterClipboardFormatA(PCSTR(b"PNG\0".as_ptr()));

						let hglobal = GlobalAlloc(GMEM_MOVEABLE, data.len())
							.map_err(|_| ClipboardError::AllocationFailed)?;

						if hglobal.is_invalid() {
							return Err(ClipboardError::AllocationFailed);
						}

						let ptr = GlobalLock(hglobal);
						if ptr.is_null() {
							let _ = GlobalUnlock(hglobal);
							return Err(ClipboardError::LockFailed);
						}

						std::ptr::copy_nonoverlapping(data.as_ptr(), ptr as *mut u8, data.len());
						let _ = GlobalUnlock(hglobal);

						SetClipboardData(png_format, Some(HANDLE(hglobal.0)))
							.map_err(|_| ClipboardError::SetFailed)?;
					}

					Image::BMP(data) => {
						let hglobal = GlobalAlloc(GMEM_MOVEABLE, data.len())
							.map_err(|_| ClipboardError::AllocationFailed)?;

						if hglobal.is_invalid() {
							return Err(ClipboardError::AllocationFailed);
						}

						let ptr = GlobalLock(hglobal);
						if ptr.is_null() {
							let _ = GlobalUnlock(hglobal);
							return Err(ClipboardError::LockFailed);
						}

						std::ptr::copy_nonoverlapping(data.as_ptr(), ptr as *mut u8, data.len());
						let _ = GlobalUnlock(hglobal);

						SetClipboardData(CF_DIB, Some(HANDLE(hglobal.0)))
							.map_err(|_| ClipboardError::SetFailed)?;
					}

					_ => return Err(ClipboardError::FormatNotAvailable),
				},
			}

			Ok(())
		}
	}
}

impl Drop for ClipboardHandle {
	fn drop(&mut self) {
		unsafe {
			CloseClipboard().ok();
		}
	}
}

enum ThreadCommand {
	GetData,
	Write(ClipboardData),
	Exit,
}

pub struct WindowsClipboard {
	sender: Sender<ThreadCommand>,
	join_handle: Option<JoinHandle<()>>,
}

impl InternalClipboard for WindowsClipboard {
	fn new<F: FnMut(crate::ClipboardEvent) + crate::WasmOrSend>(callback: F) -> Self {
		let (sender, receiver) = mpsc::channel();
		let join_handle = Some(spawn_thread(receiver, callback));
		Self {
			sender,
			join_handle,
		}
	}

	fn get_data(&self) {
		let _ = self.sender.send(ThreadCommand::GetData);
	}

	fn write(&self, data: ClipboardData) {
		let _ = self.sender.send(ThreadCommand::Write(data));
	}
}

fn spawn_thread<F: FnMut(crate::ClipboardEvent) + crate::WasmOrSend>(
	receiver: Receiver<ThreadCommand>,
	mut callback: F,
) -> JoinHandle<()> {
	thread::spawn(move || {
		for command in receiver {
			match command {
				ThreadCommand::GetData => {
					let mut found = false;
					for attempt in 0..N_RETRIES {
						let handle = match ClipboardHandle::new() {
							Ok(h) => h,
							Err(e) => {
								if attempt == N_RETRIES - 1 {
									callback(ClipboardEvent::FailedPasteHandling(e));
									break;
								} else {
									std::thread::sleep(std::time::Duration::from_millis(
										TIME_BETWEEN_RETRIES,
									));
									continue;
								}
							}
						};

						let mut format = 0u32;
						let mut got_data = false;
						while {
							format = unsafe { EnumClipboardFormats(format) };
							format != 0
						} {
							if let Ok(data) = handle.read_data(format) {
								#[cfg(feature = "follow_links")]
								{
									if let ClipboardData::Text(Text::Plain(ref s)) = data {
										if s.starts_with("http://") || s.starts_with("https://") {
											if let Some(image_data) = try_image_from_link() {
												callback(ClipboardEvent::Paste(image_data, None));
												found = true;
												got_data = true;
												break;
											} else {
												callback(ClipboardEvent::Paste(data, None));
												found = true;
												got_data = true;
												break;
											}
										}
									}
								}

								#[cfg(not(feature = "follow_links"))]
								{
									callback(ClipboardEvent::Paste(data, None));
									found = true;
									got_data = true;
									break;
								}
								#[cfg(feature = "follow_links")]
								if !matches!(data, ClipboardData::Text(Text::Plain(ref s)) if s.starts_with("http://") || s.starts_with("https://"))
								{
									callback(ClipboardEvent::Paste(data, None));
									found = true;
									got_data = true;
									break;
								}
							}
						}

						drop(handle);

						if got_data {
							break;
						} else if attempt < N_RETRIES - 1 {
							std::thread::sleep(std::time::Duration::from_millis(
								TIME_BETWEEN_RETRIES,
							));
						}
					}

					if !found {
						callback(ClipboardEvent::FailedPasteHandling(
							ClipboardError::FormatNotAvailable,
						));
					}
				}

				ThreadCommand::Write(data) => {
					let handle = match ClipboardHandle::new() {
						Ok(h) => h,
						Err(e) => {
							callback(ClipboardEvent::FailedPasteHandling(e));
							continue;
						}
					};

					if let Err(e) = handle.write_data(&data) {
						callback(ClipboardEvent::FailedPasteHandling(e));
					}
				}

				ThreadCommand::Exit => return,
			}
		}
	})
}

impl Drop for WindowsClipboard {
	fn drop(&mut self) {
		if let Ok(()) = self.sender.send(ThreadCommand::Exit) {
			let _ = self.join_handle.take().unwrap().join();
		}
	}
}
