pub mod package;
mod xhtml;

use package::{Identifier, Item, ItemRef, Manifest, Meta, Metadata, Package, Spine};
use xhtml::HtmlInfo;

use crate::compile::RheoCompileOptions;
use crate::config::{EpubConfig, EpubOptions};
use crate::constants::XHTML_EXT;
use crate::formats::compiler::FormatCompiler;
use crate::{OutputFormat, Result, RheoError};
use anyhow::Result as AnyhowResult;
use chrono::{DateTime, Utc};
use iref::{IriRef, IriRefBuf, iri::Fragment};
use itertools::Itertools;
use std::{
    fmt::Write as _,
    fs::File,
    io::{BufWriter, Write},
    num::NonZero,
    path::{Path, PathBuf},
};
use tracing::info;
use typst::{
    diag::{EcoString, eco_format},
    ecow::eco_vec,
    foundations::{NativeElement, StyleChain},
    model::{HeadingElem, OutlineNode},
};
use typst_html::HtmlDocument;
use uuid::Uuid;
use zip::{result::ZipError, write::SimpleFileOptions};

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
        // If we only have one item, then its nav is just its outline.
        items[0]
            .outline
            .take()
            .ok_or_else(|| RheoError::invalid_data("EPUB item missing outline"))?
    } else {
        // If we have multiple items, generate a new level of outline which contains a link
        // to each item.
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
///
/// See: EPUB 3.3 Package document <https://www.w3.org/TR/epub-33/#sec-package-doc>
pub fn generate_package(items: &[EpubItem], config: &EpubConfig) -> AnyhowResult<String> {
    let info = &items[0].document.info;
    let language = info.locale.unwrap_or_default().rfc_3066();
    let title = match &config.merge {
        None => items[0].title(),
        Some(combined) => combined.title.clone().into(),
    };

    const INTERNAL_UNIQUE_ID: &str = "uid";

    // If the user did not provide a unique ID, we generate a UUID for them.
    let identifier = {
        let content = match &config.identifier {
            Some(id) => id.into(),
            None => eco_format!("urn:uuid:{}", Uuid::new_v4()),
        };
        Identifier {
            id: INTERNAL_UNIQUE_ID.into(),
            content,
        }
    };

    // Concatenate all authors into a comma-separated string.
    let creator = (!info.author.is_empty()).then(|| info.author.join(", ").into());

    let date = config.date.as_ref().map(date_format);

    let meta = vec![
        // Record a timestamp for when this document is generated.
        Meta {
            property: "dcterms:modified".into(),
            content: date_format(&chrono::Utc::now()),
        },
        // Indicate that this document is a portable EPUB.
        Meta {
            property: "ppub:valid".into(),
            content: ".".into(),
        },
    ];

    let metadata = Metadata {
        identifier,
        title,
        language: language.clone(),
        creator,
        date,
        meta,
    };

    let mut manifest_items = Vec::new();
    let mut spine_itemrefs = Vec::new();

    manifest_items.push(Item {
        id: "nav".into(),
        href: IriRefBuf::new("nav.xhtml".into()).unwrap(),
        media_type: XHTML_MEDIATYPE.into(),
        properties: Some("nav".into()), // required by spec
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

        manifest_items.push(Item {
            id: id.clone(),
            href: item.href.clone(),
            media_type: XHTML_MEDIATYPE.into(),
            properties,
        });

        spine_itemrefs.push(ItemRef {
            id: Some(eco_format!("{id}ref")),
            idref: id,
        });
    }

    let manifest = Manifest {
        items: manifest_items,
    };
    let spine = Spine {
        itemref: spine_itemrefs,
    };

    let package = Package {
        version: "3.0".into(),
        unique_identifier: INTERNAL_UNIQUE_ID.into(),
        lang: language,
        prefix: "ppub: http://example.com/ppub".into(), // to support portable EPUB properties
        metadata,
        manifest,
        spine,
    };

    Ok(package.to_xml()?)
}

/// Combines all EPUB components into the final .epub i.e. zip file.
///
/// See: EPUB 3.3 Open Container Format <https://www.w3.org/TR/epub-33/#sec-ocf>
pub fn zip_epub(
    epub_path: &Path,
    package_string: String,
    nav_xhtml: String,
    items: &[EpubItem],
) -> AnyhowResult<()> {
    let file = File::create(epub_path).map_err(ZipError::Io)?;
    let file = BufWriter::new(file);
    let mut zip = zip::ZipWriter::new(file);

    let opts = SimpleFileOptions::default();

    // The mimetype file must (a) be first in the archive and (b) be stored without compression.
    zip.start_file(
        "mimetype",
        opts.compression_method(zip::CompressionMethod::Stored),
    )?;
    zip.write_all(EPUB_MEDIATYPE.as_bytes())?;

    // The EPUB root metadata file must be exactly at `META-INF/container.xml`.
    // See `CONTAINER_XML` for its pre-baked definition.
    zip.add_directory("META-INF", opts)?;
    zip.start_file("META-INF/container.xml", opts)?;
    zip.write_all(CONTAINER_XML.as_bytes())?;

    // All other files go in the `EPUB` directory (by convention, not standard).
    zip.add_directory("EPUB", opts)?;

    zip.start_file("EPUB/package.opf", opts)?;
    zip.write_all(package_string.as_bytes())?;

    zip.start_file("EPUB/nav.xhtml", opts)?;
    zip.write_all(nav_xhtml.as_bytes())?;

    for item in items {
        let filename = format!("EPUB/{}", item.href);
        zip.start_file(&filename, opts)?;
        zip.write_all(item.xhtml.as_bytes())?;
    }

    zip.finish()?;

    Ok(())
}

// ============================================================================
// EPUB compilation (implementation function)
// ============================================================================

/// Implementation: Compile multiple Typst files to EPUB format.
///
/// Generates a spine from the EPUB configuration, compiles each file to HTML,
/// generates navigation, and packages everything into a .epub (zip) file.
fn compile_epub_impl(
    config: &EpubConfig,
    epub_path: &Path,
    root: &Path,
    repo_root: &Path,
) -> Result<()> {
    let inner = || -> AnyhowResult<()> {
        let spine = crate::spine::generate_spine(root, config.merge.as_ref(), false)?;

        let mut items = spine
            .into_iter()
            .map(|path| EpubItem::create(path, root, repo_root))
            .collect::<AnyhowResult<Vec<_>>>()?;

        let nav_xhtml = generate_nav_xhtml(&mut items)?;
        let package_string = generate_package(&items, config)?;
        zip_epub(epub_path, package_string, nav_xhtml, &items)
    };

    inner().map_err(|e| RheoError::EpubGeneration {
        count: 1,
        errors: e.to_string(),
    })?;

    info!(output = %epub_path.display(), "successfully generated EPUB");
    Ok(())
}

/// Compile Typst documents to EPUB (unified API).
///
/// Currently routes to the implementation function. EPUB compilation does not
/// yet support incremental compilation (only fresh compilation is available).
///
/// # Arguments
/// * `options` - Compilation options (input, output, root, repo_root, world)
/// * `epub_options` - EPUB-specific options (wraps EpubConfig)
///
/// # Returns
/// * `Result<()>` - Success or compilation error
pub fn compile_epub_new(options: RheoCompileOptions, epub_options: EpubOptions) -> Result<()> {
    // Note: EPUB doesn't support incremental compilation yet, so we ignore options.world
    // and always do fresh compilation
    compile_epub_impl(
        &epub_options.config,
        &options.output,
        &options.root,
        &options.repo_root,
    )
}

// ============================================================================
// FormatCompiler trait implementation
// ============================================================================

/// EPUB compiler implementation
pub use crate::formats::compiler::EpubCompiler;

impl FormatCompiler for EpubCompiler {
    type Config = EpubOptions;

    fn format(&self) -> OutputFormat {
        OutputFormat::Epub
    }

    fn supports_per_file(&self, _config: &Self::Config) -> bool {
        // EPUB never supports per-file (always merged)
        false
    }

    fn compile(&self, options: RheoCompileOptions, config: &Self::Config) -> Result<()> {
        compile_epub_new(options, config.clone())
    }
}

// ============================================================================
// EPUB compilation implementation
// ============================================================================

pub struct EpubItem {
    href: IriRefBuf,
    document: HtmlDocument,
    xhtml: String,
    info: HtmlInfo,
    outline: Option<Vec<OutlineNode<EcoString>>>,
}

fn text_to_id(s: &str) -> EcoString {
    // TODO: handle all the cases described here:
    // https://developer.mozilla.org/en-US/docs/Web/CSS/Reference/Values/ident#syntax
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
    pub fn create(path: PathBuf, root: &Path, repo_root: &Path) -> AnyhowResult<Self> {
        info!(file = %path.display(), "compiling spine file");
        let document = crate::formats::html::compile_html_to_document(&path, root, repo_root)?;
        let parent = path.parent().unwrap();
        let bare_file = path.strip_prefix(parent).unwrap();
        let href = IriRefBuf::new(bare_file.with_extension("xhtml").display().to_string())?;
        let (heading_ids, outline) = Self::outline(&document, &href);
        // Export to HTML and transform links for XHTML
        let html_string = crate::formats::html::compile_document_to_string(&document)?;
        let html_string =
            crate::postprocess::transform_links(&html_string, &path, root, XHTML_EXT)?;
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
        // Adapted from https://github.com/typst/typst/blob/02cd1c13de50363010b41b95148233dc952042c2/crates/typst-pdf/src/outline.rs#L7
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
                anchored_href.set_fragment(Some(Fragment::new(&id).unwrap())); // TODO: when can this panic?
                let link = eco_format!(r#"<a href="{anchored_href}">{entry}</a>"#);
                ((link, level, true), id)
            })
            .unzip();
        (heading_ids, OutlineNode::build_tree(nodes))
    }

    fn title(&self) -> EcoString {
        match &self.document.info.title {
            Some(title) => title.clone(),
            // Default title must not be empty, so we just use the filename as a fallback
            None => self.href.path().as_str().into(),
        }
    }

    fn id(&self) -> EcoString {
        // Use href as a stand-in for item ID.
        // Eg `chapters/foo.typ` becomes `chapters-foo`
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
