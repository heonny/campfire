//! Conservative, local-only detection of a module's root Go executable.

use regex_lite::Regex;
use std::path::Path;
use std::sync::LazyLock;

pub struct GoProject {
    pub runnable: bool,
}

pub fn detect(dir: &Path) -> Option<GoProject> {
    if !dir.join("go.mod").is_file() {
        return None;
    }
    let runnable = std::fs::read_dir(dir).ok().is_some_and(|entries| {
        entries.filter_map(Result::ok).any(|entry| {
            entry.path().is_file()
                && entry.file_name().to_str().is_some_and(unconditional_source)
                && std::fs::read_to_string(entry.path()).is_ok_and(|source| has_main(&source))
        })
    });
    Some(GoProject { runnable })
}

fn unconditional_source(name: &str) -> bool {
    let Some(stem) = name.strip_suffix(".go") else {
        return false;
    };
    if name.starts_with(['.', '_']) || stem.ends_with("_test") {
        return false;
    }
    // Avoid guessing GOOS/GOARCH or build flags from the host running Campfire.
    let suffix = stem.rsplit_once('_').map(|(_, suffix)| suffix);
    !matches!(
        suffix,
        Some(
            "aix"
                | "android"
                | "darwin"
                | "dragonfly"
                | "freebsd"
                | "hurd"
                | "illumos"
                | "ios"
                | "js"
                | "linux"
                | "nacl"
                | "netbsd"
                | "openbsd"
                | "plan9"
                | "solaris"
                | "wasip1"
                | "windows"
                | "zos"
                | "386"
                | "amd64"
                | "amd64p32"
                | "arm"
                | "arm64"
                | "arm64be"
                | "armbe"
                | "loong64"
                | "mips"
                | "mips64"
                | "mips64le"
                | "mipsle"
                | "ppc"
                | "ppc64"
                | "ppc64le"
                | "riscv"
                | "riscv64"
                | "s390"
                | "s390x"
                | "sparc"
                | "sparc64"
                | "wasm"
        )
    )
}

fn has_main(source: &str) -> bool {
    if source.lines().any(|line| {
        let line = line.trim();
        line.starts_with("//go:build") || line.starts_with("// +build")
    }) {
        return false;
    }
    static NON_CODE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"(?s)//[^\n]*|/\*.*?\*/|"(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*'|`[^`]*`"#)
            .expect("static Go comment and literal pattern")
    });
    static PACKAGE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"\A\s*package\s+main\s*(?:;|\n)").expect("static Go package pattern")
    });
    static MAIN: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"\bfunc\s+main\s*\(\s*\)\s*\{").expect("static Go entry point pattern")
    });
    let code = NON_CODE.replace_all(source, " ");
    PACKAGE.is_match(&code) && MAIN.is_match(&code)
}
