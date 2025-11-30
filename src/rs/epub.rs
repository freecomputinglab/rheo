use crate::{
    compile,
    config::EpubConfig,
    epub::{
        package::{Identifier, Item, ItemRef, Manifest, Meta, Metadata, Package, Spine},
        xhtml::HtmlInfo,
    },
};
use anyhow::{Result, bail, ensure};
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
use walkdir::WalkDir;
use zip::{result::ZipError, write::SimpleFileOptions};

mod package;
mod xhtml;

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

pub fn generate_nav_xhtml(items: &mut [EpubItem]) -> String {
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
        items[0].outline.take().unwrap()
    } else {
        items
            .iter_mut()
            .map(|item| {
                let entry = eco_format!(r#"<a href="{}">{}</a>"#, item.href, item.title());
                let children = item.outline.take().unwrap();
                OutlineNode {
                    entry,
                    level: NonZero::new(1).unwrap(),
                    children,
                }
            })
            .collect()
    };

    stringify_outline(&mut buf, &outline, 12);

    buf.push_str(NAV_FOOTER);
    buf
}

pub fn generate_package(items: &[EpubItem], config: &EpubConfig) -> Result<String> {
    let info = &items[0].document.info;
    let language = info.locale.unwrap_or_default().rfc_3066();
    let title = if items.len() == 1 {
        items[0].title().clone()
    } else {
        match &config.title {
            Some(title) => title.clone().into(),
            None => bail!("must have [epub.title] for multi-document EPUB"),
        }
    };

    let metadata = Metadata {
        identifier: Identifier {
            id: "uid".into(),
            content: "identifier".into(), // todo
        },
        title,
        language: language.clone(),
        creator: info.author.first().cloned(),
        date: None, // TODO: get this from rheo.toml?
        meta: vec![
            Meta {
                property: "dcterms:modified".into(),
                content: chrono::Utc::now()
                    .format("%Y-%m-%dT%H:%M:%SZ")
                    .to_string()
                    .into(),
            },
            Meta {
                property: "ppub:valid".into(),
                content: ".".into(),
            },
        ],
    };

    // Generate manifest items and spine itemrefs for each file in the spine
    let mut manifest_items = Vec::new();
    let mut spine_itemrefs = Vec::new();

    // Add navigation document to manifest
    manifest_items.push(Item {
        id: "nav".into(),
        href: "nav.xhtml".into(),
        media_type: "application/xhtml+xml".into(),
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

        let id = EcoString::from(item.path.file_stem().unwrap().to_string_lossy().to_string());

        manifest_items.push(Item {
            id: id.clone(),
            href: item.href.clone(),
            media_type: "application/xhtml+xml".into(),
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
        unique_identifier: "uid".into(),
        lang: language,
        prefix: "ppub: http://example.com/ppub".into(),
        metadata,
        manifest,
        spine,
    };

    Ok(package.to_xml()?)
}

pub fn zip_epub(
    epub_path: &Path,
    package_string: String,
    nav_xhtml: String,
    items: &[EpubItem],
) -> Result<()> {
    let file = File::create(epub_path).map_err(ZipError::Io)?;
    let file = BufWriter::new(file);
    let mut zip = zip::ZipWriter::new(file);

    let opts = SimpleFileOptions::default();

    // The mimetype file must (a) be first in the archive and (b) be stored without compression.
    zip.start_file(
        "mimetype",
        opts.compression_method(zip::CompressionMethod::Stored),
    )?;
    zip.write_all(b"application/epub+zip")?;

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

pub fn generate_spine(root: &Path, spine: &[String]) -> Result<Vec<PathBuf>> {
    if spine.is_empty() {
        let mut typst_files = WalkDir::new(root)
            .into_iter()
            .filter_map(|entry| Some(entry.ok()?.path().to_path_buf()))
            .filter(|entry| {
                matches!(
                    entry
                        .extension()
                        .map(|ext| ext.to_string_lossy())
                        .as_deref(),
                    Some("typ")
                )
            })
            .collect::<Vec<_>>();
        ensure!(!typst_files.is_empty(), "need at least one .typ file");
        typst_files.sort_by_cached_key(|p| p.file_name().unwrap().to_os_string());
        Ok(typst_files)
    } else {
        let mut typst_files = Vec::new();
        for path in spine {
            eprintln!("{}", root.join(path).display());
            let glob = glob::glob(&root.join(path).display().to_string())?;
            let mut glob_files = glob
                .filter_map(|entry| entry.ok())
                .filter(|path| path.is_file())
                .collect::<Vec<_>>();
            glob_files.sort_by_cached_key(|p| p.file_name().unwrap().to_os_string());
            typst_files.extend(glob_files);
        }
        ensure!(!typst_files.is_empty(), "need at least one .typ file");
        Ok(typst_files)
    }
}

pub struct EpubItem {
    path: PathBuf,
    href: EcoString,
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
    pub fn create(path: PathBuf, root: &Path, repo_root: &Path) -> Result<Self> {
        info!(file = %path.display(), "compiling spine file");
        let document = compile::compile_html_to_document(&path, root, repo_root)?;
        let parent = path.parent().unwrap();
        let bare_file = path.strip_prefix(parent).unwrap();
        let href = EcoString::from(bare_file.with_extension("xhtml").display().to_string());
        let (heading_ids, outline) = Self::outline(&document, &href);
        let html_string = compile::compile_document_to_string(&document, &path, root)?;
        let (xhtml, info) = xhtml::html_to_portable_xhtml(&html_string, &heading_ids);

        Ok(EpubItem {
            path,
            href,
            document,
            xhtml,
            info,
            outline: Some(outline),
        })
    }

    fn outline(doc: &HtmlDocument, href: &str) -> (Vec<EcoString>, Vec<OutlineNode<EcoString>>) {
        // Adapted from https://github.com/typst/typst/blob/02cd1c13de50363010b41b95148233dc952042c2/crates/typst-pdf/src/outline.rs#L7
        let elems = doc.introspector.query(&HeadingElem::ELEM.select());
        let (nodes, heading_ids): (Vec<_>, Vec<_>) = elems
            .iter()
            .map(|elem| {
                let heading = elem.to_packed::<HeadingElem>().unwrap();
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
                let link = eco_format!(r#"<a href="{href}#{id}">{entry}</a>"#);
                ((link, level, true), id)
            })
            .unzip();
        (heading_ids, OutlineNode::build_tree(nodes))
    }

    fn title(&self) -> &EcoString {
        match &self.document.info.title {
            Some(title) => title,
            // Default title must not be empty, so we just use the filename as a fallback
            None => &self.href,
        }
    }
}
