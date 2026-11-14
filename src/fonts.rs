use std::ops::Range;

use skrifa::instance::{LocationRef, Size};
use skrifa::{FontRef, MetadataProvider};

const REGULAR: &[u8] = include_bytes!("../assets/fonts/NotoSans-Regular.ttf");
const BOLD: &[u8] = include_bytes!("../assets/fonts/NotoSans-Bold.ttf");

/// A glyph placed by its advance width alone, with no kerning or ligatures.
pub struct PlacedGlyph {
    pub glyph_id: u32,
    /// Advance width in font units.
    pub advance_units: f32,
    pub text_range: Range<usize>,
}

/// An embedded font with the vertical metrics the layout needs, per unit of font size.
pub struct Face {
    data: &'static [u8],
    pub pdf_font: krilla::text::Font,
    units_per_em: f64,
    ascender: f64,
    descender: f64,
    line_gap: f64,
}

impl Face {
    fn load(data: &'static [u8]) -> Face {
        let font = FontRef::new(data).expect("embedded font is valid");
        let metrics = font.metrics(Size::unscaled(), LocationRef::default());
        let units_per_em = f64::from(metrics.units_per_em);
        let per_em = |value: f32| f64::from(value) / units_per_em;
        Face {
            data,
            pdf_font: krilla::text::Font::new(data.into(), 0).expect("embedded font is valid"),
            units_per_em,
            ascender: per_em(metrics.ascent),
            descender: per_em(metrics.descent),
            line_gap: per_em(metrics.leading),
        }
    }

    fn font(&self) -> FontRef<'static> {
        FontRef::new(self.data).expect("embedded font is valid")
    }

    pub fn ascender(&self, size: f64) -> f64 {
        self.ascender * size
    }

    /// Distance between baselines of consecutive lines.
    pub fn line_height(&self, size: f64) -> f64 {
        (self.ascender - self.descender + self.line_gap) * size
    }

    /// Maps each character to its glyph and advance width. Characters missing from
    /// the font use the `.notdef` glyph.
    pub fn glyphs(&self, text: &str) -> Vec<PlacedGlyph> {
        let font = self.font();
        let charmap = font.charmap();
        let metrics = font.glyph_metrics(Size::unscaled(), LocationRef::default());
        text.char_indices()
            .map(|(index, ch)| {
                let glyph = charmap.map(ch).unwrap_or_default();
                PlacedGlyph {
                    glyph_id: glyph.to_u32(),
                    advance_units: metrics.advance_width(glyph).unwrap_or_default(),
                    text_range: index..index + ch.len_utf8(),
                }
            })
            .collect()
    }

    /// Advance width as a fraction of the font size.
    pub fn advance(&self, glyph: &PlacedGlyph) -> f64 {
        f64::from(glyph.advance_units) / self.units_per_em
    }

    pub fn width(&self, text: &str, size: f64) -> f64 {
        self.glyphs(text).iter().map(|g| self.advance(g)).sum::<f64>() * size
    }
}

/// The two embedded Noto Sans weights.
pub struct Fonts {
    pub regular: Face,
    pub bold: Face,
}

impl Fonts {
    pub fn embedded() -> Fonts {
        Fonts { regular: Face::load(REGULAR), bold: Face::load(BOLD) }
    }

    pub fn face(&self, bold: bool) -> &Face {
        if bold { &self.bold } else { &self.regular }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noto_sans_metrics() {
        let fonts = Fonts::embedded();
        for face in [&fonts.regular, &fonts.bold] {
            assert!((face.ascender(1000.0) - 1069.0).abs() < 1e-6);
            assert!((face.line_height(1000.0) - 1362.0).abs() < 1e-6);
        }
    }

    #[test]
    fn measures_text() {
        let fonts = Fonts::embedded();
        // Values from the Prawn reference: the label column of the meta block.
        assert!((fonts.bold.width("Invoice No.:", 12.0) - 70.872).abs() < 1e-3);
        assert_eq!(fonts.regular.width("", 12.0), 0.0);
        let glyphs = fonts.regular.glyphs("Ľa");
        assert_eq!(glyphs[0].text_range, 0..2);
        assert_eq!(glyphs[1].text_range, 2..3);
        assert!(glyphs.iter().all(|g| g.glyph_id != 0));
    }
}
