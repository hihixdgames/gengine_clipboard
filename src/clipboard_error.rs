#[derive(Debug)]
pub enum ClipboardError {
    OpenFailed,
    FormatNotAvailable,
    LockFailed,
    ReadFailed,
    Utf16ConversionFailed,
    Empty,
    InUse,
    Unknown(String),
}
