//! Screenshot capture module
//!
//! Captures full screen screenshots and converts them to base64 for LLM analysis.

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use screenshots::image::ImageOutputFormat;
use screenshots::Screen;
use std::io::Cursor;

/// Screenshot result with base64-encoded image data
#[derive(Debug, Clone, serde::Serialize)]
pub struct ScreenshotResult {
    pub base64_data: String,
    pub width: u32,
    pub height: u32,
    pub format: String,
}

/// Capture the primary screen as a PNG image
pub fn capture_screen() -> Result<ScreenshotResult, String> {
    // Get all screens
    let screens = Screen::all().map_err(|e| format!("Failed to get screens: {}", e))?;

    // Use the primary screen (first one)
    let screen = screens.first().ok_or("No screens found")?;

    // Capture the screen
    let image = screen
        .capture()
        .map_err(|e| format!("Failed to capture screen: {}", e))?;

    let width = image.width();
    let height = image.height();

    // Convert to PNG bytes
    let mut png_bytes = Cursor::new(Vec::new());
    image
        .write_to(&mut png_bytes, ImageOutputFormat::Png)
        .map_err(|e| format!("Failed to encode PNG: {}", e))?;

    // Encode to base64
    let base64_data = BASE64.encode(png_bytes.into_inner());

    println!("[Screenshot] Captured {}x{} image ({} bytes base64)", width, height, base64_data.len());

    Ok(ScreenshotResult {
        base64_data,
        width,
        height,
        format: "png".to_string(),
    })
}

/// Capture screen and return as data URL for direct use in HTML/LLM
pub fn capture_screen_as_data_url() -> Result<String, String> {
    let result = capture_screen()?;
    Ok(format!("data:image/png;base64,{}", result.base64_data))
}
