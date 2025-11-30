use html5ever::{ParseOpts, tendril::TendrilSink};
use markup5ever_rcdom::{Handle, NodeData, RcDom};
use std::{fmt::Write, slice};
use typst::diag::EcoString;

pub struct HtmlInfo {
    pub scripted: bool,
    pub mathml: bool,
}

// TODO: should factor the XHTML-izing and portabl-izing code into seaprate functions.
pub fn html_to_portable_xhtml(html_string: &str, heading_ids: &[EcoString]) -> (String, HtmlInfo) {
    let dom = html5ever::parse_document(RcDom::default(), ParseOpts::default())
        .from_utf8()
        .read_from(&mut html_string.as_bytes())
        .expect("`Read` should not panic for `&[u8]`");

    struct Walker<'a> {
        buf: String,
        heading_ids: slice::Iter<'a, EcoString>,
        info: HtmlInfo,
    }

    impl Walker<'_> {
        fn walk(&mut self, handle: &Handle) {
            match &handle.data {
                NodeData::Document => {
                    // XHTML needs an `<?xml?>` declaration at the top.
                    self.buf
                        .push_str(r#"<?xml version="1.0" encoding="UTF-8"?>"#);
                    self.buf.push_str("\n<!DOCTYPE html>");

                    for child in handle.children.borrow().iter() {
                        self.walk(child);
                    }
                }

                NodeData::Text { contents } => {
                    // Escape text content for XHTML
                    let text = contents.borrow();
                    let escaped = text
                        .replace("&", "&amp;")
                        .replace("<", "&lt;")
                        .replace(">", "&gt;");
                    self.buf.push_str(&escaped);
                }

                NodeData::Element { name, attrs, .. } => {
                    write!(self.buf, "<{}", name.local).unwrap();

                    if &name.local == "script" {
                        self.info.scripted = true;
                    }

                    if &name.local == "math" {
                        self.info.mathml = true;
                    }

                    if &name.local == "html" {
                        write!(self.buf, " xmlns=\"http://www.w3.org/1999/xhtml\"").unwrap();
                    }

                    for attr in attrs.borrow().iter() {
                        // Escape attribute values properly for XHTML
                        let escaped_value = attr
                            .value
                            .replace("&", "&amp;")
                            .replace("\"", "&quot;")
                            .replace("<", "&lt;")
                            .replace(">", "&gt;");
                        write!(self.buf, " {}=\"{}\"", attr.name.local, escaped_value).unwrap();
                    }

                    let mut chars = name.local.chars();
                    if chars.next().unwrap() == 'h'
                        && let Some(c) = chars.next()
                        && c.is_numeric()
                        && c != '1'
                    {
                        let id = self.heading_ids.next().expect("missing heading id!");
                        write!(self.buf, " id=\"{id}\"").unwrap();
                    }

                    write!(self.buf, ">").unwrap();

                    if &name.local == "body" {
                        self.buf.push_str("<article>");
                    }

                    for child in handle.children.borrow().iter() {
                        self.walk(child);
                    }

                    if &name.local == "body" {
                        self.buf.push_str("</article>");
                    }

                    // Unconditionally close all tags, to handle case like unclosed `<p>` or `<meta>`.
                    write!(self.buf, "</{}>", name.local).unwrap();
                }

                _ => {}
            }
        }
    }

    let mut walker = Walker {
        buf: String::new(),
        heading_ids: heading_ids.iter(),
        info: HtmlInfo {
            scripted: false,
            mathml: false,
        },
    };
    walker.walk(&dom.document);

    (walker.buf, walker.info)
}

#[test]
fn test_html_to_xhtml() {
    let input = r#"<!DOCTYPE html>
<html>
    <head>
        <meta name="foo" content="bar">
    </head>
    <body>
        <h1>Test</h1>
        <p>Hello
        <p>World
    </body>
</html>"#;

    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE html><html xmlns="http://www.w3.org/1999/xhtml"><head>
        <meta name="foo" content="bar"></meta>
    </head>
    <body><article>
        <h1 id="test">Test</h1>
        <p>Hello
        </p><p>World
    
</p></article></body></html>"#;

    let (actual, _) = html_to_portable_xhtml(input, &["test".into()]);
    assert_eq!(expected, actual);
}
