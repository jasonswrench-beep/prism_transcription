//! Plain text and Word export (T7, the first two of its targets). Both use one layout: a header with the file label
//! and date, then one paragraph per turn, the speaker's name in bold (Word) and timestamps as T5 sets them.

use crate::model::{hms, Transcript};
use std::io::Write;

#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum Timestamps {
    /// One at the start of every turn (the default, open decision 11).
    #[default]
    PerTurn,
    /// One every `seconds`, placed before the first word at or after each mark.
    Every { seconds: f64 },
    None,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct ExportOptions {
    #[serde(default)]
    pub timestamps: Timestamps,
    /// Word only: number the lines.
    #[serde(default)]
    pub line_numbers: bool,
    /// Hide fillers and repeated words (the clean view's rule, `edit::clean_mask`); verbatim when false (the
    /// default, open decision 10).
    #[serde(default)]
    pub clean: bool,
}

/// The header line: "INT-07 · 2026-03-04" (the date is the recording's, when the user gave one).
pub fn header_line(label: &str, date: Option<&str>) -> String {
    match date.filter(|d| !d.trim().is_empty()) {
        Some(d) => format!("{label} \u{b7} {d}"),
        None => label.to_string(),
    }
}

/// A paragraph as pieces, so text and Word share one layout: (timestamp before the speaker, speaker, body).
struct Para {
    stamp: Option<String>,
    speaker: String,
    body: String,
}

fn paragraphs(t: &Transcript, opts: &ExportOptions) -> Vec<Para> {
    let mut next_mark = 0.0;
    t.turns
        .iter()
        .map(|turn| {
            let cleaned;
            let turn = if opts.clean {
                let keep = crate::edit::clean_mask(&turn.words);
                cleaned = crate::model::Turn { words: turn.words.iter().zip(keep).filter(|(_, k)| *k).map(|(w, _)| w.clone()).collect(), ..turn.clone() };
                &cleaned
            } else {
                turn
            };
            let speaker = t.speaker(&turn.speaker).map(|s| s.display_name().to_string()).unwrap_or_else(|| turn.speaker.clone());
            let (stamp, body) = match opts.timestamps {
                Timestamps::PerTurn => (Some(format!("[{}]", hms(turn.start))), turn.text()),
                Timestamps::None => (None, turn.text()),
                Timestamps::Every { seconds } => {
                    let step = seconds.max(1.0);
                    let mut parts = Vec::with_capacity(turn.words.len());
                    for w in &turn.words {
                        if w.start >= next_mark {
                            parts.push(format!("[{}]", hms((w.start / step).floor() * step)));
                            next_mark = ((w.start / step).floor() + 1.0) * step;
                        }
                        parts.push(w.text.clone());
                    }
                    (None, parts.join(" "))
                }
            };
            Para { stamp, speaker, body }
        })
        .collect()
}

pub fn to_text(t: &Transcript, label: &str, date: Option<&str>, opts: &ExportOptions) -> String {
    let mut out = String::new();
    out.push_str(&header_line(label, date));
    out.push_str("\n\n");
    for p in paragraphs(t, opts) {
        if let Some(s) = &p.stamp {
            out.push_str(s);
            out.push(' ');
        }
        out.push_str(&p.speaker);
        out.push_str(": ");
        out.push_str(&p.body);
        out.push_str("\n\n");
    }
    out
}

fn xml_escape(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            // Control characters other than tab, newline and return are not allowed in XML 1.0.
            c if (c as u32) < 0x20 && c != '\t' && c != '\n' && c != '\r' => {}
            c => o.push(c),
        }
    }
    o
}

fn run(text: &str, bold: bool) -> String {
    format!(
        r#"<w:r>{}<w:t xml:space="preserve">{}</w:t></w:r>"#,
        if bold { "<w:rPr><w:b/></w:rPr>" } else { "" },
        xml_escape(text)
    )
}

const W_NS: &str = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships""#;

/// A .docx file, written by hand: the parts Word needs and nothing else.
pub fn to_docx(t: &Transcript, label: &str, date: Option<&str>, title: &str, opts: &ExportOptions) -> std::io::Result<Vec<u8>> {
    let mut body = String::new();
    for p in paragraphs(t, opts) {
        body.push_str(r#"<w:p><w:pPr><w:spacing w:after="160"/></w:pPr>"#);
        if let Some(s) = &p.stamp {
            body.push_str(&run(&format!("{s} "), false));
        }
        body.push_str(&run(&format!("{}:", p.speaker), true));
        body.push_str(&run(&format!(" {}", p.body), false));
        body.push_str("</w:p>");
    }
    let line_numbers = if opts.line_numbers { r#"<w:lnNumType w:countBy="1" w:restart="continuous"/>"# } else { "" };
    let document = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document {W_NS}><w:body>{body}<w:sectPr><w:headerReference w:type="default" r:id="rIdHeader"/>{line_numbers}<w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr></w:body></w:document>"#
    );
    let header = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:hdr {W_NS}><w:p>{}</w:p></w:hdr>"#,
        run(&header_line(label, date), false)
    );
    let core = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>{}</dc:title><dc:creator>Prism Transcribe</dc:creator></cp:coreProperties>"#,
        xml_escape(title)
    );
    let parts: [(&str, String); 6] = [
        (
            "[Content_Types].xml",
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/header1.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml"/><Override PartName="/docProps/core.xml" ContentType="application/vnd.openxmlformats-package.core-properties+xml"/></Types>"#.into(),
        ),
        (
            "_rels/.rels",
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties" Target="docProps/core.xml"/></Relationships>"#.into(),
        ),
        (
            "word/_rels/document.xml.rels",
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdHeader" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/header" Target="header1.xml"/></Relationships>"#.into(),
        ),
        ("word/document.xml", document),
        ("word/header1.xml", header),
        ("docProps/core.xml", core),
    ];
    let mut zw = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let o = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (name, data) in parts {
        zw.start_file(name, o).map_err(std::io::Error::other)?;
        zw.write_all(data.as_bytes())?;
    }
    Ok(zw.finish().map_err(std::io::Error::other)?.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::tests::sample;
    use std::io::Read;

    #[test]
    fn text_layout() {
        let t = sample();
        let s = to_text(&t, "INT-07", Some("2026-03-04"), &ExportOptions::default());
        assert_eq!(s, "INT-07 \u{b7} 2026-03-04\n\n[00:00:00] Interviewer: Hello, Maria.\n\n[00:00:02] Speaker 2: Hi there.\n\n");
        let none = to_text(&t, "INT-07", None, &ExportOptions { timestamps: Timestamps::None, ..Default::default() });
        let mut um = t.clone();
        um.turns[0].words.insert(0, crate::model::Word { text: "Um,".into(), start: 0.3, end: 0.45, confidence: 1.0, speaker: "S1".into() });
        assert!(to_text(&um, "x", None, &ExportOptions::default()).contains("Interviewer: Um, Hello,"), "verbatim keeps it");
        assert!(to_text(&um, "x", None, &ExportOptions { clean: true, ..Default::default() }).contains("Interviewer: Hello, Maria."), "clean hides it");
        assert!(none.starts_with("INT-07\n\nInterviewer: Hello,"));
        let every = to_text(&t, "x", None, &ExportOptions { timestamps: Timestamps::Every { seconds: 2.0 }, ..Default::default() });
        assert!(every.contains("Interviewer: [00:00:00] Hello, Maria."), "{every}");
        assert!(every.contains("Speaker 2: [00:00:02] Hi there."), "{every}");
    }

    #[test]
    fn docx_has_the_parts_word_needs() {
        let mut t = sample();
        t.turns[0].words[1].text = "<Maria> & co.".into();
        let bytes = to_docx(&t, "INT-07", Some("2026-03-04"), "INT-07", &ExportOptions { line_numbers: true, ..Default::default() }).unwrap();
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        for name in ["[Content_Types].xml", "_rels/.rels", "word/document.xml", "word/header1.xml", "word/_rels/document.xml.rels", "docProps/core.xml"] {
            assert!(z.by_name(name).is_ok(), "{name}");
        }
        let mut doc = String::new();
        z.by_name("word/document.xml").unwrap().read_to_string(&mut doc).unwrap();
        assert!(doc.contains("<w:b/></w:rPr><w:t xml:space=\"preserve\">Interviewer:</w:t>"));
        assert!(doc.contains("&lt;Maria&gt; &amp; co."), "text is escaped");
        assert!(doc.contains("w:lnNumType"));
        let mut hdr = String::new();
        z.by_name("word/header1.xml").unwrap().read_to_string(&mut hdr).unwrap();
        assert!(hdr.contains("INT-07 \u{b7} 2026-03-04"));
    }
}
