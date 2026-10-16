//! Terminal ASCII and Unicode QR code rendering for list sharing.

use qrcode::render::unicode;
use qrcode::QrCode;
use crate::SyncError;

/// Renders a string (such as an iroh-docs `DocTicket`) into a compact, high-contrast
/// Unicode QR code using half-block characters suitable for terminal output.
pub fn render_qr_terminal(content: &str) -> Result<String, SyncError> {
    let code = QrCode::new(content.as_bytes())
        .map_err(|e| SyncError::Other(format!("Failed to generate QR code: {e}")))?;

    let rendered = code
        .render::<unicode::Dense1x2>()
        .dark_color(unicode::Dense1x2::Light)
        .light_color(unicode::Dense1x2::Dark)
        .build();

    Ok(rendered)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_qr() {
        let text = "https://groshe.app/join/test-ticket";
        let qr = render_qr_terminal(text).unwrap();
        assert!(!qr.is_empty());
        assert!(qr.contains('▀') || qr.contains('▄') || qr.contains('█'));
    }
}