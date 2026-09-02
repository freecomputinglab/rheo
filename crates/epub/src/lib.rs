pub mod package;
mod xhtml;

use package::{Item, ItemRef, Package};
use xhtml::HtmlInfo;

use chrono::{DateTime, Utc};
use iref::{IriRef, IriRefBuf, iri::Fragment};
use itertools::Itertools;
use rheo_core::html_dom::{Heading, HtmlDom};
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

        // EPUB reading order is spine order, contributed pages last. `outputs`
        // arrives in whatever order the compiled bundle's file map yields, which
        // happens to be spine order today but is an undocumented property of a
        // dependency — and reading order is not something to leave to that.
        let ordered = spine_order(ctx.spine, outputs);

        // Language and author describe the publication, so they come from its
        // FIRST spine vertebra rather than from whichever output came first.
        let language = match ordered.first() {
            Some(first) => extract_language(&first.html_string()?).unwrap_or_else(|| "en".into()),
            None => "en".to_string(),
        };

        // `dc:creator` (see `Package::creator`) is a single optional string,
        // while `DocumentInfo.author` carries zero, one, or many authors
        // (`#set document(author: ("A", "B"))`) — so several are joined rather
        // than all but the first being dropped.
        let author = ordered
            .first()
            .filter(|o| !o.author.is_empty())
            .map(|o| o.author.join(", "));

        let mut items = ordered
            .iter()
            .map(|o| {
                EpubItem::from_html_string(o.output_path.clone(), o.html_string()?, o.contributed)
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

/// `outputs` in spine order — each vertebra of `spine.flat_vertebrae()` in
/// pre-order, then every output with no matching vertebra (a marrow-contributed
/// page) in the order it arrived.
fn spine_order<'a>(
    spine: &rheo_core::reticulate::VirtualSpine,
    outputs: &'a [CastVertebra],
) -> Vec<&'a CastVertebra> {
    let mut ordered: Vec<&CastVertebra> = spine
        .flat_vertebrae()
        .iter()
        .filter_map(|v| outputs.iter().find(|o| o.output_path == v.output_path))
        .collect();
    let placed: std::collections::HashSet<&str> =
        ordered.iter().map(|o| o.output_path.as_str()).collect();
    ordered.extend(
        outputs
            .iter()
            .filter(|o| !placed.contains(o.output_path.as_str())),
    );
    ordered
}

const CONTAINER_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container" version="1.0">
	<rootfiles>
		<rootfile full-path="EPUB/package.opf" media-type="application/oebps-package+xml"/>
	</rootfiles>
</container>"#;

fn nav_header(lang: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml" xml:lang="{lang}" lang="{lang}" xmlns:epub="http://www.idpf.org/2007/ops">
	<head>
		<meta charset="utf-8"/>
		<title>Navigation</title>
	</head>
	<body>
        <nav epub:type="toc" id="toc">
"#
    )
}

const NAV_FOOTER: &str = r#"        </nav>
    </body>
</html>"#;

pub fn generate_nav_xhtml(items: &mut [EpubItem], language: &str) -> Result<String> {
    let mut buf = String::new();
    buf.push_str(&nav_header(language));

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
/// `spine_title` is a plain `Option<&str>` — the title is the only spine field
/// this plugin reads.
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
    rheo_core::html_dom::HtmlDom::parse(html_string)
        .ok()?
        .html_lang()
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

        let mut dom = HtmlDom::parse(&html_string)?;
        let outline = Self::outline_from_headings(dom.collect_headings(), &href);
        let (xhtml, info) = xhtml::html_to_portable_xhtml(&dom)?;

        Ok(EpubItem {
            href,
            xhtml,
            info,
            outline: Some(outline),
            contributed,
        })
    }

    /// Builds the nav outline tree from headings already stamped by
    /// [`HtmlDom::collect_headings`], anchoring each entry at this item's own
    /// `href#id`.
    fn outline_from_headings(headings: Vec<Heading>, href: &IriRef) -> Vec<OutlineNode<EcoString>> {
        let nodes: Vec<(EcoString, NonZero<usize>, bool)> = headings
            .into_iter()
            .map(|h| {
                let mut anchored = href.to_owned();
                anchored.set_fragment(Fragment::new(h.id.as_bytes()).ok());
                let link = eco_format!(r#"<a href="{anchored}">{}</a>"#, h.text);
                (link, NonZero::new(h.level as usize).unwrap(), true)
            })
            .collect();
        OutlineNode::build_tree(nodes)
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
        let last = segments
            .next_back()
            .expect("href is built from a non-empty output path, so has at least one segment");
        let file_name = Path::new(last.as_str())
            .file_stem()
            .and_then(|s| s.to_str())
            .expect("href always ends in .xhtml, so its file stem is valid UTF-8");
        segments
            .map(|seg| seg.as_str())
            .chain([file_name])
            .join("-")
            .into()
    }
}
