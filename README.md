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

## Datenbank-Workflow

Die Datenbank liegt standardmaessig unter `data/pdf-explorer.sqlite`. Alle Befehle koennen mit `--database <pfad>` auf eine andere SQLite-Datei zeigen.

### Paket 1: Datenbank-Grundlage

Datenbankdatei anlegen:

```bash
cargo run -- db init
```

Migrationen ausfuehren:

```bash
cargo run -- db migrate
```

In der Praxis reicht meist `db migrate`, weil die SQLite-Datei bei Bedarf angelegt wird.

### Paket 2: Kernschema

Das Kernschema wird durch die Migrationen angelegt. Es umfasst aktuell Quellen, Importlaeufe, Wettbewerbe, Vereine, Sportler, Disziplinen und Ergebnisse.

```bash
cargo run -- db migrate --database data/pdf-explorer.sqlite
```

### Paket 3: Import aus vorhandenem Export

Einen vorhandenen `podium-export.json` in die Datenbank importieren:

```bash
cargo run -- import-podium --input reports/archive/2026/landesmeisterschaften/podium-export.json --database data/pdf-explorer.sqlite
```

Der Import speichert Wettbewerb, Vereine, Sportler, Disziplinen, Ergebnisse und einen Importlauf. Wiederholte Importe derselben Datei werden ueber Hashes/Fingerprints duplikatfrei behandelt.

### Paket 4: Parserlaeufe als Datenquelle

Der gleiche `import-podium`-Aufruf erzeugt zusaetzlich einen Parserlauf und einzelne Parser-Ergebniszeilen:

```bash
cargo run -- import-podium --input reports/archive/2026/landesmeisterschaften/podium-export.json --database data/pdf-explorer.sqlite
```

Dabei bleiben drei Schichten getrennt:

```text
PDF/Export-Rohdaten -> Parserzeilen -> kanonische Ergebnisse
```

Parserzeilen behalten die urspruenglichen Werte. Kanonische Ergebnisse verweisen auf die Parserzeile und koennen Konflikte markieren, wenn ein spaeterer Lauf andere Daten fuer dasselbe fachliche Ergebnis liefert.

### Paket 5: Manuelle Korrekturen

Eine Vereinskorrektur speichern:

```bash
cargo run -- manual-override add-club --from "Schützenverein Reinfeld" --to "Schützenverein Reinfeld e.V."
```

Eine Sportlerkorrektur speichern:

```bash
cargo run -- manual-override add-athlete --from "R hl, Eberhard" --to "Rühl, Eberhard"
```

Aktive Korrekturen anzeigen:

```bash
cargo run -- manual-override list
```

Beim Datenbankimport werden aktive Korrekturen automatisch auf kanonische Vereins- und Sportlernamen angewendet:

```bash
cargo run -- import-podium --input reports/archive/2026/landesmeisterschaften/podium-export.json --database data/pdf-explorer.sqlite
```

Auch der Podiumsreport kann die Korrekturen schon im JSON-/HTML-Zwischenschritt verwenden:

```bash
cargo run -- export-podium --crawl-report reports/archive/2026/landesmeisterschaften/crawl-report.json --output reports/archive/2026/landesmeisterschaften/podium-export.json --html-output reports/archive/2026/landesmeisterschaften/podium-export.html --focus-association-code OD --max-place 3 --override-database data/pdf-explorer.sqlite
```

Wichtig: Manuelle Korrekturen veraendern nicht die Parser-Rohdaten. Sie wirken auf die kanonische Sicht und optional auf den lesbaren Export.

### Paket 6: Mannschaften

Die Mannschaftstabellen werden per Migration angelegt:

```bash
cargo run -- db migrate --database data/pdf-explorer.sqlite
```

Danach reicht der normale Import eines Podiums-Exports. Mannschaftszeilen werden automatisch in `teams`, `team_members` und `team_result_members` ueberfuehrt:

```bash
cargo run -- import-podium --input reports/archive/2026/landesmeisterschaften/podium-export.json --database data/pdf-explorer.sqlite
```

Dabei wird die Mannschaftsnummer aus dem Roh-Vereinsnamen getrennt gespeichert. Aus `Ahrensburger SchG I` wird also ein kanonischer Verein und eine Mannschaft mit `team_number = I`.

Eine einfache Kontrolle per SQLite:

```bash
sqlite3 data/pdf-explorer.sqlite "SELECT canonical_name, team_number, rank, medal FROM teams ORDER BY canonical_name;"
```

Mannschaftsmedaillen pro Sportler:

```bash
sqlite3 data/pdf-explorer.sqlite "SELECT athletes.canonical_name, teams.canonical_name, teams.rank, team_result_members.medal FROM team_result_members JOIN teams ON teams.id = team_result_members.team_id LEFT JOIN athletes ON athletes.id = team_result_members.athlete_id ORDER BY athletes.canonical_name;"
```

### Paket 7: Deutsche Meisterschaften und Teilnahme

Zuerst werden wie bisher die LM- und DM-Reports erzeugt:

```bash
cargo run -- crawl-report "https://www.ndsb-sh.de/sport/landesmeisterschaften" --source-name landesmeisterschaften --year 2026 --focus Stormarn --focus-association-code OD
cargo run -- crawl-report "https://www.ndsb-sh.de/sport/deutsche-meisterschaften" --source-name deutsche-meisterschaften --year 2026 --focus "Deutsche Meisterschaften" --focus-association-code OD --max-depth 0 --max-pages 1
```

Dann den LM-Podiumsreport und den DM-Teilnahmereport erzeugen:

```bash
cargo run -- export-podium --crawl-report reports/archive/2026/landesmeisterschaften/crawl-report.json --output reports/archive/2026/landesmeisterschaften/podium-export.json --html-output reports/archive/2026/landesmeisterschaften/podium-export.html --focus-association-code OD --max-place 3 --override-database data/pdf-explorer.sqlite
cargo run -- export-participation --club-source-report reports/archive/2026/landesmeisterschaften/crawl-report.json --results-report reports/archive/2026/deutsche-meisterschaften/crawl-report.json --output reports/archive/2026/deutsche-meisterschaften/participation-export.json --html-output reports/archive/2026/deutsche-meisterschaften/participation-export.html --focus-association-code OD
```

Beide Exporte in die Datenbank importieren:

```bash
cargo run -- import-podium --input reports/archive/2026/landesmeisterschaften/podium-export.json --database data/pdf-explorer.sqlite
cargo run -- import-participation --input reports/archive/2026/deutsche-meisterschaften/participation-export.json --database data/pdf-explorer.sqlite
```

`import-participation` legt die Deutsche Meisterschaft als eigene `competition` mit `scope = DM` an. Die Treffer werden als Ergebnisse mit `participation_only = 1` gespeichert. Wenn im Teilnahmereport Schützennamen erkannt wurden, werden diese mit Sportlern verknüpft; ansonsten bleibt die Teilnahme auf Verein/Quelle nachvollziehbar.

Kombinierte Auswertung LM-Medaille zu DM-Teilnahme:

```bash
sqlite3 data/pdf-explorer.sqlite "SELECT athlete_name, club_name, year, lm_medal, lm_rank, has_dm_participation FROM lm_medals_with_dm_participation ORDER BY club_name, athlete_name;"
```

Die View `lm_medals_with_dm_participation` zeigt LM-Medaillen und markiert, ob fuer denselben Sportler und Verein im selben Jahr eine DM-Teilnahme importiert wurde.

### Paket 8: HTML-/JSON-Reports aus Datenbank

Die bisherigen Datei-Exporte bleiben bestehen. Zusaetzlich koennen Podiums- und kombinierte Reports aus der Datenbank erzeugt werden.

Podiumsreport aus kanonischen DB-Ergebnissen:

```bash
cargo run -- export-db-podium --database data/pdf-explorer.sqlite --year 2026 --competition-scope LM --focus-association-code OD --max-place 3 --output reports/archive/2026/landesmeisterschaften/db-podium-export.json --html-output reports/archive/2026/landesmeisterschaften/db-podium-export.html
```

Kombinierter LM-/DM-Report aus der Datenbank:

```bash
cargo run -- export-db-combined --database data/pdf-explorer.sqlite --year 2026 --focus-association-code OD --max-place 3 --output reports/archive/2026/db-combined-export.json --html-output reports/archive/2026/db-combined-export.html
```

Die Filter `--year`, `--competition-scope`, `--focus-association-code` und `--max-place` werden direkt als Datenbankfilter angewendet. Die erzeugten JSON-Strukturen bleiben kompatibel zu den bestehenden Exportformaten:

```text
export-db-podium   -> PodiumExport
export-db-combined -> CombinedExport
```

### Paket 9: GUI-Grundlage

Technische Basis:

- Rust Backend
- lokale Weboberflaeche
- serverseitig gerendertes HTML
- SQLite als Datenquelle

Die erste GUI ist lesend. Der Einstiegspunkt sind Importlaeufe; von dort aus fuehrt die Navigation weiter zu Ergebnissen, Sportlern, Vereinen und spaeter Ehrungen. Ehrungen sind in Paket 9 zunaechst nur als Platzhalter vorgesehen.

Lokale Weboberflaeche starten:

```bash
cargo run -- serve --database data/pdf-explorer.sqlite --bind 127.0.0.1:7878
```

Danach im Browser oeffnen:

```text
http://127.0.0.1:7878/import-runs
```

Verfuegbare lesende Ansichten:

```text
/import-runs
/import-runs/<id>/results
/results
/athletes
/clubs
/corrections
/corrections/issues
/honors
```

### Paket 10: UI fuer Korrekturen

Die lokale Weboberflaeche enthaelt jetzt erste Schreibaktionen fuer manuelle Korrekturen. Parserdaten werden dabei nicht veraendert; Korrekturen werden als `manual_overrides` gespeichert.

Korrekturen oeffnen:

```text
http://127.0.0.1:7878/corrections
```

Parserfaelle mit Konfliktstatus oder fehlender Normalisierung:

```text
http://127.0.0.1:7878/corrections/issues
```

Moeglich ist aktuell:

- globale Korrektur fuer Vereinsnamen speichern
- globale Korrektur fuer Sportlernamen speichern
- aktive Korrekturen zuruecknehmen
- Korrekturhistorie ansehen
- auffaellige Parserzeilen ansehen und Rohwerte ins Korrekturformular uebernehmen

Die Zusammenfuehrung von Sportlern und Vereinen erfolgt in diesem Paket zunaechst ueber Namenskorrekturen. Echte ID-basierte Merge-Aktionen bleiben ein spaeterer Ausbau.

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
