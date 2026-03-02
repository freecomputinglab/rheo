//! An EPUB 3 package document with XML serialization.
//!
//! For context on any given field, see the linked EPUB specification document
//! at the header of each struct.

use iref::IriRefBuf;
use serde::{Deserialize, Serialize};
use typst::diag::EcoString;

// To understand the idiosyncratic serde renames, see:
// https://docs.rs/serde-xml-rs/latest/serde_xml_rs/

/// https://www.w3.org/TR/epub-33/#sec-package-doc
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename = "package")]
pub struct Package {
    #[serde(rename = "@version")]
    pub version: EcoString,
    #[serde(rename = "@unique-identifier")]
    pub unique_identifier: EcoString,
    #[serde(rename = "@xml:lang")]
    pub lang: EcoString,
    #[serde(rename = "@prefix")]
    pub prefix: EcoString,
    pub metadata: Metadata,
    pub manifest: Manifest,
    pub spine: Spine,
}

impl Package {
    pub fn to_xml(&self) -> Result<String, serde_xml_rs::Error> {
        // Note: wcrichto evaluated quick-xml for serialization as of Nov 2025,
        // but it doesn't support serializing namespaced elements. Waiting on this issue:
        // https://github.com/tafia/quick-xml/issues/218
        let config = serde_xml_rs::SerdeXml::new()
            .default_namespace("http://www.idpf.org/2007/opf")
            .namespace("dc", "http://purl.org/dc/elements/1.1/");
        config.to_string(self)
    }

    /// Create a PackageBuilder for constructing a Package.
    pub fn builder(title: impl Into<EcoString>) -> PackageBuilder {
        PackageBuilder::new(title)
    }
}

/// Builder for constructing EPUB Package documents with validation.
pub struct PackageBuilder {
    version: EcoString,
    unique_identifier: EcoString,
    lang: EcoString,
    prefix: EcoString,
    identifier: Option<Identifier>,
    title: EcoString,
    language: Option<EcoString>,
    creator: Option<EcoString>,
    date: Option<EcoString>,
    meta: Vec<Meta>,
    manifest_items: Vec<Item>,
    spine_itemrefs: Vec<ItemRef>,
}

impl PackageBuilder {
    /// Create a new PackageBuilder with the given title.
    ///
    /// Sets defaults:
    /// - version: "3.0"
    /// - unique_identifier: "uid"
    /// - lang: "en"
    /// - prefix: "ppub: http://example.com/ppub"
    pub fn new(title: impl Into<EcoString>) -> Self {
        Self {
            version: "3.0".into(),
            unique_identifier: "uid".into(),
            lang: "en".into(),
            prefix: "ppub: http://example.com/ppub".into(),
            identifier: None,
            title: title.into(),
            language: None,
            creator: None,
            date: None,
            meta: Vec::new(),
            manifest_items: Vec::new(),
            spine_itemrefs: Vec::new(),
        }
    }

    /// Set the EPUB version (default: "3.0").
    pub fn version(mut self, version: impl Into<EcoString>) -> Self {
        self.version = version.into();
        self
    }

    /// Set the unique identifier attribute (default: "uid").
    pub fn unique_identifier(mut self, id: impl Into<EcoString>) -> Self {
        self.unique_identifier = id.into();
        self
    }

    /// Set the language attribute (xml:lang).
    pub fn lang(mut self, lang: impl Into<EcoString>) -> Self {
        self.lang = lang.into();
        self
    }

    /// Set the prefix declaration (default: "ppub: http://example.com/ppub").
    pub fn prefix(mut self, prefix: impl Into<EcoString>) -> Self {
        self.prefix = prefix.into();
        self
    }

    /// Set the metadata identifier.
    pub fn identifier(mut self, id: impl Into<EcoString>, content: impl Into<EcoString>) -> Self {
        self.identifier = Some(Identifier {
            id: id.into(),
            content: content.into(),
        });
        self
    }

    /// Set the metadata language (dc:language).
    pub fn language(mut self, language: impl Into<EcoString>) -> Self {
        self.language = Some(language.into());
        self
    }

    /// Set the metadata creator (dc:creator).
    pub fn creator(mut self, creator: impl Into<EcoString>) -> Self {
        self.creator = Some(creator.into());
        self
    }

    /// Set the metadata date (dc:date).
    pub fn date(mut self, date: impl Into<EcoString>) -> Self {
        self.date = Some(date.into());
        self
    }

    /// Add a metadata element.
    pub fn add_meta(
        mut self,
        property: impl Into<EcoString>,
        content: impl Into<EcoString>,
    ) -> Self {
        self.meta.push(Meta {
            property: property.into(),
            content: content.into(),
        });
        self
    }

    /// Add a manifest item.
    pub fn add_item(mut self, item: Item) -> Self {
        self.manifest_items.push(item);
        self
    }

    /// Add a spine itemref.
    pub fn add_spine_ref(mut self, itemref: ItemRef) -> Self {
        self.spine_itemrefs.push(itemref);
        self
    }

    /// Build the Package, validating before returning.
    pub fn build(self) -> Result<Package, ValidationError> {
        // Create default identifier if not set
        let identifier = self.identifier.unwrap_or_else(|| Identifier {
            id: self.unique_identifier.clone(),
            content: "urn:uuid:00000000-0000-0000-0000-000000000000".into(),
        });

        // Use lang as language if language not explicitly set
        let language = self.language.unwrap_or_else(|| self.lang.clone());

        let metadata = Metadata {
            identifier,
            title: self.title,
            language,
            creator: self.creator,
            date: self.date,
            meta: self.meta,
        };

        let manifest = Manifest {
            items: self.manifest_items,
        };

        let spine = Spine {
            itemref: self.spine_itemrefs,
        };

        let package = Package {
            version: self.version,
            unique_identifier: self.unique_identifier,
            lang: self.lang,
            prefix: self.prefix,
            metadata,
            manifest,
            spine,
        };

        // Validate before returning
        package.validate()?;

        Ok(package)
    }
}

/// Validation error for Package construction.
#[derive(Debug, Clone)]
pub struct ValidationError {
    pub message: String,
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Package validation error: {}", self.message)
    }
}

impl std::error::Error for ValidationError {}

/// Trait for XML serialization.
pub trait ToXml {
    fn to_xml(&self) -> Result<String, serde_xml_rs::Error>;
}

impl ToXml for Package {
    fn to_xml(&self) -> Result<String, serde_xml_rs::Error> {
        Package::to_xml(self)
    }
}

/// Trait for validation.
pub trait Validate {
    fn validate(&self) -> Result<(), ValidationError>;
}

impl Validate for Package {
    fn validate(&self) -> Result<(), ValidationError> {
        // Check required fields present
        if self.metadata.title.is_empty() {
            return Err(ValidationError {
                message: "metadata title is required".to_string(),
            });
        }

        if self.metadata.language.is_empty() {
            return Err(ValidationError {
                message: "metadata language is required".to_string(),
            });
        }

        // Verify spine references exist in manifest
        for itemref in &self.spine.itemref {
            let found = self
                .manifest
                .items
                .iter()
                .any(|item| item.id == itemref.idref);
            if !found {
                return Err(ValidationError {
                    message: format!("spine itemref '{}' not found in manifest", itemref.idref),
                });
            }
        }

        // Ensure unique identifiers in manifest
        let mut ids = std::collections::HashSet::new();
        for item in &self.manifest.items {
            if !ids.insert(&item.id) {
                return Err(ValidationError {
                    message: format!("duplicate manifest item id: '{}'", item.id),
                });
            }
        }

        Ok(())
    }
}

/// https://www.w3.org/TR/epub-33/#sec-pkg-metadata
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Metadata {
    #[serde(rename = "dc:identifier")]
    pub identifier: Identifier,
    #[serde(rename = "dc:title")]
    pub title: EcoString,
    #[serde(rename = "dc:language")]
    pub language: EcoString,
    #[serde(rename = "dc:creator")]
    pub creator: Option<EcoString>,
    #[serde(rename = "dc:date")]
    pub date: Option<EcoString>,
    #[serde(rename = "meta")]
    pub meta: Vec<Meta>,
}

/// https://www.w3.org/TR/epub-33/#sec-meta-elem
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Meta {
    #[serde(rename = "@property")]
    pub property: EcoString,
    #[serde(rename = "#text")]
    pub content: EcoString,
}

/// https://www.w3.org/TR/epub-33/#sec-opf-dcidentifier
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Identifier {
    #[serde(rename = "@id")]
    pub id: EcoString,
    #[serde(rename = "#text")]
    pub content: EcoString,
}

/// https://www.w3.org/TR/epub-33/#sec-pkg-manifest
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    #[serde(rename = "item", default)]
    pub items: Vec<Item>,
}

/// https://www.w3.org/TR/epub-33/#sec-item-elem
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Item {
    #[serde(rename = "@id")]
    pub id: EcoString,
    #[serde(rename = "@href")]
    pub href: IriRefBuf,
    #[serde(rename = "@media-type")]
    pub media_type: EcoString,
    #[serde(rename = "@properties")]
    pub properties: Option<EcoString>,
}

/// https://www.w3.org/TR/epub-33/#sec-pkg-spine
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Spine {
    #[serde(default)]
    pub itemref: Vec<ItemRef>,
}

/// https://www.w3.org/TR/epub-33/#sec-itemref-elem
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ItemRef {
    #[serde(rename = "@id")]
    pub id: Option<EcoString>,
    #[serde(rename = "@idref")]
    pub idref: EcoString,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_package_serde() {
        let opf_str = r#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="uid" xml:lang="en-US"
    prefix="ppub: http://example.com/ppub">
    <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
        <dc:identifier id="uid">code.examle.identifier</dc:identifier>
        <dc:title>Portable EPUBs</dc:title>
        <dc:creator>Will Crichton</dc:creator>
        <dc:language>en-US</dc:language>
        <dc:date>2024-01-24</dc:date>
        <meta property="dcterms:modified">2023-01-24T00:00:00Z</meta>
        <meta property="ppub:valid">.</meta>
    </metadata>
    <manifest>
        <item id="page" href="index.xhtml" media-type="application/xhtml+xml" />
        <item id="nav" href="nav.xhtml" properties="nav" media-type="application/xhtml+xml" />
        <item id="img-tags" href="img/tags.jpg" media-type="image/jpeg" />
    </manifest>
    <spine>
        <itemref id="pageref" idref="page" />
    </spine>
</package>"#;
        let opf = serde_xml_rs::from_str::<Package>(opf_str).unwrap();

        // Check that the `Package` contains every expected field deserialized from the string.
        assert_eq!(opf.version, "3.0");
        assert_eq!(opf.unique_identifier, "uid");
        assert_eq!(opf.lang, "en-US");
        assert_eq!(opf.prefix, "ppub: http://example.com/ppub");

        assert_eq!(opf.metadata.identifier.id, "uid");
        assert_eq!(opf.metadata.identifier.content, "code.examle.identifier");
        assert_eq!(opf.metadata.title, "Portable EPUBs");
        assert_eq!(opf.metadata.creator, Some("Will Crichton".into()));
        assert_eq!(opf.metadata.language, "en-US");
        assert_eq!(opf.metadata.date, Some("2024-01-24".into()));
        assert_eq!(opf.metadata.meta.len(), 2);
        assert_eq!(opf.metadata.meta[0].property, "dcterms:modified");
        assert_eq!(opf.metadata.meta[0].content, "2023-01-24T00:00:00Z");
        assert_eq!(opf.metadata.meta[1].property, "ppub:valid");
        assert_eq!(opf.metadata.meta[1].content, ".");

        assert_eq!(opf.manifest.items.len(), 3);
        assert_eq!(opf.manifest.items[0].id, "page");
        assert_eq!(opf.manifest.items[0].href, "index.xhtml");
        assert_eq!(opf.manifest.items[0].media_type, "application/xhtml+xml");
        assert_eq!(opf.manifest.items[0].properties, None);
        assert_eq!(opf.manifest.items[1].id, "nav");
        assert_eq!(opf.manifest.items[1].href, "nav.xhtml");
        assert_eq!(opf.manifest.items[1].media_type, "application/xhtml+xml");
        assert_eq!(opf.manifest.items[1].properties, Some("nav".into()));
        assert_eq!(opf.manifest.items[2].id, "img-tags");
        assert_eq!(opf.manifest.items[2].href, "img/tags.jpg");
        assert_eq!(opf.manifest.items[2].media_type, "image/jpeg");

        assert_eq!(opf.spine.itemref.len(), 1);
        assert_eq!(opf.spine.itemref[0].id, Some("pageref".into()));
        assert_eq!(opf.spine.itemref[0].idref, "page");

        // Check that serialization works by round-tripping `Package` to a string and back,
        // then checking that the resulting package is the same as the input.
        let opf_str_again = opf.to_xml().unwrap();
        let opf_again = serde_xml_rs::from_str::<Package>(&opf_str_again).unwrap();
        assert_eq!(opf, opf_again);
    }

    #[test]
    fn test_package_builder_basic() {
        let package = Package::builder("Test Book")
            .lang("en-US")
            .identifier("uid", "test-id-123")
            .language("en-US")
            .creator("Test Author")
            .add_meta("test:property", "test-value")
            .add_item(Item {
                id: "page1".into(),
                href: IriRefBuf::new("page1.xhtml".into()).unwrap(),
                media_type: "application/xhtml+xml".into(),
                properties: None,
            })
            .add_spine_ref(ItemRef {
                id: Some("page1ref".into()),
                idref: "page1".into(),
            })
            .build()
            .unwrap();

        assert_eq!(package.metadata.title, "Test Book");
        assert_eq!(package.metadata.language, "en-US");
        assert_eq!(package.metadata.creator, Some("Test Author".into()));
        assert_eq!(package.metadata.meta.len(), 1);
        assert_eq!(package.manifest.items.len(), 1);
        assert_eq!(package.spine.itemref.len(), 1);
    }

    #[test]
    fn test_package_builder_validation_missing_title() {
        let result = PackageBuilder::new("").language("en").build();

        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("title is required"));
    }

    #[test]
    fn test_package_builder_validation_invalid_spine_ref() {
        let result = Package::builder("Test")
            .language("en")
            .add_item(Item {
                id: "page1".into(),
                href: IriRefBuf::new("page1.xhtml".into()).unwrap(),
                media_type: "application/xhtml+xml".into(),
                properties: None,
            })
            .add_spine_ref(ItemRef {
                id: None,
                idref: "page2".into(), // References non-existent item
            })
            .build();

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .message
                .contains("not found in manifest")
        );
    }

    #[test]
    fn test_package_builder_validation_duplicate_ids() {
        let result = Package::builder("Test")
            .language("en")
            .add_item(Item {
                id: "page1".into(),
                href: IriRefBuf::new("page1.xhtml".into()).unwrap(),
                media_type: "application/xhtml+xml".into(),
                properties: None,
            })
            .add_item(Item {
                id: "page1".into(), // Duplicate ID
                href: IriRefBuf::new("page2.xhtml".into()).unwrap(),
                media_type: "application/xhtml+xml".into(),
                properties: None,
            })
            .build();

        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("duplicate"));
    }
}
