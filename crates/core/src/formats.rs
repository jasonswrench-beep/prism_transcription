//! The other export targets of T7 beside Word and plain text: captions (SubRip and WebVTT) and the table (CSV and
//! XLSX). The PrismQDA package and REFI-QDA are in [`crate::package`].
//!
//! **Captions** follow the usual broadcast rules: at most two lines of at most 42 characters per cue, a cue never
//! longer than 7 seconds, a new cue at a pause of a second or more and, once a cue has some words, at the end of a
//! sentence. Times are the words' own (T5), in hh:mm:ss.mmm (SubRip writes a comma). Notation such as [laughter]
//! stays in, as captions should describe sounds. Speaker names are optional: SubRip puts "Name: " before a turn's
//! first cue; WebVTT marks every cue with a voice tag, `<v Name>`.
//!
//! **The table** has one row per turn: file, speaker, role, start, end, the start and end in seconds, and the text,
//! ready for PrismStatistics or a spreadsheet. CSV is UTF-8 with a byte-order mark, so Excel reads accents rightly;
//! XLSX is written by hand (the parts Excel needs), with times as real time values.

use crate::model::{Role, Transcript, Turn, Word};
use std::io::Write;

// ---------------------------------------------------------------------------------------------------------
// Captions
// ---------------------------------------------------------------------------------------------------------

pub const CAPTION_LINE: usize = 42;
pub const CAPTION_LINES: usize = 2;
const MAX_CUE_SECONDS: f64 = 7.0;
const PAUSE_BREAK: f64 = 1.0;
const MIN_CUE_SECONDS: f64 = 0.7;

#[derive(Clone, Copy, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct CaptionOptions {
    /// Show who is speaking.
    #[serde(default)]
    pub speaker_names: bool,
    /// Leave out fillers (the clean view's rule).
    #[serde(default)]
    pub clean: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Cue {
    pub start: f64,
    pub end: f64,
    pub lines: Vec<String>,
    /// The speaker's shown name, when names are on.
    pub speaker: Option<String>,
    /// The first cue of its turn.
    pub turn_start: bool,
}

fn shown_words(turn: &Turn, clean: bool) -> Vec<&Word> {
    if !clean {
        return turn.words.iter().collect();
    }
    let keep = crate::edit::clean_mask(&turn.words);
    turn.words.iter().zip(keep).filter(|(_, k)| *k).map(|(w, _)| w).collect()
}

/// Lay words into lines of at most `width` characters; `None` when they need more than `max` lines.
fn lay_out(words: &[&str], first_prefix: usize, width: usize, max: usize) -> Option<Vec<String>> {
    let mut lines: Vec<String> = vec![String::new()];
    for w in words {
        let n = lines.len();
        let cur = lines.last_mut().unwrap();
        let used = cur.chars().count() + if n == 1 { first_prefix } else { 0 };
        if cur.is_empty() {
            cur.push_str(w);
        } else if used + 1 + w.chars().count() <= width {
            cur.push(' ');
            cur.push_str(w);
        } else {
            if n == max {
                return None;
            }
            lines.push(w.to_string());
        }
    }
    Some(lines)
}

/// The cues for a transcript.
pub fn cues(t: &Transcript, o: &CaptionOptions, prefix_len: impl Fn(&str) -> usize) -> Vec<Cue> {
    let mut out: Vec<Cue> = Vec::new();
    for turn in &t.turns {
        let words = shown_words(turn, o.clean);
        if words.is_empty() {
            continue;
        }
        let name = t.speaker(&turn.speaker).map(|s| s.display_name().to_string()).unwrap_or_else(|| turn.speaker.clone());
        let speaker = o.speaker_names.then(|| name.clone());
        let mut first = true;
        let mut i = 0;
        while i < words.len() {
            let prefix = if first && o.speaker_names { prefix_len(&name) } else { 0 };
            let start = words[i].start;
            let mut j = i + 1;
            // Grow the cue word by word while it fits and no break is due.
            while j < words.len() {
                let texts: Vec<&str> = words[i..=j].iter().map(|w| w.text.as_str()).collect();
                let fits = lay_out(&texts, prefix, CAPTION_LINE, CAPTION_LINES).is_some();
                let too_long = words[j].end - start > MAX_CUE_SECONDS;
                let pause = words[j].start - words[j - 1].end >= PAUSE_BREAK;
                let sentence = words[j - 1].text.ends_with(['.', '?', '!']) && words[j - 1].end - start >= 1.5;
                if !fits || too_long || pause || sentence {
                    break;
                }
                j += 1;
            }
            let texts: Vec<&str> = words[i..j].iter().map(|w| w.text.as_str()).collect();
            // A single word longer than a line still gets its own cue.
            let lines = lay_out(&texts, prefix, CAPTION_LINE, CAPTION_LINES).unwrap_or_else(|| vec![texts.join(" ")]);
            out.push(Cue { start, end: words[j - 1].end.max(start), lines, speaker: speaker.clone(), turn_start: first });
            first = false;
            i = j;
        }
    }
    // Cues never overlap and are on screen long enough to read, where the next one leaves room.
    out.sort_by(|a, b| a.start.total_cmp(&b.start));
    for k in 0..out.len() {
        let next = out.get(k + 1).map(|c| c.start).unwrap_or(f64::INFINITY);
        let c = &mut out[k];
        if c.end - c.start < MIN_CUE_SECONDS {
            c.end = (c.start + MIN_CUE_SECONDS).min(next);
        }
        if c.end > next {
            c.end = next;
        }
    }
    out
}

fn srt_time(t: f64) -> String {
    crate::model::hms_ms(t).replace('.', ",")
}

pub fn to_srt(t: &Transcript, o: &CaptionOptions) -> String {
    let mut s = String::new();
    for (k, c) in cues(t, o, |n| n.chars().count() + 2).iter().enumerate() {
        s.push_str(&format!("{}\n{} --> {}\n", k + 1, srt_time(c.start), srt_time(c.end)));
        for (li, line) in c.lines.iter().enumerate() {
            if li == 0 && c.turn_start {
                if let Some(n) = &c.speaker {
                    s.push_str(n);
                    s.push_str(": ");
                }
            }
            s.push_str(line);
            s.push('\n');
        }
        s.push('\n');
    }
    s
}

fn vtt_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

pub fn to_vtt(t: &Transcript, o: &CaptionOptions) -> String {
    let mut s = String::from("WEBVTT\n\n");
    // The voice tag is markup, not shown text, so it does not count toward the line length.
    for c in cues(t, o, |_| 0) {
        s.push_str(&format!("{} --> {}\n", crate::model::hms_ms(c.start), crate::model::hms_ms(c.end)));
        for (li, line) in c.lines.iter().enumerate() {
            match (&c.speaker, li) {
                (Some(n), 0) => s.push_str(&format!("<v {}>{}", vtt_escape(n), vtt_escape(line))),
                _ => s.push_str(&vtt_escape(line)),
            }
            s.push('\n');
        }
        s.push('\n');
    }
    s
}

// ---------------------------------------------------------------------------------------------------------
// The table
// ---------------------------------------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub file: String,
    pub speaker: String,
    pub role: String,
    pub start: f64,
    pub end: f64,
    pub text: String,
}

pub fn role_word(r: Role) -> &'static str {
    match r {
        Role::Interviewer => "interviewer",
        Role::Moderator => "moderator",
        Role::Participant => "participant",
        Role::Other => "other",
    }
}

/// One row per turn of one file.
pub fn rows(label: &str, t: &Transcript, clean: bool) -> Vec<Row> {
    t.turns
        .iter()
        .filter_map(|turn| {
            let words = shown_words(turn, clean);
            if words.is_empty() {
                return None;
            }
            let sp = t.speaker(&turn.speaker);
            Some(Row {
                file: label.to_string(),
                speaker: sp.map(|s| s.display_name().to_string()).unwrap_or_else(|| turn.speaker.clone()),
                role: role_word(sp.map(|s| s.role).unwrap_or_default()).to_string(),
                start: turn.start,
                end: turn.end,
                text: words.iter().map(|w| w.text.as_str()).collect::<Vec<_>>().join(" "),
            })
        })
        .collect()
}

pub const TABLE_HEADER: [&str; 8] = ["file", "speaker", "role", "start", "end", "start_seconds", "end_seconds", "text"];

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

pub fn to_csv(rows: &[Row]) -> String {
    let mut s = String::from("\u{feff}");
    s.push_str(&TABLE_HEADER.join(","));
    s.push_str("\r\n");
    for r in rows {
        let cells = [
            csv_field(&r.file),
            csv_field(&r.speaker),
            r.role.clone(),
            crate::model::hms_ms(r.start),
            crate::model::hms_ms(r.end),
            format!("{:.3}", r.start),
            format!("{:.3}", r.end),
            csv_field(&r.text),
        ];
        s.push_str(&cells.join(","));
        s.push_str("\r\n");
    }
    s
}

fn xml(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            c if (c as u32) < 0x20 && c != '\t' && c != '\n' && c != '\r' => {}
            c => o.push(c),
        }
    }
    o
}

fn col(i: usize) -> char {
    (b'A' + i as u8) as char
}

/// The table as an Excel workbook: one sheet, a bold frozen header row, times as time values ([h]:mm:ss.000).
pub fn to_xlsx(rows: &[Row], title: &str) -> std::io::Result<Vec<u8>> {
    let mut sheet = String::new();
    sheet.push_str(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetViews><sheetView workbookViewId="0"><pane ySplit="1" topLeftCell="A2" activePane="bottomLeft" state="frozen"/></sheetView></sheetViews><cols><col min="1" max="1" width="16" customWidth="1"/><col min="2" max="2" width="18" customWidth="1"/><col min="3" max="3" width="12" customWidth="1"/><col min="4" max="5" width="13" customWidth="1"/><col min="6" max="7" width="13" customWidth="1"/><col min="8" max="8" width="90" customWidth="1"/></cols><sheetData>"#);
    let text_cell = |r: usize, c: usize, s: &str, style: u32| format!(r#"<c r="{}{}" t="inlineStr" s="{style}"><is><t xml:space="preserve">{}</t></is></c>"#, col(c), r, xml(s));
    let num_cell = |r: usize, c: usize, v: f64, style: u32| format!(r#"<c r="{}{}" s="{style}"><v>{}</v></c>"#, col(c), r, v);
    sheet.push_str(r#"<row r="1">"#);
    for (c, h) in TABLE_HEADER.iter().enumerate() {
        sheet.push_str(&text_cell(1, c, h, 1));
    }
    sheet.push_str("</row>");
    for (k, r) in rows.iter().enumerate() {
        let n = k + 2;
        sheet.push_str(&format!(r#"<row r="{n}">"#));
        sheet.push_str(&text_cell(n, 0, &r.file, 0));
        sheet.push_str(&text_cell(n, 1, &r.speaker, 0));
        sheet.push_str(&text_cell(n, 2, &r.role, 0));
        sheet.push_str(&num_cell(n, 3, r.start / 86_400.0, 2));
        sheet.push_str(&num_cell(n, 4, r.end / 86_400.0, 2));
        sheet.push_str(&num_cell(n, 5, (r.start * 1000.0).round() / 1000.0, 3));
        sheet.push_str(&num_cell(n, 6, (r.end * 1000.0).round() / 1000.0, 3));
        sheet.push_str(&text_cell(n, 7, &r.text, 4));
        sheet.push_str("</row>");
    }
    sheet.push_str(&format!(r#"</sheetData><autoFilter ref="A1:H{}"/></worksheet>"#, rows.len() + 1));
    let styles = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><numFmts count="2"><numFmt numFmtId="164" formatCode="[h]:mm:ss.000"/><numFmt numFmtId="165" formatCode="0.000"/></numFmts><fonts count="2"><font><sz val="11"/><name val="Calibri"/></font><font><b/><sz val="11"/><name val="Calibri"/></font></fonts><fills count="2"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill></fills><borders count="1"><border><left/><right/><top/><bottom/><diagonal/></border></borders><cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs><cellXfs count="5"><xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/><xf numFmtId="0" fontId="1" fillId="0" borderId="0" xfId="0" applyFont="1"/><xf numFmtId="164" fontId="0" fillId="0" borderId="0" xfId="0" applyNumberFormat="1"/><xf numFmtId="165" fontId="0" fillId="0" borderId="0" xfId="0" applyNumberFormat="1"/><xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0" applyAlignment="1"><alignment wrapText="1" vertical="top"/></xf></cellXfs><cellStyles count="1"><cellStyle name="Normal" xfId="0" builtinId="0"/></cellStyles></styleSheet>"#;
    let sheet_name = xml(&title.chars().filter(|c| !"[]:*?/\\".contains(*c)).take(31).collect::<String>()).trim().to_string();
    let sheet_name = if sheet_name.is_empty() { "Transcript".to_string() } else { sheet_name };
    let parts: [(&str, String); 7] = [
        ("[Content_Types].xml", r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/><Override PartName="/docProps/core.xml" ContentType="application/vnd.openxmlformats-package.core-properties+xml"/></Types>"#.into()),
        ("_rels/.rels", r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties" Target="docProps/core.xml"/></Relationships>"#.into()),
        ("xl/workbook.xml", format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="{sheet_name}" sheetId="1" r:id="rId1"/></sheets><definedNames><definedName name="_xlnm._FilterDatabase" localSheetId="0" hidden="1">'{}'!$A$1:$H${}</definedName></definedNames></workbook>"#, sheet_name.replace('\'', "''"), rows.len() + 1)),
        ("xl/_rels/workbook.xml.rels", r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"#.into()),
        ("xl/worksheets/sheet1.xml", sheet),
        ("xl/styles.xml", styles.into()),
        ("docProps/core.xml", format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>{}</dc:title><dc:creator>Prism Transcribe</dc:creator></cp:coreProperties>"#, xml(title))),
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
    use crate::model::{Speaker, Word};
    use std::io::Read;

    fn talk(spec: &[(&str, &str, f64)]) -> Transcript {
        // (speaker, text, start): words 0.3 s long, 0.05 s apart.
        let mut t = Transcript::new("F001");
        t.speakers = vec![
            Speaker { id: "S1".into(), label: "Speaker 1".into(), name: Some("Interviewer".into()), role: Role::Interviewer, attributes: Default::default() },
            Speaker { id: "S2".into(), label: "Speaker 2".into(), name: Some("P07".into()), role: Role::Participant, attributes: Default::default() },
        ];
        for (sp, text, at) in spec {
            let mut x = *at;
            let words: Vec<Word> = text
                .split_whitespace()
                .map(|w| {
                    let wd = Word { text: w.into(), start: x, end: x + 0.3, confidence: 0.9, speaker: sp.to_string() };
                    x += 0.35;
                    wd
                })
                .collect();
            t.turns.push(Turn { speaker: sp.to_string(), start: words[0].start, end: words.last().unwrap().end, words });
        }
        t
    }

    #[test]
    fn captions_keep_to_two_lines_of_42() {
        let long = "So when the programme moved online the first thing that changed was office hours, because students simply stopped coming to them at all, and nobody quite knew why.";
        let t = talk(&[("S1", "How did it change?", 1.0), ("S2", long, 3.0)]);
        let c = cues(&t, &CaptionOptions::default(), |_| 0);
        assert!(c.len() >= 3, "{c:#?}");
        for cue in &c {
            assert!(cue.lines.len() <= 2 && cue.lines.iter().all(|l| l.chars().count() <= 42), "{cue:?}");
            assert!(cue.end - cue.start <= 7.0 + 1e-9 && cue.end > cue.start);
        }
        assert!(c.windows(2).all(|p| p[0].end <= p[1].start + 1e-9), "no overlap");
        // Every word is captioned once, in order.
        let all: Vec<String> = c.iter().flat_map(|x| x.lines.join(" ").split(' ').map(String::from).collect::<Vec<_>>()).collect();
        let want: Vec<&str> = "How did it change?".split(' ').chain(long.split(' ')).collect();
        assert_eq!(all, want);
    }

    #[test]
    fn srt_and_vtt_layout() {
        let t = talk(&[("S1", "How did it change?", 1.0), ("S2", "Office hours. [laughter]", 3.0)]);
        let o = CaptionOptions { speaker_names: true, clean: false };
        let srt = to_srt(&t, &o);
        assert!(srt.starts_with("1\n00:00:01,000 --> 00:00:02,350\nInterviewer: How did it change?\n\n2\n00:00:03,000 --> "), "{srt}");
        assert!(srt.contains("P07: Office hours. [laughter]\n") || srt.contains("P07: Office hours.\n"), "{srt}");
        let vtt = to_vtt(&t, &o);
        assert!(vtt.starts_with("WEBVTT\n\n00:00:01.000 --> 00:00:02.350\n<v Interviewer>How did it change?\n"), "{vtt}");
        let plain = to_srt(&t, &CaptionOptions::default());
        assert!(!plain.contains("Interviewer:"));
    }

    #[test]
    fn a_pause_starts_a_new_cue() {
        let mut t = talk(&[("S2", "Well I think", 1.0)]);
        t.turns[0].words[2].start = 4.0;
        t.turns[0].words[2].end = 4.3;
        let c = cues(&t, &CaptionOptions::default(), |_| 0);
        assert_eq!(c.iter().map(|x| x.lines.join(" ")).collect::<Vec<_>>(), ["Well I", "think"]);
    }

    #[test]
    fn the_table_as_csv_and_xlsx() {
        let mut t = talk(&[("S1", "Um, how, \"exactly\"?", 61.5), ("S2", "Fine.", 64.0)]);
        t.speakers[1].attributes.insert("site".into(), "North".into());
        let r = rows("INT-07", &t, false);
        assert_eq!(r[0].role, "interviewer");
        let csv = to_csv(&r);
        assert!(csv.starts_with("\u{feff}file,speaker,role,start,end,start_seconds,end_seconds,text\r\n"));
        assert!(csv.contains("INT-07,Interviewer,interviewer,00:01:01.500,00:01:02.500,61.500,62.500,\"Um, how, \"\"exactly\"\"?\"\r\n"), "{csv}");
        assert!(rows("INT-07", &t, true)[0].text.starts_with("how,"), "clean drops the filler");
        let x = to_xlsx(&r, "INT-07").unwrap();
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(x)).unwrap();
        let mut sheet = String::new();
        z.by_name("xl/worksheets/sheet1.xml").unwrap().read_to_string(&mut sheet).unwrap();
        assert!(sheet.contains("<t xml:space=\"preserve\">Um, how, &quot;exactly&quot;?</t>"));
        assert!(sheet.contains(&format!("<v>{}</v>", 61.5 / 86_400.0)), "a time value");
        for part in ["[Content_Types].xml", "xl/workbook.xml", "xl/styles.xml", "xl/_rels/workbook.xml.rels", "_rels/.rels"] {
            assert!(z.by_name(part).is_ok(), "{part}");
        }
    }
}
