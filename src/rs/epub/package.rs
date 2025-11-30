use serde::{Deserialize, Serialize};
use typst::diag::EcoString;

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
        let config = serde_xml_rs::SerdeXml::new()
            .default_namespace("http://www.idpf.org/2007/opf")
            .namespace("dc", "http://purl.org/dc/elements/1.1/");
        config.to_string(self)
    }
}

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

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Meta {
    #[serde(rename = "@property")]
    pub property: EcoString,
    #[serde(rename = "#text")]
    pub content: EcoString,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Identifier {
    #[serde(rename = "@id")]
    pub id: EcoString,
    #[serde(rename = "#text")]
    pub content: EcoString,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    #[serde(rename = "item", default)]
    pub items: Vec<Item>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Item {
    #[serde(rename = "@id")]
    pub id: EcoString,
    #[serde(rename = "@href")]
    pub href: EcoString,
    #[serde(rename = "@media-type")]
    pub media_type: EcoString,
    #[serde(rename = "@properties")]
    pub properties: Option<EcoString>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Spine {
    #[serde(default)]
    pub itemref: Vec<ItemRef>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ItemRef {
    #[serde(rename = "@id")]
    pub id: Option<EcoString>,
    #[serde(rename = "@idref")]
    pub idref: EcoString,
}

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

    let opf_str_again = opf.to_xml().unwrap();
    let opf_again = serde_xml_rs::from_str::<Package>(&opf_str_again).unwrap();
    assert_eq!(opf, opf_again);
}
