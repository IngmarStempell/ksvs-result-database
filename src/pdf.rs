use std::fs;
use std::panic::{self, AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, anyhow};
use serde::Serialize;

#[derive(Debug, Clone)]
pub struct ExtractOptions {
    pub min_text_chars: usize,
}

impl Default for ExtractOptions {
    fn default() -> Self {
        Self { min_text_chars: 80 }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PdfDocument {
    pub source_path: PathBuf,
    pub file_size_bytes: u64,
    pub text: String,
    pub text_char_count: usize,
    pub needs_ocr: bool,
}

#[derive(Debug, Clone)]
pub struct PdfExtractor {
    options: ExtractOptions,
}

impl PdfExtractor {
    #[must_use]
    pub const fn new(options: ExtractOptions) -> Self {
        Self { options }
    }

    /// Extracts plain text and basic file metadata from a PDF.
    ///
    /// # Errors
    ///
    /// Returns an error if the file metadata cannot be read, if the PDF text
    /// extractor fails, or if the extractor panics on malformed PDF content.
    pub fn extract(&self, path: impl AsRef<Path>) -> Result<PdfDocument> {
        let path = path.as_ref();
        let metadata = fs::metadata(path)
            .with_context(|| format!("could not read metadata for {}", path.display()))?;
        let previous_panic_hook = panic::take_hook();
        panic::set_hook(Box::new(|_| {}));
        let extraction = catch_unwind(AssertUnwindSafe(|| pdf_extract::extract_text(path)));
        panic::set_hook(previous_panic_hook);

        let mut text = match extraction {
            Ok(Ok(text)) => text,
            Ok(Err(error)) => extract_text_with_pdftotext(path).with_context(|| {
                format!(
                    "could not extract text from {}; pdf_extract failed: {error}",
                    path.display()
                )
            })?,
            Err(panic) => {
                let panic_message = panic_message(&panic);
                extract_text_with_pdftotext(path).with_context(|| {
                    format!(
                        "could not extract text from {}; pdf_extract panicked: {panic_message}",
                        path.display()
                    )
                })?
            }
        };
        if has_suspicious_control_characters(&text)
            && let Ok(fallback_text) = extract_text_with_pdftotext(path)
        {
            text = fallback_text;
        }
        let text_char_count = text.chars().count();

        Ok(PdfDocument {
            source_path: path.to_path_buf(),
            file_size_bytes: metadata.len(),
            needs_ocr: is_ocr_candidate(text_char_count, self.options.min_text_chars),
            text,
            text_char_count,
        })
    }
}

fn extract_text_with_pdftotext(path: &Path) -> Result<String> {
    let output = Command::new("pdftotext")
        .arg("-layout")
        .arg(path)
        .arg("-")
        .output()
        .with_context(|| format!("could not run pdftotext for {}", path.display()))?;

    if !output.status.success() {
        return Err(anyhow!(
            "pdftotext failed for {}: {}",
            path.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    String::from_utf8(output.stdout)
        .with_context(|| format!("pdftotext returned non-UTF-8 text for {}", path.display()))
}

const fn is_ocr_candidate(text_char_count: usize, min_text_chars: usize) -> bool {
    text_char_count < min_text_chars
}

fn has_suspicious_control_characters(text: &str) -> bool {
    text.chars().any(|character| {
        character.is_control() && !matches!(character, '\n' | '\r' | '\t' | '\x0c')
    })
}

fn panic_message(panic: &Box<dyn std::any::Any + Send>) -> String {
    panic.downcast_ref::<&str>().map_or_else(
        || {
            panic
                .downcast_ref::<String>()
                .map_or_else(|| "unknown panic".to_string(), Clone::clone)
        },
        |message| (*message).to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::{has_suspicious_control_characters, is_ocr_candidate};

    #[test]
    fn marks_sparse_text_as_ocr_candidate() {
        assert!(is_ocr_candidate(20, 80));
        assert!(!is_ocr_candidate(80, 80));
        assert!(!is_ocr_candidate(120, 80));
    }

    #[test]
    fn identifies_null_bytes_as_suspicious_extraction_artifacts() {
        assert!(has_suspicious_control_characters("Eberhard R\0hl"));
        assert!(!has_suspicious_control_characters("Eberhard Rühl\n"));
    }
}
