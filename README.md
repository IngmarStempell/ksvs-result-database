# PDF Explorer

Rust-Anwendung für den Kreisschützenverband Stormarn zum Crawlen, Parsen und Verwalten von Schützenergebnissen aus PDFs. Die Ergebnisse werden in SQLite gespeichert, über eine lokale Weboberfläche zugänglich gemacht und als HTML oder JSON exportiert.

Unterstützt werden DAVID21+-Ergebnislisten, Einzel- und Mannschaftsergebnisse sowie der Abgleich von LM-Medaillen mit DM-Teilnahmen. Importläufe, Parser-Rohdaten und manuelle Namenskorrekturen bleiben nachvollziehbar.

## Inhalt

- [Anwendung starten](#anwendung-starten)
- [Ergebnisse laden und importieren](#ergebnisse-laden-und-importieren)
- [Weboberfläche bedienen](#weboberfläche-bedienen)
- [Reports exportieren](#reports-exportieren)
- [Weitere CLI-Befehle](#weitere-cli-befehle)
- [Dateien und Datenhaltung](#dateien-und-datenhaltung)
- [Entwicklung](#entwicklung)

## Anwendung starten

### 1. Voraussetzungen prüfen

Benötigt werden eine Rust-Toolchain mit Cargo, ein nativer C/C++-Compiler für den Build und ein Webbrowser. Unter macOS liefern die Xcode Command Line Tools die Build-Werkzeuge (`xcode-select --install`, falls noch nicht installiert).

```bash
rustc --version
cargo --version
```

Beim ersten Build lädt Cargo die Abhängigkeiten herunter; dafür ist Internetzugang erforderlich. Das erste Kompilieren kann einige Minuten dauern. Ein separater Datenbankserver ist nicht erforderlich.

### 2. Projektverzeichnis öffnen

Alle folgenden Befehle im Terminal aus diesem Verzeichnis ausführen:

```bash
cd "/Users/ingmar/Documents/Rust - pdf explorer"
```

### 3. Datenbank vorbereiten

```bash
cargo run -- db migrate --database data/pdf-explorer.sqlite
```

Der Befehl legt die SQLite-Datei bei Bedarf an und führt ausstehende Migrationen aus. Eine vorhandene Datenbank wird weiterverwendet.

### 4. Optional: Ergebnisse importieren

Für eine gefüllte Oberfläche den Abschnitt [Ergebnisse laden und importieren](#ergebnisse-laden-und-importieren) ausführen. Sind bereits Daten importiert oder soll zunächst die leere Oberfläche geöffnet werden, direkt mit Schritt 5 fortfahren.

### 5. Weboberfläche starten

```bash
cargo run -- serve --database data/pdf-explorer.sqlite --bind 127.0.0.1:7878
```

Warten, bis `web UI running at http://127.0.0.1:7878` im Terminal erscheint. Das Terminal bleibt während der Nutzung geöffnet. `serve` führt ebenfalls ausstehende Migrationen aus.

### 6. Anwendung im Browser öffnen

[Lokale Weboberfläche öffnen](http://127.0.0.1:7878/import-runs). Die Startansicht zeigt die Importläufe; über die Navigation sind Ergebnisse, Sportler, Vereine und weitere Ansichten erreichbar.

### 7. Beenden und erneut starten

Zum Beenden im Server-Terminal `Ctrl+C` drücken. Die Daten bleiben in `data/pdf-explorer.sqlite` gespeichert. Beim nächsten Mal genügen Schritt 2, Schritt 5 und das Öffnen der Browseradresse.

Falls Port 7878 bereits belegt ist, mit `--bind 127.0.0.1:7879` starten und im Browser ebenfalls Port 7879 verwenden. Bei einer anderen Datenbankdatei für Migration, Import und Server immer denselben `--database`-Pfad angeben.

## Ergebnisse laden und importieren

Der Datenfluss besteht aus drei Schritten:

```text
Webseite/PDFs → Crawl-Report → JSON-Export → SQLite
```

Crawling und Export erzeugen Dateien. Erst der anschließende Import übernimmt die Ergebnisse in die Datenbank und macht sie in der Weboberfläche sichtbar.

### Landesmeisterschaften

Der vorbereitete Workflow lädt die LM-2026-Quelle und erstellt einen Podiumsreport als JSON und HTML. Er benötigt `make` und Internetzugang:

```bash
make lm-2026-podium
```

Standardmäßig enthält der Podiumsreport alle Kreisverbände. Für einen auf Stormarn (`OD`) begrenzten Export stattdessen ausführen:

```bash
make lm-2026-podium PODIUM_FOCUS_CODE=OD
```

Anschließend importieren; liegt die Exportdatei bereits vor, genügt dieser Befehl:

```bash
cargo run -- import-podium --input reports/archive/2026/landesmeisterschaften/podium-export.json --database data/pdf-explorer.sqlite
```

Der Import speichert Ergebnisse, Sportler, Vereine, Mannschaften sowie Import- und Parserläufe. Wiederholte Importe derselben Datei werden über Hashes und Fingerprints duplikatfrei behandelt.

### Deutsche Meisterschaften

Der Teilnahmeabgleich nutzt bekannte Vereine aus dem LM-Crawl-Report. Deshalb zuerst den LM-Workflow ausführen. Danach die DM-Quelle crawlen:

```bash
cargo run -- crawl-report "https://www.ndsb-sh.de/sport/deutsche-meisterschaften" --source-name deutsche-meisterschaften --year 2026 --focus "Deutsche Meisterschaften" --focus-association-code OD --max-depth 0 --max-pages 1
```

Teilnahmereport erzeugen und importieren:

```bash
cargo run -- export-participation --club-source-report reports/archive/2026/landesmeisterschaften/crawl-report.json --results-report reports/archive/2026/deutsche-meisterschaften/crawl-report.json --output reports/archive/2026/deutsche-meisterschaften/participation-export.json --html-output reports/archive/2026/deutsche-meisterschaften/participation-export.html --focus-association-code OD
cargo run -- import-participation --input reports/archive/2026/deutsche-meisterschaften/participation-export.json --database data/pdf-explorer.sqlite
```

Erkannte Schützennamen werden mit Sportlern verknüpft. Treffer ohne erkannten Namen bleiben über Verein und Quelle nachvollziehbar. Die kombinierte Auswertung zeigt LM-Medaillen mit DM-Teilnahmen desselben Sportlers und Vereins im selben Jahr.

### Andere Quellen und Jahre

Beispiel für einen eigenen Crawl-Aufruf:

```bash
cargo run -- crawl-report "https://www.ndsb-sh.de/sport/landesmeisterschaften" --source-name landesmeisterschaften --year 2026 --focus Stormarn --focus-association-code OD
```

Mit `--year` und ohne eigene Ausgabe- oder Downloadpfade legt der Crawler ein Archiv je Jahr und Quelle an. Einzelne, nicht verlinkte PDFs lassen sich über `--extra-pdf-url "<PDF-URL>"` ergänzen. Weitere Optionen zeigt `cargo run -- crawl-report --help`.

DAVID21+-PDFs werden automatisch geparst. Andere Formate werden zur manuellen Prüfung abgelegt und in den Reports aufgeführt. PDFs mit sehr wenig extrahierbarem Text erhalten den Status `needs_ocr`; eine automatische OCR-Verarbeitung ist noch nicht umgesetzt.

## API

| Bereich | Pfad | Funktion |
| --- | --- | --- |
| Importläufe | `/import-runs` | Importe und zugehörige Ergebnisse öffnen |
| Ergebnisse | `/results` | Nach Suchtext, Jahr, Wettbewerb, Kreis und Wertung filtern sowie nach Verein oder Sportler gruppieren |
| Sportler | `/athletes` | Sportler suchen und Ergebnisverläufe ansehen |
| Vereine | `/clubs` | Vereine suchen, Namen und Aliase direkt bearbeiten und Vereine zusammenführen |
| Mannschaften | `/teams` | Mannschaften und Mitglieder prüfen |
| Quellen | `/sources` | PDF-Quellen und zugehörige Ergebnisse ansehen |
| Parserläufe | `/parser-runs` | Parserstatus, Rohdaten und auffällige Zeilen prüfen |
| Kombinierte Auswertung | `/combined` | LM-Medaillen mit DM-Teilnahmen abgleichen |
| Korrekturen | `/corrections` | Namenskorrekturen anlegen, zurücknehmen und Historie ansehen |
| Auffällige Parserzeilen | `/corrections/issues` | Konflikte und fehlende Normalisierung prüfen |
| Vereinsaliase | `/club-aliases` | Schreibvarianten anlegen und deaktivieren |
| Ehrungen | `/honors` | Platzhalter für die geplante Ehrungslogik |

Filter und Seitenwahl bleiben in der URL erhalten. Große Listen bieten eine Seitengrößenauswahl und Vor-/Zurück-Navigation. Detailseiten sind aus den jeweiligen Listen erreichbar.

In der Vereinsliste öffnet **Bearbeiten** den Editor direkt auf derselben Seite. Auch die Vereinsdetailseite bietet **Name und Aliase bearbeiten**. Dort lassen sich Namen sofort ändern sowie Aliase anlegen, bearbeiten und deaktivieren. Der bisherige Vereinsname bleibt nach einer Umbenennung als Alias erhalten; die Änderung wird in der Korrekturhistorie als `applied` protokolliert. Bereits belegte Namen werden abgewiesen.

Eine falsch angelegte Vereinsentität kann dort außerdem in einen bestehenden Zielverein zusammengeführt werden. Dabei werden Ergebnisse, Mannschaften und Aliase auf den Zielverein übertragen; die Vereins-ID der Ergebnisse bleibt fachlich über den Zielverein erhalten. Der bisherige Name wird als Alias übernommen und der Vorgang in der Korrekturhistorie protokolliert. Eine Zusammenführung ist endgültig und sollte vor dem Speichern geprüft werden.

Korrekturen unter `/corrections` gelten global für Vereins- und Sportlernamen. Aktive Korrekturen werden beim Import auf kanonische Namen angewendet; Parser-Rohdaten bleiben unverändert. Echte ID-basierte Zusammenführungen sind noch nicht umgesetzt.

## Reports exportieren

### Aus der Datenbank

Podiumsreport aus den kanonischen LM-Ergebnissen:

```bash
cargo run -- export-db-podium --database data/pdf-explorer.sqlite --year 2026 --competition-scope LM --focus-association-code OD --max-place 3 --output reports/archive/2026/landesmeisterschaften/db-podium-export.json --html-output reports/archive/2026/landesmeisterschaften/db-podium-export.html
```

Kombinierter LM-/DM-Report:

```bash
cargo run -- export-db-combined --database data/pdf-explorer.sqlite --year 2026 --focus-association-code OD --max-place 3 --output reports/archive/2026/db-combined-export.json --html-output reports/archive/2026/db-combined-export.html
```

Die Filter werden direkt in den Datenbankabfragen angewendet. Die JSON-Ausgaben bleiben kompatibel zu den dateibasierten Exportformaten.

### Aus vorhandenen Reports

Ein Podiumsreport lässt sich auch direkt aus einem Crawl-Report erzeugen. `--override-database` bezieht aktive Namenskorrekturen ein; die Option kann entfallen, wenn keine Korrekturen angewendet werden sollen.

```bash
cargo run -- export-podium --crawl-report reports/archive/2026/landesmeisterschaften/crawl-report.json --output reports/archive/2026/landesmeisterschaften/podium-export.json --html-output reports/archive/2026/landesmeisterschaften/podium-export.html --focus-association-code OD --max-place 3 --override-database data/pdf-explorer.sqlite
```

Vorhandene Podiums- und Teilnahmereports zusammenführen:

```bash
cargo run -- export-combined --podium-export reports/archive/2026/landesmeisterschaften/podium-export.json --participation-export reports/archive/2026/deutsche-meisterschaften/participation-export.json --output reports/archive/2026/combined-export.json --html-output reports/archive/2026/combined-export.html
```

## Weitere CLI-Befehle

### Einzelne PDFs prüfen

Den Beispielpfad durch eine vorhandene PDF-Datei ersetzen:

```bash
cargo run -- parse ./pfad/zum/dokument.pdf
cargo run -- parse ./pfad/zum/dokument.pdf --format json
cargo run -- parse-sport ./pfad/zur/ergebnisliste.pdf
```

### Korrekturen und Vereinsaliase pflegen

```bash
cargo run -- manual-override add-club --from "Schützenverein Reinfeld" --to "Schützenverein Reinfeld e.V."
cargo run -- manual-override add-athlete --from "R hl, Eberhard" --to "Rühl, Eberhard"
cargo run -- manual-override list
cargo run -- club-alias add --alias "SchV Reinfeld 1" --club "Schützenverein Reinfeld" --association-code OD
cargo run -- club-alias list --all
```

Einen Alias mit `cargo run -- club-alias deactivate <ID>` deaktivieren; die ID stammt aus der Aliasliste.

### Generierte Dateien entfernen

```bash
cargo run -- clean
```

Dieser Befehl löscht `.pdf-explorer/`, `data/downloads/`, `data/manual-review/`, `data/archive/`, `reports/` und `tmp/`. Die standardmäßige SQLite-Datei bleibt erhalten.

Alle Befehle und ihre Optionen lassen sich mit `cargo run -- --help` beziehungsweise `cargo run -- <befehl> --help` anzeigen.

## Dateien und Datenhaltung

| Pfad | Inhalt |
| --- | --- |
| `data/pdf-explorer.sqlite` | Standarddatenbank |
| `.pdf-explorer/` | Crawl-Manifeste mit ETag, Änderungszeit, Hash und lokalem Dateipfad |
| `data/archive/<jahr>/<quelle>/downloads/` | Heruntergeladene PDFs eines archivierten Laufs |
| `data/archive/<jahr>/<quelle>/manual-review/` | PDFs zur manuellen Prüfung |
| `reports/archive/<jahr>/<quelle>/` | Crawl-Reports und Exporte als JSON und HTML |

Die Datenhaltung trennt PDF-/Export-Rohdaten, Parserzeilen und kanonische Ergebnisse. Ergebnisse verweisen auf ihre Parserherkunft; abweichende spätere Läufe können Konflikte markieren. Vereinsaliase und manuelle Korrekturen ergänzen die kanonische Sicht.

Das Schema enthält außerdem Organisationen und Startkontexte als Grundlage für weitere Wettbewerbsebenen. Fachliche Anforderungen, Umsetzungsstand und offene Aufgaben stehen zentral in [Datenbank-Anforderungen und Lücken](docs/datenbank-anforderungen-und-luecken.md).

## Entwicklung

### Projektstruktur

| Pfad | Zuständigkeit |
| --- | --- |
| `src/main.rs`, `src/cli.rs`, `src/app.rs` | Einstieg, CLI-Argumente und Ablaufsteuerung |
| `src/pdf.rs` | PDF-Textextraktion |
| `src/sport_results/` | Sportfachliches Datenmodell und DAVID21+-Parser |
| `src/ingest/` | Crawling, Downloads und Crawl-Reports |
| `src/import/` | Import von Exportdateien in die Datenbank |
| `src/storage/`, `migrations/` | SQLite-Verbindungen, Modelle, Repositories und Migrationen |
| `src/application/query.rs` | Lesende GUI-Abfragen |
| `src/web.rs` | Lokaler Webserver und HTML-Ansichten |
| `src/export/` | Datei- und Datenbankexporte |
| `templates/` | HTML-Templates für Weboberfläche und Reports |
| `tests/` | Integrationstests |

### Prüfungen

```bash
make verify
```

Führt Formatierung, Tests, `cargo check` und Clippy aus. Clippy immer mit diesen Optionen starten:

```bash
cargo clippy --all-targets --all-features -- -W clippy::pedantic -W clippy::nursery -D warnings
```

### Weiterführende Dokumentation

- [Anforderungen, Datenmodell und Priorisierung](docs/datenbank-anforderungen-und-luecken.md)
- [DAVID21+-Formatanalyse](docs/david21-format-analysis.md)
- [NDSB-Bogen-Formatanalyse](docs/ndsb-bogen-format-analysis.md)
