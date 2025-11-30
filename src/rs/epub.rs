use crate::epub::package::{Identifier, Item, ItemRef, Manifest, Meta, Metadata, Package, Spine};
use anyhow::Result;
use std::{
    fs::File,
    io::{BufWriter, Write},
    path::Path,
};
use typst::{diag::EcoString, foundations::Smart};
use typst_html::HtmlDocument;
use zip::{
    result::{ZipError, ZipResult},
    write::SimpleFileOptions,
};

mod package;
mod xhtml;

const CONTAINER_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container" version="1.0">
	<rootfiles>
		<rootfile full-path="EPUB/package.opf" media-type="application/oebps-package+xml"/>
	</rootfiles>
</container>"#;

fn generate_package(document: &HtmlDocument) -> Package {
    let language = EcoString::from("en-US");
    let metadata = Metadata {
        identifier: Identifier {
            id: "uid".into(),
            content: "identifier".into(), // todo
        },
        title: match &document.info.title {
            Some(title) => title.clone(),
            None => EcoString::new(),
        },
        language: language.clone(),
        creator: document.info.author.first().cloned(),
        date: match document.info.date {
            Smart::Custom(Some(date)) => Some(date.display(Smart::Auto).unwrap()),
            _ => None,
        },
        meta: vec![Meta {
            property: "ppub:valid".into(),
            content: ".".into(),
        }],
    };
    let page_id = EcoString::from("page");
    let page_href = EcoString::from("index.xhtml");
    let manifest = Manifest {
        items: vec![Item {
            id: page_id.clone(),
            href: page_href,
            media_type: "application/xhtml+xml".into(),
            properties: None,
        }],
    };
    let spine = Spine {
        itemref: vec![ItemRef {
            id: Some("pageref".into()),
            idref: page_id,
        }],
    };
    Package {
        version: "3.0".into(),
        unique_identifier: "uid".into(),
        lang: language,
        prefix: "ppub: http://example.com/ppub".into(),
        metadata,
        manifest,
        spine,
    }
}

fn zip_epub(epub_path: &Path, package_string: String, xhtml_string: String) -> ZipResult<()> {
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

    zip.start_file("EPUB/index.xhtml", opts)?;
    zip.write_all(xhtml_string.as_bytes())?;

    zip.finish()?;

    Ok(())
}

pub fn generate_epub(html_string: String, epub_path: &Path, document: &HtmlDocument) -> Result<()> {
    let package = generate_package(document);
    let package_string = package.to_xml()?;

    let xhtml_string = xhtml::html_to_portable_xhtml(&html_string);

    zip_epub(epub_path, package_string, xhtml_string)?;

    Ok(())
}
