//! Compares generated invoices with the reference PDFs in `assets/invoices/`, made by
//! the original Ruby/Prawn implementation, and with the README sample in `docs/`.
//!
//! Both PDFs are read with poppler (`pdftotext`, `pdftoppm`): every word must match
//! and sit within `POSITION_TOLERANCE` points of the reference, and the rendered pages
//! must look nearly identical. On failure the generated PDF and a difference image are
//! left in `target/reference-diffs/`.

use std::path::{Path, PathBuf};
use std::process::Command;

use jiff::civil::Date;
use picoinvoice::fonts::Fonts;
use picoinvoice::layout::{self, Op};
use picoinvoice::{Config, Invoice};

/// Largest allowed distance between a word's position and the reference, in points.
const POSITION_TOLERANCE: f64 = 0.5;
/// Largest allowed mean difference between the rendered pages, as a fraction of
/// black-on-white. A missing horizontal rule alone comes to about 0.001.
const MAX_MEAN_PIXEL_DIFFERENCE: f64 = 0.0002;

struct Case {
    name: &'static str,
    config: &'static str,
    date: &'static str,
    hours: u32,
    number: &'static str,
}

const CASES: &[Case] = &[
    Case { name: "sample", config: "examples/picoinvoice.yml", date: "2026-10-01", hours: 168, number: "2026009" },
    Case { name: "minimal", config: "tests/fixtures/minimal.yml", date: "2026-06-01", hours: 8, number: "2026005" },
    Case {
        name: "diacritics",
        config: "tests/fixtures/diacritics.yml",
        date: "2027-01-04",
        hours: 160,
        number: "2027001",
    },
    Case { name: "large", config: "tests/fixtures/large.yml", date: "2026-03-02", hours: 220, number: "2026002" },
];

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn out_dir() -> PathBuf {
    let dir = root().join("target/reference-diffs");
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn reference(case: &Case) -> PathBuf {
    root().join(format!("assets/invoices/{}.pdf", case.name))
}

/// Lays out a case, lets `edit` change the drawing operations, and writes the PDF
/// to `target/reference-diffs/<file_name>.pdf`.
fn render_case(case: &Case, file_name: &str, edit: impl FnOnce(&mut Vec<Op>)) -> PathBuf {
    let config = Config::load(&root().join(case.config)).unwrap();
    let date: Date = case.date.parse().unwrap();
    let invoice = Invoice::new(&config, case.number.to_string(), date, case.hours).unwrap();
    let fonts = Fonts::embedded();
    let mut ops = layout::layout(&invoice, &config, &fonts);
    edit(&mut ops);
    let pdf = picoinvoice::pdf::render(&ops, &fonts, "test").unwrap();
    let path = out_dir().join(format!("{file_name}.pdf"));
    std::fs::write(&path, pdf).unwrap();
    path
}

fn poppler(tool: &str, args: &[&str]) -> Vec<u8> {
    let output = Command::new(tool).args(args).output().unwrap_or_else(|e| {
        panic!(
            "running `{tool}` failed ({e}). These tests need poppler: \
             `brew install poppler` on macOS, `apt-get install poppler-utils` on Debian/Ubuntu."
        )
    });
    assert!(output.status.success(), "{tool} {args:?} failed: {}", String::from_utf8_lossy(&output.stderr));
    output.stdout
}

#[derive(Debug, Clone)]
struct Word {
    text: String,
    bbox: [f64; 4],
}

fn words(pdf: &Path) -> Vec<Word> {
    let html = String::from_utf8(poppler("pdftotext", &["-bbox", pdf.to_str().unwrap(), "-"])).unwrap();
    html.lines()
        .filter_map(|line| line.trim().strip_prefix("<word "))
        .map(|line| {
            let attr = |name: &str| -> f64 {
                let start = line.find(&format!("{name}=\"")).unwrap() + name.len() + 2;
                let end = start + line[start..].find('"').unwrap();
                line[start..end].parse().unwrap()
            };
            let text_start = line.find('>').unwrap() + 1;
            let text_end = line.rfind("</word>").unwrap();
            Word {
                text: unescape(&line[text_start..text_end]),
                bbox: [attr("xMin"), attr("yMin"), attr("xMax"), attr("yMax")],
            }
        })
        .collect()
}

fn unescape(text: &str) -> String {
    text.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&")
}

/// Describes every word that is missing, extra, or out of place.
fn compare_words(expected: &[Word], actual: &[Word]) -> Vec<String> {
    let mut problems = Vec::new();
    let mut unused: Vec<Option<&Word>> = actual.iter().map(Some).collect();
    for word in expected {
        let distance = |other: &Word| word.bbox.iter().zip(other.bbox).map(|(a, b)| (a - b).abs()).fold(0.0, f64::max);
        let best = unused
            .iter()
            .enumerate()
            .filter_map(|(i, w)| w.filter(|w| w.text == word.text).map(|w| (i, distance(w))))
            .min_by(|a, b| a.1.total_cmp(&b.1));
        match best {
            Some((i, d)) => {
                if d > POSITION_TOLERANCE {
                    let found = unused[i].unwrap().bbox;
                    problems
                        .push(format!("`{}` is {d:.3}pt off: expected {:?}, found {found:?}", word.text, word.bbox));
                }
                unused[i] = None;
            }
            None => problems.push(format!("`{}` at {:?} is missing", word.text, word.bbox)),
        }
    }
    problems.extend(unused.into_iter().flatten().map(|w| format!("unexpected `{}` at {:?}", w.text, w.bbox)));
    problems
}

struct Gray {
    width: usize,
    height: usize,
    pixels: Vec<u8>,
}

fn rasterize(pdf: &Path) -> Gray {
    let pgm = poppler("pdftoppm", &["-r", "72", "-gray", pdf.to_str().unwrap()]);
    // Binary PGM: "P5 <width> <height> <max>", one whitespace byte, then one byte per pixel.
    let mut fields = Vec::new();
    let mut pos = 0;
    while fields.len() < 4 {
        while pgm[pos].is_ascii_whitespace() {
            pos += 1;
        }
        let start = pos;
        while !pgm[pos].is_ascii_whitespace() {
            pos += 1;
        }
        fields.push(String::from_utf8(pgm[start..pos].to_vec()).unwrap());
    }
    assert_eq!(fields[0], "P5");
    let (width, height): (usize, usize) = (fields[1].parse().unwrap(), fields[2].parse().unwrap());
    let pixels = pgm[pos + 1..].to_vec();
    assert_eq!(pixels.len(), width * height);
    Gray { width, height, pixels }
}

/// Mean absolute difference as a fraction of white, and an image of the differences
/// (dark where the pages differ).
fn compare_pixels(expected: &Gray, actual: &Gray) -> (f64, Vec<u8>) {
    assert_eq!((expected.width, expected.height), (actual.width, actual.height), "page sizes differ");
    let diff: Vec<u8> = expected.pixels.iter().zip(&actual.pixels).map(|(a, b)| 255 - a.abs_diff(*b)).collect();
    let total: u64 = diff.iter().map(|d| u64::from(255 - d)).sum();
    (total as f64 / (255.0 * diff.len() as f64), diff)
}

/// Describes how `actual_pdf` differs from `expected_pdf`; empty when they match.
fn differences(name: &str, expected_pdf: &Path, actual_pdf: &Path) -> Vec<String> {
    let mut problems = compare_words(&words(expected_pdf), &words(actual_pdf));

    let (expected, actual) = (rasterize(expected_pdf), rasterize(actual_pdf));
    let (difference, diff) = compare_pixels(&expected, &actual);
    if difference > MAX_MEAN_PIXEL_DIFFERENCE {
        let path = out_dir().join(format!("{name}-diff.pgm"));
        let mut pgm = format!("P5\n{} {}\n255\n", expected.width, expected.height).into_bytes();
        pgm.extend(diff);
        std::fs::write(&path, pgm).unwrap();
        problems.push(format!(
            "pages differ by {:.4}% (limit {:.4}%), see {}",
            difference * 100.0,
            MAX_MEAN_PIXEL_DIFFERENCE * 100.0,
            path.display()
        ));
    }

    problems
        .into_iter()
        .map(|p| format!("{name}: {p}\n    (generated {}, reference {})", actual_pdf.display(), expected_pdf.display()))
        .collect()
}

#[test]
fn matches_the_reference_invoices() {
    let problems: Vec<String> = CASES
        .iter()
        .flat_map(|case| differences(case.name, &reference(case), &render_case(case, case.name, |_| ())))
        .collect();
    assert!(problems.is_empty(), "\n{}", problems.join("\n"));
}

/// The README shows `docs/sample-invoice.png`, made from `docs/sample-invoice.pdf` by
/// `mise run sample`. This fails when a layout change makes the sample outdated.
#[test]
fn readme_sample_is_up_to_date() {
    let sample = root().join("docs/sample-invoice.pdf");
    assert!(sample.exists(), "{} is missing, run `mise run sample`", sample.display());
    let generated = render_case(&CASES[0], "readme-sample", |_| ());
    let problems = differences("readme-sample", &sample, &generated);
    assert!(problems.is_empty(), "\n{}\nRun `mise run sample` if the change is intended.", problems.join("\n"));
}

/// Makes sure the comparison is strict enough to notice small layout mistakes.
#[test]
fn notices_small_layout_changes() {
    let case = &CASES[0];

    let without_rule = render_case(case, "check-missing-rule", |ops| {
        let last_rule = ops.iter().rposition(|op| matches!(op, Op::Line { .. })).unwrap();
        ops.remove(last_rule);
    });
    let problems = differences("check-missing-rule", &reference(case), &without_rule);
    assert!(problems.iter().any(|p| p.contains("pages differ")), "a missing rule went unnoticed");

    let shifted = render_case(case, "check-shifted-word", |ops| {
        for op in ops.iter_mut() {
            if let Op::Text { x, text, .. } = op
                && text == "Bill To"
            {
                *x += 1.0;
            }
        }
    });
    let problems = differences("check-shifted-word", &reference(case), &shifted);
    assert!(problems.iter().any(|p| p.contains("`Bill` is 1.000pt off")), "a 1pt shift went unnoticed: {problems:?}");
}
