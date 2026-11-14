//! Lays out the invoice page as positioned text and lines.
//!
//! Coordinates are PDF points with the origin at the bottom left. The drawing
//! model follows the original Prawn implementation: a cursor moves down the page,
//! flowing text puts its first baseline one ascender below the cursor and then
//! advances by the line height, and `draw_text` puts its baseline at the cursor.

use crate::config::{Address, Config};
use crate::fonts::Fonts;
use crate::invoice::Invoice;

pub const PAGE_WIDTH: f64 = 595.28;
pub const PAGE_HEIGHT: f64 = 841.89;
const MARGIN: f64 = 50.0;
const FONT_SIZE: f64 = 12.0;

#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    /// Text with its baseline starting at `(x, y)`.
    Text { x: f64, y: f64, size: f64, bold: bool, text: String },
    /// A horizontal line, 1pt wide.
    Line { x1: f64, x2: f64, y: f64 },
}

#[derive(Clone, Copy)]
enum Align {
    Left,
    Right,
}

struct Canvas<'a> {
    fonts: &'a Fonts,
    ops: Vec<Op>,
    cursor: f64,
    left: f64,
    width: f64,
}

impl Canvas<'_> {
    fn right(&self) -> f64 {
        self.left + self.width
    }

    fn move_down(&mut self, amount: f64) {
        self.cursor -= amount;
    }

    fn width_of(&self, text: &str, size: f64, bold: bool) -> f64 {
        self.fonts.face(bold).width(text, size)
    }

    /// Flowing text, wrapped to the current bounds.
    fn text(&mut self, text: &str, bold: bool) {
        let face = self.fonts.face(bold);
        for line in wrap(text, self.width, |s| face.width(s, FONT_SIZE)) {
            let y = self.cursor - face.ascender(FONT_SIZE);
            self.ops.push(Op::Text { x: self.left, y, size: FONT_SIZE, bold, text: line });
            self.move_down(face.line_height(FONT_SIZE));
        }
    }

    /// Text with its baseline at the cursor. Does not move the cursor.
    fn draw_text(&mut self, text: &str, x: f64, size: f64, bold: bool) {
        self.ops.push(Op::Text { x, y: self.cursor, size, bold, text: text.to_string() });
    }

    /// A single line of text in a box whose top is at the cursor. Text that does
    /// not fit the width is cut at the end of the first wrapped line.
    fn text_box(&mut self, text: &str, x: f64, width: f64, size: f64, bold: bool, align: Align) {
        let face = self.fonts.face(bold);
        let Some(line) = wrap(text, width, |s| face.width(s, size)).into_iter().next() else { return };
        let x = match align {
            Align::Left => x,
            Align::Right => x + width - face.width(&line, size),
        };
        let y = self.cursor - face.ascender(size);
        self.ops.push(Op::Text { x, y, size, bold, text: line });
    }

    fn horizontal_rule(&mut self) {
        self.ops.push(Op::Line { x1: self.left, x2: self.right(), y: self.cursor });
    }

    /// Runs `draw` in a box of fixed height at the cursor, then moves the cursor
    /// to the bottom of the box.
    fn bounding_box(&mut self, left: f64, width: f64, height: f64, draw: impl FnOnce(&mut Self)) {
        let (top, outer_left, outer_width) = (self.cursor, self.left, self.width);
        self.left = left;
        self.width = width;
        draw(self);
        self.left = outer_left;
        self.width = outer_width;
        self.cursor = top - height;
    }
}

/// Greedy word wrap. Words wider than the line are broken between characters.
fn wrap(text: &str, max_width: f64, width: impl Fn(&str) -> f64) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        let candidate = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
        if width(&candidate) <= max_width {
            line = candidate;
            continue;
        }
        if !line.is_empty() {
            lines.push(std::mem::take(&mut line));
        }
        for ch in word.chars() {
            let candidate = format!("{line}{ch}");
            if !line.is_empty() && width(&candidate) > max_width {
                lines.push(std::mem::take(&mut line));
                line.push(ch);
            } else {
                line = candidate;
            }
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

pub fn layout(invoice: &Invoice, config: &Config, fonts: &Fonts) -> Vec<Op> {
    let mut canvas =
        Canvas { fonts, ops: Vec::new(), cursor: PAGE_HEIGHT - MARGIN, left: MARGIN, width: PAGE_WIDTH - 2.0 * MARGIN };
    let c = &mut canvas;

    let title = fonts.bold.ascender(26.0);
    c.ops.push(Op::Text { x: c.left, y: c.cursor - title, size: 26.0, bold: true, text: "Invoice".into() });
    c.move_down(fonts.bold.line_height(26.0));
    c.move_down(6.0);
    draw_meta(c, invoice);

    c.move_down(20.0);
    draw_addresses(c, config);

    c.move_down(10.0);
    c.horizontal_rule();
    c.move_down(12.0);
    draw_item(c, invoice);
    c.move_down(12.0);
    c.horizontal_rule();
    c.move_down(32.0);
    draw_totals(c, invoice);
    c.move_down(18.0);
    draw_payment_details(c, config);

    canvas.ops
}

/// Right-aligned label/value table at the cursor: bold labels aligned right, values left.
fn draw_table(c: &mut Canvas, rows: &[(&str, String, f64, bool)]) {
    const GAP: f64 = 8.0;
    const ROW_HEIGHT: f64 = 16.0;
    let label_width = rows.iter().map(|(label, _, size, _)| c.width_of(label, *size, true)).fold(0.0, f64::max);
    let value_width = rows.iter().map(|(_, value, size, bold)| c.width_of(value, *size, *bold)).fold(0.0, f64::max);
    let x = c.right() - (label_width + GAP + value_width);
    for (label, value, size, bold) in rows {
        let label_x = x + label_width - c.width_of(label, *size, true);
        c.draw_text(label, label_x, *size, true);
        c.draw_text(value, x + label_width + GAP, *size, *bold);
        c.move_down(ROW_HEIGHT);
    }
}

fn draw_meta(c: &mut Canvas, invoice: &Invoice) {
    draw_table(
        c,
        &[
            ("Invoice No.:", invoice.number.clone(), FONT_SIZE, false),
            ("Issue Date:", invoice.issue_date.to_string(), FONT_SIZE, false),
            ("Due Date:", invoice.due_date.to_string(), FONT_SIZE, false),
        ],
    );
}

fn draw_addresses(c: &mut Canvas, config: &Config) {
    const COLUMN_GAP: f64 = 20.0;
    const BOX_HEIGHT: f64 = 150.0;
    let column_width = (c.width - COLUMN_GAP) / 2.0;
    let top = c.cursor;
    let from = &config.from;

    c.bounding_box(c.left, column_width, BOX_HEIGHT, |c| {
        c.text("From", true);
        c.text(&from.name, false);
        draw_address(c, &from.address);
        c.move_down(4.0);
        if let Some(number) = &from.registration_number {
            c.text(&format!("Business Registration No.: {number}"), false);
        }
        if let Some(number) = &from.tax_number {
            c.text(&format!("Tax Identification No.: {number}"), false);
        }
        for note in &from.notes {
            c.text(note, false);
        }
    });

    c.cursor = top;
    c.bounding_box(c.left + column_width + COLUMN_GAP, column_width, BOX_HEIGHT, |c| {
        c.text("Bill To", true);
        c.text(&config.bill_to.name, false);
        draw_address(c, &config.bill_to.address);
    });

    c.move_down(10.0);
}

fn draw_address(c: &mut Canvas, address: &Address) {
    for line in address.lines() {
        c.text(&line, false);
    }
}

fn draw_item(c: &mut Canvas, invoice: &Invoice) {
    const ROW_HEIGHT: f64 = 20.0;
    const SIZE: f64 = 11.0;
    let ratios = [0.5, 0.12, 0.18];
    let mut widths: Vec<f64> = ratios.iter().map(|r| (c.width * r).round()).collect();
    widths.push(c.width - widths.iter().sum::<f64>());
    let aligns = [Align::Left, Align::Right, Align::Right, Align::Right];

    let header = ["Description", "Qty", "Unit Price", "Amount"].map(String::from);
    let values = [
        invoice.description.clone(),
        format!("{} hrs", invoice.hours),
        invoice.unit_price.to_string(),
        invoice.total.to_string(),
    ];

    for (row, bold) in [(header, true), (values, false)] {
        let mut x = c.left;
        for ((text, width), align) in row.iter().zip(&widths).zip(aligns) {
            c.text_box(text, x, *width, SIZE, bold, align);
            x += width;
        }
        c.move_down(ROW_HEIGHT);
    }
}

fn draw_totals(c: &mut Canvas, invoice: &Invoice) {
    let total = invoice.total.to_string();
    draw_table(
        c,
        &[
            ("Subtotal", total.clone(), FONT_SIZE, false),
            ("Tax (0%)", invoice.total.zero_label(), FONT_SIZE, false),
            ("Total Due", total, FONT_SIZE + 1.0, true),
        ],
    );
}

fn draw_payment_details(c: &mut Canvas, config: &Config) {
    let Some(bank) = &config.from.bank_details else { return };
    c.move_down(10.0);
    c.text("Bank Transfer To", true);
    let lines = [
        ("Bank Name", &bank.bank_name),
        ("Account Holder", &bank.account_holder_name),
        ("Account Number", &bank.account_number),
        ("SWIFT", &bank.swift_code),
    ];
    for (label, value) in lines {
        if let Some(value) = value {
            c.text(&format!("{label}: {value}"), false);
        }
    }
    if let Some(address) = &bank.address {
        c.text("Bank Address", true);
        draw_address(c, address);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chars(s: &str) -> f64 {
        s.chars().count() as f64
    }

    #[test]
    fn wraps_words() {
        assert_eq!(wrap("one two three", 7.0, chars), ["one two", "three"]);
        assert_eq!(wrap("  one   two ", 20.0, chars), ["one two"]);
        assert!(wrap("", 10.0, chars).is_empty());
    }

    #[test]
    fn breaks_long_words() {
        assert_eq!(wrap("abcdefgh ij", 3.0, chars), ["abc", "def", "gh", "ij"]);
        assert_eq!(wrap("x", 0.0, chars), ["x"]);
    }
}
