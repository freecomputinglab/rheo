pub mod package;
mod xhtml;

use package::{Item, ItemRef, Package};
use xhtml::HtmlInfo;

use chrono::{DateTime, Utc};
use iref::{IriRef, IriRefBuf, iri::Fragment};
use itertools::Itertools;
use rheo_core::compile::RheoCompileOptions;
use rheo_core::config::{PluginSection, UniversalSpine};
use rheo_core::html_compile::{compile_document_to_string, compile_html_to_document};
use rheo_core::pdf_utils::DocumentTitle;
use rheo_core::reticulate::spine::RheoSpine;
use rheo_core::typst_types::{
    EcoString, HeadingElem, HtmlDocument, NativeElement, OutlineNode, StyleChain, eco_format,
    eco_vec,
};
use rheo_core::{FormatPlugin, PluginContext, Result, RheoError, SpineOptions};
use std::{
    fmt::Write as _,
    fs::File,
    io::{BufWriter, Write},
    num::NonZero,
    path::{Path, PathBuf},
};
use tracing::info;
use uuid::Uuid;
use zip::write::SimpleFileOptions;

pub struct EpubPlugin;

impl FormatPlugin for EpubPlugin {
    fn name(&self) -> &'static str {
        "epub"
    }

    /// EPUB always merges multiple files into a single output.
    fn default_merge(&self) -> bool {
        true
    }

    /// Set EPUB smart defaults: infer spine title from project name when no config exists.
    fn apply_defaults(&self, section: &mut PluginSection, project_name: &str) {
        let spine = section.spine.get_or_insert_with(|| UniversalSpine {
            title: None,
            vertebrae: vec![],
            merge: None,
        });
        if spine.title.is_none() {
            spine.title = Some(DocumentTitle::to_readable_name(project_name));
        }
    }

    fn compile(&self, ctx: PluginContext<'_>) -> Result<()> {
        compile_epub_with_spine(&ctx.spine, &ctx.options, &ctx.config)
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
<html xmlns="http://www.w3.org/1999/xhtml" xml:lang="en" lang="en" xmlns:epub="http://www.idpf.org/2007/ops">
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

pub fn generate_nav_xhtml(items: &mut [EpubItem]) -> Result<String> {
    let mut buf = String::new();
    buf.push_str(NAV_HEADER);

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

    let outline = if items.len() == 1 {
        items[0]
            .outline
            .take()
            .ok_or_else(|| RheoError::invalid_data("EPUB item missing outline"))?
    } else {
        items
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
pub fn generate_package(
    items: &[EpubItem],
    spine: &UniversalSpine,
    identifier: Option<&str>,
    date: Option<&DateTime<Utc>>,
) -> Result<String> {
    let info = &items[0].document.info;
    let language = info.locale.unwrap_or_default().rfc_3066();
    let title = spine
        .title
        .as_deref()
        .map(EcoString::from)
        .unwrap_or_else(|| items[0].title());

    const INTERNAL_UNIQUE_ID: &str = "uid";

    let identifier_content = match identifier {
        Some(id) => id.into(),
        None => eco_format!("urn:uuid:{}", Uuid::new_v4()),
    };

    let mut builder = Package::builder(title)
        .unique_identifier(INTERNAL_UNIQUE_ID)
        .lang(language.clone())
        .identifier(INTERNAL_UNIQUE_ID, identifier_content)
        .language(language);

    if !info.author.is_empty() {
        builder = builder.creator(info.author.join(", "));
    }

    if let Some(d) = date {
        builder = builder.date(date_format(d));
    }

    builder = builder
        .add_meta("dcterms:modified", date_format(&chrono::Utc::now()))
        .add_meta("ppub:valid", ".");

    builder = builder.add_item(Item {
        id: "nav".into(),
        href: IriRefBuf::new("nav.xhtml".into())
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

        builder = builder
            .add_item(Item {
                id: id.clone(),
                href: item.href.clone(),
                media_type: XHTML_MEDIATYPE.into(),
                properties,
            })
            .add_spine_ref(ItemRef {
                id: Some(eco_format!("{id}ref")),
                idref: id,
            });
    }

    let package = builder.build().map_err(|e| RheoError::EpubGeneration {
        count: 1,
        errors: format!("Package validation failed: {}", e),
    })?;

    let xml = package.to_xml().map_err(|e| RheoError::EpubGeneration {
        count: 1,
        errors: format!("Package XML generation failed: {}", e),
    })?;

    Ok(xml)
}

/// Combines all EPUB components into the final .epub (zip) file.
pub fn zip_epub(
    epub_path: &Path,
    package_string: String,
    nav_xhtml: String,
    items: &[EpubItem],
) -> Result<()> {
    let file = File::create(epub_path).map_err(|e| RheoError::io(e, "creating EPUB file"))?;
    let file = BufWriter::new(file);
    let mut zip = zip::ZipWriter::new(file);

    let opts = SimpleFileOptions::default();

    zip.start_file(
        "mimetype",
        opts.compression_method(zip::CompressionMethod::Stored),
    )
    .map_err(|e| RheoError::EpubGeneration {
        count: 1,
        errors: format!("failed to start mimetype file: {}", e),
    })?;
    zip.write_all(EPUB_MEDIATYPE.as_bytes())
        .map_err(|e| RheoError::io(e, "writing mimetype"))?;

    zip.add_directory("META-INF", opts)
        .map_err(|e| RheoError::EpubGeneration {
            count: 1,
            errors: format!("failed to add META-INF directory: {}", e),
        })?;
    zip.start_file("META-INF/container.xml", opts)
        .map_err(|e| RheoError::EpubGeneration {
            count: 1,
            errors: format!("failed to start container.xml: {}", e),
        })?;
    zip.write_all(CONTAINER_XML.as_bytes())
        .map_err(|e| RheoError::io(e, "writing container.xml"))?;

    zip.add_directory("EPUB", opts)
        .map_err(|e| RheoError::EpubGeneration {
            count: 1,
            errors: format!("failed to add EPUB directory: {}", e),
        })?;

    zip.start_file("EPUB/package.opf", opts)
        .map_err(|e| RheoError::EpubGeneration {
            count: 1,
            errors: format!("failed to start package.opf: {}", e),
        })?;
    zip.write_all(package_string.as_bytes())
        .map_err(|e| RheoError::io(e, "writing package.opf"))?;

    zip.start_file("EPUB/nav.xhtml", opts)
        .map_err(|e| RheoError::EpubGeneration {
            count: 1,
            errors: format!("failed to start nav.xhtml: {}", e),
        })?;
    zip.write_all(nav_xhtml.as_bytes())
        .map_err(|e| RheoError::io(e, "writing nav.xhtml"))?;

    for item in items {
        let filename = format!("EPUB/{}", item.href);
        zip.start_file(&filename, opts)
            .map_err(|e| RheoError::EpubGeneration {
                count: 1,
                errors: format!("failed to start file {}: {}", filename, e),
            })?;
        zip.write_all(item.xhtml.as_bytes())
            .map_err(|e| RheoError::io(e, format!("writing {}", filename)))?;
    }

    zip.finish().map_err(|e| RheoError::EpubGeneration {
        count: 1,
        errors: format!("failed to finish EPUB zip: {}", e),
    })?;
    Ok(())
}

fn parse_identifier(section: &PluginSection) -> Option<String> {
    section
        .extra
        .get("identifier")
        .and_then(|v| v.as_str())
        .map(String::from)
}

fn parse_date(section: &PluginSection) -> Option<DateTime<Utc>> {
    section
        .extra
        .get("date")
        .and_then(|v| v.as_datetime())
        .and_then(|dt| {
            chrono::DateTime::parse_from_rfc3339(&dt.to_string())
                .ok()
                .map(|d| d.with_timezone(&Utc))
        })
}

fn compile_epub_impl(
    spine: &SpineOptions,
    epub_path: &Path,
    root: &Path,
    identifier: Option<&str>,
    date: Option<&DateTime<Utc>>,
) -> Result<()> {
    // Convert SpineOptions to UniversalSpine for RheoSpine::build
    let universal_spine = UniversalSpine {
        title: spine.title.clone(),
        vertebrae: spine.vertebrae.clone(),
        merge: Some(spine.merge),
    };

    // Build RheoSpine with AST-transformed sources (.typ links → .xhtml)
    // EPUB handles concatenation itself via create_from_source, so merge=false
    let rheo_spine = RheoSpine::build(root, Some(&universal_spine), "epub", false)?;

    // Get the spine file paths
    let spine_paths =
        rheo_core::reticulate::spine::generate_spine(root, Some(&universal_spine), false)?;

    let mut items = spine_paths
        .iter()
        .zip(rheo_spine.source.iter())
        .map(|(path, transformed_source)| {
            EpubItem::create_from_source(path.clone(), transformed_source, root)
        })
        .collect::<Result<Vec<_>>>()?;

    let nav_xhtml = generate_nav_xhtml(&mut items)?;
    let package_string = generate_package(&items, &universal_spine, identifier, date)?;
    zip_epub(epub_path, package_string, nav_xhtml, &items)?;

    info!(output = %epub_path.display(), "successfully generated EPUB");
    Ok(())
}

/// Compile Typst documents to EPUB using resolved spine options.
fn compile_epub_with_spine(
    spine: &SpineOptions,
    options: &RheoCompileOptions<'_>,
    section: &PluginSection,
) -> Result<()> {
    let identifier = parse_identifier(section);
    let date = parse_date(section);
    compile_epub_impl(
        spine,
        &options.output,
        &options.root,
        identifier.as_deref(),
        date.as_ref(),
    )
}

pub struct EpubItem {
    href: IriRefBuf,
    document: HtmlDocument,
    xhtml: String,
    info: HtmlInfo,
    outline: Option<Vec<OutlineNode<EcoString>>>,
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

impl EpubItem {
    pub fn create_from_source(
        path: PathBuf,
        transformed_source: &str,
        root: &Path,
    ) -> Result<Self> {
        info!(file = %path.display(), "compiling spine file with transformed source");

        let mut temp_file = tempfile::NamedTempFile::new_in(root)
            .map_err(|e| RheoError::io(e, "creating temp file for EPUB item"))?;
        temp_file
            .write_all(transformed_source.as_bytes())
            .map_err(|e| RheoError::io(e, "writing transformed source to temp file"))?;
        temp_file
            .flush()
            .map_err(|e| RheoError::io(e, "flushing temp file"))?;

        let temp_path = temp_file.path();
        let document = compile_html_to_document(temp_path, root, "epub")?;

        let parent = path.parent().ok_or_else(|| {
            RheoError::invalid_data(format!("path has no parent: {}", path.display()))
        })?;
        let bare_file = path
            .strip_prefix(parent)
            .map_err(|e| RheoError::invalid_data(format!("invalid path prefix: {}", e)))?;
        let href = IriRefBuf::new(bare_file.with_extension("xhtml").display().to_string())
            .map_err(|e| RheoError::EpubGeneration {
                count: 1,
                errors: format!("invalid href for EPUB item: {}", e),
            })?;
        let (heading_ids, outline) = Self::outline(&document, &href);

        let html_string = compile_document_to_string(&document)?;
        let (xhtml, info) = xhtml::html_to_portable_xhtml(&html_string, &heading_ids);

        Ok(EpubItem {
            href,
            document,
            xhtml,
            info,
            outline: Some(outline),
        })
    }

    fn outline(doc: &HtmlDocument, href: &IriRef) -> (Vec<EcoString>, Vec<OutlineNode<EcoString>>) {
        let elems = doc.introspector.query(&HeadingElem::ELEM.select());
        let (nodes, heading_ids): (Vec<_>, Vec<_>) = elems
            .iter()
            .map(|elem| {
                let heading = elem
                    .to_packed::<HeadingElem>()
                    .expect("must be heading b/c queried for headings");
                let level = heading.resolve_level(StyleChain::default());
                let text = heading.body.plain_text();
                let id = match heading.label() {
                    Some(label) => label.resolve().to_string().into(),
                    None => text_to_id(&text),
                };
                let entry = match &heading.numbers {
                    Some(num) => eco_format!("{num} {text}"),
                    None => text,
                };
                let mut anchored_href = href.to_owned();
                anchored_href.set_fragment(Some(
                    Fragment::new(&id).expect("heading ID should be a valid IRI fragment"),
                ));
                let link = eco_format!(r#"<a href="{anchored_href}">{entry}</a>"#);
                ((link, level, true), id)
            })
            .unzip();
        (heading_ids, OutlineNode::build_tree(nodes))
    }

    fn title(&self) -> EcoString {
        match &self.document.info.title {
            Some(title) => title.clone(),
            None => self.href.path().as_str().into(),
        }
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
