use std::sync::{Arc, Mutex, mpsc::{self, Sender}};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use windows::core::PCSTR;
use windows::Win32::Foundation::*;
use windows::Win32::System::Memory::*;
use windows::Win32::Graphics::Gdi::{
    BITMAP, BITMAPINFO, BITMAPINFOHEADER, RGBQUAD, GetObjectW, GetDIBits, DIB_RGB_COLORS, HBITMAP, BI_RGB, GetDC, ReleaseDC,
};
use windows::Win32::System::DataExchange::{
    EnumClipboardFormats, OpenClipboard, CloseClipboard, IsClipboardFormatAvailable, GetClipboardData, RegisterClipboardFormatA, GetClipboardFormatNameW,
};
use windows::Win32::UI::Shell::{HDROP, DragQueryFileW};

const CF_UNICODETEXT: u32 = 13;
const CF_BITMAP: u32 = 2;
const GIF87A: &[u8] = b"GIF87a";
const GIF89A: &[u8] = b"GIF89a";
const PNG_MAGIC: &[u8] = b"\x89PNG\r\n\x1a\n";
const JPEG_MAGIC: &[u8] = b"\xFF\xD8\xFF";
const CF_HDROP: u32 = 15;
const WEBP_MAGIC: &[u8] = b"RIFF";
const WEBP_SIGNATURE: &[u8] = b"WEBP";
const ICO_MAGIC: &[u8] = b"\x00\x00\x01\x00";
const TIFF_MAGIC_LE: &[u8] = b"II*\x00";
const TIFF_MAGIC_BE: &[u8] = b"MM\x00*";

pub use crate::clipboard_data::*;
pub use crate::clipboard_error::*;

pub fn try_read_clipboard_format(format: u32) -> Result<Vec<u8>, ClipboardError> {
    unsafe {
        let handle = GetClipboardData(format).map_err(|_| ClipboardError::Empty)?;
        if handle.0.is_null() {
            let _ = CloseClipboard();
            return Err(ClipboardError::Empty);
        }
        let hglobal = HGLOBAL(handle.0 as *mut _);
        let ptr = GlobalLock(hglobal) as *const u8;
        if ptr.is_null() {
            let _ = CloseClipboard();
            return Err(ClipboardError::LockFailed);
        }
        let size = GlobalSize(hglobal);
        let slice = std::slice::from_raw_parts(ptr, size);
        let data = slice.to_vec();
        let _ = GlobalUnlock(hglobal);
        Ok(data)
    }
}

pub fn try_read_clipboard_text() -> Result<String, ClipboardError> {
    unsafe {
        if OpenClipboard(None).is_err() {
            return Err(ClipboardError::OpenFailed);
        }
        IsClipboardFormatAvailable(CF_UNICODETEXT).map_err(|_| ClipboardError::FormatNotAvailable)?;
        let handle = GetClipboardData(CF_UNICODETEXT).map_err(|_| ClipboardError::Empty)?;
        if handle.0.is_null() {
            let _ = CloseClipboard();
            return Err(ClipboardError::Empty);
        }
        let hglobal = HGLOBAL(handle.0 as *mut std::ffi::c_void);
        let ptr = GlobalLock(hglobal) as *const u16;
        if ptr.is_null() {
            let _ = CloseClipboard();
            return Err(ClipboardError::LockFailed);
        }
        let len = (0..).take_while(|&i| *ptr.add(i) != 0).count();
        let slice = std::slice::from_raw_parts(ptr, len);
        let result = String::from_utf16(slice).map_err(|_| ClipboardError::Utf16ConversionFailed);
        let _ = CloseClipboard();
        let _ = GlobalUnlock(hglobal);
        result
    }
}

pub fn try_read_clipboard_image() -> Result<Vec<u8>, ClipboardError> {
    unsafe {
        if OpenClipboard(None).is_err() {
            return Err(ClipboardError::OpenFailed);
        }
        IsClipboardFormatAvailable(CF_BITMAP).map_err(|_| ClipboardError::FormatNotAvailable)?;
        let handle = GetClipboardData(CF_BITMAP).map_err(|_| ClipboardError::Empty)?;
        if handle.0.is_null() {
            let _ = CloseClipboard();
            return Err(ClipboardError::Empty);
        }
        let hbitmap = HBITMAP(handle.0);
        let mut bmp = BITMAP::default();
        if GetObjectW(hbitmap.into(), std::mem::size_of::<BITMAP>() as i32, Some(&mut bmp as *mut _ as *mut _)) == 0 {
            let _ = CloseClipboard();
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
        let mut buffer = vec![0u8; width * height * 4];
        let hdc = GetDC(None);
        if hdc.0.is_null() {
            let _ = CloseClipboard();
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
        let _ = CloseClipboard();
        if scanlines == 0 {
            return Err(ClipboardError::LockFailed);
        }
        Ok(buffer)
    }
}

fn detect_image_type(data: &[u8]) -> Option<Image> {
    if data.starts_with(GIF87A) || data.starts_with(GIF89A) {
        Some(Image::GIF(data.to_vec()))
    } else if data.starts_with(PNG_MAGIC) {
        Some(Image::PNG(data.to_vec()))
    } else if data.starts_with(JPEG_MAGIC) {
        Some(Image::JPEG(data.to_vec()))
    } else if data.len() > 12
        && &data[0..4] == WEBP_MAGIC
        && &data[8..12] == WEBP_SIGNATURE
    {
        Some(Image::WEBP(data.to_vec()))
    } else if data.starts_with(ICO_MAGIC) {
        Some(Image::ICO(data.to_vec()))
    } else if data.starts_with(TIFF_MAGIC_LE) || data.starts_with(TIFF_MAGIC_BE) {
        Some(Image::TIFF(data.to_vec()))
    } else {
        None
    }
}

#[derive(Debug)]
pub enum ClipboardEvent {
    StartedPasteHandling,
    FailedPasteHandling(ClipboardError),
    Paste(ClipboardData, Option<ClipboardError>),
}

#[cfg(target_os = "windows")]
#[allow(dead_code)]
pub struct WindowsClipboard {
    callback: Arc<Mutex<Box<dyn FnMut(ClipboardEvent) + Send>>>,
}

pub enum ClipboardCommand {
    Copy,
}

pub struct Clipboard {
    #[cfg(target_os = "windows")]
    #[allow(dead_code)]
    internal: WindowsClipboard,
    callback: Arc<Mutex<Box<dyn FnMut(ClipboardEvent) + Send>>>,
    last_plain_text: Arc<Mutex<Option<String>>>,
    last_html_text: Arc<Mutex<Option<String>>>,
    last_image: Arc<Mutex<Option<Vec<u8>>>>,
    sender: Sender<ClipboardCommand>,
    handle: JoinHandle<()>,
}

impl Clipboard {
    pub fn new<F>(callback: F) -> Self
    where
        F: FnMut(ClipboardEvent) + Send + 'static,
    {
        let callback: Arc<Mutex<Box<dyn FnMut(ClipboardEvent) + Send>>> =
            Arc::new(Mutex::new(Box::new(callback)));
        let last_plain_text = Arc::new(Mutex::new(None));
        let last_html_text = Arc::new(Mutex::new(None));
        let last_image = Arc::new(Mutex::new(None));

        let (sender, receiver) = mpsc::channel();

        let cb_clone = Arc::clone(&callback);
        let last_plain_text_clone = Arc::clone(&last_plain_text);
        let last_html_text_clone = Arc::clone(&last_html_text);
        let last_image_clone = Arc::clone(&last_image);

        let handle = thread::spawn(move || {
            while let Ok(command) = receiver.recv() {
                match command {
                    ClipboardCommand::Copy => {
                        let png_format = unsafe { RegisterClipboardFormatA(PCSTR(b"PNG\0".as_ptr())) };
                        let jpeg_format = unsafe { RegisterClipboardFormatA(PCSTR(b"JPEG\0".as_ptr())) };
                        let image_png_format = unsafe { RegisterClipboardFormatA(PCSTR(b"image/png\0".as_ptr())) };
                        let webp_format = unsafe { RegisterClipboardFormatA(PCSTR(b"WEBP\0".as_ptr())) };
                        let html_format = unsafe { RegisterClipboardFormatA(PCSTR(b"HTML Format\0".as_ptr())) };

                        'outer: for _ in 0..5 {
                            let mut opened = false;
                            for _ in 0..5 {
                                unsafe {
                                    if OpenClipboard(None).is_ok() {
                                        opened = true;
                                        break;
                                    } else {
                                        if let Ok(mut cb) = cb_clone.lock() {
                                            cb(ClipboardEvent::FailedPasteHandling(ClipboardError::InUse));
                                        }
                                        return;
                                    }
                                }
                                std::thread::sleep(Duration::from_millis(100));
                            }
                            if !opened {
                                if let Ok(mut cb) = cb_clone.lock() {
                                    cb(ClipboardEvent::FailedPasteHandling(ClipboardError::OpenFailed));
                                }
                                return;
                            }

                            unsafe {
                                let mut format = 0u32;
                                if EnumClipboardFormats(0) == 0 {
                                    let _ = CloseClipboard();
                                    std::thread::sleep(Duration::from_millis(100));
                                    continue;
                                }

                                while {
                                    format = EnumClipboardFormats(format);
                                    format != 0
                                } {
                                    match format {
                                        f if f == html_format => {
                                            if let Ok(data) = try_read_clipboard_format(f) {
                                                if let Ok(html) = String::from_utf8(data) {
                                                    let mut last = last_html_text_clone.lock().unwrap();
                                                    if last.as_ref() != Some(&html) {
                                                        if let Ok(mut cb) = cb_clone.lock() {
                                                            cb(ClipboardEvent::Paste(
                                                                ClipboardData::Text(Text::HTML(html.clone())),
                                                                None,
                                                            ));
                                                        }
                                                        *last = Some(html);
                                                        break 'outer;
                                                    }
                                                }
                                            }
                                        }
                                        f if f == CF_UNICODETEXT => {
                                            if let Ok(text) = try_read_clipboard_text() {
                                                let mut last = last_plain_text_clone.lock().unwrap();
                                                if last.as_ref() != Some(&text) {
                                                    if let Ok(mut cb) = cb_clone.lock() {
                                                        cb(ClipboardEvent::Paste(
                                                            ClipboardData::Text(Text::Plain(text.clone())),
                                                            None,
                                                        ));
                                                    }
                                                    *last = Some(text);
                                                    break 'outer;
                                                }
                                            }
                                        }
                                        f if f == png_format || f == image_png_format || f == jpeg_format || f == webp_format => {
                                            if let Ok(data) = try_read_clipboard_format(f) {
                                                if let Some(image) = detect_image_type(&data) {
                                                    let mut last = last_image_clone.lock().unwrap();
                                                    if last.as_ref() != Some(&data) {
                                                        if let Ok(mut cb) = cb_clone.lock() {
                                                            cb(ClipboardEvent::Paste(
                                                                ClipboardData::Image(image),
                                                                None,
                                                            ));
                                                        }
                                                        *last = Some(data);
                                                        break 'outer;
                                                    }
                                                }
                                            }
                                        }
                                        f if f == CF_BITMAP => {
                                            if let Ok(data) = try_read_clipboard_image() {
                                                let mut last = last_image_clone.lock().unwrap();
                                                if last.as_ref() != Some(&data) {
                                                    if let Ok(mut cb) = cb_clone.lock() {
                                                        cb(ClipboardEvent::Paste(
                                                            ClipboardData::Image(Image::BMP(data.clone())),
                                                            None,
                                                        ));
                                                    }
                                                    *last = Some(data);
                                                    break 'outer;
                                                }
                                            }
                                        }
                                        f if f == CF_HDROP => {
                                            let handle = GetClipboardData(CF_HDROP);
                                            if let Ok(hdrop) = handle {
                                                let hdrop = HDROP(hdrop.0);
                                                let file_count = DragQueryFileW(hdrop, 0xFFFFFFFF, None);
                                                for i in 0..file_count {
                                                    let mut buf = [0u16; 260];
                                                    let len = DragQueryFileW(hdrop, i, Some(&mut buf));
                                                    if len > 0 {
                                                        if let Ok(path) = String::from_utf16(&buf[..len as usize]) {
                                                            let path = Path::new(&path);
                                                            if let Some(ext) = path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()) {
                                                                if ["png", "jpg", "jpeg", "gif", "webp"].contains(&ext.as_str()) {
                                                                    if let Ok(data) = std::fs::read(path) {
                                                                        let mut last = last_image_clone.lock().unwrap();
                                                                        if last.as_ref() != Some(&data) {
                                                                            if let Some(image) = detect_image_type(&data) {
                                                                                if let Ok(mut cb) = cb_clone.lock() {
                                                                                    cb(ClipboardEvent::Paste(
                                                                                        ClipboardData::Image(image),
                                                                                        None,
                                                                                    ));
                                                                                }
                                                                                *last = Some(data);
                                                                                break 'outer;
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                                let _ = CloseClipboard();
                            }
                            std::thread::sleep(Duration::from_millis(100));
                        }
                    }
                }
            }
        });

        Clipboard {
            internal: WindowsClipboard { callback: callback.clone() },
            callback,
            last_plain_text,
            last_html_text,
            last_image,
            sender,
            handle,
        }
    }

    pub fn copy(&self) {
        let cb_clone = Arc::clone(&self.callback);
        let last_plain_text = Arc::clone(&self.last_plain_text);
        let last_html_text = Arc::clone(&self.last_html_text);
        let last_image = Arc::clone(&self.last_image);

        thread::spawn(move || {
            for _ in 0..5 {
                let mut is_new = false;

                let mut opened = false;
                for _ in 0..5 {
                    unsafe {
                        if OpenClipboard(None).is_ok() {
                            opened = true;
                            break;
                        }
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
                if opened {
                    unsafe {
                        let mut format = 0u32;
                        if EnumClipboardFormats(0) != 0 {
                            while {
                                format = EnumClipboardFormats(format);
                                format != 0
                            } {
                                match format {
                                    f if f == CF_UNICODETEXT => {
                                        if let Ok(text) = try_read_clipboard_text() {
                                            let last = last_plain_text.lock().unwrap();
                                            if last.as_ref() != Some(&text) {
                                                is_new = true;
                                                break;
                                            }
                                        }
                                    }
                                    f if f == RegisterClipboardFormatA(PCSTR(b"HTML Format\0".as_ptr())) => {
                                        if let Ok(data) = try_read_clipboard_format(f) {
                                            if let Ok(html) = String::from_utf8(data) {
                                                let last = last_html_text.lock().unwrap();
                                                if last.as_ref() != Some(&html) {
                                                    is_new = true;
                                                    break;
                                                }
                                            }
                                        }
                                    }
                                    f if f == RegisterClipboardFormatA(PCSTR(b"PNG\0".as_ptr()))
                                      || f == RegisterClipboardFormatA(PCSTR(b"image/png\0".as_ptr()))
                                      || f == RegisterClipboardFormatA(PCSTR(b"JPEG\0".as_ptr()))
                                      || f == RegisterClipboardFormatA(PCSTR(b"WEBP\0".as_ptr())) => {
                                        if let Ok(data) = try_read_clipboard_format(f) {
                                            let last = last_image.lock().unwrap();
                                            if last.as_ref() != Some(&data) {
                                                is_new = true;
                                                break;
                                            }
                                        }
                                    }
                                    f if f == CF_BITMAP => {
                                        if let Ok(data) = try_read_clipboard_image() {
                                            let last = last_image.lock().unwrap();
                                            if last.as_ref() != Some(&data) {
                                                is_new = true;
                                                break;
                                            }
                                        }
                                    }
                                    f if f == CF_HDROP => {
                                        let handle = GetClipboardData(CF_HDROP);
                                        if let Ok(hdrop) = handle {
                                            let hdrop = HDROP(hdrop.0);
                                            let file_count = DragQueryFileW(hdrop, 0xFFFFFFFF, None);
                                            for i in 0..file_count {
                                                let mut buf = [0u16; 260];
                                                let len = DragQueryFileW(hdrop, i, Some(&mut buf));
                                                if len > 0 {
                                                    if let Ok(path) = String::from_utf16(&buf[..len as usize]) {
                                                        let path = Path::new(&path);
                                                        if let Some(ext) = path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()) {
                                                            if ["png", "jpg", "jpeg", "gif", "webp"].contains(&ext.as_str()) {
                                                                if let Ok(data) = std::fs::read(path) {
                                                                    let last = last_image.lock().unwrap();
                                                                    if last.as_ref() != Some(&data) {
                                                                        is_new = true;
                                                                        break;
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    _ => {}
                                }
                                if is_new {
                                    break;
                                }
                            }
                        }
                        let _ = CloseClipboard();
                    }
                }

                if !is_new {
                    std::thread::sleep(Duration::from_secs(1));
                    continue;
                }

                let png_format = unsafe { RegisterClipboardFormatA(PCSTR(b"PNG\0".as_ptr())) };
                let jpeg_format = unsafe { RegisterClipboardFormatA(PCSTR(b"JPEG\0".as_ptr())) };
                let image_png_format = unsafe { RegisterClipboardFormatA(PCSTR(b"image/png\0".as_ptr())) };
                let webp_format = unsafe { RegisterClipboardFormatA(PCSTR(b"WEBP\0".as_ptr())) };
                let html_format = unsafe { RegisterClipboardFormatA(PCSTR(b"HTML Format\0".as_ptr())) };

                let mut opened = false;
                for _ in 0..5 {
                    unsafe {
                        if OpenClipboard(None).is_ok() {
                            opened = true;
                            break;
                        } else {
                            if let Ok(mut cb) = cb_clone.lock() {
                                cb(ClipboardEvent::FailedPasteHandling(ClipboardError::InUse));
                            }
                            return;
                        }
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
                if !opened {
                    if let Ok(mut cb) = cb_clone.lock() {
                        cb(ClipboardEvent::FailedPasteHandling(ClipboardError::OpenFailed));
                    }
                    return;
                }

                unsafe {
                    let mut format = 0u32;
                    if EnumClipboardFormats(0) == 0 {
                        let _ = CloseClipboard();
                        std::thread::sleep(Duration::from_millis(100));
                        continue;
                    }

                    while {
                        format = EnumClipboardFormats(format);
                        format != 0
                    } {
                        match format {
                            f if f == html_format => {
                                if let Ok(data) = try_read_clipboard_format(f) {
                                    if let Ok(html) = String::from_utf8(data) {
                                        let mut last = last_html_text.lock().unwrap();
                                        if last.as_ref() != Some(&html) {
                                            if let Ok(mut cb) = cb_clone.lock() {
                                                cb(ClipboardEvent::Paste(
                                                    ClipboardData::Text(Text::HTML(html.clone())),
                                                    None,
                                                ));
                                            }
                                            *last = Some(html);
                                            break;
                                        }
                                    }
                                }
                            }
                            f if f == CF_UNICODETEXT => {
                                if let Ok(text) = try_read_clipboard_text() {
                                    let mut last = last_plain_text.lock().unwrap();
                                    if last.as_ref() != Some(&text) {
                                        if let Ok(mut cb) = cb_clone.lock() {
                                            cb(ClipboardEvent::Paste(
                                                ClipboardData::Text(Text::Plain(text.clone())),
                                                None,
                                            ));
                                        }
                                        *last = Some(text);
                                        break;
                                    }
                                }
                            }
                            f if f == png_format || f == image_png_format || f == jpeg_format || f == webp_format => {
                                if let Ok(data) = try_read_clipboard_format(f) {
                                    if let Some(image) = detect_image_type(&data) {
                                        let mut last = last_image.lock().unwrap();
                                        if last.as_ref() != Some(&data) {
                                            if let Ok(mut cb) = cb_clone.lock() {
                                                cb(ClipboardEvent::Paste(
                                                    ClipboardData::Image(image),
                                                    None,
                                                ));
                                            }
                                            *last = Some(data);
                                            break;
                                        }
                                    }
                                }
                            }
                            f if f == CF_BITMAP => {
                                if let Ok(data) = try_read_clipboard_image() {
                                    let mut last = last_image.lock().unwrap();
                                    if last.as_ref() != Some(&data) {
                                        if let Ok(mut cb) = cb_clone.lock() {
                                            cb(ClipboardEvent::Paste(
                                                ClipboardData::Image(Image::BMP(data.clone())),
                                                None,
                                            ));
                                        }
                                        *last = Some(data);
                                        break;
                                    }
                                }
                            }
                            f if f == CF_HDROP => {
                                let handle = GetClipboardData(CF_HDROP);
                                if let Ok(hdrop) = handle {
                                    let hdrop = HDROP(hdrop.0);
                                    let file_count = DragQueryFileW(hdrop, 0xFFFFFFFF, None);
                                    for i in 0..file_count {
                                        let mut buf = [0u16; 260];
                                        let len = DragQueryFileW(hdrop, i, Some(&mut buf));
                                        if len > 0 {
                                            if let Ok(path) = String::from_utf16(&buf[..len as usize]) {
                                                let path = Path::new(&path);
                                                if let Some(ext) = path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()) {
                                                    if ["png", "jpg", "jpeg", "gif", "webp"].contains(&ext.as_str()) {
                                                        if let Ok(data) = std::fs::read(path) {
                                                            let mut last = last_image.lock().unwrap();
                                                            if last.as_ref() != Some(&data) {
                                                                if let Some(image) = detect_image_type(&data) {
                                                                    if let Ok(mut cb) = cb_clone.lock() {
                                                                        cb(ClipboardEvent::Paste(
                                                                            ClipboardData::Image(image),
                                                                            None,
                                                                        ));
                                                                    }
                                                                    *last = Some(data);
                                                                    break;
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    let _ = CloseClipboard();
                }
                break;
            }
        });
    }
}
