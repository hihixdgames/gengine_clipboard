#[derive(Debug)]
pub enum ClipboardError {
	Timeout,
	/// The clipboard, with which this program is trying to communicate, fails to uphold the agreed behaviour.
	ForeignClipboardError,
	AllocationFailed,
	SetFailed,
	OpenFailed,
	FormatNotAvailable,
	LockFailed,
	ReadFailed,
	Utf16ConversionFailed,
	Empty,
	InUse,
	WriteFailed,
	UnsupportedMimeType,
	Unknown(String),
}
