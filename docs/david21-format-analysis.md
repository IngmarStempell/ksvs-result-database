# DAVID21+ PDF Format Analysis

Sample file:

`Data/_sport2025_ergebnisse_ergeb_haupt_2025_1.10.10.pdf`

## PDF Properties

- Creator/producer: PDF24
- PDF version: 1.6
- Pages: 2
- Page size: A4
- Encrypted: no
- Tagged: no
- Text extraction quality: good
- OCR required for this sample: no

## Document Structure

The sample contains two result lists for the same event and discipline:

- `Ergebnisliste Mannschaft 1.10.10`
- `Ergebnisliste Einzel 1.10.10`

Shared event metadata:

- Event: `Landesmeisterschaft 2025`
- Date: `14.06.2025`
- Location: `Kellinghusen`
- System: `DV-System DAVID21+`
- Discipline code: `1.10.10`
- Discipline: `Luftgewehr`
- Class: `Herren I`

## Mannschaft Rows

Team result rows follow this pattern:

```text
<rank> <association> <club> <total> Ringen
```

Example:

```text
1 OH SSV Kassau I 1204,6 Ringen
```

The following rows belong to the current team until the next team starts:

```text
<start_number> <name> <total>
```

Example:

```text
430 Dietmayr, Markus 405,3
```

Out-of-competition teams are introduced by:

```text
Außer Konkurrenz haben geschossen:
```

Those team rows may not have a rank.

## Einzel Rows

Regular individual result rows follow this pattern:

```text
<rank> <start_number> <name> <association> <club> <series_1> <series_2> <series_3> <series_4> <total>
```

Example:

```text
1 431 Jeger, Florian OH SSV Kassau I 98,2 103,2 99,1 105,8 406,3
```

Not-started rows use `na` instead of a numeric rank and only contain the total:

```text
na <start_number> <name> <association> <club> 0,0
```

Out-of-competition individual rows have no rank:

```text
<start_number> <name> <association> <club> <series_1> <series_2> <series_3> <series_4> <total>
```

## Parsed Sample Counts

- Regular teams: 6
- Regular team members: 18
- Out-of-competition teams: 1
- Regular individual rows: 40
- Out-of-competition individual rows: 1

## Parser Implications

- The PDF does not need OCR if future files are generated similarly.
- The parser should treat this as a domain-specific table format, not plain prose.
- Decimal values use German comma notation in the PDF and are normalized to numeric values in JSON.
- Clubs can contain spaces, punctuation, numbers, and roman numerals.
- Association codes are usually two uppercase letters, but at least one sample row uses `14`, so the parser accepts two alphanumeric characters.
- A robust storage model should keep both team results and individual results, linked by event metadata, discipline code, start number, club, and association.
