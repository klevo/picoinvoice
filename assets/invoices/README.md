# Reference invoices

These PDFs were made by picoinvoice's original Ruby implementation (Prawn) from
the made-up data in `examples/picoinvoice.yml` and `tests/fixtures/`. The tests in
`tests/reference.rs` regenerate each one and check that the words, their positions
and the rendered page still match.

They are a fixed record of the original layout: don't regenerate them. If the
layout changes on purpose, update the tests instead.

| File | Config | `--date` | `--hours` | Number |
|---|---|---|---|---|
| `sample.pdf` | `examples/picoinvoice.yml` | 2026-10-01 | 168 | 2026009 |
| `minimal.pdf` | `tests/fixtures/minimal.yml` | 2026-06-01 | 8 | 2026005 |
| `diacritics.pdf` | `tests/fixtures/diacritics.yml` | 2027-01-04 | 160 | 2027001 |
| `large.pdf` | `tests/fixtures/large.yml` | 2026-03-02 | 220 | 2026002 |
