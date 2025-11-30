use html5ever::{tendril::TendrilSink, ParseOpts};
use markup5ever_rcdom::{Handle, NodeData, RcDom};
use std::fmt::Write;

// TODO: should factor the XHTML-izing and portabl-izing code into seaprate functions.
pub fn html_to_portable_xhtml(html_string: &str) -> String {
    let dom = html5ever::parse_document(RcDom::default(), ParseOpts::default())
        .from_utf8()
        .read_from(&mut html_string.as_bytes())
        .expect("`Read` should not panic for `&[u8]`");

    fn walk(handle: &Handle, buf: &mut String) {
        match &handle.data {
            NodeData::Document => {
                // XHTML needs an `<?xml?>` declaration at the top.
                buf.push_str(r#"<?xml version="1.0" encoding="UTF-8"?>"#);
                buf.push_str("\n<!DOCTYPE html>");

                for child in handle.children.borrow().iter() {
                    walk(child, buf);
                }
            }

            NodeData::Text { contents } => {
                buf.push_str(&contents.borrow());
            }

            NodeData::Element { name, attrs, .. } => {
                write!(buf, "<{}", name.local).unwrap();

                for attr in attrs.borrow().iter() {
                    write!(buf, " {}=\"{}\"", attr.name.local, attr.value).unwrap();
                }

                write!(buf, ">").unwrap();

                if &name.local == "body" {
                    buf.push_str("<article>");
                }

                for child in handle.children.borrow().iter() {
                    walk(child, buf);
                }

                if &name.local == "body" {
                    buf.push_str("</article>");
                }

                // Unconditionally close all tags, to handle case like unclosed `<p>` or `<meta>`.
                write!(buf, "</{}>", name.local).unwrap();
            }

            _ => {}
        }
    }

    let mut buf = String::new();
    walk(&dom.document, &mut buf);
    buf
}

#[test]
fn test_html_to_xhtml() {
    let input = r#"<!DOCTYPE html>
<html>
    <head>
        <meta name="foo" content="bar">
    </head>
    <body>
        <p>Hello
        <p>World
    </body>
</html>"#;

    let expected = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE html><html><head>
        <meta name="foo" content="bar"></meta>
    </head>
    <body><article>
        <p>Hello
        </p><p>World
    
</p></article></body></html>"#;

    let actual = html_to_portable_xhtml(input);
    assert_eq!(expected, actual);
}
