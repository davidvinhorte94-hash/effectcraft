//! Where the Android app keeps its files.
//!
//! Android gives every app a private folder (`internal_data_path`) and, on most devices, an
//! app-specific folder on shared storage (`external_data_path`, `Android/data/<package>/files`).
//! The second one needs no permission, a computer reaches it over USB and file managers can open
//! it. Settings and auto-saves stay in the private folder; the folders people work with
//! (`Projects`, `Media`, `Exports`) go on shared storage when it exists.
//!
//! The system file picker needs JNI and is not wired yet, so the host's pick hooks resolve to
//! these folders: Open picks the newest project, Import takes the media in `Media`, saving writes
//! into `Projects` and `Exports`.
//!
//! Everything here is `std` only, so it is unit-tested on any host.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Project file extensions (the desktop app's Open dialog offers both).
const PROJECT_EXTENSIONS: &[&str] = &["ecproj", "ecprojx"];
/// What a project is called when the UI offers no usable name.
const FALLBACK_PROJECT_NAME: &str = "Untitled Project.ecproj";
/// What an export is called when the UI offers no usable name.
const FALLBACK_EXPORT_NAME: &str = "Export";
/// Longer names are not file names a phone's file system keeps (255 bytes, with room for UTF-8).
const MAX_NAME_CHARS: usize = 120;

/// The folders the app uses, resolved once at startup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    /// Settings, shortcut presets, auto-saves, installed models (always the private folder).
    pub config: PathBuf,
    /// Saved projects.
    pub projects: PathBuf,
    /// Footage to import.
    pub media: PathBuf,
    /// Render Queue outputs and other exported files.
    pub exports: PathBuf,
}

impl Layout {
    /// `private`: the app's own folder. `shared`: its folder on shared storage, when the device
    /// has one; the user-facing folders fall back to `private` without it.
    pub fn new(private: &Path, shared: Option<&Path>) -> Layout {
        let user = shared.unwrap_or(private);
        Layout { config: private.join("config"), projects: user.join("Projects"), media: user.join("Media"), exports: user.join("Exports") }
    }

    /// Creates the folders. Returns one message for each that could not be created; the app
    /// still starts, and anything that needs such a folder reports its own error.
    pub fn create_dirs(&self) -> Vec<String> {
        [&self.config, &self.projects, &self.media, &self.exports]
            .into_iter()
            .filter_map(|dir| std::fs::create_dir_all(dir).err().map(|e| format!("{}: {e}", dir.display())))
            .collect()
    }

    /// Where Save writes the project the UI calls `name`.
    pub fn project_path(&self, name: &str) -> String {
        path_string(&self.projects.join(safe_file_name(name, FALLBACK_PROJECT_NAME)))
    }

    /// Where an export called `name` goes (a bare name, or a path whose folder is ignored).
    pub fn export_path(&self, name: &str) -> String {
        path_string(&self.exports.join(safe_file_name(name, FALLBACK_EXPORT_NAME)))
    }

    /// The most recently modified project in `Projects`, the newest path winning a tie.
    pub fn newest_project(&self) -> Option<PathBuf> {
        let mut best: Option<(SystemTime, PathBuf)> = None;
        for entry in std::fs::read_dir(&self.projects).ok()?.flatten() {
            let path = entry.path();
            if !has_extension(&path, PROJECT_EXTENSIONS) {
                continue;
            }
            let Ok(meta) = entry.metadata() else { continue };
            if !meta.is_file() {
                continue;
            }
            let modified = meta.modified().unwrap_or(UNIX_EPOCH);
            if best.as_ref().is_none_or(|(t, p)| (modified, &path) > (*t, p)) {
                best = Some((modified, path));
            }
        }
        best.map(|(_, path)| path)
    }

    /// The files in `Media` (not its subfolders) with one of `exts`, any file when `exts` is
    /// empty, sorted by path. Extensions match ignoring case and a leading dot.
    pub fn media_files(&self, exts: &[&str]) -> Vec<String> {
        let Ok(dir) = std::fs::read_dir(&self.media) else { return Vec::new() };
        let mut files: Vec<PathBuf> = dir
            .flatten()
            .filter(|entry| entry.metadata().is_ok_and(|m| m.is_file()))
            .map(|entry| entry.path())
            .filter(|path| exts.is_empty() || has_extension(path, exts))
            .collect();
        files.sort();
        files.iter().map(|path| path_string(path)).collect()
    }
}

/// The last component of `name` as a file name that stays inside its folder: no directories, no
/// control characters, no leading or trailing dots (so never `.` or `..`). `fallback` when
/// nothing usable is left or the name is absurdly long.
pub fn safe_file_name(name: &str, fallback: &str) -> String {
    let last = name.rsplit(['/', '\\']).next().unwrap_or("");
    let cleaned: String = last.chars().filter(|c| !c.is_control()).collect();
    let cleaned = cleaned.trim().trim_matches('.').trim();
    if cleaned.is_empty() || cleaned.chars().count() > MAX_NAME_CHARS { fallback.to_string() } else { cleaned.to_string() }
}

fn has_extension(path: &Path, exts: &[&str]) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|ext| exts.iter().any(|x| x.trim_start_matches('.').eq_ignore_ascii_case(ext)))
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::Duration;

    /// A fresh empty folder under the system temp dir.
    fn temp(tag: &str) -> PathBuf {
        static N: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!("effectcraft-android-{tag}-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn touch(path: &Path, age_secs: u64) {
        std::fs::write(path, b"x").unwrap();
        let file = std::fs::File::options().write(true).open(path).unwrap();
        file.set_modified(SystemTime::now() - Duration::from_secs(age_secs)).unwrap();
    }

    #[test]
    fn user_folders_follow_shared_storage_and_fall_back_to_private() {
        let private = Path::new("/data/user/0/app/files");
        let shared = Path::new("/storage/emulated/0/Android/data/app/files");
        let with = Layout::new(private, Some(shared));
        assert_eq!(with.config, private.join("config"));
        assert_eq!(with.projects, shared.join("Projects"));
        assert_eq!(with.media, shared.join("Media"));
        assert_eq!(with.exports, shared.join("Exports"));
        let without = Layout::new(private, None);
        assert_eq!(without.config, private.join("config"));
        assert_eq!(without.projects, private.join("Projects"));
    }

    #[test]
    fn file_names_never_leave_their_folder() {
        for hostile in ["../../etc/passwd", "..\\..\\x", "/abs/olute/name.ecproj", "a/b/../name.ecproj"] {
            let name = safe_file_name(hostile, "fallback");
            assert!(!name.contains('/') && !name.contains('\\') && name != "..", "{hostile:?} -> {name:?}");
        }
        assert_eq!(safe_file_name("../../etc/passwd", "fb"), "passwd");
        assert_eq!(safe_file_name("My Project.ecproj", "fb"), "My Project.ecproj");
        assert_eq!(safe_file_name("  ..  ", "fb"), "fb");
        assert_eq!(safe_file_name("..", "fb"), "fb");
        assert_eq!(safe_file_name("", "fb"), "fb");
        assert_eq!(safe_file_name("dir/", "fb"), "fb");
        assert_eq!(safe_file_name("a\u{0}b\nc.ecproj", "fb"), "abc.ecproj");
        assert_eq!(safe_file_name(&"x".repeat(MAX_NAME_CHARS + 1), "fb"), "fb");
        assert_eq!(safe_file_name(&"x".repeat(MAX_NAME_CHARS), "fb").len(), MAX_NAME_CHARS);
    }

    #[test]
    fn save_and_export_paths_stay_in_their_folders() {
        let layout = Layout::new(Path::new("/p"), Some(Path::new("/s")));
        assert_eq!(layout.project_path("Intro.ecproj"), "/s/Projects/Intro.ecproj");
        assert_eq!(layout.project_path("../../x.ecproj"), "/s/Projects/x.ecproj");
        assert_eq!(layout.project_path(""), "/s/Projects/Untitled Project.ecproj");
        assert_eq!(layout.export_path("/sdcard/out/clip.mp4"), "/s/Exports/clip.mp4");
        assert_eq!(layout.export_path("."), "/s/Exports/Export");
    }

    #[test]
    fn create_dirs_makes_every_folder_and_reports_the_ones_it_cannot() {
        let root = temp("create");
        let layout = Layout::new(&root.join("private"), Some(&root.join("shared")));
        assert!(layout.create_dirs().is_empty());
        for dir in [&layout.config, &layout.projects, &layout.media, &layout.exports] {
            assert!(dir.is_dir(), "{}", dir.display());
        }
        // A file where the parent folder should be: every folder under it fails, none panics.
        let blocker = root.join("blocker");
        std::fs::write(&blocker, b"file").unwrap();
        let broken = Layout::new(&blocker, None);
        assert_eq!(broken.create_dirs().len(), 4);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn newest_project_is_the_most_recently_modified_one() {
        let root = temp("newest");
        let layout = Layout::new(&root, None);
        assert_eq!(layout.newest_project(), None, "no folder yet");
        layout.create_dirs();
        assert_eq!(layout.newest_project(), None, "empty folder");
        touch(&layout.projects.join("old.ecproj"), 3000);
        touch(&layout.projects.join("new.ECPROJ"), 10);
        touch(&layout.projects.join("newer-but-not-a-project.txt"), 1);
        std::fs::create_dir(layout.projects.join("folder.ecproj")).unwrap();
        assert_eq!(layout.newest_project(), Some(layout.projects.join("new.ECPROJ")));
        touch(&layout.projects.join("packed.ecprojx"), 1);
        assert_eq!(layout.newest_project(), Some(layout.projects.join("packed.ecprojx")));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn media_files_filter_by_extension_and_skip_folders() {
        let root = temp("media");
        let layout = Layout::new(&root, None);
        assert!(layout.media_files(&["mp4"]).is_empty(), "no folder yet");
        layout.create_dirs();
        for name in ["b.MP4", "a.mp4", "c.png", "noext"] {
            touch(&layout.media.join(name), 0);
        }
        std::fs::create_dir(layout.media.join("clips.mp4")).unwrap();
        let names = |files: Vec<String>| files.iter().map(|f| Path::new(f).file_name().unwrap().to_string_lossy().into_owned()).collect::<Vec<_>>();
        assert_eq!(names(layout.media_files(&["mp4"])), ["a.mp4", "b.MP4"]);
        assert_eq!(names(layout.media_files(&[".png", "mp4"])), ["a.mp4", "b.MP4", "c.png"]);
        assert_eq!(names(layout.media_files(&[])), ["a.mp4", "b.MP4", "c.png", "noext"]);
        assert!(layout.media_files(&["mov"]).is_empty());
        std::fs::remove_dir_all(&root).ok();
    }
}
