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
- Mannschaften, Mannschaftsmitglieder und Mannschaftsmedaillen pro Mitglied in SQLite speichern
- Mannschaftsnummern als eigene Team-Eigenschaft vom Vereinsnamen trennen
- Deutsche Meisterschaften als eigene Competition importieren
- DM-Teilnahmen ohne Platzierung als `participation_only`-Ergebnisse speichern
- LM-Medaillen mit DM-Teilnahmen ueber eine Datenbank-View kombinieren
- Podiums- und kombinierte HTML-/JSON-Reports optional aus der Datenbank erzeugen
- Reportfilter wie Jahr, Wettbewerbsebene, Kreis und Platzierung direkt in Datenbankabfragen anwenden

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

Die Datenbank-Grundlage, die erste Korrekturschicht und die GUI-Grundlage sind angelegt. Kurzfristig geht es jetzt darum, die priorisierten offenen Punkte aus der zentralen Sofort-Liste umzusetzen.

Als Datenobjekte sind inzwischen im Kern modelliert oder fachlich vorgesehen:

- Quellen und Dokumente
- Importlaeufe
- Wettbewerbe
- Vereine
- Sportler
- Disziplinen
- Mannschaften und Mannschaftsmitglieder
- manuelle Korrekturen
- Ehrungsregeln und Ehrungsvorschlaege als naechster grosser Ausbau

Der Import aus dem bestehenden `podium-export.json` ist umgesetzt. Dadurch bleibt der PDF-Parser unangetastet, und das Datenmodell kann bereits mit echten Daten validiert werden.

Das Konzept der Parserlaeufe ist ebenfalls umgesetzt. Ein Parserlauf ist ein nachvollziehbarer Importvorgang mit Eingabedatei, Parserversion, Zeitpunkt, Ergebnisstatus und erzeugten Roh-Ergebniszeilen. Spaetere Parserverbesserungen duerfen neue Laeufe erzeugen, ohne alte Rohdaten unkontrolliert zu ueberschreiben.

Die erste manuelle Korrekturschicht ist angelegt. Akut priorisiert sind Filter/Suche, eine stabile fachliche Service-Schicht, Detailseiten, bessere Statusansichten, UI-Validierung, praezisere Teilnahmemodellierung, Alias-Datenbasis, kombinierte Auswertung und ein allgemeineres Wettbewerbsmodell. Pruefstatus soll nur fuer auffaellige Parserfaelle gelten, nicht fuer jede Parserzeile.

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

Die GUI sollte auf derselben fachlichen Schicht arbeiten wie die Exporte. Sie darf nicht direkt Parser-Rohdaten veraendern, sondern soll Korrekturen, Zusammenfuehrungen und Pruefentscheidungen fuer auffaellige Faelle speichern. Dadurch koennen Parserlaeufe jederzeit erneut ausgefuehrt und mit bestehenden manuellen Entscheidungen abgeglichen werden.

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

Stand der Umsetzung:

- `teams`, `team_members` und `team_result_members` sind vorhanden
- `podium-export.json`-Importe erzeugen fuer Mannschaftszeilen eigene Teamdaten
- Mannschaftsmedaillen werden pro Mitglied ueber `team_result_members.medal` auswertbar
- Mannschaftsnummern wie `I`, `II` oder `1` werden als `team_number` gespeichert
- der kanonische Verein bleibt getrennt vom Mannschaftsnamen
- Roh- und Parser-Schreibweisen von Vereinen werden beim Import als `club_aliases` am kanonischen Verein gespeichert
- Vereinsaliase koennen per CLI und Weboberflaeche angezeigt, angelegt und deaktiviert werden
- unter `/clubs` und auf Vereinsdetailseiten koennen Vereinsnamen und Aliase direkt bearbeitet werden; Umbenennungen behalten Vereins-ID und Ergebniszuordnung bei, bewahren den alten Namen als Alias und protokollieren den Eingriff als `manual_overrides` mit Status `applied`
- der Vereinseditor verhindert Namenskonflikte mit anderen Vereinen; weitergehende automatische Zusammenfuehrungen bleiben offen
- Vereine koennen im Vereinseditor in einen bestehenden Zielverein zusammengefuehrt werden; Ergebnisse, Mannschaften und Aliase werden uebertragen, der bisherige Name als Alias erhalten und der Vorgang als `applied` protokolliert

Offene Punkte: siehe zentrale Priorisierung, Spaeter 10-13 fuer Mannschaftsdetails.

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
organizations.parent_id
```

Verein, Kreisverband, Landesverband und Nation koennen langfristig als Organisationen mit Typ modelliert werden.

Stand der Umsetzung:

- `organizations` und `organization_aliases` sind vorhanden
- Standardorganisationen fuer OD, NDSB, DSB, Deutschland, ISSF und IOC werden per Migration angelegt
- Organisationen koennen ueber `parent_id` hierarchisch verbunden werden
- `competitions.organizer_organization_id` kann auf den Veranstalter zeigen
- `results.representing_organization_id` und `results.start_context` bereiten den Start fuer Verein, Verband oder Nation vor
- neue Competitions werden fuer bekannte Scopes automatisch einem Veranstalter zugeordnet: KM -> OD, LM -> NDSB, DM -> DSB, WM -> ISSF, Olympia -> IOC

Offene Punkte: siehe zentrale Priorisierung, Sofort 8 fuer fachlichen Ausbau und Spaeter 4 fuer genauere Teilnahmedaten.

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

Offene Punkte: siehe zentrale Priorisierung, Sofort 4 und 6 sowie Spaeter 8-9.

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

Die GUI soll zunaechst als lokale Weboberflaeche mit Rust Backend umgesetzt werden. Die Seiten werden serverseitig als HTML gerendert. Das passt gut zu SQLite, den bestehenden Report-Templates und dem CLI-orientierten Importfluss.

Die erste GUI ist bewusst eine lesende Verwaltungsoberflaeche. Der Einstiegspunkt sind Importlaeufe. Von dort aus kann man zu den zugehoerigen Ergebnissen, Sportlern, Vereinen und spaeter zu Ehrungsvorschlaegen navigieren.

Schreibende Korrekturen sind inzwischen in Paket 10 in begrenzter Form freigegeben. Weitergehende Merge-, Pruef- und Ruecknahmefunktionen bleiben ueber die zentrale Priorisierung gesteuert, damit Parserdaten, kanonische Werte und manuelle Eingriffe fachlich sauber getrennt bleiben.

## Priorisierung offener Punkte

Diese Liste ist die fuehrende Arbeitsliste fuer offene Fragen aus den Paketen. Lokale Paketabschnitte verweisen auf diese Nummern, damit offene Punkte nicht an mehreren Stellen auseinanderlaufen.

### Sofort

1. Mehr DB-Filter plus Filter/Suche in GUI-Listen - in Arbeit, erste serverseitige Filter fuer Ergebnisse, Sportler, Vereine und kombinierte Auswertung umgesetzt
2. Stabile API-/Service-Schicht fuer CLI und GUI - in Arbeit, lesende Abfragen in `application::query` gebuendelt
3. Detailseiten fuer Sportler, Vereine, Importlaeufe und Quellen - in Arbeit, Detailseiten fuer Sportler, Vereine, Quellen und Importlauf-Ergebnisse umgesetzt
4. Weitergehende Statusansichten fuer Parserlaeufe und manuelle Nachbearbeitung - in Arbeit, Parserlauf-Liste, Parserlauf-Detailseite und auffaellige Parserzeilen je Parserlauf umgesetzt
5. UI-Validierung gegen existierende Sportler- und Vereinsnamen - in Arbeit, Korrekturformular bietet vorhandene Namen als Vorschlaege an
6. Vereinsabgleich ueber `club_aliases` als Datenbasis - in Arbeit, Tabelle, Repository-Funktionen, automatische Befuellung, CLI-Pflege und GUI-Pflege umgesetzt
7. Kombinierte Auswertung als HTML-/GUI-Ansicht - in Arbeit, `/combined` zeigt LM-Medaillen mit DM-Teilnahme aus der Datenbank
8. Allgemeines Wettbewerbsmodell fuer DM, WM, Olympia usw. - in Arbeit, Organisationen, Organisationsaliase und Startkontext als Schema-Grundlage umgesetzt
9. Pagination oder bewusst steuerbare Seitengroessen fuer grosse Datenmengen - in Arbeit, Web-Listen nutzen `page` und `page_size`
10. GUI-Ansichten fuer Mannschaften und Mitglieder - in Arbeit, `/teams` und `/teams/<id>` zeigen Mannschaften und Mitglieder aus der Datenbank

### Spaeter

1. DB-Reports fuer reine Teilnahme- oder Konfliktlisten
2. UI-Ansichten, die dieselben DB-Abfragen interaktiv nutzen
3. Teilnahme genauer nach Disziplin/Klasse modellieren
4. Echte ID-basierte Merge-Aktionen fuer Sportler und Vereine
5. Detailansichten zum Vergleich von Rohwert, Parserwert, kanonischem Wert und Override
6. Pruefstatus nur fuer auffaellige Parserfaelle, nicht fuer jede Parserzeile
7. Konfliktaufloesung mit Bezug auf konkrete Quelle, PDF oder Parserzeile
8. Ruecknahme mit Grund/Kommentar statt nur Statuswechsel
9. Mannschaftsgesamtringe direkt aus der Mannschaftszeile speichern, sobald der Import nicht mehr nur den bisherigen Podium-Export konsumiert
10. Reihenfolge der Mannschaftsmitglieder aus dem Originalparser stabiler uebernehmen
11. Mannschaften in HTML-/DB-Reports explizit als eigene Gruppe anzeigen

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

### Paket 6: Mannschaften - erledigt

- `teams`, `team_members`, `team_result_members` ergaenzen
- Mannschaftsmedaillen pro Mitglied auswertbar machen
- Mannschaftsnummern fachlich von Vereinsnamen trennen

Umgesetzt ist zunaechst der Importpfad aus `podium-export.json`. Die Teamdaten werden aus bestehenden Mannschafts-Ergebniszeilen abgeleitet. Mannschaftsgesamtergebnisse koennen spaeter genauer werden, wenn der direkte Parserlauf aus den PDF-Strukturen nicht mehr ueber den reduzierten Podium-Export gehen muss.

### Paket 7: Deutsche Meisterschaften und Teilnahme - erledigt

- DM als eigene `competition` importieren
- Teilnahme ohne Platzierung modellieren
- vorhandene Vereinserkennung gegen bekannte Kreisvereine nutzen
- kombinierte Auswertung LM-Medaille zu DM-Teilnahme erzeugen

Stand der Umsetzung:

- `participation-export.json` kann per `import-participation` in die Datenbank importiert werden
- der Import legt eine Competition `DM-<Jahr>` mit `scope = DM` an
- DM-Treffer werden als `results.participation_only = true` gespeichert
- erkannte Schuetzennamen werden mit `athletes` verknuepft
- Treffer ohne erkannte Schuetzen bleiben ueber Verein, Quelle und PDF nachvollziehbar
- die View `lm_medals_with_dm_participation` kombiniert LM-Medaillen mit DM-Teilnahmen desselben Jahres
- aktive `club_aliases` werden beim Import zur Aufloesung bekannter Vereins-Schreibweisen genutzt
- `import-participation` speichert bekannte Schreibweisen ebenfalls als Vereinsaliase
- Vereinsaliase sind ueber CLI und Weboberflaeche pflegbar

Offene Punkte: siehe zentrale Priorisierung, Sofort 7-8 sowie Spaeter 4.

### Paket 8: HTML-/JSON-Reports aus Datenbank - erledigt

- bisherige Reports optional aus Datenbank statt JSON-Dateien erzeugen
- Filter auf Datenbankabfragen stuetzen
- Exportformate stabil halten
- alte JSON-Exporte weiterhin konsumierbar lassen

Stand der Umsetzung:

- `export-db-podium` erzeugt `PodiumExport` JSON und HTML aus kanonischen DB-Ergebnissen
- `export-db-combined` erzeugt `CombinedExport` JSON und HTML aus LM-Ergebnissen und DM-Teilnahmen in SQLite
- Filter fuer Jahr, Wettbewerbsebene, Kreis und Platz bis laufen in SQL-Abfragen
- die Weboberflaeche nutzt fuer Ergebnis-, Sportler-, Vereins- und kombinierte Ansichten serverseitige SQLite-Filter
- lesende Abfragen sind in `application::query` gebuendelt und koennen schrittweise von Web, GUI und CLI-Exporten wiederverwendet werden
- die bisherigen JSON-Datei-Exporter `export-podium`, `export-participation` und `export-combined` bleiben unveraendert nutzbar
- die bestehenden HTML-Templates werden weiterverwendet

Offene Punkte: siehe zentrale Priorisierung, Sofort 1-2 fuer weiteren Filter-/Service-Ausbau sowie Spaeter 1-2.

### Paket 9: GUI-Grundlage - erledigt

- lokale Weboberflaeche mit Rust Backend festlegen
- serverseitig gerendertes HTML als erste UI-Technologie nutzen
- Navigation fuer Importlaeufe, Ergebnisse, Sportler, Vereine und Ehrungen anlegen
- Importlaeufe als Einstiegspunkt der Oberflaeche bauen
- von Importlaeufen in die importierten Daten verzweigen
- lesende Ansichten zuerst bauen
- Ehrungen zunaechst nur als Platzhalter-Navigation aufnehmen
- Schreibaktionen erst nach klarer Korrekturschicht freigeben

Stand der Umsetzung:

- `serve` startet eine lokale, lesende Weboberflaeche mit Rust Backend
- die Datenbank wird ueber `--database` ausgewaehlt
- der lokale Socket wird ueber `--bind` ausgewaehlt
- `/` leitet auf `/import-runs`
- `/import-runs` zeigt Importlaeufe als Einstiegspunkt
- `/import-runs/<id>/results` zeigt die Ergebnisse eines Importlaufs
- `/results?q=&year=&scope=&kreis=&wertung=` zeigt eine filterbare Ergebnisliste
- `/athletes?q=&verein=&year=` zeigt eine filterbare Sportlerliste
- `/clubs?q=&kreis=&year=` zeigt eine filterbare Vereinsliste
- `/teams?q=&year=&scope=&kreis=&page=&page_size=` zeigt eine filterbare Mannschaftsliste
- `/athletes/<id>` zeigt Ergebnisse eines Sportlers
- `/clubs/<id>` zeigt Ergebnisse eines Vereins
- `/teams/<id>` zeigt Mannschaftsdetails und Mitglieder
- `/sources` zeigt bekannte PDF-Quellen
- `/sources/<id>` zeigt Quelle und verknuepfte Ergebnisse
- `/parser-runs` zeigt Parserlaeufe und auffaellige Zeilen
- `/parser-runs/<id>` zeigt Statusdetails eines Parserlaufs
- `/combined?q=&year=&verein=` zeigt LM-Medaillen mit DM-Teilnahme
- `/honors` ist als Platzhalter fuer die spaetere Ehrungslogik vorhanden
- die GUI nutzt Template-Dateien unter `templates/web-*.html`
- die Listen schneiden nicht still bei 500 Eintraegen ab
- grosse Listen besitzen steuerbare Seitengroessen und Seitenwechsel ueber URL-Parameter
- Filterparameter bleiben in der URL erhalten und koennen weitergegeben werden

Offene Punkte: siehe zentrale Priorisierung, Sofort 1, 3-4 und 9-10 fuer weiteren Ausbau. Echte Ehrungsvorschlaege bleiben Paket 11.

### Paket 10: UI fuer Korrekturen - erledigt

- grafische Oberflaeche fuer manuelle Korrekturen
- Schreibaktionen fuer Korrekturen erst hier freigeben
- Ansichten fuer ungepruefte Parserfaelle
- Zusammenfuehrung von Sportlern und Vereinen
- Korrekturen fuer Vereinsnamen und Sportlernamen bearbeiten
- Korrekturen nachvollziehbar speichern, anzeigen und zuruecknehmen
- Historie der Korrekturen anzeigen

Stand der Umsetzung:

- `/corrections` zeigt eine grafische Oberflaeche fuer manuelle Korrekturen
- Korrekturen werden als `manual_overrides` gespeichert
- Vereinsnamen und Sportlernamen koennen als globale Korrektur angelegt werden
- aktive Korrekturen koennen ueber die GUI zurueckgenommen werden
- die Tabelle zeigt aktive und zurueckgenommene Korrekturen als Historie
- Anlage- und Aktualisierungszeit bleiben sichtbar
- `/corrections/issues` zeigt auffaellige Parserzeilen mit Konfliktstatus oder fehlender Normalisierung
- Parserfaelle bieten Links, um Korrekturformulare mit Rohwerten vorzubelegen
- das Korrekturformular bietet vorhandene Sportler- und Vereinsnamen als Vorschlaege an
- Zusammenfuehrung von Sportlern und Vereinen erfolgt in diesem Paket zunaechst ueber Namenskorrekturen
- Parser-Rohdaten werden weiterhin nicht veraendert

Offene Punkte: siehe zentrale Priorisierung, Sofort 5 fuer weitere Validierungsqualitaet sowie Spaeter 5-9. Der Pruefstatus ist bewusst auf auffaellige Parserfaelle begrenzt, nicht auf jede Parserzeile.

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


Du bist ein extrem token-effizienter Programmier-Assistent. Dein Ziel ist es, präzise Lösungen mit minimalem Text- und Codeaufwand zu liefern.

Befolge strikt diese Regeln zur Token-Ersparnis:
1. Keine Prosa oder Höflichkeitsfloskeln ("Gerne helfe ich...", "Hier ist der Code..."). Start direkt mit der Antwort.
2. Erkläre Code NUR, wenn ich explizit danach frage.
3. Wenn Code geändert wird, gib NIEMALS die gesamte Datei aus. Zeige AUSSCHLIESSLICH den geänderten Codeblock oder die spezifische Funktion. Nutze Kommentare wie `// ... restlicher Code unverändert ...`, um den Kontext zu wahren.
4. Nutze so wenig Ausgabe-Tokens wie möglich, ohne die Korrektheit des Codes zu gefährden.
