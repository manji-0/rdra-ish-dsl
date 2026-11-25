//! The fingerprint line of the files `codegen` writes (docs/design/code-check.md): a hash of
//! the model's source text and one of the file's body. The CLI compares a generated file with
//! what the model generates now; the LSP, which generates nothing, compares only the model
//! hash, so both live here.

use std::io::BufRead;

/// What follows the comment marker (`//` in code, `#` in a traces file) on a fingerprint line.
pub const FINGERPRINT: &str = "rdra-ish codegen fingerprint:";

/// FNV-1a, 64 bits: it tells a changed text, not a forged one.
#[derive(Clone, Copy, Debug)]
pub struct Fnv(u64);

impl Default for Fnv {
    fn default() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }
}

impl Fnv {
    pub fn add(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.0 = (self.0 ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3);
        }
    }

    /// As 16 hex digits.
    pub fn hex(self) -> String {
        format!("{:016x}", self.0)
    }
}

/// The hash of `bytes`, as 16 hex digits.
pub fn hash(bytes: &[u8]) -> String {
    let mut h = Fnv::default();
    h.add(bytes);
    h.hex()
}

/// The hash of a model's source texts: `\r\n` read as `\n`, the texts sorted, so the load order
/// (which the CLI and the LSP reach differently) does not count, and joined by NUL. The
/// `model=` of a fingerprint.
pub fn model_hash<'a>(texts: impl IntoIterator<Item = &'a str>) -> String {
    let mut texts: Vec<String> = texts.into_iter().map(|t| t.replace("\r\n", "\n")).collect();
    texts.sort();
    hash(texts.join("\u{0}").as_bytes())
}

/// What a fingerprint line records.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fingerprint {
    /// The hash of the model's source text the file was generated from.
    pub model: String,
    /// The hash of the file's body, without the fingerprint line.
    pub code: String,
}

/// Whether `line` is a fingerprint line, after any comment marker.
pub fn is_fingerprint_line(line: &str) -> bool {
    let rest = line.trim_start_matches(['/', '#']).trim_start();
    rest.starts_with(FINGERPRINT)
}

/// The fingerprint of the line `line`, if it is one.
pub fn parse_line(line: &str) -> Option<Fingerprint> {
    if !is_fingerprint_line(line) {
        return None;
    }
    let rest = line.trim_start_matches(['/', '#']).trim_start();
    let mut model = None;
    let mut code = None;
    for word in rest[FINGERPRINT.len()..].split_whitespace() {
        if let Some(v) = word.strip_prefix("model=") {
            model = Some(v.to_string());
        } else if let Some(v) = word.strip_prefix("code=") {
            code = Some(v.to_string());
        }
    }
    Some(Fingerprint {
        model: model?,
        code: code?,
    })
}

/// The first fingerprint line among the first lines of `text`.
pub fn find(text: &str) -> Option<Fingerprint> {
    text.lines().take(8).find_map(parse_line)
}

/// The fingerprint of a file and the hash of its body as [`line_hash`] reads it, in one pass
/// that never holds the whole file: for a traces file, which can be hundreds of megabytes.
pub fn scan_file(path: &std::path::Path) -> std::io::Result<(Option<Fingerprint>, String)> {
    let mut reader = std::io::BufReader::new(std::fs::File::open(path)?);
    let mut found = None;
    let mut h = Fnv::default();
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        let text = line.trim_end_matches(['\n', '\r']);
        if is_fingerprint_line(text) {
            if found.is_none() {
                found = parse_line(text);
            }
            continue;
        }
        h.add(text.as_bytes());
        h.add(b"\n");
    }
    Ok((found, h.hex()))
}

/// The hash of `text` line by line, each line ended by `\n`, the fingerprint line left out:
/// what [`scan_file`] computes of a file holding `text`.
pub fn line_hash(text: &str) -> String {
    let mut h = Fnv::default();
    for line in text.lines().filter(|l| !is_fingerprint_line(l)) {
        h.add(line.as_bytes());
        h.add(b"\n");
    }
    h.hex()
}

/// The fingerprint of the file at `path`, read from its first lines only.
pub fn read_file(path: &std::path::Path) -> std::io::Result<Option<Fingerprint>> {
    let reader = std::io::BufReader::new(std::fs::File::open(path)?);
    for line in reader.lines().take(8) {
        if let Some(f) = parse_line(&line?) {
            return Ok(Some(f));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_reads_in_both_comment_styles() {
        let want = Some(Fingerprint {
            model: "00ff".to_string(),
            code: "abcd".to_string(),
        });
        assert_eq!(
            parse_line("// rdra-ish codegen fingerprint: model=00ff code=abcd"),
            want
        );
        assert_eq!(
            parse_line("# rdra-ish codegen fingerprint: model=00ff code=abcd"),
            want
        );
        assert_eq!(parse_line("# rdra-ish conformance traces 3"), None);
    }

    #[test]
    fn the_streamed_hash_is_the_text_hash() {
        let text = "# rdra-ish conformance traces 3\n# rdra-ish codegen fingerprint: model=1 code=2\nT 1\r\nC 2 Pay 1\n";
        let path = std::env::temp_dir().join(format!("rdra-fp-{}.txt", std::process::id()));
        std::fs::write(&path, text).unwrap();
        let (found, streamed) = scan_file(&path).unwrap();
        std::fs::remove_file(&path).ok();
        assert_eq!(streamed, line_hash(text));
        assert_eq!(found.map(|f| f.code), Some("2".to_string()));
        assert_eq!(
            line_hash(text),
            line_hash("# rdra-ish conformance traces 3\nT 1\nC 2 Pay 1")
        );
    }

    #[test]
    fn the_model_hash_reads_crlf_as_lf() {
        assert_eq!(model_hash(["a\r\nb", "c"]), model_hash(["a\nb", "c"]));
        assert_ne!(model_hash(["ab", "c"]), model_hash(["a", "bc"]));
    }
}
