//! Small filesystem helpers shared across persistence modules.

use std::path::{Path, PathBuf};

/// The user's home directory, if the platform can tell us.
fn home_dir() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|d| d.home_dir().to_path_buf())
}

/// A path for display: the home directory prefix shown as `~`, so a long
/// absolute path fits a text field with its telling tail visible.
pub fn collapse_home(path: &Path) -> String {
    if let Some(home) = home_dir()
        && let Ok(rest) = path.strip_prefix(&home)
    {
        if rest.as_os_str().is_empty() {
            return "~".to_owned();
        }
        return format!("~/{}", rest.to_string_lossy());
    }
    path.to_string_lossy().into_owned()
}

/// The inverse of [`collapse_home`] for user-typed text: a leading `~` (alone
/// or `~/…`) becomes the home directory; anything else is taken literally.
pub fn expand_home(text: &str) -> PathBuf {
    let text = text.trim();
    if let Some(home) = home_dir() {
        if text == "~" {
            return home;
        }
        if let Some(rest) = text.strip_prefix("~/") {
            return home.join(rest);
        }
    }
    PathBuf::from(text)
}

/// Write `bytes` to `path` atomically: create parent directories, write to a
/// unique per-process temp file alongside the target, then rename it into place.
/// A crash mid-write can therefore never leave a half-written (corrupt) file at
/// `path` — the reader sees either the old contents or the complete new ones.
///
/// The temp file is `<filename>.tmp.<pid>` in the same directory, so the final
/// rename stays on one filesystem (a cross-device rename would fail). On Windows
/// `rename` replaces the destination atomically only when it is not held open
/// with a share mode that forbids replace; callers here never keep these files
/// open across calls, so that holds.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let Some(file_name) = path.file_name() else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "path has no file name",
        ));
    };
    let mut tmp_name = file_name.to_os_string();
    tmp_name.push(format!(".tmp.{}", std::process::id()));
    let tmp = path.with_file_name(tmp_name);

    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_dir(tag: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("campfire-fsutil-{tag}-{}", std::process::id()));
        p
    }

    #[test]
    fn writes_content_and_creates_parent_dirs() {
        let dir = temp_dir("write");
        let path = dir.join("nested").join("out.txt");
        write_atomic(&path, b"hello").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "hello");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn overwrites_existing_and_leaves_no_temp_file() {
        let dir = temp_dir("overwrite");
        let path = dir.join("out.txt");
        write_atomic(&path, b"first").unwrap();
        write_atomic(&path, b"second").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "second");

        let mut tmp_name = path.file_name().unwrap().to_os_string();
        tmp_name.push(format!(".tmp.{}", std::process::id()));
        assert!(!path.with_file_name(tmp_name).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod home_tests {
    use super::*;

    #[test]
    fn home_collapses_and_expands_back() {
        let home = home_dir().expect("home dir");
        let inside = home.join("work").join("api");
        assert_eq!(collapse_home(&inside), "~/work/api");
        assert_eq!(expand_home("~/work/api"), inside);
        assert_eq!(collapse_home(&home), "~");
        assert_eq!(expand_home("~"), home);
    }

    #[test]
    fn paths_outside_home_are_untouched() {
        let outside = Path::new("/opt/srv");
        assert_eq!(collapse_home(outside), "/opt/srv");
        assert_eq!(expand_home("/opt/srv"), PathBuf::from("/opt/srv"));
        assert_eq!(expand_home("~user/x"), PathBuf::from("~user/x")); // not ours to expand
    }
}
