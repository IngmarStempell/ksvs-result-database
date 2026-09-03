# Datenbank, Ergebnisverlaeufe und fachliche Luecken

Stand: 2026-09-03

## Zielbild

Die bestehende Excel-Tabelle soll schrittweise durch eine lokale Datenbank ersetzt werden. Die Datenbank soll Ergebnis- und Medaillenverlaeufe von Sportlern, Vereinen, Mannschaften und Wettbewerben nachvollziehbar speichern.

Langfristig soll darauf eine grafische Bearbeitungsoberflaeche aufsetzen. In dieser GUI sollen Ergebnisse, Sportler, Vereine, Mannschaften, Quellen und manuelle Korrekturen gepflegt werden koennen. Fuer das laufende Jahr soll die Anwendung auf Basis eines kleinen Regelwerks Vorschlaege fuer Sportlerehrungen anzeigen.

Wichtig ist dabei nicht nur ein finales, bereinigtes Ergebnis, sondern auch die Herkunft:

- aus welcher Quelle ein Ergebnis stammt
- aus welchem PDF es gelesen wurde
- welcher Importlauf die Daten erzeugt hat
- welche Parserdaten urspruenglich erkannt wurden
- welche manuellen Korrekturen vorgenommen wurden

Die Anwendung soll damit langfristig mehr sein als ein Parser: Sie soll eine fachliche Ergebnisdatenbank mit nachvollziehbarer Korrektur-, Export-, Bearbeitungs- und Ehrungsauswertungsfaehigkeit werden.

## Stand jetzt

Die Anwendung kann aktuell:

- Webseiten crawlen und PDF-Links verfolgen
- PDFs herunterladen und Aenderungen erkennen
- ETag, Hash und lokale Dateien fuer Wiederholbarkeit nutzen
- PDFs nach DAVID21+ und manueller Kontrolle klassifizieren
- DAVID21+-Ergebnisse fuer Landesmeisterschaften parsen
- Mannschafts- und Einzelwertungen erkennen
- Platzierungen, Schuetzen, Vereine, Disziplinen, Klassen, Ringe und PDF-Quellen exportieren
- Landesmeisterschaften und Deutsche Meisterschaften getrennt crawlen
- bekannte Kreisvereine fuer Stormarn ueber den Kreiscode OD ableiten
- Podiums-/Ergebnisreports als JSON und HTML erzeugen
- HTML-Reports filtern und gruppieren
- Templates fuer HTML-Seiten aus externen Template-Dateien laden
- eine lokale SQLite-Datenbank initialisieren und migrieren
- Kernschema fuer Quellen, Importlaeufe, Wettbewerbe, Vereine, Sportler, Disziplinen und Ergebnisse anlegen
- `podium-export.json` in die Datenbank importieren
- Importlaeufe mit Eingabehash, Parsername und Parserversion speichern
- Parserlaeufe und einzelne Parser-Ergebniszeilen speichern
- Rohwerte, normalisierte Parserwerte und kanonische Ergebnis-IDs trennen
- wiederholte Importe derselben Exportdatei reproduzierbar und duplikatfrei ausfuehren
- Konflikte zwischen vorhandenen kanonischen Ergebnissen und neuen Parserlaeufen markieren
- manuelle Korrekturen fuer Vereins- und Sportlernamen in SQLite speichern
- Importe und optionale Exporte mit aktiven manuellen Korrekturen kanonisieren
- Parser-Rohdaten trotz Korrekturen unveraendert erhalten

Die Modulgrenzen sind aktuell grob:

- `app` und `cli`: Einstieg, CLI-Argumente, Ablaufsteuerung
- `ingest`: Crawl, Download, Aenderungserkennung, Klassifikation, Crawl-Report
- `pdf`: PDF-Textextraktion
- `sport_results`: sportfachliche Ergebnislisten und DAVID21+-Parser
- `export`: JSON-/HTML-Exporte und Zusammenfuehrung
- `templates`: HTML, CSS und JavaScript der Reports
- `storage`: SQLite-Verbindung, Migrationen, Insert-/Upsert-Repository
- `import`: Uebernahme vorhandener Exporte in Parserlaeufe, Parserzeilen und kanonische Ergebnisse

## Planung akut

Die Datenbank-Grundlage ist angelegt. Kurzfristig geht es jetzt darum, die fachliche Korrekturschicht und die spaetere GUI vorzubereiten.

Als naechste Datenobjekte sollten modelliert werden:

- Quellen und Dokumente
- Importlaeufe
- Wettbewerbe
- Vereine
- Sportler
- Disziplinen
- Mannschaften und Mannschaftsmitglieder
- manuelle Korrekturen
- Ehrungsregeln und Ehrungsvorschlaege

Der Import aus dem bestehenden `podium-export.json` ist umgesetzt. Dadurch bleibt der PDF-Parser unangetastet, und das Datenmodell kann bereits mit echten Daten validiert werden.

Das Konzept der Parserlaeufe ist ebenfalls umgesetzt. Ein Parserlauf ist ein nachvollziehbarer Importvorgang mit Eingabedatei, Parserversion, Zeitpunkt, Ergebnisstatus und erzeugten Roh-Ergebniszeilen. Spaetere Parserverbesserungen duerfen neue Laeufe erzeugen, ohne alte Rohdaten unkontrolliert zu ueberschreiben.

Die erste manuelle Korrekturschicht ist angelegt. Akut fehlt jetzt vor allem die fachliche Freigabelogik: Sie entscheidet, welche Parserzeilen geprueft sind, welche Konflikte offen bleiben und welche kanonischen Ergebnisse fuer Ehrungen belastbar sind.

## Planung Zukunft

Das Datenmodell soll nicht nur LM und DM koennen, sondern offen fuer weitere Wettbewerbsebenen bleiben:

- Kreismeisterschaft
- Landesmeisterschaft
- Deutsche Meisterschaft
- Weltmeisterschaft
- Olympia
- weitere Wettbewerbe wie Qualifikation, Rangliste oder Rundenwettkampf

Deshalb sollte der Wettbewerb nicht als starres Enum in den Ergebnissen stecken, sondern als eigene Entitaet:

```text
competitions
- id
- code
- name
- year
- scope
- organizer
- association_code nullable
- country_code nullable
- date_from nullable
- date_to nullable
```

Beispiele:

- KM Stormarn: `scope = district`, `association_code = OD`
- LM NDSB: `scope = state`
- DM DSB: `scope = national`, `country_code = DE`
- WM: `scope = international`
- Olympia: `scope = olympic`

Ergebnisse haengen dann an `competition_id`. Dadurch bleiben Abfragen ueber alle Ebenen moeglich, ohne spaeter Sondertabellen fuer DM, WM oder Olympia bauen zu muessen.

Die GUI sollte auf derselben fachlichen Schicht arbeiten wie die Exporte. Sie darf nicht direkt Parser-Rohdaten veraendern, sondern soll Korrekturen, Zusammenfuehrungen und Freigaben speichern. Dadurch koennen Parserlaeufe jederzeit erneut ausgefuehrt und mit bestehenden manuellen Entscheidungen abgeglichen werden.

Fuer Sportlerehrungen sollte ein eigenes Regelwerk entstehen. Dieses Regelwerk berechnet aus den kanonischen Ergebnissen des laufenden Jahres Ehrungsvorschlaege, zum Beispiel nach Wettbewerbsebene, Platzierung, Medaille, Teilnahme, Mannschaftsbeteiligung, Verein oder Altersklasse. Die Vorschlaege sollen in der GUI sichtbar sein und dort bestaetigt, ausgeschlossen oder kommentiert werden koennen.

## Luecken

### Identitaet von Sportlern

Die groesste fachliche Luecke ist die stabile Identifikation von Sportlern.

Problemfaelle:

- unterschiedliche Schreibweisen
- Vorname/Nachname-Reihenfolge
- Abkuerzungen
- Tippfehler
- OCR-Fehler
- Namensaenderungen
- gleiche Namen in unterschiedlichen Vereinen
- Vereinswechsel ueber Jahre

Noetige Modellierung:

```text
athletes
athlete_aliases
athlete_club_memberships
```

### Vereinsidentitaet

Vereinsnamen sind keine stabilen Schluessel.

Bekannte Problemfaelle:

- `SchV` und `Schuetzenverein`
- `SchG` und `Schuetzengilde`
- `BSchG` und `Buergerschuetzengilde`
- vorangestellte Vereinsnummern
- Mannschaftsnummern am Ende
- historisch unterschiedliche Bezeichnungen
- Synonyme wie `Sprenge u.Umgegend` und `Schuetzenverein Sprenge`

Noetige Modellierung:

```text
clubs
club_aliases
club_memberships
```

### Disziplinen und Klassen

Disziplinen sind nicht immer eindeutig.

Problemfaelle:

- DSB-Disziplinkennzahlen gegen ausgeschriebene Namen
- nationale und internationale Bezeichnungen
- Auflage, Altersklasse und Geschlecht im selben Textbereich
- Final- und Qualifikationsergebnisse
- mehrere Wertungen in einem PDF

Noetige Modellierung:

```text
disciplines
discipline_aliases
event_categories
```

Ergebnisse sollten zusaetzlich ein Feld fuer die Phase bekommen:

```text
stage = qualification | final | aggregate | unknown
```

### Medaillenlogik

Medaillen ergeben sich nicht immer trivial aus Platz 1 bis 3.

Problemfaelle:

- geteilte Plaetze
- mehrere Bronzemedaillen
- Finalmodus
- Mannschaftsmedaillen fuer mehrere Schuetzen
- Teilnahme ohne Platzierung bei DM, WM oder Olympia

Noetige Felder:

```text
medal = gold | silver | bronze | none
medal_basis = rank | final_result | participation | manual | none
```

### Quellen und Nachvollziehbarkeit

Das System braucht eine saubere Herkunftskette.

Problemfaelle:

- gleiche URL mit geaendertem PDF-Inhalt
- Parserverbesserung erzeugt andere Ergebnisse
- manuelle Korrektur ueberstimmt Parserwert
- unterschiedliche Quellen widersprechen sich
- Rohdaten muessen auditierbar bleiben

Noetige Modellierung:

```text
source_documents
import_runs
parsed_result_rows
canonical_results
manual_overrides
```

### Zeit und Historie

Ein Jahr reicht nicht immer.

Problemfaelle:

- Wettkampfdatum und Veroeffentlichungsdatum unterscheiden sich
- Meisterschaften laufen ueber mehrere Tage
- Saisonjahr kann vom Kalenderjahr abweichen
- Archivstand soll nachvollziehbar bleiben

Noetige Felder:

```text
competition_date_from
competition_date_to
published_at
imported_at
source_valid_from
source_valid_to
```

### Mannschaften

Mannschaftsergebnisse sind eigene fachliche Objekte.

Problemfaelle:

- Mannschaft I, II, III
- Mannschaftsnummer als Parserartefakt im Vereinsnamen
- Mixed-Wertungen
- Nationalmannschaften
- Teamrang ohne vollstaendige Mitgliederliste
- Einzelschusszahl eines Teammitglieds ist nicht automatisch ein Einzelresultat

Noetige Modellierung:

```text
teams
team_members
team_result_members
```

### Organisationen

Bei KM, LM, DM, WM und Olympia reicht Verein allein nicht.

Problemfaelle:

- Kreisverband
- Landesverband
- Bundesverband
- Nation
- internationaler Verband
- Sportler startet je nach Ebene fuer Verein, Verband oder Nation

Noetige Modellierung:

```text
organizations
organization_aliases
organization_relationships
```

Verein, Kreisverband, Landesverband und Nation koennen langfristig als Organisationen mit Typ modelliert werden.

### Manuelle Korrekturen

Manuelle Eingriffe muessen nachvollziehbar und begrenzt anwendbar sein.

Problemfaelle:

- Korrektur gilt global fuer einen Verein
- Korrektur gilt nur fuer ein PDF
- Korrektur gilt nur fuer eine Ergebniszeile
- Korrektur ist ein Vorschlag, aber noch nicht bestaetigt
- alter und neuer Wert muessen vergleichbar bleiben

Noetige Modellierung:

```text
manual_overrides
- scope
- entity_type
- entity_id nullable
- source_document_id nullable
- parsed_result_row_id nullable
- field_name
- old_value
- new_value
- reason
- status
- created_at
```

Stand der Umsetzung:

- `manual_overrides` ist vorhanden
- aktive globale Korrekturen fuer `club.canonical_name` und `athlete.canonical_name` koennen per CLI angelegt und gelistet werden
- `podium-export.json`-Importe wenden aktive Korrekturen auf kanonische Vereine und Sportler an
- `export-podium` kann aktive Korrekturen optional schon fuer JSON-/HTML-Reports anwenden
- Parserzeilen behalten die urspruenglichen Rohwerte aus dem Export

Noch offen:

- Korrekturen deaktivieren oder historisieren
- Korrekturen auf ein konkretes PDF oder eine konkrete Parserzeile begrenzen
- Konflikte und unklare Korrekturen in einer GUI pruefen
- Aliase als eigene fachliche Tabellen von punktuellen Overrides trennen

### Parserlaeufe und Wiederholbarkeit

Parserlaeufe muessen eigene fachliche Objekte werden.

Problemfaelle:

- Parser wird verbessert und erkennt mehr oder andere Ergebnisse
- dieselbe Quelle wird spaeter erneut gecrawlt
- ein PDF wird ersetzt, bleibt aber unter derselben URL
- manuelle Korrekturen duerfen bei Neuimporten nicht verloren gehen
- Import aus JSON und direkter Import aus PDFs muessen nebeneinander moeglich sein
- fehlerhafte Parserlaeufe muessen nachvollziehbar bleiben

Noetige Modellierung:

```text
parser_runs
- id
- source_document_id nullable
- source_report_id nullable
- parser_name
- parser_version
- started_at
- finished_at nullable
- status
- input_hash nullable
- error nullable
```

```text
parsed_result_rows
- id
- parser_run_id
- source_document_id
- row_index
- row_fingerprint
- canonical_fingerprint
- source_name
- competition_year
- competition_scope
- result_kind
- raw_payload
- raw_* Felder
- normalized_* Felder
- conflict_status
- conflict_result_id nullable
```

Stand der Umsetzung:

- `parser_runs` und `parsed_result_rows` sind vorhanden
- `podium-export.json` erzeugt beim Import einen Parserlauf
- jede Exportzeile wird als Parserzeile gespeichert
- kanonische Ergebnisse referenzieren die Parserzeile
- wiederholte gleiche Parserlaeufe werden ueber Input-Hash, Parsername und Parserversion wiedererkannt
- Ergebnisduplikate werden ueber technische und kanonische Fingerprints vermieden
- Konflikte werden auf Parserzeilen markiert, wenn ein neuer Parserlauf ein bereits vorhandenes kanonisches Ergebnis mit abweichender Rohbasis findet

### Grafische Bearbeitung

Die zukuenftige GUI ist nicht nur ein Report, sondern eine Arbeitsoberflaeche zur Datenpflege.

Noetige Ansichten:

- Importlaeufe und Parserstatus
- ungepruefte oder problematische Ergebniszeilen
- Sportler zusammenfuehren und Aliase pflegen
- Vereine zusammenfuehren und Aliase pflegen
- Disziplinen und Klassen normalisieren
- Mannschaften und Mitglieder kontrollieren
- manuelle Korrekturen ansehen, anlegen und zuruecknehmen
- finale Ergebnisliste je Jahr und Wettbewerb anzeigen
- Ehrungsvorschlaege fuer das laufende Jahr pruefen

Die GUI sollte fachlich auf `canonical_results` und `manual_overrides` arbeiten. Parserdaten bleiben read-only und dienen als Herkunftsnachweis.

### Sportlerehrungen

Die Sportlerehrungen brauchen ein eigenes, kleines Regelwerk.

Problemfaelle:

- Ehrungskriterien koennen sich von Jahr zu Jahr aendern
- Mannschaftsmedaillen zaehlen anders als Einzelmedaillen
- Teilnahme an DM, WM oder Olympia kann auch ohne Medaille relevant sein
- mehrere Ergebnisse eines Sportlers muessen zusammengefasst werden
- gleiche Leistung darf eventuell nur einmal zur Ehrung fuehren
- Ehrungsvorschlag kann manuell ausgeschlossen oder bestaetigt werden
- Sonderfaelle sollen kommentiert werden koennen

Noetige Modellierung:

```text
honor_rule_sets
- id
- name
- year nullable
- valid_from nullable
- valid_to nullable
- status
```

```text
honor_rules
- id
- rule_set_id
- priority
- competition_scope nullable
- competition_code nullable
- result_kind nullable
- medal nullable
- max_rank nullable
- participation_only nullable
- points
- label
- is_active
```

```text
honor_suggestions
- id
- rule_set_id
- athlete_id
- year
- reason
- score
- status
- created_at
- decided_at nullable
- decision_note nullable
```

Das Regelwerk sollte zunaechst bewusst klein bleiben. Es muss nicht sofort eine frei programmierbare Rule Engine sein. Fuer den Anfang reichen datenbankgestuetzte Regeln mit klaren Feldern, die spaeter erweitert werden koennen.

### GUI-Technologie

Die GUI-Technologie ist noch offen.

Moegliche Wege:

- lokale Weboberflaeche mit Rust Backend, zum Beispiel `axum`
- Desktop-App mit Web-Frontend, zum Beispiel Tauri
- einfache serverseitige HTML-Oberflaeche mit Templates
- spaeter API plus separates Frontend

Fuer den Anfang ist eine lokale Weboberflaeche mit Rust Backend wahrscheinlich der pragmatischste Weg. Sie passt gut zu SQLite, HTML-Reports und den bestehenden Templates.

## Umsetzungsskizze

### Paket 1: Datenbank-Grundlage - erledigt

- Dependency fuer SQLite-Zugriff einfuehren
- Migrationen einrichten
- `storage`-Modul anlegen
- Datenbankdatei unter `data/` definieren
- CLI-Kommandos vorbereiten:
  - `db init`
  - `db migrate`

### Paket 2: Kernschema - erledigt

- Tabellen fuer `source_documents`, `import_runs`, `competitions`, `clubs`, `athletes`, `disciplines`, `results` anlegen
- eindeutige technische IDs verwenden
- Rohwerte und kanonische IDs getrennt halten
- erste Queries und Repository-Funktionen schreiben

### Paket 3: Import aus vorhandenem Export - erledigt

- `podium-export.json` in die Datenbank importieren
- LM-Ergebnisse als `competition` + `results` speichern
- Vereine und Sportler aus Rohdaten ableiten
- Importlauf speichern
- Parserlauf oder Importlauf mit Parserversion und Eingabehash speichern
- Duplikate ueber Hash/Quelle/Jahr/Disziplin/Rang/Name vermeiden

### Paket 4: Parserlaeufe als Datenquelle - erledigt

- `parser_runs` und `parsed_result_rows` einfuehren
- bestehende Crawl- und Exportdaten in Parserlaeufe ueberfuehren
- Rohdaten, normalisierte Parserdaten und kanonische Ergebnisse trennen
- Neuimport gleicher Quellen reproduzierbar machen
- Konflikte zwischen altem kanonischem Ergebnis und neuem Parserlauf markieren

Umgesetzt ist zunaechst der Weg ueber `podium-export.json`. Der direkte Parserlauf aus frisch extrahierten PDF-Daten bleibt ein spaeterer Ausbau, sobald der Datenbank-Import nicht mehr nur vorhandene Exporte konsumieren soll.

### Paket 5: Manuelle Korrekturen - erledigt

- `manual_overrides` einfuehren
- Korrekturen fuer Vereinsnamen und Sportlernamen speichern
- Anzeige-/Exportlogik auf kanonische Werte plus Overrides umstellen
- Parserdaten unveraendert erhalten

Umgesetzt ist zunaechst eine aktive globale Korrekturschicht fuer Vereins- und Sportlernamen. Die feinere fachliche Bearbeitung mit Statuswechseln, GUI-Pruefung, PDF-spezifischen Overrides und Alias-Tabellen bleibt Teil der folgenden Pakete.

### Paket 6: Mannschaften

- `teams`, `team_members`, `team_result_members` ergaenzen
- Mannschaftsmedaillen pro Mitglied auswertbar machen
- Mannschaftsnummern fachlich von Vereinsnamen trennen

### Paket 7: Deutsche Meisterschaften und Teilnahme

- DM als eigene `competition` importieren
- Teilnahme ohne Platzierung modellieren
- vorhandene Vereinserkennung gegen bekannte Kreisvereine nutzen
- kombinierte Auswertung LM-Medaille zu DM-Teilnahme erzeugen

### Paket 8: HTML-/JSON-Reports aus Datenbank

- bisherige Reports optional aus Datenbank statt JSON-Dateien erzeugen
- Filter auf Datenbankabfragen stuetzen
- Exportformate stabil halten
- alte JSON-Exporte weiterhin konsumierbar lassen

### Paket 9: GUI-Grundlage

- lokale Weboberflaeche oder Desktop-Huelle festlegen
- Navigation fuer Importlaeufe, Ergebnisse, Sportler, Vereine und Ehrungen anlegen
- lesende Ansichten zuerst bauen
- Schreibaktionen erst nach klarer Korrekturschicht freigeben

### Paket 10: UI fuer Korrekturen

- grafische Oberflaeche fuer manuelle Korrekturen
- Ansichten fuer ungepruefte Parserfaelle
- Zusammenfuehrung von Sportlern und Vereinen
- Historie der Korrekturen anzeigen

### Paket 11: Sportlerehrungen

- Tabellen fuer `honor_rule_sets`, `honor_rules` und `honor_suggestions` anlegen
- erstes Regelwerk fuer das laufende Jahr definieren
- Ehrungsvorschlaege aus kanonischen Ergebnissen berechnen
- GUI-Ansicht fuer Vorschlaege, Bestaetigung, Ausschluss und Kommentare bauen
- Export fuer Ehrungslisten vorbereiten

### Paket 12: Erweiterung auf KM, WM, Olympia

- neue Competition-Typen ueber Daten anlegen, nicht ueber Sondercode
- Nationen und internationale Organisationen ergaenzen
- Disziplin-Mappings erweitern
- Teilnahme, Qualifikation, Finale und Medaillenlogik ausbauen

## Leitentscheidung

Parserdaten sollen nicht ueberschrieben werden.

Die Anwendung sollte drei Schichten behalten:

```text
raw source
parsed fact
canonical fact
```

Diese Trennung macht das System robuster gegen Parserfehler, Quellformatwechsel, OCR-Probleme und manuelle Korrekturen.

Die GUI und das Ehrungsregelwerk sollen auf `canonical fact` arbeiten. Parserlaeufe liefern neue `parsed facts`; manuelle Korrekturen entscheiden, wie daraus kanonische Fakten werden.
