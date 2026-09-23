use std::sync::LazyLock;

use scraper::{ElementRef, Html, Selector};
use url::Url;

use crate::srcset::parse_srcset;
use crate::url_compat::same_host;
use crate::{Headings, OpenGraph};

macro_rules! selector {
    ($value:literal) => {
        LazyLock::new(|| Selector::parse($value).expect("constant selector must be valid"))
    };
}

static BASE_SELECTOR: LazyLock<Selector> = selector!("base[href]");
static TITLE_SELECTOR: LazyLock<Selector> = selector!("title");
static H1_SELECTOR: LazyLock<Selector> = selector!("h1");
static META_SELECTOR: LazyLock<Selector> = selector!("meta");
static LINK_SELECTOR: LazyLock<Selector> =
    selector!("a, iframe, video, source, audio, embed, object");
static IMAGE_SELECTOR: LazyLock<Selector> = selector!("img");
static SOURCE_SRCSET_SELECTOR: LazyLock<Selector> = selector!("source[srcset]");

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LinkElement {
    Anchor,
    Iframe,
    Video,
    Source,
    Audio,
    Embed,
    Object,
}

impl LinkElement {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Anchor => "a",
            Self::Iframe => "iframe",
            Self::Video => "video",
            Self::Source => "source",
            Self::Audio => "audio",
            Self::Embed => "embed",
            Self::Object => "object",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ParsedLink {
    pub(crate) element: LinkElement,
    pub(crate) url: Url,
    pub(crate) original_url: String,
    pub(crate) text: String,
    pub(crate) is_external: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ParsedImage {
    pub(crate) src: Url,
    pub(crate) alt: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ImageReferenceElement {
    Image,
    Source,
    Meta,
}

impl ImageReferenceElement {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Image => "img",
            Self::Source => "source",
            Self::Meta => "meta",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ImageReferenceAttribute {
    Src,
    Srcset,
    Content,
}

impl ImageReferenceAttribute {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Src => "src",
            Self::Srcset => "srcset",
            Self::Content => "content",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ImageReference {
    pub(crate) url: Option<Url>,
    pub(crate) original_url: String,
    pub(crate) element: ImageReferenceElement,
    pub(crate) attribute: ImageReferenceAttribute,
    pub(crate) descriptor: Option<String>,
    pub(crate) alt: Option<String>,
}

impl ImageReference {
    fn new(
        original_url: &str,
        base_url: &Url,
        element: ImageReferenceElement,
        attribute: ImageReferenceAttribute,
        alt: Option<String>,
        descriptor: Option<String>,
    ) -> Self {
        Self {
            url: resolve_url(original_url, base_url),
            original_url: original_url.to_owned(),
            element,
            attribute,
            descriptor,
            alt,
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ParsedPage {
    pub(crate) title: Option<String>,
    pub(crate) description: Option<String>,
    pub(crate) headings: Headings,
    pub(crate) links: Vec<ParsedLink>,
    pub(crate) images: Vec<ParsedImage>,
    pub(crate) image_alt_texts: Vec<Option<String>>,
    pub(crate) image_references: Vec<ImageReference>,
    pub(crate) open_graph: OpenGraph,
}

pub(crate) fn parse_html(input: &[u8], page_url: &Url) -> ParsedPage {
    let source = String::from_utf8_lossy(input);
    let document = Html::parse_document(&source);
    let document_base_url = document_base_url(&document, page_url);
    let metadata = extract_metadata(&document, &document_base_url);
    let (images, image_alt_texts) = extract_images(&document, &document_base_url);

    ParsedPage {
        title: first_element_text(&document, &TITLE_SELECTOR),
        description: metadata.description,
        headings: Headings {
            h1: element_texts(&document, &H1_SELECTOR),
        },
        links: extract_links(&document, &document_base_url, page_url),
        images,
        image_alt_texts,
        image_references: extract_image_references(
            &document,
            &document_base_url,
            &metadata.open_graph_images,
        ),
        open_graph: metadata.open_graph,
    }
}

pub(crate) fn is_html_content_type(content_type: &str) -> bool {
    if content_type.is_empty() {
        return true;
    }

    let normalized = content_type.to_ascii_lowercase();

    normalized.contains("text/html") || normalized.contains("application/xhtml")
}

fn document_base_url(document: &Html, page_url: &Url) -> Url {
    document
        .select(&BASE_SELECTOR)
        .next()
        .and_then(|element| element.attr("href"))
        .and_then(|raw| resolve_url(raw, page_url))
        .unwrap_or_else(|| page_url.clone())
}

fn first_element_text(document: &Html, selector: &Selector) -> Option<String> {
    document
        .select(selector)
        .next()
        .map(|element| text_content(element).trim().to_owned())
}

fn element_texts(document: &Html, selector: &Selector) -> Vec<String> {
    document
        .select(selector)
        .map(|element| text_content(element).trim().to_owned())
        .collect()
}

fn extract_links(document: &Html, base_url: &Url, page_url: &Url) -> Vec<ParsedLink> {
    document
        .select(&LINK_SELECTOR)
        .filter_map(|element| {
            let (link_element, attribute) = match element.value().name() {
                "a" => (LinkElement::Anchor, "href"),
                "iframe" => (LinkElement::Iframe, "src"),
                "video" => (LinkElement::Video, "src"),
                "source" => (LinkElement::Source, "src"),
                "audio" => (LinkElement::Audio, "src"),
                "embed" => (LinkElement::Embed, "src"),
                "object" => (LinkElement::Object, "data"),
                _ => return None,
            };
            let original_url = element.attr(attribute)?;
            let url = resolve_url(original_url, base_url)?;

            Some(ParsedLink {
                element: link_element,
                is_external: !same_host(&url, page_url),
                url,
                original_url: original_url.to_owned(),
                text: link_text(element, link_element),
            })
        })
        .collect()
}

fn link_text(element: ElementRef<'_>, kind: LinkElement) -> String {
    match kind {
        LinkElement::Anchor => text_content(element).trim().to_owned(),
        LinkElement::Iframe => format!("[iframe] {}", element.attr("title").unwrap_or_default()),
        LinkElement::Source => element.attr("type").map_or_else(
            || "[source]".to_owned(),
            |media_type| {
                if media_type.is_empty() {
                    "[source]".to_owned()
                } else {
                    format!("[source type={media_type}]")
                }
            },
        ),
        LinkElement::Video => "[video]".to_owned(),
        LinkElement::Audio => "[audio]".to_owned(),
        LinkElement::Embed => "[embed]".to_owned(),
        LinkElement::Object => "[object]".to_owned(),
    }
}

fn extract_images(document: &Html, base_url: &Url) -> (Vec<ParsedImage>, Vec<Option<String>>) {
    let mut images = Vec::new();
    let mut alt_texts = Vec::new();

    for element in document.select(&IMAGE_SELECTOR) {
        let alt = element.attr("alt").map(str::to_owned);

        alt_texts.push(alt.clone());

        if let Some(src) = element
            .attr("src")
            .and_then(|raw| resolve_url(raw, base_url))
        {
            images.push(ParsedImage { src, alt });
        }
    }

    (images, alt_texts)
}

fn extract_image_references(
    document: &Html,
    base_url: &Url,
    open_graph_images: &[String],
) -> Vec<ImageReference> {
    let mut references = Vec::new();

    for image in document.select(&IMAGE_SELECTOR) {
        let alt = image.attr("alt").map(str::to_owned);

        if let Some(src) = image.attr("src") {
            references.push(ImageReference::new(
                src,
                base_url,
                ImageReferenceElement::Image,
                ImageReferenceAttribute::Src,
                alt.clone(),
                None,
            ));
        }

        if let Some(srcset) = image.attr("srcset") {
            references.extend(parse_srcset(srcset).into_iter().map(|candidate| {
                ImageReference::new(
                    &candidate.url,
                    base_url,
                    ImageReferenceElement::Image,
                    ImageReferenceAttribute::Srcset,
                    alt.clone(),
                    candidate.descriptor,
                )
            }));
        }
    }

    for source in document.select(&SOURCE_SRCSET_SELECTOR) {
        let Some(picture) = source
            .ancestors()
            .filter_map(ElementRef::wrap)
            .find(|ancestor| ancestor.value().name() == "picture")
        else {
            continue;
        };

        let alt = picture
            .select(&IMAGE_SELECTOR)
            .next()
            .and_then(|image| image.attr("alt"))
            .map(str::to_owned);
        let srcset = source
            .attr("srcset")
            .expect("selector requires a srcset attribute");

        references.extend(parse_srcset(srcset).into_iter().map(|candidate| {
            ImageReference::new(
                &candidate.url,
                base_url,
                ImageReferenceElement::Source,
                ImageReferenceAttribute::Srcset,
                alt.clone(),
                candidate.descriptor,
            )
        }));
    }

    for content in open_graph_images {
        references.push(ImageReference::new(
            content,
            base_url,
            ImageReferenceElement::Meta,
            ImageReferenceAttribute::Content,
            None,
            None,
        ));
    }

    references
}

#[derive(Default)]
struct Metadata {
    description: Option<String>,
    open_graph: OpenGraph,
    open_graph_images: Vec<String>,
}

fn extract_metadata(document: &Html, base_url: &Url) -> Metadata {
    let mut metadata = Metadata::default();

    for element in document.select(&META_SELECTOR) {
        let Some(content) = element.attr("content") else {
            continue;
        };

        if metadata.description.is_none()
            && element
                .attr("name")
                .is_some_and(|name| name.eq_ignore_ascii_case("description"))
        {
            metadata.description = Some(content.to_owned());
        }

        let Some(property) = element.attr("property") else {
            continue;
        };

        match property.to_ascii_lowercase().as_str() {
            "og:image" => {
                metadata.open_graph_images.push(content.to_owned());

                if metadata.open_graph.image.is_none() {
                    metadata.open_graph.image =
                        resolve_url(content, base_url).map(|url| url.to_string());
                }
            }
            "og:url" => {
                if metadata.open_graph.url.is_none() {
                    metadata.open_graph.url =
                        resolve_url(content, base_url).map(|url| url.to_string());
                }
            }
            "og:title" => {
                metadata
                    .open_graph
                    .title
                    .get_or_insert_with(|| content.to_owned());
            }
            "og:description" => {
                metadata
                    .open_graph
                    .description
                    .get_or_insert_with(|| content.to_owned());
            }
            "og:type" => {
                metadata
                    .open_graph
                    .object_type
                    .get_or_insert_with(|| content.to_owned());
            }
            "og:site_name" => {
                metadata
                    .open_graph
                    .site_name
                    .get_or_insert_with(|| content.to_owned());
            }
            "og:locale" => {
                metadata
                    .open_graph
                    .locale
                    .get_or_insert_with(|| content.to_owned());
            }
            _ => {}
        }
    }

    metadata
}

fn resolve_url(value: &str, base_url: &Url) -> Option<Url> {
    base_url.join(value).ok()
}

fn text_content(element: ElementRef<'_>) -> String {
    element.text().collect()
}

#[cfg(test)]
mod tests {
    use super::{
        ImageReferenceAttribute, ImageReferenceElement, LinkElement, is_html_content_type,
        parse_html,
    };
    use url::Url;

    const BASE_URL: &str = "https://example.com/articles/page.html";

    fn parse_fixture(name: &str) -> super::ParsedPage {
        let input = match name {
            "complete" => include_bytes!("../tests/fixtures/html/page-complete.html").as_slice(),
            "minimal" => include_bytes!("../tests/fixtures/html/page-minimal.html").as_slice(),
            _ => unreachable!("unknown fixture"),
        };

        parse_html(input, &Url::parse(BASE_URL).unwrap())
    }

    #[test]
    fn complete_fixture_preserves_metadata_and_document_order() {
        let page = parse_fixture("complete");

        assert_eq!(page.title.as_deref(), Some("Example Page"));
        assert_eq!(
            page.description.as_deref(),
            Some("An example page used to test HTML extraction.")
        );
        assert_eq!(page.headings.h1, ["Main heading", "Secondary heading"]);
        assert_eq!(page.links.len(), 12);
        assert_eq!(
            page.links
                .iter()
                .map(|link| link.url.as_str())
                .collect::<Vec<_>>(),
            [
                "https://example.com/about?source=page#team",
                "https://example.com:8443/status",
                "https://other.example/contact",
                "https://example.com/articles/page.html#details",
                "mailto:hello@example.com",
                "https://example.com/frame.html",
                "https://example.com/media/intro.mp4",
                "https://example.com/media/intro.webm",
                "https://example.com/media/intro.ogv",
                "https://example.com/media/intro.mp3",
                "https://example.com/media/document.pdf",
                "https://example.com/media/chart.svg",
            ]
        );
        assert_eq!(page.links[0].element, LinkElement::Anchor);
        assert_eq!(page.links[0].original_url, "../about?source=page#team");
        assert_eq!(page.links[0].text, "About us");
        assert!(!page.links[0].is_external);
        assert!(page.links[1].is_external);
        assert_eq!(page.links[5].text, "[iframe] Example frame");
        assert_eq!(page.links[7].text, "[source type=video/webm]");
        assert_eq!(page.links[8].text, "[source]");

        assert_eq!(page.images.len(), 2);
        assert_eq!(
            page.images[0].src.as_str(),
            "https://example.com/images/hero.jpg"
        );
        assert_eq!(page.images[0].alt.as_deref(), Some("Example hero"));
        assert_eq!(page.images[1].alt, None);
        assert_eq!(
            page.image_alt_texts,
            [
                Some("Example hero".to_owned()),
                None,
                Some("Invalid image URL".to_owned())
            ]
        );

        assert_eq!(page.image_references.len(), 9);
        assert_eq!(
            page.image_references
                .iter()
                .map(|reference| reference.url.as_ref().map(Url::as_str))
                .collect::<Vec<_>>(),
            [
                Some("https://example.com/images/hero.jpg"),
                Some("https://example.com/images/hero@2x.jpg"),
                Some("https://cdn.example.com/hero@3x.jpg"),
                Some("https://cdn.example.com/logo.svg"),
                None,
                Some("https://example.com/images/hero-small.webp"),
                Some("https://example.com/images/hero-large.webp"),
                Some("https://example.com/images/social.png"),
                Some("https://example.com/images/social-wide.png"),
            ]
        );
        assert_eq!(
            page.image_references[0].element,
            ImageReferenceElement::Image
        );
        assert_eq!(
            page.image_references[0].attribute,
            ImageReferenceAttribute::Src
        );
        assert_eq!(page.image_references[0].original_url, "/images/hero.jpg");
        assert_eq!(page.image_references[1].descriptor.as_deref(), Some("2x"));
        assert_eq!(
            page.image_references[5].element,
            ImageReferenceElement::Source
        );
        assert_eq!(
            page.image_references[5].attribute,
            ImageReferenceAttribute::Srcset
        );
        assert_eq!(
            page.image_references[5].alt.as_deref(),
            Some("Example hero")
        );
        assert_eq!(
            page.image_references[7].element,
            ImageReferenceElement::Meta
        );
        assert_eq!(
            page.image_references[7].attribute,
            ImageReferenceAttribute::Content
        );

        assert_eq!(
            page.open_graph.title.as_deref(),
            Some("Example Open Graph Title")
        );
        assert_eq!(
            page.open_graph.image.as_deref(),
            Some("https://example.com/images/social.png")
        );
        assert_eq!(page.open_graph.url.as_deref(), Some(BASE_URL));
        assert_eq!(page.open_graph.object_type.as_deref(), Some("website"));
        assert_eq!(page.open_graph.site_name.as_deref(), Some("Example Site"));
        assert_eq!(page.open_graph.locale.as_deref(), Some("en_US"));
    }

    #[test]
    fn minimal_fixture_has_initialized_empty_collections() {
        let page = parse_fixture("minimal");

        assert_eq!(page.title, None);
        assert_eq!(page.description, None);
        assert!(page.headings.h1.is_empty());
        assert!(page.links.is_empty());
        assert!(page.images.is_empty());
        assert!(page.image_alt_texts.is_empty());
        assert!(page.image_references.is_empty());
    }

    #[test]
    fn base_and_internationalized_host_rules_are_normalized() {
        let page_url = Url::parse(BASE_URL).unwrap();
        let cases = [
            (
                r#"<base target="_blank"><base href="https://cdn.example/assets/"><a href="guide.html">Guide</a>"#,
                "https://cdn.example/assets/guide.html",
            ),
            (
                r#"<base href="https://cdn.example/assets/"><base href="https://ignored.example/"><a href="guide.html">Guide</a>"#,
                "https://cdn.example/assets/guide.html",
            ),
            (
                r#"<base href="https://[invalid"><base href="https://ignored.example/"><a href="guide.html">Guide</a>"#,
                "https://example.com/articles/guide.html",
            ),
        ];
        for (source, expected) in cases {
            let page = parse_html(source.as_bytes(), &page_url);
            assert_eq!(page.links[0].url.as_str(), expected);
        }

        let base = Url::parse("https://xn--bcher-kva.example/start").unwrap();
        let page = parse_html(
            r#"<a href="HTTPS://BÜCHER.example:443/next">next</a>"#.as_bytes(),
            &base,
        );

        assert_eq!(
            page.links[0].url.as_str(),
            "https://xn--bcher-kva.example/next"
        );
        assert!(!page.links[0].is_external);
    }

    #[test]
    fn malformed_utf8_is_recovered_by_the_html_parser() {
        let page = parse_html(
            b"<title>bad \xff title</title><h1>still parsed</h1>",
            &Url::parse(BASE_URL).unwrap(),
        );

        assert_eq!(page.title.as_deref(), Some("bad � title"));
        assert_eq!(page.headings.h1, ["still parsed"]);
    }

    #[test]
    fn missing_attributes_picture_fallback_and_external_hosts_are_handled() {
        let source = r#"
            <img alt="missing source">
            <picture><div><source srcset="/one.webp 1x"></div></picture>
            <source srcset="/outside.webp 1x">
            <meta property="og:image">
            <iframe src="/frame"></iframe>
            <a href="https://EXAMPLE.com:443/same-default-port">same</a>
            <a href="http://example.com/same-host-other-scheme">same host</a>
            <a href="https://example.com:8443/different-port">port</a>
            <a href="https://other.example/different-host">host</a>
        "#;
        let page = parse_html(source.as_bytes(), &Url::parse(BASE_URL).unwrap());

        assert!(page.images.is_empty());
        assert_eq!(page.image_alt_texts, [Some("missing source".to_owned())]);
        assert_eq!(page.image_references.len(), 1);
        assert_eq!(
            page.image_references[0].element,
            ImageReferenceElement::Source
        );
        assert_eq!(page.image_references[0].alt, None);
        assert_eq!(page.links[0].text, "[iframe] ");
        assert_eq!(
            page.links[1..]
                .iter()
                .map(|link| link.is_external)
                .collect::<Vec<_>>(),
            [false, false, true, true]
        );
    }

    #[test]
    fn html_content_types_are_recognized() {
        assert!(is_html_content_type(""));
        assert!(is_html_content_type("Text/HTML; charset=utf-8"));
        assert!(is_html_content_type("application/xhtml+xml"));
        assert!(!is_html_content_type("application/json"));
    }
}
