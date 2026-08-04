use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use percent_encoding::percent_decode_str;
use reqwest::StatusCode;
use reqwest::blocking::Client;
use reqwest::header::{ETAG, HeaderMap, IF_MODIFIED_SINCE, IF_NONE_MATCH, LAST_MODIFIED};
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use url::Url;

use crate::pdf::{ExtractOptions, PdfExtractor};
use crate::sport_results::{Rank, SportResultList, SportResultsParser};

const MANIFEST_FILE: &str = "pdf-manifest.json";
const USER_AGENT: &str = "pdf-explorer/0.1";

#[derive(Debug, Clone)]
pub struct CrawlConfig {
    pub source_url: String,
    pub source_name: String,
    pub state_dir: PathBuf,
    pub download_dir: PathBuf,
    pub manual_review_dir: PathBuf,
    pub report_path: PathBuf,
    pub html_report_path: PathBuf,
    pub focus: String,
    pub focus_association_code: String,
    pub year: Option<String>,
    pub extra_pdf_urls: Vec<String>,
    pub max_depth: usize,
    pub max_pages: usize,
    pub min_text_chars: usize,
}

#[derive(Debug)]
pub struct CrawlReporter {
    config: CrawlConfig,
    client: Client,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrawlReport {
    pub generated_at: DateTime<Utc>,
    pub source_url: String,
    #[serde(default)]
    pub source_name: String,
    pub focus: String,
    pub focus_association_code: String,
    pub discovered_pdf_count: usize,
    pub downloaded_count: usize,
    pub changed_count: usize,
    pub unchanged_count: usize,
    pub removed_count: usize,
    pub auto_processed_count: usize,
    pub manual_review_count: usize,
    pub failed_count: usize,
    pub removed_pdfs: Vec<RemovedPdf>,
    pub pdfs: Vec<PdfReportItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemovedPdf {
    pub url: String,
    pub previous_local_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PdfReportItem {
    pub url: String,
    pub status: PdfChangeStatus,
    pub classification: PdfClassification,
    pub local_path: Option<PathBuf>,
    pub manual_review_path: Option<PathBuf>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub sha256: Option<String>,
    pub file_size_bytes: Option<u64>,
    pub text_char_count: Option<usize>,
    pub needs_ocr: Option<bool>,
    pub david21_summary: Option<David21Summary>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PdfChangeStatus {
    New,
    Changed,
    Unchanged,
    Removed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PdfClassification {
    David21,
    ManualReview,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct David21Summary {
    pub event_name: String,
    pub event_date: Option<String>,
    pub location: Option<String>,
    pub discipline_code: Option<String>,
    pub discipline: Option<String>,
    pub class_name: Option<String>,
    pub team_results: usize,
    pub team_members: usize,
    pub individual_results: usize,
    pub out_of_competition_team_results: usize,
    pub out_of_competition_individual_results: usize,
    pub associations: Vec<String>,
    pub podium_associations: Vec<String>,
    pub association_placements: Vec<AssociationPlacement>,
    pub focus: David21FocusSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssociationPlacement {
    pub association_code: String,
    pub place: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct David21FocusSummary {
    pub association_code: String,
    pub team_results: usize,
    pub team_members: usize,
    pub individual_results: usize,
    pub out_of_competition_team_results: usize,
    pub out_of_competition_team_members: usize,
    pub out_of_competition_individual_results: usize,
    pub podium_team_results: usize,
    pub podium_team_members: usize,
    pub podium_individual_results: usize,
    pub podium_clubs: Vec<String>,
    pub clubs: Vec<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct PdfManifest {
    pdfs: BTreeMap<String, PdfManifestEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PdfManifestEntry {
    local_path: PathBuf,
    etag: Option<String>,
    last_modified: Option<String>,
    sha256: String,
    file_size_bytes: u64,
    last_seen_at: DateTime<Utc>,
}

#[derive(Debug)]
struct DownloadedPdf {
    url: String,
    local_path: PathBuf,
    etag: Option<String>,
    last_modified: Option<String>,
    sha256: String,
    file_size_bytes: u64,
    change_status: PdfChangeStatus,
}

impl CrawlReporter {
    /// Creates a crawler and report generator.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP client cannot be constructed.
    pub fn new(config: CrawlConfig) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent(USER_AGENT)
            .build()
            .context("could not create HTTP client")?;

        Ok(Self { config, client })
    }

    /// Runs discovery, download/change detection, PDF classification, and report writing.
    ///
    /// # Errors
    ///
    /// Returns an error if state directories cannot be created, the source page
    /// cannot be fetched, or report/manifest files cannot be written.
    pub fn run(&self) -> Result<CrawlReport> {
        fs::create_dir_all(&self.config.state_dir).context("could not create state directory")?;
        fs::create_dir_all(&self.config.download_dir)
            .context("could not create download directory")?;
        fs::create_dir_all(&self.config.manual_review_dir)
            .context("could not create manual review directory")?;
        if let Some(parent) = self.config.report_path.parent() {
            fs::create_dir_all(parent).context("could not create report directory")?;
        }
        if let Some(parent) = self.config.html_report_path.parent() {
            fs::create_dir_all(parent).context("could not create HTML report directory")?;
        }

        let mut manifest = self.load_manifest()?;
        let previous_urls: BTreeSet<_> = manifest.pdfs.keys().cloned().collect();
        let pdf_urls = self.discover_pdf_urls()?;
        let current_urls: BTreeSet<_> = pdf_urls.iter().cloned().collect();
        let removed_pdfs = previous_urls
            .difference(&current_urls)
            .filter_map(|url| {
                manifest.pdfs.get(url).map(|entry| RemovedPdf {
                    url: url.clone(),
                    previous_local_path: entry.local_path.clone(),
                })
            })
            .collect::<Vec<_>>();

        let mut items = Vec::new();
        let now = Utc::now();
        for pdf_url in pdf_urls {
            match self.download_pdf(&pdf_url, manifest.pdfs.get(&pdf_url)) {
                Ok(downloaded) => {
                    let item = self.analyze_downloaded_pdf(&downloaded);
                    manifest.pdfs.insert(
                        pdf_url,
                        PdfManifestEntry {
                            local_path: downloaded.local_path,
                            etag: downloaded.etag,
                            last_modified: downloaded.last_modified,
                            sha256: downloaded.sha256,
                            file_size_bytes: downloaded.file_size_bytes,
                            last_seen_at: now,
                        },
                    );
                    items.push(item);
                }
                Err(error) => items.push(PdfReportItem {
                    url: pdf_url,
                    status: PdfChangeStatus::Failed,
                    classification: PdfClassification::Unknown,
                    local_path: None,
                    manual_review_path: None,
                    etag: None,
                    last_modified: None,
                    sha256: None,
                    file_size_bytes: None,
                    text_char_count: None,
                    needs_ocr: None,
                    david21_summary: None,
                    error: Some(error.to_string()),
                }),
            }
        }

        for removed in &removed_pdfs {
            manifest.pdfs.remove(&removed.url);
        }

        let report = build_report(
            &self.config,
            current_urls.len(),
            removed_pdfs,
            items,
            Utc::now(),
        );

        self.save_manifest(&manifest)?;
        self.save_report(&report)?;
        self.save_html_report(&report)?;

        Ok(report)
    }

    fn discover_pdf_urls(&self) -> Result<Vec<String>> {
        let root = Url::parse(&self.config.source_url).context("source URL is invalid")?;
        let element_selector = Selector::parse("h1, h2, h3, a[href]").expect("valid selector");
        let mut queue = VecDeque::from([(root.clone(), 0_usize)]);
        let mut visited_pages = BTreeSet::new();
        let mut pdf_urls = BTreeSet::new();

        if is_pdf_url(&root) {
            pdf_urls.insert(root.to_string());
        }
        for extra_pdf_url in &self.config.extra_pdf_urls {
            let url = Url::parse(extra_pdf_url)
                .with_context(|| format!("extra PDF URL is invalid: {extra_pdf_url}"))?;
            if !is_pdf_url(&url) {
                bail!("extra PDF URL is not a PDF URL: {extra_pdf_url}");
            }
            pdf_urls.insert(url.to_string());
        }
        if is_pdf_url(&root) {
            return Ok(pdf_urls.into_iter().collect());
        }

        while let Some((page_url, depth)) = queue.pop_front() {
            if visited_pages.len() >= self.config.max_pages {
                break;
            }
            if !visited_pages.insert(page_url.clone()) {
                continue;
            }

            let html = self.fetch_text(page_url.as_str())?;
            let document = Html::parse_document(&html);
            let page_text = document.root_element().text().collect::<String>();
            let mut current_year = extract_year(&page_text);
            for element in document.select(&element_selector) {
                let element_name = element.value().name();
                let element_text = element.text().collect::<String>();
                if matches!(element_name, "h1" | "h2" | "h3") {
                    current_year = extract_year(&element_text);
                    continue;
                }

                let Some(href) = element.value().attr("href") else {
                    continue;
                };
                let Ok(link) = resolve_link(&page_url, &root, href) else {
                    continue;
                };
                if !self.link_matches_scope(
                    &root,
                    &page_url,
                    &link,
                    &element_text,
                    current_year.as_deref(),
                ) {
                    continue;
                }

                if is_pdf_url(&link) {
                    pdf_urls.insert(link.to_string());
                } else if depth < self.config.max_depth
                    && link.domain() == root.domain()
                    && is_http_url(&link)
                {
                    queue.push_back((link, depth + 1));
                }
            }
        }

        Ok(pdf_urls.into_iter().collect())
    }

    fn link_matches_scope(
        &self,
        root: &Url,
        page_url: &Url,
        link: &Url,
        link_text: &str,
        current_year: Option<&str>,
    ) -> bool {
        let Some(year) = &self.config.year else {
            return true;
        };

        let year_token = format!("sport{year}");
        if link.as_str().contains(year)
            || link.as_str().contains(&year_token)
            || link_text.contains(year)
        {
            return true;
        }

        page_url == root && current_year == Some(year.as_str())
    }

    fn fetch_text(&self, url: &str) -> Result<String> {
        let response = self
            .client
            .get(url)
            .send()
            .with_context(|| format!("could not fetch {url}"))?
            .error_for_status()
            .with_context(|| format!("HTTP error while fetching {url}"))?;

        response
            .text()
            .with_context(|| format!("could not read response body from {url}"))
    }

    fn download_pdf(
        &self,
        url: &str,
        previous: Option<&PdfManifestEntry>,
    ) -> Result<DownloadedPdf> {
        let previous_with_local_file = previous.filter(|entry| entry.local_path.is_file());
        let mut request = self.client.get(url);
        if let Some(previous) = previous_with_local_file {
            if let Some(etag) = &previous.etag {
                request = request.header(IF_NONE_MATCH, etag);
            }
            if let Some(last_modified) = &previous.last_modified {
                request = request.header(IF_MODIFIED_SINCE, last_modified);
            }
        }

        let response = request
            .send()
            .with_context(|| format!("could not download PDF {url}"))?;

        if response.status() == StatusCode::NOT_MODIFIED {
            let Some(previous) = previous_with_local_file else {
                bail!("server returned 304 without a previous manifest entry for {url}");
            };

            return Ok(DownloadedPdf {
                url: url.to_string(),
                local_path: previous.local_path.clone(),
                etag: previous.etag.clone(),
                last_modified: previous.last_modified.clone(),
                sha256: previous.sha256.clone(),
                file_size_bytes: previous.file_size_bytes,
                change_status: PdfChangeStatus::Unchanged,
            });
        }

        let status = response.status();
        let headers = response.headers().clone();
        let bytes = response
            .error_for_status()
            .with_context(|| format!("HTTP {status} while downloading {url}"))?
            .bytes()
            .with_context(|| format!("could not read PDF bytes from {url}"))?;
        let sha256 = hex::encode(Sha256::digest(&bytes));
        let file_size_bytes = bytes.len() as u64;
        let local_path = self.config.download_dir.join(pdf_filename(url, &sha256)?);
        fs::write(&local_path, &bytes)
            .with_context(|| format!("could not write {}", local_path.display()))?;

        let change_status = match previous_with_local_file {
            None => PdfChangeStatus::New,
            Some(previous) if previous.sha256 == sha256 => PdfChangeStatus::Unchanged,
            Some(_) => PdfChangeStatus::Changed,
        };

        Ok(DownloadedPdf {
            url: url.to_string(),
            local_path,
            etag: header_to_string(&headers, ETAG.as_str()),
            last_modified: header_to_string(&headers, LAST_MODIFIED.as_str()),
            sha256,
            file_size_bytes,
            change_status,
        })
    }

    fn analyze_downloaded_pdf(&self, downloaded: &DownloadedPdf) -> PdfReportItem {
        match PdfExtractor::new(ExtractOptions {
            min_text_chars: self.config.min_text_chars,
        })
        .extract(&downloaded.local_path)
        {
            Ok(document) if is_david21_text(&document.text) => {
                match SportResultsParser::new().parse(&document.text) {
                    Ok(result_list) => PdfReportItem {
                        url: downloaded.url.clone(),
                        status: downloaded.change_status.clone(),
                        classification: PdfClassification::David21,
                        local_path: Some(downloaded.local_path.clone()),
                        manual_review_path: None,
                        etag: downloaded.etag.clone(),
                        last_modified: downloaded.last_modified.clone(),
                        sha256: Some(downloaded.sha256.clone()),
                        file_size_bytes: Some(downloaded.file_size_bytes),
                        text_char_count: Some(document.text_char_count),
                        needs_ocr: Some(document.needs_ocr),
                        david21_summary: Some(summarize_david21(
                            &result_list,
                            &self.config.focus_association_code,
                        )),
                        error: None,
                    },
                    Err(error) => self.manual_review_item(
                        downloaded,
                        Some(document.text_char_count),
                        Some(document.needs_ocr),
                        Some(format!(
                            "DAVID21+ classification matched but parsing failed: {error}"
                        )),
                    ),
                }
            }
            Ok(document) => self.manual_review_item(
                downloaded,
                Some(document.text_char_count),
                Some(document.needs_ocr),
                None,
            ),
            Err(error) => self.manual_review_item(
                downloaded,
                None,
                None,
                Some(format!("text extraction failed: {error}")),
            ),
        }
    }

    fn manual_review_item(
        &self,
        downloaded: &DownloadedPdf,
        text_char_count: Option<usize>,
        needs_ocr: Option<bool>,
        error: Option<String>,
    ) -> PdfReportItem {
        let review_path = self
            .config
            .manual_review_dir
            .join(downloaded.local_path.file_name().unwrap_or_default());
        let copy_result = copy_if_different(&downloaded.local_path, &review_path);
        let error = match (error, copy_result.err()) {
            (Some(error), Some(copy_error)) => {
                Some(format!("{error}; manual review copy failed: {copy_error}"))
            }
            (Some(error), None) => Some(error),
            (None, Some(copy_error)) => Some(format!("manual review copy failed: {copy_error}")),
            (None, None) => None,
        };

        PdfReportItem {
            url: downloaded.url.clone(),
            status: downloaded.change_status.clone(),
            classification: PdfClassification::ManualReview,
            local_path: Some(downloaded.local_path.clone()),
            manual_review_path: Some(review_path),
            etag: downloaded.etag.clone(),
            last_modified: downloaded.last_modified.clone(),
            sha256: Some(downloaded.sha256.clone()),
            file_size_bytes: Some(downloaded.file_size_bytes),
            text_char_count,
            needs_ocr,
            david21_summary: None,
            error,
        }
    }

    fn manifest_path(&self) -> PathBuf {
        let manifest_scope = manifest_scope(&self.config.source_name, self.config.year.as_deref());
        if manifest_scope.trim().is_empty() {
            return self.config.state_dir.join(MANIFEST_FILE);
        }

        self.config.state_dir.join(format!(
            "pdf-manifest-{}.json",
            source_slug(&manifest_scope)
        ))
    }

    fn load_manifest(&self) -> Result<PdfManifest> {
        let path = self.manifest_path();
        if !path.exists() {
            return Ok(PdfManifest::default());
        }

        let content = fs::read_to_string(&path)
            .with_context(|| format!("could not read {}", path.display()))?;
        serde_json::from_str(&content)
            .with_context(|| format!("could not parse manifest {}", path.display()))
    }

    fn save_manifest(&self, manifest: &PdfManifest) -> Result<()> {
        let path = self.manifest_path();
        let content = serde_json::to_string_pretty(manifest)?;
        fs::write(&path, content).with_context(|| format!("could not write {}", path.display()))
    }

    fn save_report(&self, report: &CrawlReport) -> Result<()> {
        let content = serde_json::to_string_pretty(report)?;
        fs::write(&self.config.report_path, content)
            .with_context(|| format!("could not write {}", self.config.report_path.display()))
    }

    fn save_html_report(&self, report: &CrawlReport) -> Result<()> {
        let report_dir = self.config.html_report_path.parent();
        let content = render_html_report(report, report_dir);
        fs::write(&self.config.html_report_path, content)
            .with_context(|| format!("could not write {}", self.config.html_report_path.display()))
    }
}

fn build_report(
    config: &CrawlConfig,
    discovered_pdf_count: usize,
    removed_pdfs: Vec<RemovedPdf>,
    pdfs: Vec<PdfReportItem>,
    generated_at: DateTime<Utc>,
) -> CrawlReport {
    CrawlReport {
        generated_at,
        source_url: config.source_url.clone(),
        source_name: config.source_name.clone(),
        focus: config.focus.clone(),
        focus_association_code: config.focus_association_code.clone(),
        discovered_pdf_count,
        downloaded_count: pdfs
            .iter()
            .filter(|pdf| matches!(pdf.status, PdfChangeStatus::New | PdfChangeStatus::Changed))
            .count(),
        changed_count: pdfs
            .iter()
            .filter(|pdf| matches!(pdf.status, PdfChangeStatus::Changed))
            .count(),
        unchanged_count: pdfs
            .iter()
            .filter(|pdf| matches!(pdf.status, PdfChangeStatus::Unchanged))
            .count(),
        removed_count: removed_pdfs.len(),
        auto_processed_count: pdfs
            .iter()
            .filter(|pdf| pdf.classification == PdfClassification::David21)
            .count(),
        manual_review_count: pdfs
            .iter()
            .filter(|pdf| pdf.classification == PdfClassification::ManualReview)
            .count(),
        failed_count: pdfs
            .iter()
            .filter(|pdf| pdf.status == PdfChangeStatus::Failed || pdf.error.is_some())
            .count(),
        removed_pdfs,
        pdfs,
    }
}

fn is_http_url(url: &Url) -> bool {
    matches!(url.scheme(), "http" | "https")
}

fn resolve_link(page_url: &Url, root: &Url, href: &str) -> Result<Url, url::ParseError> {
    if href.starts_with("download_/") || href.starts_with("fileadmin/") {
        return root.join(&format!("/{href}"));
    }

    page_url.join(href)
}

fn extract_year(text: &str) -> Option<String> {
    text.split(|character: char| !character.is_ascii_digit())
        .find(|part| part.len() == 4 && part.starts_with("20"))
        .map(ToOwned::to_owned)
}

fn is_pdf_url(url: &Url) -> bool {
    is_http_url(url) && url.path().to_ascii_lowercase().ends_with(".pdf")
}

fn is_david21_text(text: &str) -> bool {
    text.contains("DV-System DAVID21+") && text.contains("Ergebnisliste")
}

fn summarize_david21(
    result_list: &SportResultList,
    focus_association_code: &str,
) -> David21Summary {
    David21Summary {
        event_name: result_list.event.name.clone(),
        event_date: result_list.event.date.clone(),
        location: result_list.event.location.clone(),
        discipline_code: result_list.event.discipline_code.clone(),
        discipline: result_list.event.discipline.clone(),
        class_name: result_list.event.class_name.clone(),
        team_results: result_list.team_results.len(),
        team_members: result_list
            .team_results
            .iter()
            .map(|team| team.members.len())
            .sum(),
        individual_results: result_list.individual_results.len(),
        out_of_competition_team_results: result_list.out_of_competition_team_results.len(),
        out_of_competition_individual_results: result_list
            .out_of_competition_individual_results
            .len(),
        associations: david21_associations(result_list),
        podium_associations: david21_podium_associations(result_list),
        association_placements: david21_association_placements(result_list),
        focus: summarize_david21_focus(result_list, focus_association_code),
    }
}

fn david21_associations(result_list: &SportResultList) -> Vec<String> {
    let mut associations = BTreeSet::new();

    for team in result_list
        .team_results
        .iter()
        .chain(&result_list.out_of_competition_team_results)
    {
        associations.insert(team.association.clone());
    }

    for result in result_list
        .individual_results
        .iter()
        .chain(&result_list.out_of_competition_individual_results)
    {
        associations.insert(result.association.clone());
    }

    associations.into_iter().collect()
}

fn david21_podium_associations(result_list: &SportResultList) -> Vec<String> {
    let mut associations = BTreeSet::new();

    for team in &result_list.team_results {
        if team.rank.is_some_and(is_podium_place) {
            associations.insert(team.association.clone());
        }
    }

    for result in &result_list.individual_results {
        if is_podium_rank(&result.rank) {
            associations.insert(result.association.clone());
        }
    }

    associations.into_iter().collect()
}

fn david21_association_placements(result_list: &SportResultList) -> Vec<AssociationPlacement> {
    let mut placements = BTreeSet::new();

    for team in &result_list.team_results {
        if let Some(place) = team.rank {
            placements.insert((team.association.clone(), place));
        }
    }

    for result in &result_list.individual_results {
        if let Rank::Place(place) = result.rank {
            placements.insert((result.association.clone(), place));
        }
    }

    placements
        .into_iter()
        .map(|(association_code, place)| AssociationPlacement {
            association_code,
            place,
        })
        .collect()
}

fn summarize_david21_focus(
    result_list: &SportResultList,
    focus_association_code: &str,
) -> David21FocusSummary {
    let code = focus_association_code.trim();
    let mut clubs = BTreeSet::new();
    let mut podium_clubs = BTreeSet::new();

    let team_results = result_list
        .team_results
        .iter()
        .filter(|team| team.association == code)
        .inspect(|team| {
            clubs.insert(team.club.clone());
        })
        .count();
    let team_members = result_list
        .team_results
        .iter()
        .filter(|team| team.association == code)
        .map(|team| team.members.len())
        .sum();
    let individual_results = result_list
        .individual_results
        .iter()
        .filter(|result| result.association == code)
        .inspect(|result| {
            clubs.insert(result.club.clone());
        })
        .count();
    let out_of_competition_team_results = result_list
        .out_of_competition_team_results
        .iter()
        .filter(|team| team.association == code)
        .inspect(|team| {
            clubs.insert(team.club.clone());
        })
        .count();
    let out_of_competition_team_members = result_list
        .out_of_competition_team_results
        .iter()
        .filter(|team| team.association == code)
        .map(|team| team.members.len())
        .sum();
    let out_of_competition_individual_results = result_list
        .out_of_competition_individual_results
        .iter()
        .filter(|result| result.association == code)
        .inspect(|result| {
            clubs.insert(result.club.clone());
        })
        .count();
    let podium_team_results = result_list
        .team_results
        .iter()
        .filter(|team| team.association == code && team.rank.is_some_and(is_podium_place))
        .inspect(|team| {
            podium_clubs.insert(team.club.clone());
        })
        .count();
    let podium_team_members = result_list
        .team_results
        .iter()
        .filter(|team| team.association == code && team.rank.is_some_and(is_podium_place))
        .map(|team| team.members.len())
        .sum();
    let podium_individual_results = result_list
        .individual_results
        .iter()
        .filter(|result| result.association == code && is_podium_rank(&result.rank))
        .inspect(|result| {
            podium_clubs.insert(result.club.clone());
        })
        .count();

    David21FocusSummary {
        association_code: code.to_string(),
        team_results,
        team_members,
        individual_results,
        out_of_competition_team_results,
        out_of_competition_team_members,
        out_of_competition_individual_results,
        podium_team_results,
        podium_team_members,
        podium_individual_results,
        podium_clubs: podium_clubs.into_iter().collect(),
        clubs: clubs.into_iter().collect(),
    }
}

const fn is_podium_place(rank: u32) -> bool {
    matches!(rank, 1..=3)
}

const fn is_podium_rank(rank: &Rank) -> bool {
    matches!(rank, Rank::Place(place) if is_podium_place(*place))
}

fn header_to_string(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(ToOwned::to_owned)
}

fn pdf_filename(url: &str, sha256: &str) -> Result<String> {
    let url = Url::parse(url).context("invalid PDF URL")?;
    let raw_name = url
        .path_segments()
        .and_then(Iterator::last)
        .filter(|name| !name.is_empty())
        .unwrap_or("download.pdf");
    let decoded_name = percent_decode_str(raw_name).decode_utf8_lossy();
    let sanitized = sanitize_filename(&decoded_name);
    let short_hash = sha256.get(..12).unwrap_or(sha256);

    Ok(format!("{short_hash}-{sanitized}"))
}

fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|character| match character {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '.' | '-' | '_' => character,
            _ => '_',
        })
        .collect()
}

fn source_slug(source_name: &str) -> String {
    let slug: String = source_name
        .trim()
        .chars()
        .map(|character| match character {
            'a'..='z' | 'A'..='Z' | '0'..='9' => character.to_ascii_lowercase(),
            _ => '-',
        })
        .collect();
    let slug = slug.trim_matches('-');

    if slug.is_empty() {
        "default".to_string()
    } else {
        slug.to_string()
    }
}

fn manifest_scope(source_name: &str, year: Option<&str>) -> String {
    match (
        source_name.trim(),
        year.map(str::trim).filter(|value| !value.is_empty()),
    ) {
        ("", None) => String::new(),
        ("", Some(year)) => year.to_string(),
        (source_name, None) => source_name.to_string(),
        (source_name, Some(year)) => format!("{source_name}-{year}"),
    }
}

fn copy_if_different(source: &Path, destination: &Path) -> Result<()> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    if destination.exists() && fs::read(source)? == fs::read(destination)? {
        return Ok(());
    }
    fs::copy(source, destination)?;
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn render_html_report(report: &CrawlReport, report_dir: Option<&Path>) -> String {
    let mut rows = String::new();
    for pdf in &report.pdfs {
        let _ = writeln!(
            rows,
            "<tr class=\"{}\" data-associations=\"{}\" data-placements=\"{}\"><td><span class=\"badge\">{}</span></td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td><code>{}</code></td><td>{}</td><td>{}</td></tr>",
            status_class(&pdf.status),
            escape_html(&podium_association_codes(pdf.david21_summary.as_ref()).join(" ")),
            escape_html(&association_placement_data(pdf.david21_summary.as_ref())),
            escape_html(&format!("{:?}", pdf.status)),
            escape_html(&format!("{:?}", pdf.classification)),
            link_html(&pdf.url, &pdf.url),
            path_cell(pdf.local_path.as_ref(), report_dir),
            path_cell(pdf.manual_review_path.as_ref(), report_dir),
            escape_html(&podium_association_codes(pdf.david21_summary.as_ref()).join(", ")),
            david21_cell(pdf.david21_summary.as_ref()),
            escape_html(pdf.error.as_deref().unwrap_or(""))
        );
    }

    let removed_rows = if report.removed_pdfs.is_empty() {
        "<p>Keine Abgänge erkannt.</p>".to_string()
    } else {
        let mut content = String::from("<ul>");
        for removed in &report.removed_pdfs {
            let _ = write!(
                content,
                "<li>{} <span class=\"muted\">{}</span></li>",
                link_html(&removed.url, &removed.url),
                escape_html(&removed.previous_local_path.display().to_string())
            );
        }
        content.push_str("</ul>");
        content
    };

    format!(
        r#"<!doctype html>
<html lang="de">
<head>
  <meta charset="utf-8">
  <title>PDF Explorer Report</title>
  <style>
    :root {{ color-scheme: light; font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; }}
    body {{ margin: 32px; color: #1f2933; background: #f7f8fa; }}
    main {{ max-width: 1280px; margin: 0 auto; }}
    h1 {{ margin: 0 0 8px; font-size: 28px; }}
    h2 {{ margin-top: 32px; font-size: 20px; }}
    .muted {{ color: #667085; }}
    .summary {{ display: grid; grid-template-columns: repeat(auto-fit, minmax(150px, 1fr)); gap: 12px; margin: 24px 0; }}
    .metric {{ background: white; border: 1px solid #d8dde6; border-radius: 8px; padding: 14px; }}
    .metric strong {{ display: block; font-size: 24px; margin-bottom: 4px; }}
    table {{ width: 100%; border-collapse: collapse; background: white; border: 1px solid #d8dde6; }}
    th, td {{ padding: 10px; border-bottom: 1px solid #e5e9f0; text-align: left; vertical-align: top; font-size: 14px; }}
    th {{ background: #eef2f6; font-weight: 700; }}
    a {{ color: #175cd3; text-decoration: none; }}
    a:hover {{ text-decoration: underline; }}
    .local-file code {{ color: inherit; }}
    .badge {{ display: inline-block; min-width: 82px; padding: 3px 8px; border-radius: 999px; background: #eef2f6; font-size: 12px; text-align: center; }}
    tr.new .badge {{ background: #dcfae6; }}
    tr.changed .badge {{ background: #fef0c7; }}
    tr.failed .badge {{ background: #fee4e2; }}
    .focus {{ margin-top: 4px; color: #175cd3; }}
    .podium {{ font-weight: 700; }}
    .filters {{ display: flex; flex-wrap: wrap; align-items: end; gap: 16px; margin: 24px 0; padding: 16px; background: white; border: 1px solid #d8dde6; border-radius: 8px; }}
    .filters label {{ display: grid; gap: 6px; font-size: 13px; font-weight: 700; }}
    .filters input {{ min-width: 120px; padding: 8px 10px; border: 1px solid #b9c2d0; border-radius: 6px; font-size: 14px; }}
    .filters button {{ padding: 8px 12px; border: 1px solid #b9c2d0; border-radius: 6px; background: #eef2f6; cursor: pointer; }}
    .hidden {{ display: none; }}
    code {{ font-family: ui-monospace, SFMono-Regular, Menlo, monospace; font-size: 12px; }}
  </style>
</head>
<body>
<main>
  <h1>PDF Explorer Report</h1>
  <p class="muted">Ursprung: {source_name} · Quelle: {source} · erzeugt: {generated_at} · Fokus: {focus} ({focus_code})</p>

  <section class="summary">
    <div class="metric"><strong>{discovered}</strong>PDFs gefunden</div>
    <div class="metric"><strong>{downloaded}</strong>neu/geändert</div>
    <div class="metric"><strong>{unchanged}</strong>unverändert</div>
    <div class="metric"><strong>{removed}</strong>Abgänge</div>
    <div class="metric"><strong>{auto}</strong>automatisch verarbeitet</div>
    <div class="metric"><strong>{manual}</strong>manuelle Kontrolle</div>
    <div class="metric"><strong>{failed}</strong>Fehler</div>
  </section>

  <h2>PDFs</h2>
  <section class="filters" aria-label="Reportfilter">
    <label>Kreiscode
      <input id="associationFilter" value="{focus_code}" placeholder="z.B. OD, SE, OH">
    </label>
    <label>Platz bis
      <input id="maxPlaceFilter" type="number" min="1" step="1" value="3" placeholder="alle">
    </label>
    <button type="button" id="clearAssociationFilter">Alle Kreise</button>
    <button type="button" id="clearPlaceFilter">Alle Plätze</button>
    <span class="muted"><span id="visibleRows">0</span> von <span id="totalRows">0</span> Zeilen passend sichtbar</span>
  </section>
  <table>
    <thead>
      <tr>
        <th>Status</th>
        <th>Format</th>
        <th>URL</th>
        <th>Lokal</th>
        <th>Manuelle Kontrolle</th>
        <th>Podest-Kreise</th>
        <th>DAVID21+ / Fokus</th>
        <th>Fehler</th>
      </tr>
    </thead>
    <tbody>
      {rows}
    </tbody>
  </table>

  <h2>Abgänge</h2>
  {removed_rows}
</main>
<script>
  const associationFilter = document.getElementById("associationFilter");
  const maxPlaceFilter = document.getElementById("maxPlaceFilter");
  const clearAssociationFilter = document.getElementById("clearAssociationFilter");
  const clearPlaceFilter = document.getElementById("clearPlaceFilter");
  const rows = Array.from(document.querySelectorAll("tbody tr"));
  const visibleRows = document.getElementById("visibleRows");
  const totalRows = document.getElementById("totalRows");

  function applyAssociationFilter() {{
    const code = associationFilter.value.trim().toUpperCase();
    const maxPlace = Number.parseInt(maxPlaceFilter.value, 10);
    const hasPlaceLimit = Number.isFinite(maxPlace) && maxPlace > 0;
    let visible = 0;
    for (const row of rows) {{
      const placements = row.dataset.placements
        .split(/\s+/)
        .filter(Boolean)
        .map((value) => {{
          const [association, place] = value.split(":");
          return {{ association: association.toUpperCase(), place: Number.parseInt(place, 10) }};
        }})
        .filter((placement) => Number.isFinite(placement.place));
      const matches = code === "" && !hasPlaceLimit || placements.some((placement) => {{
        const codeMatches = code === "" || placement.association === code;
        const placeMatches = !hasPlaceLimit || placement.place <= maxPlace;
        return codeMatches && placeMatches;
      }});
      row.classList.toggle("hidden", !matches);
      if (matches) {{
        visible += 1;
      }}
    }}
    visibleRows.textContent = String(visible);
    totalRows.textContent = String(rows.length);
  }}

  associationFilter.addEventListener("input", applyAssociationFilter);
  maxPlaceFilter.addEventListener("input", applyAssociationFilter);
  clearAssociationFilter.addEventListener("click", () => {{
    associationFilter.value = "";
    applyAssociationFilter();
  }});
  clearPlaceFilter.addEventListener("click", () => {{
    maxPlaceFilter.value = "";
    applyAssociationFilter();
  }});
  applyAssociationFilter();
</script>
</body>
</html>
"#,
        source = link_html(&report.source_url, &report.source_url),
        source_name = escape_html(&report.source_name),
        generated_at = escape_html(&report.generated_at.to_rfc3339()),
        focus = escape_html(&report.focus),
        focus_code = escape_html(&report.focus_association_code),
        discovered = report.discovered_pdf_count,
        downloaded = report.downloaded_count,
        unchanged = report.unchanged_count,
        removed = report.removed_count,
        auto = report.auto_processed_count,
        manual = report.manual_review_count,
        failed = report.failed_count,
        rows = rows,
        removed_rows = removed_rows
    )
}

fn david21_cell(summary: Option<&David21Summary>) -> String {
    let Some(summary) = summary else {
        return String::new();
    };
    let focus = &summary.focus;
    let clubs = if focus.podium_clubs.is_empty() {
        "keine Podest-Vereine".to_string()
    } else {
        escape_html(&focus.podium_clubs.join(", "))
    };

    format!(
        "{}<br><span class=\"muted\">{} {} · Teams {} · Einzel {}</span><div class=\"focus\"><span class=\"podium\">{} Podest: Teams {}, Schützen {}, Einzel {}</span><br>{}</div>",
        escape_html(&summary.event_name),
        escape_html(summary.discipline.as_deref().unwrap_or("")),
        escape_html(summary.class_name.as_deref().unwrap_or("")),
        summary.team_results,
        summary.individual_results,
        escape_html(&focus.association_code),
        focus.podium_team_results,
        focus.podium_team_members,
        focus.podium_individual_results,
        clubs
    )
}

fn podium_association_codes(summary: Option<&David21Summary>) -> Vec<String> {
    summary.map_or_else(Vec::new, |summary| summary.podium_associations.clone())
}

fn association_placement_data(summary: Option<&David21Summary>) -> String {
    summary.map_or_else(String::new, |summary| {
        summary
            .association_placements
            .iter()
            .map(|placement| format!("{}:{}", placement.association_code, placement.place))
            .collect::<Vec<_>>()
            .join(" ")
    })
}

fn path_cell(path: Option<&PathBuf>, report_dir: Option<&Path>) -> String {
    path.map_or_else(String::new, |path| {
        let label = escape_html(&path.display().to_string());
        local_path_href(path, report_dir).map_or_else(
            || format!("<code>{label}</code>"),
            |href| {
                let href = escape_html(&href);
                format!("<a class=\"local-file\" href=\"{href}\"><code>{label}</code></a>")
            },
        )
    })
}

fn local_path_href(path: &Path, report_dir: Option<&Path>) -> Option<String> {
    if let Some(href) = relative_local_href(path, report_dir) {
        return Some(href);
    }

    local_file_href(path)
}

fn relative_local_href(path: &Path, report_dir: Option<&Path>) -> Option<String> {
    if path.is_absolute() {
        return None;
    }

    let report_dir = report_dir?;
    if report_dir.is_absolute() {
        return None;
    }

    let parent_hops = report_dir
        .components()
        .filter(|component| matches!(component, std::path::Component::Normal(_)))
        .count();
    let mut href = "../".repeat(parent_hops);
    href.push_str(&path.to_string_lossy().replace(' ', "%20"));
    Some(href)
}

fn local_file_href(path: &Path) -> Option<String> {
    let absolute_path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().ok()?.join(path)
    };

    Url::from_file_path(absolute_path)
        .ok()
        .map(|url| url.to_string())
}

fn link_html(href: &str, label: &str) -> String {
    format!(
        "<a href=\"{}\">{}</a>",
        escape_html(href),
        escape_html(label)
    )
}

const fn status_class(status: &PdfChangeStatus) -> &'static str {
    match status {
        PdfChangeStatus::New => "new",
        PdfChangeStatus::Changed => "changed",
        PdfChangeStatus::Unchanged => "unchanged",
        PdfChangeStatus::Removed => "removed",
        PdfChangeStatus::Failed => "failed",
    }
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[cfg(test)]
mod tests {
    use super::{
        CrawlConfig, PdfChangeStatus, PdfClassification, PdfReportItem, association_placement_data,
        build_report, david21_association_placements, david21_podium_associations, escape_html,
        extract_year, is_david21_text, is_http_url, is_pdf_url, link_html, local_path_href,
        manifest_scope, path_cell, pdf_filename, podium_association_codes, resolve_link,
        sanitize_filename, source_slug, status_class, summarize_david21, summarize_david21_focus,
    };
    use crate::sport_results::{
        EventInfo, IndividualResult, Rank, SportResultList, TeamMemberResult, TeamResult,
    };
    use chrono::Utc;
    use std::path::{Path, PathBuf};
    use url::Url;

    #[test]
    fn detects_pdf_urls_case_insensitively() {
        let url = Url::parse("https://example.org/files/Result.PDF").unwrap();

        assert!(is_pdf_url(&url));
    }

    #[test]
    fn identifies_david21_text() {
        assert!(is_david21_text(
            "Landesmeisterschaft\nDV-System DAVID21+\nErgebnisliste Einzel"
        ));
        assert!(!is_david21_text("Norddeutscher Schützenbund"));
    }

    #[test]
    fn creates_stable_safe_pdf_filenames() {
        let filename = pdf_filename(
            "https://example.org/a/b/Ergebnis Liste 1.10.10.pdf",
            "abcdef1234567890",
        )
        .unwrap();

        assert_eq!(filename, "abcdef123456-Ergebnis_Liste_1.10.10.pdf");
    }

    #[test]
    fn escapes_html_control_characters() {
        assert_eq!(
            escape_html("<a href=\"x\">A&B</a>"),
            "&lt;a href=&quot;x&quot;&gt;A&amp;B&lt;/a&gt;"
        );
    }

    #[test]
    fn creates_relative_links_for_local_report_paths() {
        let href = local_path_href(
            &PathBuf::from("data/downloads/result file.pdf"),
            Some(std::path::Path::new("reports")),
        )
        .unwrap();

        assert_eq!(href, "../data/downloads/result%20file.pdf");
    }

    #[test]
    fn resolves_ndsb_root_relative_download_links() {
        let root = Url::parse("https://www.ndsb-sh.de/sport/landesmeisterschaften").unwrap();
        let page = Url::parse("https://www.ndsb-sh.de/sport/landesmeisterschaften").unwrap();

        let url = resolve_link(&page, &root, "download_/sport2025/results.pdf").unwrap();

        assert_eq!(
            url.as_str(),
            "https://www.ndsb-sh.de/download_/sport2025/results.pdf"
        );
    }

    #[test]
    fn extracts_year_from_heading_text() {
        assert_eq!(extract_year(" 2025 "), Some("2025".to_string()));
        assert_eq!(extract_year("Landesmeisterschaft"), None);
    }

    #[test]
    fn podium_associations_only_include_ranks_one_to_three() {
        let event = EventInfo {
            name: "Event".to_string(),
            date: None,
            location: None,
            system: None,
            discipline_code: None,
            discipline: None,
            class_name: None,
        };
        let result_list = SportResultList {
            event: event.clone(),
            team_results: vec![
                TeamResult {
                    event: event.clone(),
                    rank: Some(3),
                    association: "OD".to_string(),
                    club: "SchV Reinfeld".to_string(),
                    total: 100.0,
                    members: vec![TeamMemberResult {
                        start_number: 1,
                        name: "A".to_string(),
                        total: 100.0,
                    }],
                },
                TeamResult {
                    event: event.clone(),
                    rank: Some(4),
                    association: "SE".to_string(),
                    club: "Other".to_string(),
                    total: 90.0,
                    members: Vec::new(),
                },
            ],
            individual_results: vec![
                IndividualResult {
                    event: event.clone(),
                    rank: Rank::Place(2),
                    start_number: 2,
                    name: "B".to_string(),
                    association: "OD".to_string(),
                    club: "SchV Trittau".to_string(),
                    series: Vec::new(),
                    total: 99.0,
                },
                IndividualResult {
                    event,
                    rank: Rank::Place(4),
                    start_number: 3,
                    name: "C".to_string(),
                    association: "OH".to_string(),
                    club: "Other".to_string(),
                    series: Vec::new(),
                    total: 80.0,
                },
            ],
            out_of_competition_team_results: Vec::new(),
            out_of_competition_individual_results: Vec::new(),
        };

        assert_eq!(david21_podium_associations(&result_list), vec!["OD"]);
        assert_eq!(david21_association_placements(&result_list).len(), 4);

        let focus = summarize_david21_focus(&result_list, "OD");
        assert_eq!(focus.podium_team_results, 1);
        assert_eq!(focus.podium_team_members, 1);
        assert_eq!(focus.podium_individual_results, 1);

        let summary = summarize_david21(&result_list, "OD");
        assert_eq!(
            association_placement_data(Some(&summary)),
            "OD:2 OD:3 OH:4 SE:4"
        );
    }

    #[test]
    fn sanitizes_filenames_by_replacing_special_characters() {
        assert_eq!(
            sanitize_filename("Ergebnis Liste 1.10.10.pdf"),
            "Ergebnis_Liste_1.10.10.pdf"
        );
        assert_eq!(sanitize_filename("test<>&\"'file.pdf"), "test_____file.pdf");
    }

    #[test]
    fn creates_lowercase_slug_from_source_name() {
        assert_eq!(
            source_slug("deutsche-meisterschaften"),
            "deutsche-meisterschaften"
        );
        assert_eq!(
            source_slug("Landesmeisterschaften"),
            "landesmeisterschaften"
        );
        assert_eq!(source_slug("  OD  "), "od");
        assert_eq!(source_slug("---"), "default");
    }

    #[test]
    fn builds_manifest_scope_from_source_and_year() {
        assert_eq!(manifest_scope("", None), "");
        assert_eq!(manifest_scope("", Some("2026")), "2026");
        assert_eq!(
            manifest_scope("landesmeisterschaften", None),
            "landesmeisterschaften"
        );
        assert_eq!(
            manifest_scope("landesmeisterschaften", Some("2026")),
            "landesmeisterschaften-2026"
        );
    }

    #[test]
    fn identifies_http_and_https_urls() {
        assert!(is_http_url(&Url::parse("https://example.org").unwrap()));
        assert!(is_http_url(&Url::parse("http://example.org").unwrap()));
        assert!(!is_http_url(&Url::parse("ftp://example.org").unwrap()));
    }

    #[test]
    fn counts_downloaded_and_classified_pdfs_in_report() {
        let config = CrawlConfig {
            source_url: "https://example.org".to_string(),
            source_name: "test".to_string(),
            state_dir: PathBuf::from(".pdf-explorer"),
            download_dir: PathBuf::from("data/downloads"),
            manual_review_dir: PathBuf::from("data/manual-review"),
            report_path: PathBuf::from("reports/report.json"),
            html_report_path: PathBuf::from("reports/report.html"),
            focus: "Stormarn".to_string(),
            focus_association_code: "OD".to_string(),
            year: None,
            extra_pdf_urls: Vec::new(),
            max_depth: 1,
            max_pages: 25,
            min_text_chars: 80,
        };
        let item = PdfReportItem {
            url: "https://example.org/file.pdf".to_string(),
            status: PdfChangeStatus::New,
            classification: PdfClassification::David21,
            local_path: Some(PathBuf::from("data/downloads/file.pdf")),
            manual_review_path: None,
            etag: None,
            last_modified: None,
            sha256: None,
            file_size_bytes: None,
            text_char_count: None,
            needs_ocr: None,
            david21_summary: None,
            error: None,
        };
        let report = build_report(&config, 1, vec![], vec![item], Utc::now());

        assert_eq!(report.discovered_pdf_count, 1);
        assert_eq!(report.downloaded_count, 1);
        assert_eq!(report.auto_processed_count, 1);
        assert_eq!(report.manual_review_count, 0);
        assert_eq!(report.failed_count, 0);
    }

    #[test]
    fn maps_change_status_to_css_class() {
        assert_eq!(status_class(&PdfChangeStatus::New), "new");
        assert_eq!(status_class(&PdfChangeStatus::Changed), "changed");
        assert_eq!(status_class(&PdfChangeStatus::Unchanged), "unchanged");
        assert_eq!(status_class(&PdfChangeStatus::Removed), "removed");
        assert_eq!(status_class(&PdfChangeStatus::Failed), "failed");
    }

    #[test]
    fn renders_html_link() {
        assert_eq!(
            link_html("https://example.org/file.pdf", "file.pdf"),
            "<a href=\"https://example.org/file.pdf\">file.pdf</a>"
        );
    }

    #[test]
    fn renders_path_cell_with_local_file_link() {
        let path = PathBuf::from("data/downloads/result.pdf");
        let cell = path_cell(Some(&path), Some(Path::new("reports")));
        assert!(cell.contains("data/downloads/result.pdf"));
        assert!(cell.contains("href="));
    }

    #[test]
    fn renders_path_cell_with_file_href_for_absolute_paths() {
        let path = PathBuf::from("/absolute/path/result.pdf");
        let cell = path_cell(Some(&path), Some(Path::new("reports")));
        assert!(cell.contains("/absolute/path/result.pdf"));
        assert!(cell.contains("href="));
    }

    #[test]
    fn extracts_podium_association_codes_from_summary() {
        let event = EventInfo {
            name: "Event".to_string(),
            date: None,
            location: None,
            system: None,
            discipline_code: None,
            discipline: None,
            class_name: None,
        };
        let result_list = SportResultList {
            event: event.clone(),
            team_results: vec![TeamResult {
                event: event.clone(),
                rank: Some(1),
                association: "OD".to_string(),
                club: "SchV Trittau".to_string(),
                total: 100.0,
                members: Vec::new(),
            }],
            individual_results: vec![IndividualResult {
                event,
                rank: Rank::Place(2),
                start_number: 1,
                name: "Shooter".to_string(),
                association: "OD".to_string(),
                club: "SchV Trittau".to_string(),
                series: Vec::new(),
                total: 99.0,
            }],
            out_of_competition_team_results: Vec::new(),
            out_of_competition_individual_results: Vec::new(),
        };
        let summary = summarize_david21(&result_list, "OD");
        assert_eq!(podium_association_codes(Some(&summary)), vec!["OD"]);
    }

    #[test]
    fn formats_association_placement_data() {
        let event = EventInfo {
            name: "Event".to_string(),
            date: None,
            location: None,
            system: None,
            discipline_code: None,
            discipline: None,
            class_name: None,
        };
        let result_list = SportResultList {
            event: event.clone(),
            team_results: vec![TeamResult {
                event: event.clone(),
                rank: Some(2),
                association: "OD".to_string(),
                club: "SchV Trittau".to_string(),
                total: 100.0,
                members: Vec::new(),
            }],
            individual_results: vec![IndividualResult {
                event,
                rank: Rank::Place(3),
                start_number: 1,
                name: "Shooter".to_string(),
                association: "OH".to_string(),
                club: "Other".to_string(),
                series: Vec::new(),
                total: 99.0,
            }],
            out_of_competition_team_results: Vec::new(),
            out_of_competition_individual_results: Vec::new(),
        };
        let summary = summarize_david21(&result_list, "OD");
        assert_eq!(association_placement_data(Some(&summary)), "OD:2 OH:3");
    }
}
