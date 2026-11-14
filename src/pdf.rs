use krilla::Document;
use krilla::geom::{PathBuilder, Point};
use krilla::metadata::Metadata;
use krilla::page::PageSettings;
use krilla::paint::Stroke;
use krilla::text::{GlyphId, KrillaGlyph};

use crate::Error;
use crate::fonts::Fonts;
use crate::layout::{Op, PAGE_HEIGHT, PAGE_WIDTH};

/// Writes the laid-out page as a one-page A4 PDF.
pub fn render(ops: &[Op], fonts: &Fonts, title: &str) -> Result<Vec<u8>, Error> {
    let mut document = Document::new();
    document.set_metadata(
        Metadata::new()
            .title(title.to_string())
            .language("en".to_string())
            .creator(format!("picoinvoice {}", env!("CARGO_PKG_VERSION"))),
    );

    let settings = PageSettings::from_wh(PAGE_WIDTH as f32, PAGE_HEIGHT as f32).expect("A4 is a valid page size");
    let mut page = document.start_page_with(settings);
    let mut surface = page.surface();
    // krilla's origin is the top left corner; the layout's is the bottom left.
    let flip = |y: f64| (PAGE_HEIGHT - y) as f32;

    for op in ops {
        match op {
            Op::Text { x, y, size, bold, text } => {
                let face = fonts.face(*bold);
                let glyphs: Vec<KrillaGlyph> = face
                    .glyphs(text)
                    .into_iter()
                    .map(|g| {
                        let advance = face.advance(&g) as f32;
                        KrillaGlyph::new(GlyphId::new(g.glyph_id), advance, 0.0, 0.0, 0.0, g.text_range, None)
                    })
                    .collect();
                let start = Point::from_xy(*x as f32, flip(*y));
                surface.draw_glyphs(start, &glyphs, face.pdf_font.clone(), text, *size as f32, false);
            }
            Op::Line { x1, x2, y } => {
                let mut path = PathBuilder::new();
                path.move_to(*x1 as f32, flip(*y));
                path.line_to(*x2 as f32, flip(*y));
                let Some(path) = path.finish() else { continue };
                surface.set_fill(None);
                surface.set_stroke(Some(Stroke::default()));
                surface.draw_path(&path);
                surface.set_stroke(None);
            }
        }
    }

    surface.finish();
    page.finish();
    document.finish().map_err(|e| Error::new(format!("Cannot write the PDF: {e:?}")))
}
