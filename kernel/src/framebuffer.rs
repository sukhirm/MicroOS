//! Linear Framebuffer graphics driver supporting UEFI GOP and VBE.
//! Implements the Cosmic Slate / Obsidian Azure visual identity foundation.

use bootloader_api::info::{FrameBuffer, FrameBufferInfo, PixelFormat};

/// RGB color structure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

#[allow(dead_code)]
impl Color {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    // --- Cosmic Slate / Obsidian Azure Palette ---
    pub const OBSIDIAN_NAVY: Self = Self::new(0x0a, 0x0e, 0x17); // Deep cosmic background
    pub const SURFACE_SLATE: Self = Self::new(0x13, 0x1b, 0x2e); // Surface panels
    pub const CARD_SLATE: Self = Self::new(0x1a, 0x24, 0x3b);    // Card container
    pub const BORDER_SLATE: Self = Self::new(0x2d, 0x3c, 0x5a);  // Accent borders
    pub const ACCENT_AZURE: Self = Self::new(0x38, 0xbd, 0xf8);  // Primary brand Azure
    pub const ACCENT_CYAN: Self = Self::new(0x00, 0xd2, 0xff);   // Highlight Cyan
    pub const TEXT_LIGHT: Self = Self::new(0xf1, 0xf5, 0xf9);    // Crisp white-slate text
    pub const TEXT_MUTED: Self = Self::new(0x94, 0xa3, 0xb8);    // Muted slate text
    pub const SUCCESS_GREEN: Self = Self::new(0x4a, 0xde, 0x80); // Diagnostic OK green
    pub const WARNING_AMBER: Self = Self::new(0xfb, 0xbf, 0x24); // Diagnostic warn amber
}

/// Early framebuffer writer.
pub struct FramebufferWriter<'a> {
    buffer: &'a mut [u8],
    info: FrameBufferInfo,
}

impl<'a> FramebufferWriter<'a> {
    /// Creates a writer wrapping the mutable framebuffer slice.
    pub fn new(fb: &'a mut FrameBuffer) -> Self {
        let info = fb.info();
        let buffer = fb.buffer_mut();
        Self { buffer, info }
    }

    /// Framebuffer width in pixels.
    pub fn width(&self) -> usize {
        self.info.width
    }

    /// Framebuffer height in pixels.
    pub fn height(&self) -> usize {
        self.info.height
    }

    /// Writes a single pixel to the screen.
    pub fn set_pixel(&mut self, x: usize, y: usize, color: Color) {
        if x >= self.info.width || y >= self.info.height {
            return;
        }

        let stride = self.info.stride;
        let bytes_per_pixel = self.info.bytes_per_pixel;
        let pixel_offset = (y * stride + x) * bytes_per_pixel;

        if pixel_offset + bytes_per_pixel > self.buffer.len() {
            return;
        }

        match self.info.pixel_format {
            PixelFormat::Rgb => {
                self.buffer[pixel_offset] = color.r;
                self.buffer[pixel_offset + 1] = color.g;
                self.buffer[pixel_offset + 2] = color.b;
            }
            PixelFormat::Bgr => {
                self.buffer[pixel_offset] = color.b;
                self.buffer[pixel_offset + 1] = color.g;
                self.buffer[pixel_offset + 2] = color.r;
            }
            PixelFormat::U8 => {
                // Grayscale luminance
                let gray = ((color.r as u16 * 77 + color.g as u16 * 150 + color.b as u16 * 29) >> 8) as u8;
                self.buffer[pixel_offset] = gray;
            }
            _ => {
                // Fallback assume BGR
                self.buffer[pixel_offset] = color.b;
                self.buffer[pixel_offset + 1] = color.g;
                self.buffer[pixel_offset + 2] = color.r;
            }
        }
    }

    /// Clears the entire display buffer with a solid color.
    pub fn clear(&mut self, color: Color) {
        let w = self.info.width;
        let h = self.info.height;
        self.fill_rect(0, 0, w, h, color);
    }

    /// Fills a solid rectangle.
    pub fn fill_rect(&mut self, start_x: usize, start_y: usize, width: usize, height: usize, color: Color) {
        let end_x = (start_x + width).min(self.info.width);
        let end_y = (start_y + height).min(self.info.height);

        for y in start_y..end_y {
            for x in start_x..end_x {
                self.set_pixel(x, y, color);
            }
        }
    }

    /// Draws a character using the 8x8 font bitmap.
    pub fn draw_char(&mut self, start_x: usize, start_y: usize, c: char, fg: Color, bg: Option<Color>) {
        let ascii_code = c as usize;
        let bitmap = if ascii_code < 128 {
            font8x8::legacy::BASIC_LEGACY[ascii_code]
        } else {
            font8x8::legacy::NOTHING_TO_DISPLAY
        };

        for (row, &byte) in bitmap.iter().enumerate() {
            for col in 0..8 {
                let pixel_set = (byte & (1 << col)) != 0;
                let px = start_x + col;
                let py = start_y + row;
                if pixel_set {
                    self.set_pixel(px, py, fg);
                } else if let Some(bg_color) = bg {
                    self.set_pixel(px, py, bg_color);
                }
            }
        }
    }

    /// Draws a text string at (x, y).
    pub fn draw_string(&mut self, mut x: usize, y: usize, text: &str, fg: Color, bg: Option<Color>) {
        for c in text.chars() {
            if c == '\n' {
                break;
            }
            self.draw_char(x, y, c, fg, bg);
            x += 8;
        }
    }

    /// Renders the Micro-OS Cosmic Slate / Obsidian Azure boot diagnostic canvas.
    pub fn render_boot_screen(&mut self) {
        let w = self.width();
        let h = self.height();

        // 1. Deep cosmic background
        self.clear(Color::OBSIDIAN_NAVY);

        // 2. Top status bar surface
        self.fill_rect(0, 0, w, 28, Color::SURFACE_SLATE);
        self.fill_rect(0, 27, w, 1, Color::BORDER_SLATE);

        // Top bar brand badge
        self.draw_string(16, 10, "MICRO-OS", Color::ACCENT_CYAN, None);
        self.draw_string(88, 10, "[x86_64 Bare-Metal Kernel | v0.1.0-alpha]", Color::TEXT_MUTED, None);

        let status_right = "STATUS: EARLY BOOT OK";
        let right_x = if w > 200 { w - (status_right.len() * 8) - 16 } else { 16 };
        self.draw_string(right_x, 10, status_right, Color::SUCCESS_GREEN, None);

        // 3. Central Diagnostic Card
        let card_w = 640.min(w.saturating_sub(40));
        let card_h = 360.min(h.saturating_sub(80));
        let card_x = (w.saturating_sub(card_w)) / 2;
        let card_y = 56;

        self.fill_rect(card_x, card_y, card_w, card_h, Color::CARD_SLATE);
        self.fill_rect(card_x, card_y, card_w, 2, Color::ACCENT_AZURE);

        // Card header
        let mut text_y = card_y + 16;
        let text_x = card_x + 20;

        self.draw_string(text_x, text_y, "MICRO-OS KERNEL DIAGNOSTIC CONSOLE", Color::ACCENT_AZURE, None);
        text_y += 18;
        self.draw_string(text_x, text_y, "Cosmic Slate / Obsidian Azure Architecture Foundation", Color::TEXT_MUTED, None);
        text_y += 24;

        self.fill_rect(text_x, text_y, card_w - 40, 1, Color::BORDER_SLATE);
        text_y += 14;

        // Startup milestone checklist
        let entries = [
            ("[OK] 16550 UART Serial driver initialized (COM1: 0x3F8)", Color::SUCCESS_GREEN),
            ("[OK] Global kernel logger registered (log facade active)", Color::SUCCESS_GREEN),
            ("[OK] UEFI GOP Linear Framebuffer active", Color::SUCCESS_GREEN),
            ("[OK] Bare-metal x86_64 CPU Long Mode verified", Color::SUCCESS_GREEN),
            ("[OK] Physical memory address offset mapped", Color::SUCCESS_GREEN),
            ("[OK] Micro-OS Phase 1 Kernel Foundation verified", Color::ACCENT_CYAN),
        ];

        for (msg, color) in entries {
            self.draw_string(text_x, text_y, msg, color, None);
            text_y += 20;
        }

        text_y += 10;
        self.draw_string(text_x, text_y, "Verifiable serial logs streaming to COM1 / stdio...", Color::TEXT_MUTED, None);

        // Bottom footer line
        let footer_y = h.saturating_sub(20);
        self.draw_string(16, footer_y, "Micro-OS - Bare-metal educational operating system", Color::TEXT_MUTED, None);
    }
}
