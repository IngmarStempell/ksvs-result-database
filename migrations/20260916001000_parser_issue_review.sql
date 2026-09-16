ALTER TABLE parsed_result_rows ADD COLUMN review_status TEXT NOT NULL DEFAULT 'open';
ALTER TABLE parsed_result_rows ADD COLUMN review_note TEXT;
ALTER TABLE parsed_result_rows ADD COLUMN reviewed_at TEXT;
