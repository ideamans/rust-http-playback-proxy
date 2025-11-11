use anyhow::Result;
use html5ever::tendril::TendrilSink;
use markup5ever_rcdom::{Handle, NodeData, RcDom};
use std::default::Default;

/// Format JavaScript code using swc
pub fn format_javascript(input: &str) -> Result<String> {
    use swc_common::{FileName, GLOBALS, SourceMap, sync::Lrc};
    use swc_ecma_codegen::{Config, Emitter, text_writer::JsWriter};
    use swc_ecma_parser::{EsSyntax, Parser, StringInput, Syntax, lexer::Lexer};

    use bytes_str::BytesStr;

    GLOBALS.set(&Default::default(), || {
        let cm: Lrc<SourceMap> = Default::default();
        let input_owned = input.to_string();
        let fm = cm.new_source_file(
            FileName::Custom("input.js".into()).into(),
            BytesStr::from(input_owned),
        );
        let lexer = Lexer::new(
            Syntax::Es(EsSyntax::default()),
            Default::default(),
            StringInput::from(&*fm),
            None,
        );
        let mut parser = Parser::new_from(lexer);
        let module = parser
            .parse_module()
            .map_err(|e| anyhow::anyhow!("Failed to parse JavaScript: {:?}", e))?;

        let mut buf = Vec::new();
        let writer = JsWriter::new(cm.clone(), "\n", &mut buf, None);
        let mut emitter = Emitter {
            cfg: Config::default(),
            comments: None,
            cm: cm.clone(),
            wr: writer,
        };
        emitter
            .emit_module(&module)
            .map_err(|e| anyhow::anyhow!("Failed to emit JavaScript: {:?}", e))?;

        Ok(String::from_utf8(buf)?)
    })
}

/// Format CSS code using lightningcss
/// Note: Preserves @charset declaration as it's removed during parsing
pub fn format_css(input: &str) -> Result<String> {
    use lightningcss::printer::PrinterOptions;
    use lightningcss::stylesheet::{ParserOptions, StyleSheet};

    // Extract @charset declaration if present (must be first line per CSS spec)
    let charset_line = input
        .lines()
        .map(|line| line.trim())
        .find(|line| line.starts_with("@charset"));

    let sheet = StyleSheet::parse(input, ParserOptions::default())
        .map_err(|e| anyhow::anyhow!("Failed to parse CSS: {:?}", e))?;
    let out = sheet
        .to_css(PrinterOptions {
            minify: false,
            ..Default::default()
        })
        .map_err(|e| anyhow::anyhow!("Failed to format CSS: {:?}", e))?;

    // Re-add @charset declaration at the beginning if it existed
    if let Some(charset) = charset_line {
        Ok(format!("{}\n{}", charset, out.code))
    } else {
        Ok(out.code)
    }
}

/// Format HTML code using html5ever with pretty printing
pub fn format_html(input: &str) -> Result<String> {
    use html5ever::parse_document;

    let dom: RcDom = parse_document(RcDom::default(), Default::default())
        .from_utf8()
        .read_from(&mut input.as_bytes())
        .map_err(|e| anyhow::anyhow!("Failed to parse HTML: {:?}", e))?;

    let mut out = String::new();
    for child in dom.document.children.borrow().iter() {
        pretty_html(child, 0, &mut out);
    }
    Ok(out)
}

/// Recursively format HTML nodes with indentation
fn pretty_html(handle: &Handle, depth: usize, out: &mut String) {
    let node = handle;
    match &node.data {
        NodeData::Document => {
            for child in node.children.borrow().iter() {
                pretty_html(child, depth, out);
            }
        }
        NodeData::Doctype { name, .. } => {
            out.push_str("<!DOCTYPE ");
            out.push_str(name);
            out.push_str(">\n");
        }
        NodeData::Text { contents } => {
            let borrowed = contents.borrow();
            let text = borrowed.trim();
            if !text.is_empty() {
                out.push_str(&"  ".repeat(depth));
                out.push_str(text);
                out.push('\n');
            }
        }
        NodeData::Comment { contents } => {
            out.push_str(&"  ".repeat(depth));
            out.push_str("<!--");
            out.push_str(contents);
            out.push_str("-->\n");
        }
        NodeData::Element { name, attrs, .. } => {
            let tag_name = name.local.to_string();
            let is_void = matches!(
                tag_name.as_str(),
                "area"
                    | "base"
                    | "br"
                    | "col"
                    | "embed"
                    | "hr"
                    | "img"
                    | "input"
                    | "link"
                    | "meta"
                    | "param"
                    | "source"
                    | "track"
                    | "wbr"
            );

            // Opening tag
            out.push_str(&"  ".repeat(depth));
            out.push('<');
            out.push_str(&tag_name);

            // Attributes
            for a in attrs.borrow().iter() {
                out.push(' ');
                out.push_str(a.name.local.as_ref());
                out.push_str("=\"");
                // Escape attribute values
                for ch in a.value.chars() {
                    match ch {
                        '"' => out.push_str("&quot;"),
                        '&' => out.push_str("&amp;"),
                        '<' => out.push_str("&lt;"),
                        '>' => out.push_str("&gt;"),
                        _ => out.push(ch),
                    }
                }
                out.push('"');
            }

            if is_void {
                // Void elements don't have closing tags
                out.push_str(">\n");
            } else {
                out.push_str(">\n");

                // Children
                for child in node.children.borrow().iter() {
                    pretty_html(child, depth + 1, out);
                }

                // Closing tag
                out.push_str(&"  ".repeat(depth));
                out.push_str("</");
                out.push_str(&tag_name);
                out.push_str(">\n");
            }
        }
        NodeData::ProcessingInstruction { target, contents } => {
            out.push_str(&"  ".repeat(depth));
            out.push_str("<?");
            out.push_str(target);
            out.push(' ');
            out.push_str(contents);
            out.push_str("?>\n");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_javascript_simple() {
        let minified = "function test(){return 42;}";
        let formatted = format_javascript(minified).unwrap();
        assert!(formatted.lines().count() > 1);
        assert!(formatted.contains("function test()"));
    }

    #[test]
    fn test_format_css_simple() {
        let minified = "body{margin:0;padding:0;}";
        let formatted = format_css(minified).unwrap();
        assert!(formatted.lines().count() > 1);
        assert!(formatted.contains("body"));
    }

    #[test]
    fn test_format_html_simple() {
        let minified = "<html><head><title>Test</title></head><body><h1>Hello</h1></body></html>";
        let formatted = format_html(minified).unwrap();
        assert!(formatted.lines().count() > 5);
        assert!(formatted.contains("  <head>"));
    }

    #[test]
    fn test_format_javascript_complex() {
        let minified = "const x=1;if(x>0){console.log('positive');}else{console.log('negative');}";
        let formatted = format_javascript(minified).unwrap();
        assert!(formatted.lines().count() >= 3);
    }

    #[test]
    fn test_format_css_with_media_query() {
        let minified = "@media(min-width:768px){body{font-size:16px;}}";
        let formatted = format_css(minified).unwrap();
        assert!(formatted.contains("@media"));
        assert!(formatted.contains("body"));
    }

    #[test]
    fn test_format_html_with_attributes() {
        let minified =
            r#"<div id="test" class="container"><span data-value="123">Text</span></div>"#;
        let formatted = format_html(minified).unwrap();
        assert!(formatted.contains("id=\"test\""));
        assert!(formatted.contains("class=\"container\""));
    }
}
