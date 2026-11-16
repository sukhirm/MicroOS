//! Color Space Representation and Palette Definitions.
//!
//! Provides RGBA color representations, alpha blending, format serialization,
//! and the official Cosmic Slate / Obsidian Azure design system palette.

/// Represents an 8-bit RGBA color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

#[allow(dead_code)]
impl Color {
    /// Creates a fully opaque color from red, green, and blue components.
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    /// Creates a color with custom alpha transparency.
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// Converts this color to a 32-bit RGB value (`0x00RRGGBB`).
    pub const fn to_u32_rgb(&self) -> u32 {
        ((self.r as u32) << 16) | ((self.g as u32) << 8) | (self.b as u32)
    }

    /// Converts this color to a 32-bit BGR value (`0x00BBGGRR`).
    pub const fn to_u32_bgr(&self) -> u32 {
        ((self.b as u32) << 16) | ((self.g as u32) << 8) | (self.r as u32)
    }

    /// Constructs a color from a 32-bit RGB value (`0x00RRGGBB`).
    pub const fn from_u32_rgb(val: u32) -> Self {
        Self {
            r: ((val >> 16) & 0xFF) as u8,
            g: ((val >> 8) & 0xFF) as u8,
            b: (val & 0xFF) as u8,
            a: 255,
        }
    }

    /// Constructs a color from a 32-bit BGR value (`0x00BBGGRR`).
    pub const fn from_u32_bgr(val: u32) -> Self {
        Self {
            b: ((val >> 16) & 0xFF) as u8,
            g: ((val >> 8) & 0xFF) as u8,
            r: (val & 0xFF) as u8,
            a: 255,
        }
    }

    /// Alpha-blends this foreground color over a background color.
    ///
    /// Computes $C_{\text{out}} = \frac{\alpha \cdot C_{\text{fg}} + (255 - \alpha) \cdot C_{\text{bg}}}{255}$.
    pub fn blend_over(&self, bg: Color) -> Color {
        if self.a == 255 {
            return *self;
        }
        if self.a == 0 {
            return bg;
        }

        let alpha = self.a as u32;
        let inv_alpha = 255 - alpha;

        let r = ((self.r as u32 * alpha + bg.r as u32 * inv_alpha) / 255) as u8;
        let g = ((self.g as u32 * alpha + bg.g as u32 * inv_alpha) / 255) as u8;
        let b = ((self.b as u32 * alpha + bg.b as u32 * inv_alpha) / 255) as u8;

        Color::new(r, g, b)
    }

    // =========================================================================
    // Cosmic Slate / Obsidian Azure Design System Palette
    // =========================================================================

    /// Deep cosmic dark navy background.
    pub const OBSIDIAN_NAVY: Self = Self::new(0x0a, 0x0e, 0x17);

    /// Elevated surface panel background.
    pub const SURFACE_SLATE: Self = Self::new(0x13, 0x1b, 0x2e);

    /// Card and window container background.
    pub const CARD_SLATE: Self = Self::new(0x1a, 0x24, 0x3b);

    /// Subtle window and widget divider border.
    pub const BORDER_SLATE: Self = Self::new(0x2d, 0x3c, 0x5a);

    /// Active window border and focus indicator.
    pub const BORDER_ACTIVE: Self = Self::new(0x38, 0xbd, 0xf8);

    /// Primary brand Azure highlight.
    pub const ACCENT_AZURE: Self = Self::new(0x38, 0xbd, 0xf8);

    /// Radiant Cyan badge and brand accent.
    pub const ACCENT_CYAN: Self = Self::new(0x00, 0xd2, 0xff);

    /// Primary crisp text on dark backgrounds.
    pub const TEXT_LIGHT: Self = Self::new(0xf1, 0xf5, 0xf9);

    /// Muted secondary text and metadata labels.
    pub const TEXT_MUTED: Self = Self::new(0x94, 0xa3, 0xb8);

    /// Diagnostic and system success green.
    pub const SUCCESS_GREEN: Self = Self::new(0x4a, 0xde, 0x80);

    /// Warning amber status indicator.
    pub const WARNING_AMBER: Self = Self::new(0xfb, 0xbf, 0x24);

    /// Error and window close danger red.
    pub const DANGER_RED: Self = Self::new(0xf8, 0x71, 0x71);

    /// Pure white.
    pub const WHITE: Self = Self::new(0xff, 0xff, 0xff);

    /// Pure black.
    pub const BLACK: Self = Self::new(0x00, 0x00, 0x00);

    /// Fully transparent sentinel color.
    pub const TRANSPARENT: Self = Self::rgba(0x00, 0x00, 0x00, 0x00);
}
