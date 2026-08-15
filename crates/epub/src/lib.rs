pub mod package;
mod xhtml;

use package::{Item, ItemRef, Package};
use xhtml::HtmlInfo;

use chrono::{DateTime, Utc};
use iref::{IriRef, IriRefBuf, iri::Fragment};
use itertools::Itertools;
use rheo_core::{
    CastVertebra, FormatInitTemplate, FormatPlugin, PluginContext, PluginSection, Result,
    RheoError, Spine, SpineLayoutKind, eco_format, eco_vec,
};
use rheo_core::{DocumentTitle, EcoString, OutlineNode};
use serde::Deserialize;
use std::{
    fmt::Write as _,
    fs::File,
    io::{BufWriter, Write},
    num::NonZero,
    path::Path,
};
use tracing::info;
use typst::foundations::Bytes;
use uuid::Uuid;
use zip::write::SimpleFileOptions;

pub struct EpubPlugin;

impl FormatPlugin for EpubPlugin {
    fn name(&self) -> &'static str {
        "epub"
    }

    /// EPUB compiles each vertebra to XHTML, then packages them.
    fn extension(&self) -> &'static str {
        "xhtml"
    }

    fn spine_layout_kind(&self) -> SpineLayoutKind {
        SpineLayoutKind::OnePerVertebra
    }

    /// A marrow asset() has no page to sit beside inside the EPUB's own output
    /// directory — the container is the only place it can usefully land.
    fn embeds_bundle_assets(&self) -> bool {
        true
    }

    /// Set EPUB smart defaults: infer spine title from project name when no config exists.
    fn apply_defaults(&self, section: &mut PluginSection, project_name: &str) {
        let spine = section.spine.get_or_insert_with(|| Spine {
            title: None,
            ..Default::default()
        });
        if spine.title.is_none() {
            spine.title = Some(DocumentTitle::to_readable_name(project_name));
        }
    }

    fn format_init_template(&self) -> FormatInitTemplate {
        FormatInitTemplate {
            files: vec![],
            options_toml: Some(include_str!("templates/init/rheo_section.toml")),
        }
    }

    fn compile(&self, ctx: PluginContext<'_>, outputs: &[CastVertebra]) -> Result<()> {
        let epub_config = ctx.config.parse_extra::<EpubConfig>()?;
        let identifier = epub_config.identifier.clone();
        let date = epub_config.date_utc();

        // Extract language from the first output's HTML; default to "en".
        let language = if let Some(first_output) = outputs.first() {
            let html_string = String::from_utf8(first_output.bytes.to_vec()).map_err(|e| {
                RheoError::invalid_data(format!("EPUB HTML output is not valid UTF-8: {}", e))
            })?;
            extract_language(&html_string).unwrap_or_else(|| "en".to_string())
        } else {
            "en".to_string()
        };

        // Author comes straight from the first output's Typst-resolved
        // metadata (`CastVertebra::author`, harvested from `DocumentInfo`
        // during compilation) rather than an HTML `<meta name="author">` scrape.
        // EPUB's `dc:creator` (see `Package::creator`) is a single optional
        // string, while `DocumentInfo.author` can carry zero, one, or many
        // authors (e.g. `#set document(author: ("A", "B"))`) — multiple
        // authors are joined with `", "` rather than keeping only the first.
        let author = outputs
            .first()
            .filter(|o| !o.author.is_empty())
            .map(|o| o.author.join(", "));

        let mut items = outputs
            .iter()
            .map(|o| {
                let html_string = String::from_utf8(o.bytes.to_vec()).map_err(|e| {
                    RheoError::invalid_data(format!("EPUB HTML output is not valid UTF-8: {}", e))
                })?;
                EpubItem::from_html_string(o.output_path.clone(), html_string, o.contributed)
            })
            .collect::<Result<Vec<_>>>()?;

        let nav_xhtml = generate_nav_xhtml(&mut items, &language)?;
        let package_string = generate_package(
            &items,
            ctx.bundle_assets,
            ctx.spine.title.as_deref(),
            identifier.as_deref(),
            date.as_ref(),
            &language,
            author.as_deref(),
        )?;
        let epub_name = format!("{}.epub", ctx.project.name);
        let epub_path = ctx.output_dir.join(&epub_name);
        zip_epub(
            &epub_path,
            package_string,
            nav_xhtml,
            &items,
            ctx.bundle_assets,
        )?;

        info!(output = %epub_path.display(), "successfully generated EPUB");
        Ok(())
    }
}

const CONTAINER_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container" version="1.0">
	<rootfiles>
		<rootfile full-path="EPUB/package.opf" media-type="application/oebps-package+xml"/>
	</rootfiles>
</container>"#;

const NAV_HEADER: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml" xml:lang="{lang}" lang="{lang}" xmlns:epub="http://www.idpf.org/2007/ops">
	<head>
		<meta charset="utf-8"/>
		<title>Navigation</title>
	</head>
	<body>
        <nav epub:type="toc" id="toc">
"#;

const NAV_FOOTER: &str = r#"        </nav>
    </body>
</html>"#;

pub fn generate_nav_xhtml(items: &mut [EpubItem], language: &str) -> Result<String> {
    let mut buf = String::new();
    buf.push_str(&NAV_HEADER.replace("{lang}", language));

    fn stringify_outline(buf: &mut String, outline: &[OutlineNode<EcoString>], indent: usize) {
        let indent_str = " ".repeat(indent);
        writeln!(buf, "{indent_str}<ol>").unwrap();
        for node in outline {
            write!(buf, r#"{indent_str}<li>{}"#, node.entry).unwrap();
            if !node.children.is_empty() {
                buf.push('\n');
                stringify_outline(buf, &node.children, indent + 4);
                buf.push('\n');
                buf.push_str(&indent_str);
            }
            buf.push_str("</li>\n");
        }
        writeln!(buf, "{indent_str}</ol>").unwrap();
    }

    // Marrow-contributed pages (no matching spine vertebra) get no nav entry —
    // they stay in the EPUB container but are not part of the reading order.
    let mut nav_items: Vec<&mut EpubItem> = items.iter_mut().filter(|i| !i.contributed).collect();

    let outline = if nav_items.len() == 1 {
        nav_items[0]
            .outline
            .take()
            .ok_or_else(|| RheoError::invalid_data("EPUB item missing outline"))?
    } else {
        nav_items
            .iter_mut()
            .map(|item| {
                let entry = eco_format!(r#"<a href="{}">{}</a>"#, item.href, item.title());
                let children = item
                    .outline
                    .take()
                    .ok_or_else(|| RheoError::invalid_data("EPUB item missing outline"))?;
                Ok(OutlineNode {
                    entry,
                    level: NonZero::new(1).unwrap(),
                    children,
                })
            })
            .collect::<Result<Vec<_>>>()?
    };

    stringify_outline(&mut buf, &outline, 12);
    buf.push_str(NAV_FOOTER);
    Ok(buf)
}

const XHTML_MEDIATYPE: &str = "application/xhtml+xml";
const EPUB_MEDIATYPE: &str = "application/epub+zip";

fn date_format(dt: &DateTime<Utc>) -> EcoString {
    dt.format("%Y-%m-%dT%H:%M:%SZ").to_string().into()
}

/// Generates the package.opf XML string from the generated EPUB items.
///
/// `spine_title` takes the plain `Option<&str>` shape of `PluginContext::spine_title`
/// (the title is the only part of the old `SpineOptions` any plugin ever read;
/// the rest was a dead glob-pattern list removed with it) rather than a dedicated type.
pub fn generate_package(
    items: &[EpubItem],
    bundle_assets: &[(String, Bytes)],
    spine_title: Option<&str>,
    identifier: Option<&str>,
    date: Option<&DateTime<Utc>>,
    language: &str,
    author: Option<&str>,
) -> Result<String> {
    let title = spine_title
        .map(EcoString::from)
        .unwrap_or_else(|| items[0].title());

    const INTERNAL_UNIQUE_ID: &str = "uid";

    let identifier_content = match identifier {
        Some(id) => id.into(),
        None => eco_format!("urn:uuid:{}", Uuid::new_v4()),
    };

    let mut builder = Package::builder(title)
        .unique_identifier(INTERNAL_UNIQUE_ID)
        .lang(language)
        .identifier(INTERNAL_UNIQUE_ID, identifier_content)
        .language(language);

    if let Some(d) = date {
        builder = builder.date(date_format(d));
    }

    if let Some(a) = author {
        builder = builder.creator(a);
    }

    builder = builder
        .add_meta("dcterms:modified", date_format(&chrono::Utc::now()))
        .add_meta("ppub:valid", ".");

    builder = builder.add_item(Item {
        id: "nav".into(),
        href: IriRefBuf::new("nav.xhtml".to_string())
            .map_err(|e| RheoError::invalid_data(format!("invalid nav href: {}", e)))?,
        media_type: XHTML_MEDIATYPE.into(),
        properties: Some("nav".into()),
    });

    for item in items {
        let mut prop_list = eco_vec![];
        if item.info.scripted {
            prop_list.push("scripted");
        }
        if item.info.mathml {
            prop_list.push("mathml");
        }
        let properties = (!prop_list.is_empty()).then(|| prop_list.join(" ").into());

        let id = item.id();

        builder = builder.add_item(Item {
            id: id.clone(),
            href: item.href.clone(),
            media_type: XHTML_MEDIATYPE.into(),
            properties,
        });

        // Marrow-contributed pages stay in the manifest (and the physical
        // container) but are not part of the reading order.
        if !item.contributed {
            builder = builder.add_spine_ref(ItemRef {
                id: Some(eco_format!("{id}ref")),
                idref: id,
            });
        }
    }

    // Bundle assets (marrow `asset()` output) get a manifest item so the
    // physical bytes `zip_epub` writes are declared, but no spine ref — an
    // arbitrary data file has no reading-order position.
    for (path, _bytes) in bundle_assets {
        let media_type = mime_guess::from_path(path)
            .first_or_octet_stream()
            .to_string();
        let href = IriRefBuf::new(path.clone()).map_err(|e| {
            RheoError::invalid_data(format!("invalid href for EPUB asset {path}: {e}"))
        })?;
        builder = builder.add_item(Item {
            id: asset_item_id(path),
            href,
            media_type: media_type.into(),
            properties: None,
        });
    }

    let package = builder
        .build()
        .map_err(|e| RheoError::epub_generation(format!("Package validation failed: {}", e)))?;

    let xml = package
        .to_xml()
        .map_err(|e| RheoError::epub_generation(format!("Package XML generation failed: {}", e)))?;

    Ok(xml)
}

/// Combines all EPUB components into the final .epub (zip) file.
pub fn zip_epub(
    epub_path: &Path,
    package_string: String,
    nav_xhtml: String,
    items: &[EpubItem],
    bundle_assets: &[(String, Bytes)],
) -> Result<()> {
    let file = File::create(epub_path).map_err(|e| RheoError::io(e, "creating EPUB file"))?;
    let file = BufWriter::new(file);
    let mut zip = zip::ZipWriter::new(file);

    let opts = SimpleFileOptions::default();

    zip.start_file(
        "mimetype",
        opts.compression_method(zip::CompressionMethod::Stored),
    )
    .map_err(|e| RheoError::epub_generation(format!("failed to start mimetype file: {}", e)))?;
    zip.write_all(EPUB_MEDIATYPE.as_bytes())
        .map_err(|e| RheoError::io(e, "writing mimetype"))?;

    zip.add_directory("META-INF", opts).map_err(|e| {
        RheoError::epub_generation(format!("failed to add META-INF directory: {}", e))
    })?;
    zip.start_file("META-INF/container.xml", opts)
        .map_err(|e| RheoError::epub_generation(format!("failed to start container.xml: {}", e)))?;
    zip.write_all(CONTAINER_XML.as_bytes())
        .map_err(|e| RheoError::io(e, "writing container.xml"))?;

    zip.add_directory("EPUB", opts)
        .map_err(|e| RheoError::epub_generation(format!("failed to add EPUB directory: {}", e)))?;

    zip.start_file("EPUB/package.opf", opts)
        .map_err(|e| RheoError::epub_generation(format!("failed to start package.opf: {}", e)))?;
    zip.write_all(package_string.as_bytes())
        .map_err(|e| RheoError::io(e, "writing package.opf"))?;

    zip.start_file("EPUB/nav.xhtml", opts)
        .map_err(|e| RheoError::epub_generation(format!("failed to start nav.xhtml: {}", e)))?;
    zip.write_all(nav_xhtml.as_bytes())
        .map_err(|e| RheoError::io(e, "writing nav.xhtml"))?;

    for item in items {
        let filename = format!("EPUB/{}", item.href);
        zip.start_file(&filename, opts).map_err(|e| {
            RheoError::epub_generation(format!("failed to start file {}: {}", filename, e))
        })?;
        zip.write_all(item.xhtml.as_bytes())
            .map_err(|e| RheoError::io(e, format!("writing {}", filename)))?;
    }

    for (path, bytes) in bundle_assets {
        let filename = format!("EPUB/{}", path);
        zip.start_file(&filename, opts).map_err(|e| {
            RheoError::epub_generation(format!("failed to start file {}: {}", filename, e))
        })?;
        zip.write_all(bytes.as_slice())
            .map_err(|e| RheoError::io(e, format!("writing {}", filename)))?;
    }

    zip.finish()
        .map_err(|e| RheoError::epub_generation(format!("failed to finish EPUB zip: {}", e)))?;
    Ok(())
}

/// Typed view of the `[epub]` section's format-specific keys.
#[derive(Debug, Deserialize, Default)]
struct EpubConfig {
    /// Dublin Core identifier; auto-generated when absent.
    identifier: Option<String>,
    /// Publication date; falls back to the current time when absent.
    date: Option<toml::value::Datetime>,
}

impl EpubConfig {
    /// The configured publication date as a UTC timestamp, if both present and
    /// a valid RFC 3339 datetime.
    fn date_utc(&self) -> Option<DateTime<Utc>> {
        self.date.as_ref().and_then(|dt| {
            DateTime::parse_from_rfc3339(&dt.to_string())
                .ok()
                .map(|d| d.with_timezone(&Utc))
        })
    }
}

/// Extract the `lang` attribute from the `<html>` element.
///
/// Returns `None` if no lang attribute is found or if parsing fails.
fn extract_language(html_string: &str) -> Option<String> {
    use markup5ever_rcdom::NodeData;
    use rheo_core::util::html::HtmlDom;

    let dom = HtmlDom::parse(html_string).ok()?;

    // Walk DOM to find <html> element and extract lang attribute.
    fn find_html_lang(handle: &markup5ever_rcdom::Handle) -> Option<String> {
        match &handle.data {
            NodeData::Element { name, attrs, .. } => {
                if name.local.as_ref() == "html" {
                    for attr in attrs.borrow().iter() {
                        if attr.name.local.as_ref() == "lang" {
                            return Some(attr.value.to_string());
                        }
                    }
                }
                for child in handle.children.borrow().iter() {
                    if let Some(lang) = find_html_lang(child) {
                        return Some(lang);
                    }
                }
            }
            NodeData::Document => {
                for child in handle.children.borrow().iter() {
                    if let Some(lang) = find_html_lang(child) {
                        return Some(lang);
                    }
                }
            }
            _ => {}
        }
        None
    }

    find_html_lang(dom.document_root())
}

pub struct EpubItem {
    href: IriRefBuf,
    xhtml: String,
    info: HtmlInfo,
    outline: Option<Vec<OutlineNode<EcoString>>>,
    /// True for a marrow-contributed page (no matching spine vertebra). Kept in
    /// the manifest and physical container, but excluded from the spine
    /// reading order and the nav.xhtml table of contents.
    contributed: bool,
}

/// Manifest id for a bundle asset, distinct from any `EpubItem::id()` (which
/// derives from path segments minus extension) since two assets differing
/// only by extension (`hello.txt` / `hello.json`) would otherwise collide.
fn asset_item_id(path: &str) -> EcoString {
    format!("asset-{}", path.replace(['/', '.'], "-")).into()
}

fn text_to_id(s: &str) -> EcoString {
    s.chars()
        .map(|char| {
            if char.is_whitespace() {
                '-'
            } else {
                char.to_ascii_lowercase()
            }
        })
        .collect()
}

/// Recursively collect all text content from a DOM node.
fn text_content(handle: &markup5ever_rcdom::Handle) -> EcoString {
    use markup5ever_rcdom::NodeData;
    let mut out = EcoString::new();
    match &handle.data {
        NodeData::Text { contents } => out.push_str(&contents.borrow()),
        _ => {
            for child in handle.children.borrow().iter() {
                out.push_str(&text_content(child));
            }
        }
    }
    out
}

impl EpubItem {
    /// Build an EPUB item from HTML bytes produced by the bundle compiler.
    ///
    /// `output_path` is the filename from VirtualFs (e.g. `"chapter1.xhtml"`).
    /// The `.xhtml` extension is preserved as the EPUB item href. `contributed`
    /// is true for a marrow-contributed page with no matching spine vertebra.
    pub fn from_html_string(
        output_path: String,
        html_string: String,
        contributed: bool,
    ) -> Result<Self> {
        // Ensure the href ends in .xhtml regardless of what the compiler produced.
        use std::path::Path as StdPath;
        let xhtml_name = StdPath::new(&output_path)
            .with_extension("xhtml")
            .display()
            .to_string();
        let href = IriRefBuf::new(xhtml_name).map_err(|e| {
            RheoError::epub_generation(format!("invalid href for EPUB item: {}", e))
        })?;

        let (heading_ids, outline) = Self::outline_from_html(&html_string, &href)?;
        let (xhtml, info) = xhtml::html_to_portable_xhtml(&html_string, &heading_ids)?;

        Ok(EpubItem {
            href,
            xhtml,
            info,
            outline: Some(outline),
            contributed,
        })
    }

    /// Parse `html_string` to extract heading IDs and build the nav outline.
    ///
    /// Walks the DOM to find h2–h6 elements, derives an ID from their text content,
    /// and builds the nav tree. Returns `(heading_ids, outline)` in DOM order so that
    /// `html_to_portable_xhtml` can stamp the same IDs onto the XHTML output.
    fn outline_from_html(
        html_string: &str,
        href: &IriRef,
    ) -> Result<(Vec<EcoString>, Vec<OutlineNode<EcoString>>)> {
        use markup5ever_rcdom::{Handle, NodeData};
        use rheo_core::util::html::HtmlDom;

        let dom = HtmlDom::parse(html_string)?;

        // Recursive walk to collect (id, level, link_html) for h2–h6.
        fn collect_headings(
            handle: &Handle,
            href: &IriRef,
            out: &mut Vec<(EcoString, u8, EcoString)>,
        ) {
            match &handle.data {
                NodeData::Element { name, .. } => {
                    let tag = name.local.as_ref();
                    let mut chars = tag.chars();
                    if let Some('h') = chars.next()
                        && let Some(c) = chars.next()
                        && c.is_ascii_digit()
                        && c != '1'
                        && chars.next().is_none()
                    {
                        let level = c.to_digit(10).unwrap_or(2) as u8;
                        let text: EcoString = text_content(handle);
                        let id = text_to_id(&text);
                        let mut anchored = href.to_owned();
                        anchored.set_fragment(Fragment::new(id.as_bytes()).ok());
                        let link = eco_format!(r#"<a href="{anchored}">{text}</a>"#);
                        out.push((id, level, link));
                    }
                    for child in handle.children.borrow().iter() {
                        collect_headings(child, href, out);
                    }
                }
                NodeData::Document => {
                    for child in handle.children.borrow().iter() {
                        collect_headings(child, href, out);
                    }
                }
                _ => {}
            }
        }

        let mut raw: Vec<(EcoString, u8, EcoString)> = Vec::new();
        collect_headings(dom.document_root(), href, &mut raw);

        let heading_ids: Vec<EcoString> = raw.iter().map(|(id, _, _)| id.clone()).collect();
        let nodes: Vec<(EcoString, NonZero<usize>, bool)> = raw
            .into_iter()
            .map(|(_, level, link)| (link, NonZero::new(level as usize).unwrap(), true))
            .collect();
        let outline = OutlineNode::build_tree(nodes);

        Ok((heading_ids, outline))
    }

    fn title(&self) -> EcoString {
        // Derive title from the href filename as a fallback.
        let mut segments = self.href.path().segments();
        let filename = segments.next_back().map(|s| s.as_str()).unwrap_or("");
        Path::new(filename)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(filename)
            .into()
    }

    fn id(&self) -> EcoString {
        let mut segments = self.href.path().segments();
        let file_name = Path::new(segments.next_back().unwrap().as_str())
            .file_stem()
            .unwrap()
            .to_str()
            .unwrap();
        segments
            .map(|seg| seg.as_str())
            .chain([file_name])
            .join("-")
            .into()
    }
}
