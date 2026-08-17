use crate::errors::DomainError;

/// Maximum accepted logo upload size: 500KB. Enforced in the domain layer
/// so every upload path gets the same hard guarantee, not just whichever
/// HTTP route happened to remember a body limit.
pub const MAX_LOGO_IMAGE_SIZE: usize = 500 * 1024;

/// A validated logo image ready for object storage.
///
/// Validation is performed on the raw bytes using magic-byte detection,
/// never on the filename or client-supplied `Content-Type` header. That
/// prevents an attacker from uploading e.g. an SVG/HTML polyglot labelled
/// `image/png`, and prevents storing unsupported formats that would later
/// be served with a mismatched MIME type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogoImage {
    detected: LogoImageType,
    data: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogoImageType {
    Png,
    Jpeg,
    Gif,
    Webp,
}

impl LogoImage {
    pub const MAX_SIZE: usize = MAX_LOGO_IMAGE_SIZE;

    pub fn new(data: Vec<u8>) -> Result<Self, DomainError> {
        if data.is_empty() {
            return Err(DomainError::EmptyLogoImage);
        }

        if data.len() > Self::MAX_SIZE {
            return Err(DomainError::LogoImageTooLarge);
        }

        let detected = Self::detect_type(&data).ok_or(DomainError::UnsupportedLogoImageType)?;

        Ok(Self { detected, data })
    }

    pub fn data(&self) -> &[u8] {
        &self.data
    }

    pub fn content_type(&self) -> &'static str {
        self.detected.content_type()
    }

    pub fn file_extension(&self) -> &'static str {
        self.detected.file_extension()
    }

    fn detect_type(data: &[u8]) -> Option<LogoImageType> {
        if data.starts_with(b"\x89PNG\r\n\x1a\n") {
            return Some(LogoImageType::Png);
        }

        if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
            return Some(LogoImageType::Jpeg);
        }

        if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") {
            return Some(LogoImageType::Gif);
        }

        if data.len() >= 12 && data.starts_with(b"RIFF") && &data[8..12] == b"WEBP" {
            return Some(LogoImageType::Webp);
        }

        None
    }
}

impl LogoImageType {
    pub fn content_type(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
            Self::Gif => "image/gif",
            Self::Webp => "image/webp",
        }
    }

    pub fn file_extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::Gif => "gif",
            Self::Webp => "webp",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
    const JPEG_SIGNATURE: &[u8] = &[0xFF, 0xD8, 0xFF];
    const GIF_SIGNATURE: &[u8] = b"GIF89a";
    const WEBP_SIGNATURE: &[u8] = b"RIFF\x00\x00\x00\x00WEBP";

    #[test]
    fn accepts_png_by_magic_bytes() {
        let logo = LogoImage::new(PNG_SIGNATURE.to_vec()).unwrap();
        assert_eq!(logo.content_type(), "image/png");
        assert_eq!(logo.file_extension(), "png");
    }

    #[test]
    fn accepts_jpeg_by_magic_bytes() {
        let logo = LogoImage::new(JPEG_SIGNATURE.to_vec()).unwrap();
        assert_eq!(logo.content_type(), "image/jpeg");
        assert_eq!(logo.file_extension(), "jpg");
    }

    #[test]
    fn accepts_gif_by_magic_bytes() {
        let logo = LogoImage::new(GIF_SIGNATURE.to_vec()).unwrap();
        assert_eq!(logo.content_type(), "image/gif");
        assert_eq!(logo.file_extension(), "gif");
    }

    #[test]
    fn accepts_webp_by_magic_bytes() {
        let logo = LogoImage::new(WEBP_SIGNATURE.to_vec()).unwrap();
        assert_eq!(logo.content_type(), "image/webp");
        assert_eq!(logo.file_extension(), "webp");
    }

    #[test]
    fn rejects_empty_logo() {
        assert_eq!(LogoImage::new(vec![]), Err(DomainError::EmptyLogoImage));
    }

    #[test]
    fn rejects_larger_than_500kb() {
        let too_big = vec![0u8; MAX_LOGO_IMAGE_SIZE + 1];
        assert_eq!(LogoImage::new(too_big), Err(DomainError::LogoImageTooLarge));
    }

    #[test]
    fn rejects_unsupported_magic_bytes() {
        assert_eq!(
            LogoImage::new(b"not-an-image".to_vec()),
            Err(DomainError::UnsupportedLogoImageType)
        );
    }
}
