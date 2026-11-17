//! SigmaOS-apt-show-source — Debian Sources-index parser & lookup.
//!
//! Inspired by Debian's `apt-show-source`: parses the `Sources` index
//! (source-package records: `Package`, `Version`, `Directory`, `Files` with
//! per-file checksums, `Homepage`, maintainer fields) and answers "show me
//! the source for package X" queries.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::collections::BTreeMap;

/// Errors raised by this component.
#[derive(Debug, thiserror::Error)]
pub enum ComponentError {
    /// A stanza was missing its `Package` field.
    #[error("stanza missing Package field")]
    MissingPackage,
}

/// Result alias for component operations.
pub type ComponentResult<T> = Result<T, ComponentError>;

/// One source file entry inside a `Files:` block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFile {
    /// Filename (e.g. `bash_5.2.orig.tar.xz`).
    pub name: String,
    /// Size in bytes.
    pub size: u64,
    /// MD5 hex digest.
    pub md5: String,
    /// SHA1 hex digest.
    pub sha1: String,
    /// SHA256 hex digest.
    pub sha256: String,
}

/// One parsed `Sources` stanza.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SourcePkg {
    /// Source package name.
    pub package: String,
    /// Version.
    pub version: String,
    /// Pool directory (e.g. `pool/main/b/bash`).
    pub directory: String,
    /// Homepage URL.
    pub homepage: String,
    /// Maintainer field.
    pub maintainer: String,
    /// Binary packages built from this source.
    pub binaries: Vec<String>,
    /// Source files with checksums.
    pub files: Vec<SourceFile>,
}

/// Parsed `Sources` index.
#[derive(Debug, Default)]
pub struct SourcesIndex {
    by_name: BTreeMap<String, Vec<SourcePkg>>,
}

impl SourcesIndex {
    /// Empty index.
    pub fn new() -> Self {
        Self::default()
    }

    /// Parse `Sources` text. Stanzas separated by blank lines; `Key: value`
    /// fields; `Files:`/`Binaries:` are space-separated continuation lists.
    pub fn parse(input: &str) -> ComponentResult<Self> {
        let mut idx = SourcesIndex::new();
        let mut cur = SourcePkg::default();
        let mut have_pkg = false;
        let mut section = String::new(); // "", "files", "binaries"
        let mut pending_file: Option<SourceFile> = None;

        macro_rules! flush_file {
            () => {
                if let Some(f) = pending_file.take() { cur.files.push(f); }
            };
        }
        macro_rules! flush {
            () => {
                if have_pkg {
                    flush_file!();
                    idx.by_name.entry(cur.package.clone()).or_default().push(std::mem::take(&mut cur));
                    have_pkg = false;
                }
                section.clear();
            };
        }

        for raw in input.lines() {
            let line = raw.trim_end();
            if line.trim().is_empty() {
                flush!();
                continue;
            }
            if line.starts_with(' ') || line.starts_with('\t') {
                // Continuation line.
                let t = line.trim();
                match section.as_str() {
                    "files" => {
                        // A file stanza continuation adds fields to pending_file
                        // OR starts a new file entry "name size md5 sha1 sha256".
                        let toks: Vec<&str> = t.split_whitespace().collect();
                        if toks.len() >= 5 && toks[1].chars().all(|c| c.is_ascii_digit()) {
                            flush_file!();
                            pending_file = Some(SourceFile {
                                name: toks[0].to_string(),
                                size: toks[1].parse().unwrap_or(0),
                                md5: toks[2].to_string(),
                                sha1: toks[3].to_string(),
                                sha256: toks[4].to_string(),
                            });
                        } else if let Some(f) = pending_file.as_mut() {
                            // continuation of checksum fields (rare)
                            let _ = f;
                        }
                    }
                    "binaries" => {
                        cur.binaries.extend(t.split_whitespace().map(|s| s.to_string()));
                    }
                    _ => {}
                }
                continue;
            }
            // New field.
            let Some((key, val)) = line.split_once(':') else { continue };
            let key = key.trim();
            let val = val.trim();
            match key {
                "Package" => { flush!(); cur.package = val.to_string(); have_pkg = true; }
                "Version" => { cur.version = val.to_string(); }
                "Directory" => { cur.directory = val.to_string(); }
                "Homepage" => { cur.homepage = val.to_string(); }
                "Maintainer" => { cur.maintainer = val.to_string(); }
                "Files" => { section = "files".into(); }
                "Binaries" => {
                    cur.binaries.extend(val.split_whitespace().map(|x| x.to_string()));
                    section = "binaries".into();
                }
                _ => { section.clear(); }
            }
        }
        flush!();
        Ok(idx)
    }

    /// All versions of source package `name`.
    pub fn get(&self, name: &str) -> &[SourcePkg] {
        self.by_name.get(name).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Find the source package that builds binary `bin`.
    pub fn source_of_binary(&self, bin: &str) -> Option<&SourcePkg> {
        self.by_name.values().flatten().find(|s| s.binaries.iter().any(|b| b == bin))
    }

    /// All source package names (sorted).
    pub fn names(&self) -> Vec<&str> {
        self.by_name.keys().map(String::as_str).collect()
    }

    /// Number of source packages.
    pub fn len(&self) -> usize {
        self.by_name.values().map(Vec::len).sum()
    }

    /// Whether empty.
    pub fn is_empty(&self) -> bool {
        self.by_name.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = "Package: bash\nVersion: 5.2.15-2\nDirectory: pool/main/b/bash\nHomepage: https://www.gnu.org/software/bash/\nMaintainer: Chet <chet@gnu.org>\nBinaries: bash bash-static bash-doc\nFiles:\n bash_5.2.15.orig.tar.gz 12345 abc def 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\n bash_5.2.15-2.debian.tar.xz 999 aaa bbb ccc\n\nPackage: zlib\nVersion: 1.2.13.dfsg-1\nDirectory: pool/main/z/zlib\nBinaries: zlib1g\n";

    #[test]
    fn parses_source_stanzas() {
        let idx = SourcesIndex::parse(SRC).unwrap();
        assert_eq!(idx.len(), 2);
        let b = &idx.get("bash")[0];
        assert_eq!(b.version, "5.2.15-2");
        assert_eq!(b.directory, "pool/main/b/bash");
        assert_eq!(b.binaries, vec!["bash", "bash-static", "bash-doc"]);
    }

    #[test]
    fn parses_files_with_checksums() {
        let idx = SourcesIndex::parse(SRC).unwrap();
        let b = &idx.get("bash")[0];
        assert_eq!(b.files.len(), 2);
        assert_eq!(b.files[0].name, "bash_5.2.15.orig.tar.gz");
        assert_eq!(b.files[0].size, 12345);
        assert_eq!(b.files[0].sha256.len(), 64);
    }

    #[test]
    fn source_of_binary_lookup() {
        let idx = SourcesIndex::parse(SRC).unwrap();
        assert_eq!(idx.source_of_binary("bash").unwrap().package, "bash");
        assert_eq!(idx.source_of_binary("zlib1g").unwrap().package, "zlib");
        assert!(idx.source_of_binary("nonexistent").is_none());
    }

    #[test]
    fn names_sorted() {
        let idx = SourcesIndex::parse(SRC).unwrap();
        assert_eq!(idx.names(), vec!["bash", "zlib"]);
    }

    #[test]
    fn empty_ok() {
        assert!(SourcesIndex::parse("").unwrap().is_empty());
    }
}
