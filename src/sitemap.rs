use std::collections::{HashSet, VecDeque};
use std::io::{Cursor, Read};

use flate2::read::GzDecoder;
use quick_xml::escape::resolve_xml_entity;
use quick_xml::events::Event;
use quick_xml::reader::Reader;
use thiserror::Error;
use url::Url;

use crate::url_compat::normalize_url;

const MAX_XML_DEPTH: usize = 32;
const MAX_ENTRIES: usize = 50_000;
pub(crate) const MAX_COMPRESSED_BYTES: usize = 51 * 1024 * 1024;
const MAX_DECOMPRESSED_BYTES: usize = 50 * 1024 * 1024;

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub(crate) enum SitemapError {
    #[error("sitemap: doctype is not allowed")]
    Doctype,

    #[error("sitemap: malformed XML")]
    Malformed,

    #[error("sitemap: unsupported root element {0:?}")]
    UnsupportedRoot(String),

    #[error("sitemap: XML nesting exceeds {MAX_XML_DEPTH} elements")]
    TooDeep,

    #[error("sitemap: document exceeds {MAX_ENTRIES} entries")]
    TooManyEntries,

    #[error("sitemap: compressed document exceeds {} MiB", MAX_COMPRESSED_BYTES / 1024 / 1024)]
    CompressedSize,

    #[error("sitemap: decompressed document exceeds {} MiB", MAX_DECOMPRESSED_BYTES / 1024 / 1024)]
    DecompressedSize,

    #[error("sitemap: invalid gzip document: {0}")]
    InvalidGzip(std::io::ErrorKind),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SitemapKind {
    UrlSet,
    Index,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ParsedSitemap {
    pub(crate) kind: SitemapKind,
    pub(crate) urls: Vec<Url>,
}

/// Applies compressed and decompressed body limits before parsing XML events.
/// Gzip is detected by its magic bytes rather than a filename suffix.
pub(crate) fn parse_sitemap_document(document: &[u8]) -> Result<ParsedSitemap, SitemapError> {
    if document.len() > MAX_COMPRESSED_BYTES {
        return Err(SitemapError::CompressedSize);
    }

    if document.starts_with(&[0x1f, 0x8b]) {
        let mut decoder = GzDecoder::new(document);
        let decoded = read_decompressed(&mut decoder)?;

        parse_sitemap_xml(&decoded)
    } else {
        if document.len() > MAX_DECOMPRESSED_BYTES {
            return Err(SitemapError::DecompressedSize);
        }

        parse_sitemap_xml(document)
    }
}

fn read_decompressed(reader: &mut impl Read) -> Result<Vec<u8>, SitemapError> {
    let mut decoded = Vec::new();

    reader
        .take((MAX_DECOMPRESSED_BYTES + 1) as u64)
        .read_to_end(&mut decoded)
        .map_err(|error| SitemapError::InvalidGzip(error.kind()))?;

    if decoded.len() > MAX_DECOMPRESSED_BYTES {
        return Err(SitemapError::DecompressedSize);
    }

    Ok(decoded)
}

fn parse_sitemap_xml(document: &[u8]) -> Result<ParsedSitemap, SitemapError> {
    let mut reader = Reader::from_reader(Cursor::new(document));

    reader.config_mut().expand_empty_elements = true;
    reader.config_mut().check_comments = true;

    let mut buffer = Vec::new();
    let mut kind = None;
    let mut root_ended = false;
    let mut depth = 0usize;
    let mut entry_depth = None;
    let mut entry_count = 0usize;
    let mut entry_location = String::new();
    let mut location_depth = None;
    let mut location_text = String::new();
    let mut location_invalid = false;
    let mut urls = Vec::new();

    loop {
        let event = reader
            .read_event_into(&mut buffer)
            .map_err(|_| SitemapError::Malformed)?;

        match event {
            Event::Start(element) => {
                if root_ended {
                    return Err(SitemapError::Malformed);
                }

                depth += 1;

                if depth > MAX_XML_DEPTH {
                    return Err(SitemapError::TooDeep);
                }

                let local_name = element.local_name();
                let local_name = local_name.as_ref();

                if kind.is_none() {
                    kind = Some(match local_name {
                        "urlset" => SitemapKind::UrlSet,
                        "sitemapindex" => SitemapKind::Index,
                        other => return Err(SitemapError::UnsupportedRoot(other.to_owned())),
                    });
                } else {
                    let entry_name = match kind {
                        Some(SitemapKind::UrlSet) => "url",
                        Some(SitemapKind::Index) => "sitemap",
                        None => unreachable!("sitemap kind was checked above"),
                    };

                    if depth == 2 && local_name == entry_name {
                        entry_count += 1;
                        if entry_count > MAX_ENTRIES {
                            return Err(SitemapError::TooManyEntries);
                        }
                        entry_depth = Some(depth);
                        entry_location.clear();
                    } else if entry_depth.is_some_and(|entry| depth == entry + 1)
                        && local_name == "loc"
                        && location_depth.is_none()
                    {
                        location_depth = Some(depth);
                        location_invalid = false;
                        location_text.clear();
                    } else if location_depth.is_some_and(|location| depth > location) {
                        location_invalid = true;
                    }
                }
            }
            Event::Text(text) => {
                append_text(
                    text.xml10_content().as_ref(),
                    kind.is_some(),
                    root_ended,
                    location_depth,
                    location_invalid,
                    &mut location_text,
                )?;
            }
            Event::CData(text) => {
                append_text(
                    text.xml10_content().as_ref(),
                    kind.is_some(),
                    root_ended,
                    location_depth,
                    location_invalid,
                    &mut location_text,
                )?;
            }
            Event::GeneralRef(reference) => {
                let value = if let Some(character) = reference
                    .resolve_char_ref()
                    .map_err(|_| SitemapError::Malformed)?
                {
                    character.to_string()
                } else {
                    resolve_xml_entity(reference.as_ref())
                        .ok_or(SitemapError::Malformed)?
                        .to_owned()
                };

                append_text(
                    &value,
                    kind.is_some(),
                    root_ended,
                    location_depth,
                    location_invalid,
                    &mut location_text,
                )?;
            }
            Event::End(_) => {
                if depth == 0 {
                    return Err(SitemapError::Malformed);
                }

                if location_depth == Some(depth) {
                    if !location_invalid && entry_location.is_empty() {
                        entry_location = location_text.trim().to_owned();
                    }
                    location_depth = None;
                    location_text.clear();
                }

                if entry_depth == Some(depth) {
                    if let Some(url) = parse_http_url(&entry_location) {
                        urls.push(url);
                    }
                    entry_depth = None;
                    entry_location.clear();
                }

                if depth == 1 {
                    root_ended = true;
                }

                depth -= 1;
            }
            Event::DocType(_) => return Err(SitemapError::Doctype),
            Event::Eof => break,
            Event::Comment(_) | Event::Decl(_) | Event::PI(_) => {}
            Event::Empty(_) => return Err(SitemapError::Malformed),
        }

        buffer.clear();
    }

    let Some(kind) = kind else {
        return Err(SitemapError::Malformed);
    };

    if !root_ended || depth != 0 {
        return Err(SitemapError::Malformed);
    }

    Ok(ParsedSitemap { kind, urls })
}

fn append_text(
    value: &str,
    root_seen: bool,
    root_ended: bool,
    location_depth: Option<usize>,
    location_invalid: bool,
    location_text: &mut String,
) -> Result<(), SitemapError> {
    if !root_seen || root_ended {
        if !value.trim().is_empty() {
            return Err(SitemapError::Malformed);
        }
    } else if location_depth.is_some() && !location_invalid {
        location_text.push_str(value);
    }

    Ok(())
}

fn parse_http_url(value: &str) -> Option<Url> {
    let candidate = Url::parse(value).ok()?;

    if !matches!(candidate.scheme(), "http" | "https") || candidate.host().is_none() {
        return None;
    }

    Some(normalize_url(&candidate, true))
}

#[derive(Clone, Debug)]
pub(crate) struct SitemapFrontier {
    queue: VecDeque<Url>,
    seen: HashSet<Url>,
    max_documents: usize,
}

impl SitemapFrontier {
    pub(crate) fn new(initial: &[Url], max_documents: usize) -> Self {
        let mut frontier = Self {
            queue: VecDeque::new(),
            seen: HashSet::new(),
            max_documents,
        };
        frontier.enqueue(initial);

        frontier
    }

    pub(crate) fn next_document(&mut self) -> Option<Url> {
        self.queue.pop_front()
    }

    pub(crate) fn extend_from(&mut self, parsed: &ParsedSitemap) {
        if parsed.kind == SitemapKind::Index {
            self.enqueue(&parsed.urls);
        }
    }

    fn enqueue(&mut self, candidates: &[Url]) {
        for candidate in candidates {
            if self.seen.len() >= self.max_documents {
                break;
            }

            if !matches!(candidate.scheme(), "http" | "https") || candidate.host().is_none() {
                continue;
            }

            let normalized = normalize_url(candidate, true);

            if self.seen.insert(normalized.clone()) {
                self.queue.push_back(normalized);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::{self, Read, Write};

    use flate2::Compression;
    use flate2::write::GzEncoder;
    use url::Url;

    use super::{
        MAX_COMPRESSED_BYTES, MAX_DECOMPRESSED_BYTES, MAX_ENTRIES, MAX_XML_DEPTH, ParsedSitemap,
        SitemapError, SitemapFrontier, SitemapKind, parse_sitemap_document,
    };

    #[test]
    fn urlset_preserves_order_entities_fragments_and_international_urls() {
        let document = r#"<?xml version="1.0" encoding="UTF-8"?>
            <urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
              <url><loc>https://example.com/products?one=1&amp;two=2</loc></url>
              <url><loc>ftp://example.com/ignored</loc></url>
              <url><loc>/relative</loc></url>
              <url><loc>https://example.com/fragment#details</loc></url>
              <url><loc>https://例え.テスト/地図</loc></url>
            </urlset>"#;
        let parsed = parse_sitemap_document(document.as_bytes()).unwrap();

        assert_eq!(parsed.kind, SitemapKind::UrlSet);
        assert_eq!(
            parsed.urls.iter().map(Url::as_str).collect::<Vec<_>>(),
            [
                "https://example.com/products?one=1&two=2",
                "https://example.com/fragment#details",
                "https://xn--r8jz45g.xn--zckzah/%E5%9C%B0%E5%9B%B3",
            ]
        );
    }

    #[test]
    fn prefixed_index_is_namespace_tolerant() {
        let document = br#"<sm:sitemapindex xmlns:sm="http://www.sitemaps.org/schemas/sitemap/0.9">
            <sm:sitemap><sm:loc>https://sitemaps.example/one.xml</sm:loc></sm:sitemap>
            <sm:sitemap><sm:loc>https://sitemaps.example/two.xml.gz</sm:loc></sm:sitemap>
        </sm:sitemapindex>"#;
        let parsed = parse_sitemap_document(document).unwrap();

        assert_eq!(parsed.kind, SitemapKind::Index);
        assert_eq!(
            parsed.urls.iter().map(Url::as_str).collect::<Vec<_>>(),
            [
                "https://sitemaps.example/one.xml",
                "https://sitemaps.example/two.xml.gz"
            ]
        );
    }

    #[test]
    fn unsafe_and_malformed_documents_are_rejected() {
        let deep = format!(
            "<urlset>{}{}</urlset>",
            "<x>".repeat(MAX_XML_DEPTH),
            "</x>".repeat(MAX_XML_DEPTH)
        );
        let too_many = format!("<urlset>{}</urlset>", "<url/>".repeat(MAX_ENTRIES + 1));
        let cases = [
            ("", SitemapError::Malformed),
            (
                "<!DOCTYPE urlset [<!ENTITY x \"x\">]><urlset/>",
                SitemapError::Doctype,
            ),
            ("<urlset><url></urlset>", SitemapError::Malformed),
            (
                "<rss><channel/></rss>",
                SitemapError::UnsupportedRoot("rss".to_owned()),
            ),
            ("<urlset></urlset><urlset/>", SitemapError::Malformed),
            ("<urlset/>not-xml", SitemapError::Malformed),
            (&deep, SitemapError::TooDeep),
            (&too_many, SitemapError::TooManyEntries),
        ];

        for (document, expected) in cases {
            assert_eq!(parse_sitemap_document(document.as_bytes()), Err(expected));
        }

        assert_eq!(
            parse_sitemap_document(b"<urlset>\xff</urlset>"),
            Err(SitemapError::Malformed)
        );
    }

    #[test]
    fn nested_locations_are_ignored() {
        let document = br#"<urlset>
            <url><loc><nested>https://example.com/ignored</nested></loc></url>
            <url><loc>https://example.com/kept</loc></url>
        </urlset>"#;
        let parsed = parse_sitemap_document(document).unwrap();

        assert_eq!(parsed.urls[0].as_str(), "https://example.com/kept");
    }

    #[test]
    fn gzip_documents_and_decompression_bombs_obey_limits() {
        let document = b"<urlset><url><loc>https://example.com/page</loc></url></urlset>";
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());

        encoder.write_all(document).unwrap();

        let compressed = encoder.finish().unwrap();

        assert_eq!(
            parse_sitemap_document(&compressed).unwrap().urls[0].as_str(),
            "https://example.com/page"
        );

        let mut bomb = GzEncoder::new(Vec::new(), Compression::fast());

        io::copy(
            &mut io::repeat(b' ').take((MAX_DECOMPRESSED_BYTES + 1) as u64),
            &mut bomb,
        )
        .unwrap();

        let bomb = bomb.finish().unwrap();

        assert_eq!(
            parse_sitemap_document(&bomb),
            Err(SitemapError::DecompressedSize)
        );
        assert_eq!(
            parse_sitemap_document(&vec![0; MAX_DECOMPRESSED_BYTES + 1]),
            Err(SitemapError::DecompressedSize)
        );
        assert_eq!(
            parse_sitemap_document(&vec![0; MAX_COMPRESSED_BYTES + 1]),
            Err(SitemapError::CompressedSize)
        );
        assert!(matches!(
            parse_sitemap_document(&[0x1f, 0x8b, 0x00]),
            Err(SitemapError::InvalidGzip(_))
        ));
    }

    #[test]
    fn frontier_enqueues_indexes_breadth_first_with_deduplication_and_budget() {
        let root = Url::parse("https://example.com/root.xml").unwrap();
        let mut frontier = SitemapFrontier::new(std::slice::from_ref(&root), 4);

        assert_eq!(frontier.next_document(), Some(root));

        frontier.extend_from(&ParsedSitemap {
            kind: SitemapKind::Index,
            urls: [
                "https://example.com/one.xml",
                "https://example.com/two.xml",
                "ftp://example.com/ignored.xml",
            ]
            .map(|value| Url::parse(value).unwrap())
            .to_vec(),
        });

        assert_eq!(
            frontier.next_document().unwrap().as_str(),
            "https://example.com/one.xml"
        );

        frontier.extend_from(&ParsedSitemap {
            kind: SitemapKind::Index,
            urls: [
                "https://example.com/two.xml",
                "https://example.com/three.xml",
                "https://example.com/over-budget.xml",
            ]
            .map(|value| Url::parse(value).unwrap())
            .to_vec(),
        });

        assert_eq!(
            [frontier.next_document(), frontier.next_document()]
                .into_iter()
                .flatten()
                .map(|url| url.to_string())
                .collect::<Vec<_>>(),
            [
                "https://example.com/two.xml",
                "https://example.com/three.xml"
            ]
        );
    }
}
