use std::{path::PathBuf, time::SystemTime};

use regex::RegexBuilder;

/// Options for a local filesystem find-files walk.
#[derive(Clone, Debug)]
pub struct SearchOptions {
    pub root: PathBuf,
    /// Include pattern matched against the entry name only.
    /// Empty pattern matches every name.
    pub name_mask: String,
    /// Semicolon/comma-separated exclude patterns (name only).
    /// Matching directories are skipped entirely (not entered).
    pub exclude_mask: String,
    pub include_subfolders: bool,
    pub case_sensitive: bool,
    pub use_regex: bool,
    pub show_hidden_files: bool,
}

/// One filesystem hit from a find-files walk.
#[derive(Clone, Debug)]
pub struct SearchHit {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
    pub size_bytes: u64,
    pub modified_at: Option<SystemTime>,
}

/// Compiled include/exclude matchers for a search run.
#[derive(Clone, Debug)]
pub struct CompiledSearch {
    include: Pattern,
    excludes: Vec<Pattern>,
}

#[derive(Clone, Debug)]
enum Pattern {
    Any,
    Glob { mask: String, case_sensitive: bool },
    Regex(regex::Regex),
}

impl CompiledSearch {
    pub fn matches_include(&self, name: &str) -> bool {
        self.include.matches(name)
    }

    pub fn matches_exclude(&self, name: &str) -> bool {
        self.excludes.iter().any(|pattern| pattern.matches(name))
    }
}

impl Pattern {
    fn matches(&self, name: &str) -> bool {
        match self {
            Self::Any => true,
            Self::Glob {
                mask,
                case_sensitive,
            } => name_matches_mask(name, mask, *case_sensitive),
            Self::Regex(regex) => regex.is_match(name),
        }
    }
}

/// Compile include/exclude patterns from dialog options.
pub fn compile_search(options: &SearchOptions) -> Result<CompiledSearch, String> {
    let include = compile_pattern(
        options.name_mask.trim(),
        options.use_regex,
        options.case_sensitive,
    )?;
    let mut excludes = Vec::new();
    for part in split_exclude_patterns(&options.exclude_mask) {
        excludes.push(compile_pattern(
            part,
            options.use_regex,
            options.case_sensitive,
        )?);
    }
    Ok(CompiledSearch { include, excludes })
}

fn compile_pattern(
    pattern: &str,
    use_regex: bool,
    case_sensitive: bool,
) -> Result<Pattern, String> {
    if pattern.is_empty() {
        return Ok(Pattern::Any);
    }
    if use_regex {
        RegexBuilder::new(pattern)
            .case_insensitive(!case_sensitive)
            .build()
            .map(Pattern::Regex)
            .map_err(|error| error.to_string())
    } else {
        Ok(Pattern::Glob {
            mask: pattern.to_string(),
            case_sensitive,
        })
    }
}

fn split_exclude_patterns(raw: &str) -> Vec<&str> {
    raw.split([';', ','])
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect()
}

/// Match `name` against a simple wildcard mask (`*` any sequence, `?` one char).
/// An empty mask matches everything.
pub fn name_matches_mask(name: &str, mask: &str, case_sensitive: bool) -> bool {
    if mask.is_empty() {
        return true;
    }
    if case_sensitive {
        wildcard_match(name.as_bytes(), mask.as_bytes())
    } else {
        let name = name.to_lowercase();
        let mask = mask.to_lowercase();
        wildcard_match(name.as_bytes(), mask.as_bytes())
    }
}

fn wildcard_match(name: &[u8], mask: &[u8]) -> bool {
    let mut name_i = 0usize;
    let mut mask_i = 0usize;
    let mut star_mask = None::<usize>;
    let mut star_name = 0usize;

    while name_i < name.len() {
        if mask_i < mask.len() && (mask[mask_i] == b'?' || mask[mask_i] == name[name_i]) {
            name_i += 1;
            mask_i += 1;
            continue;
        }
        if mask_i < mask.len() && mask[mask_i] == b'*' {
            star_mask = Some(mask_i);
            star_name = name_i;
            mask_i += 1;
            continue;
        }
        if let Some(star) = star_mask {
            mask_i = star + 1;
            star_name += 1;
            name_i = star_name;
            continue;
        }
        return false;
    }

    while mask_i < mask.len() && mask[mask_i] == b'*' {
        mask_i += 1;
    }
    mask_i == mask.len()
}

#[cfg(test)]
mod tests {
    use super::{compile_search, name_matches_mask, SearchOptions};
    use std::path::PathBuf;

    fn options(name: &str, exclude: &str, use_regex: bool, case_sensitive: bool) -> SearchOptions {
        SearchOptions {
            root: PathBuf::from("."),
            name_mask: name.into(),
            exclude_mask: exclude.into(),
            include_subfolders: true,
            case_sensitive,
            use_regex,
            show_hidden_files: true,
        }
    }

    #[test]
    fn empty_mask_matches_all() {
        assert!(name_matches_mask("anything.rs", "", false));
    }

    #[test]
    fn star_and_question() {
        assert!(name_matches_mask("foo.rs", "*.rs", true));
        assert!(name_matches_mask("foo.rs", "???.rs", true));
        assert!(!name_matches_mask("fo.rs", "???.rs", true));
        assert!(name_matches_mask("report2024.txt", "report*", true));
    }

    #[test]
    fn case_sensitivity() {
        assert!(name_matches_mask("Foo.RS", "*.rs", false));
        assert!(!name_matches_mask("Foo.RS", "*.rs", true));
    }

    #[test]
    fn regex_and_exclude() {
        let compiled = compile_search(&options(r".*\.rs$", "target;node_modules", true, false))
            .expect("compile");
        assert!(compiled.matches_include("main.rs"));
        assert!(!compiled.matches_include("main.txt"));
        assert!(compiled.matches_exclude("target"));
        assert!(compiled.matches_exclude("node_modules"));
        assert!(!compiled.matches_exclude("src"));
    }
}
