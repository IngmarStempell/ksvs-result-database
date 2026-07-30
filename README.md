# ksvs-result-database
The Kreisschützenverband Stormarn is providing a tool to read the data provided by the umbrealla association and display it as html

# PDF Explorer

Rust workspace for parsing PDFs. The first milestone is plain text extraction from digital PDFs. The structure leaves room for OCR, web scraping, storage, and a UI without mixing those concerns into the parser.

## Run

```bash
cargo run -- parse ./examples/document.pdf
```

JSON output:

```bash
cargo run -- parse ./examples/document.pdf --format json
```

Structured DAVID21+ sport result output:

```bash
cargo run -- parse-sport ./Data/_sport2025_ergebnisse_ergeb_haupt_2025_1.10.10.pdf
```

Follow a page, download linked PDFs, detect changes, classify formats, and write a JSON report:

```bash
cargo run -- crawl-report "https://example.org/results"
```

For the current NDSB source:

```bash
cargo run -- crawl-report "https://www.ndsb-sh.de/sport/landesmeisterschaften" --source-name landesmeisterschaften --year 2025 --focus Stormarn --focus-association-code OD --report reports/ndsb-2025-crawl-report.json --html-report reports/ndsb-2025-crawl-report.html
```

For the German championships source:

```bash
cargo run -- crawl-report "https://www.ndsb-sh.de/sport/deutsche-meisterschaften" --source-name deutsche-meisterschaften --year 2025 --focus "Deutsche Meisterschaften" --focus-association-code OD --report reports/ndsb-2025-dm-crawl-report.json --html-report reports/ndsb-2025-dm-crawl-report.html --max-depth 0 --max-pages 1
```

When `--year` is set and no custom report/download paths are provided, the crawler writes into a year/source archive automatically. The source is inferred for known NDSB URLs if `--source-name` is omitted:

```bash
cargo run -- crawl-report "https://www.ndsb-sh.de/sport/landesmeisterschaften" --year 2026 --focus Stormarn --focus-association-code OD
```

Single result PDFs that are not linked from the overview page can be added to the same archived run:

```bash
cargo run -- crawl-report "https://www.kschv-rdeck.de/fileadmin/user_upload/ksv/NDSB_TEMP/LM-Dinge/Ergebnisse/LM-Ergebnisslisten2026.html" --source-name landesmeisterschaften --year 2026 --focus Stormarn --focus-association-code OD --extra-pdf-url "https://www.kschv-rdeck.de/fileadmin/user_upload/ksv/NDSB_TEMP/LM-Dinge/Ergebnisse/VW112_K40_260516_1045_Finale_10.pdf"
```

Run the full 2026 state championship chain from download/crawl to podium JSON and HTML:

```bash
make lm-2026-podium
```

This creates paths such as:

```text
reports/archive/2026/landesmeisterschaften/crawl-report.html
reports/archive/2026/landesmeisterschaften/crawl-report.json
reports/archive/2026/landesmeisterschaften/podium-export.html
reports/archive/2026/landesmeisterschaften/podium-export.json
data/archive/2026/landesmeisterschaften/downloads/
data/archive/2026/landesmeisterschaften/manual-review/
```

The crawler stores a source/year-specific manifest with ETag, `Last-Modified`, SHA-256, local path, and last-seen timestamp in `.pdf-explorer/`. DAVID21+ PDFs are parsed automatically. Other formats are copied to `data/manual-review/` and listed in the JSON and HTML reports under `reports/`.

If a PDF produces very little text, the result is marked as `needs_ocr`. That gives the next phase a simple handoff point for Tesseract, OCRmyPDF, or a cloud OCR service.

Create a filtered podium export from an existing crawl report:

```bash
cargo run -- export-podium --crawl-report reports/ndsb-2025-crawl-report.json --output reports/ndsb-2025-podium-export.json --html-output reports/ndsb-2025-podium-export.html --focus-association-code OD --max-place 3
```

Create a German championship participation export by matching known focus clubs from the state championship data against the German championship PDFs:

```bash
cargo run -- export-participation --club-source-report reports/ndsb-2025-crawl-report.json --results-report reports/ndsb-2025-dm-crawl-report.json --output reports/ndsb-2025-dm-participation-export.json --html-output reports/ndsb-2025-dm-participation-export.html --focus-association-code OD
```

Combine podium results and German championship participation matches into one club-oriented export:

```bash
cargo run -- export-combined --podium-export reports/ndsb-2025-podium-export.json --participation-export reports/ndsb-2025-dm-participation-export.json --output reports/ndsb-2025-combined-export.json --html-output reports/ndsb-2025-combined-export.html
```

Start over with a clean generated data foundation:

```bash
cargo run -- clean
```

This removes `.pdf-explorer/`, `data/downloads/`, `data/manual-review/`, `data/archive/`, `reports/`, and `tmp/`.

## Current Structure

- `src/export.rs`: filtered JSON and HTML exports for downstream processing
- `src/pdf.rs`: PDF extraction core
- `src/ingest.rs`: URL crawling, PDF change detection, format classification, and reporting
- `src/sport_results.rs`: first parser for DAVID21+ result lists
- `src/main.rs`: CLI wrapper
- `src/lib.rs`: shared library entry point for future OCR, scraper, storage, or UI layers

## Development Checks

```bash
make verify
```

This runs formatting, tests, `cargo check`, and Clippy with pedantic and nursery lints:

```bash
cargo clippy --all-targets --all-features -- -W clippy::pedantic -W clippy::nursery -D warnings
```

## Planned Extensions

- OCR fallback for scanned or image-heavy PDFs
- Page-level extraction and confidence metadata
- Batch processing for folders
- Web scraper input pipeline
- Database persistence
- UI for reviewing documents and extracted fields
- Re-consumable JSON/CSV exports for steering later import and review workflows
