# NDSB Bogen PDF Format Analysis

Sample file:

`data/_sport2025_ergebnisse_ndsb_LM_Bogen.pdf`

## PDF Properties

- Creator: PDFsam Basic v5.2.9
- Producer: SAMBox 3.0.18
- PDF version: 1.5
- Pages: 25
- Page size: A4
- Encrypted: no
- Tagged: no
- Text extraction quality: mixed
- OCR required for this sample: no

## High-Level Finding

This is not the same format as the DAVID21+ rifle result PDF.

The file is a merged NDSB archery result package. It contains several document segments with repeated title pages, individual result tables, and team result pages:

- Landesmeisterschaft Halle 2025
- Landesmeisterschaft im Freien 2025
- Landesmeisterschaft 2025 Feldbogen
- Landesmeisterschaft 3D 2025

The current `parse-sport` command intentionally returns no rows for this PDF because it only supports the earlier DAVID21+ result-list format.

## Detected Structure

Using page-level extraction, the sample contains:

- Total pages: 25
- Individual class sections: 104
- Team result sections: 11
- Title or cover pages: multiple

Page groups observed:

- Pages 1-7: Halle 2025 individual results
- Pages 8-9: Halle 2025 team results
- Pages 10-17: Freien 2025 individual results
- Pages 18-19: Freien 2025 team results
- Pages 20-21: Feldbogen individual results
- Page 22: 3D title page
- Pages 23-25: 3D individual results

## Individual Result Tables

Individual sections are introduced by a class heading:

```text
<bow/class name> - Spo Kennziffer: <code>
```

Examples:

```text
Recurve Herren - Spo Kennziffer: 6.20.10
Blank Schüler A - Spo Kennziffer: 6.26.20
Compound Master männlich - Spo Kennziffer: 6.65.12
```

The visible table columns vary by competition type.

Halle:

```text
Start_Nr Name Verein Land 18m 18m 10'/9' Total
```

Freien:

```text
Name Verein Land 70m 70m Scheibe Jahrg. 10'/X' Total
```

Feldbogen:

```text
unb./bek. Land Name Start_Nr Total 6'/5' Verein Jahrg.
```

3D:

```text
Scheib Jahrg. Name Verein Land Tag 1 Tag 2 11'/8' Total
```

## Text Extraction Issues

The PDF contains extractable text, but the text stream frequently merges adjacent visual columns.

Examples from the raw text:

```text
7D Pensky, Daniel Itzehoer Hockey-Club1 55929/22276 283ND*. 1999
```

Visually this row means separate fields:

- Scheibe/Start: `7D`
- Name: `Pensky, Daniel`
- Verein: `Itzehoer Hockey-Club`
- Platz: `1`
- Total: `559`
- 10'/9': `29/22`
- Series: `276`, `283`
- Land: `ND*`
- Jahrgang: `1999`

For this format, robust parsing should use page-aware and position-aware extraction, not only plain line regex over the merged text.

## Team Result Tables

Team sections are introduced by:

```text
Mannschaftsergebnis : <team competition name>
```

Examples:

```text
Mannschaftsergebnis : Recurve Auflage 40er
Mannschaftsergebnis : Mannschaft Recurve 60m
Mannschaftsergebnis : Mannschaft Compound 50m
```

Each team block visually contains:

- Rank
- Team/club name
- Land
- Three team members
- Member scores
- Team total

This is similar in concept to the DAVID21+ Mannschaft list, but the order and spacing are different enough to require a separate parser.

## Parser Implications

- Add a separate parser module, for example `ndsb_bogen`.
- Split PDFs into logical document segments before parsing rows.
- Keep event metadata per segment, not per whole PDF.
- Prefer page-aware extraction with coordinates for this format.
- Store result rows with flexible score columns, because `18m`, `70m`, `Tag 1`, `unb./bek.`, and hit-count labels differ by event type.
- Do not use OCR as the first fallback here; the problem is not missing text, but column reconstruction.
