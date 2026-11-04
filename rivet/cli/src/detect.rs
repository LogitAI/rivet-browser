//! Finding the Chromium browsers on this computer, by what is in their files.
//!
//! A program counts as a Chromium browser when it loads a Chromium `*_elf.dll` module and that
//! module, found beside the program or in its version folder, exports the functions every Chromium
//! build exports. The names of the programs play no part, so a browser Rivet has never heard of is
//! found the same way as Chrome. Everything here works on a `Files` view of the disk, so the rules
//! are tested without a disk.

use crate::pe;
use rivet_platform_windows::hold::{self, Version};
use std::path::{Path, PathBuf};

/// Functions that every Chromium build exports from its `*_elf.dll` module.
const CHROMIUM_EXPORTS: [&str; 4] = [
    "GetHandleVerifier",
    "IsBrowserProcess",
    "SignalInitializeCrashReporting",
    "GetUserDataDirectoryThunk",
];

/// What detection needs to know about the disk.
pub trait Files {
    /// The entries of a folder as names with whether each is a folder. Empty when unreadable.
    fn children(&self, folder: &Path) -> Vec<(String, bool)>;
    /// The imported and exported names of a program, or `None` when it is not a readable program.
    fn names(&self, file: &Path) -> Option<pe::Names>;
    fn is_file(&self, path: &Path) -> bool;
}

/// The real disk.
pub struct Disk;

impl Files for Disk {
    fn children(&self, folder: &Path) -> Vec<(String, bool)> {
        let Ok(entries) = std::fs::read_dir(folder) else { return Vec::new() };
        entries
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let folder = entry.file_type().ok()?.is_dir();
                Some((entry.file_name().into_string().ok()?, folder))
            })
            .collect()
    }

    fn names(&self, file: &Path) -> Option<pe::Names> {
        let mut source = std::io::BufReader::with_capacity(4096, std::fs::File::open(file).ok()?);
        pe::names(&mut source)
    }

    fn is_file(&self, path: &Path) -> bool {
        path.is_file()
    }
}

/// Why a program is not taken for a Chromium browser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rejection {
    Missing,
    NotAProgram,
    NoChromiumModule,
    ModuleMissing(String),
    NotChromium(String),
}

impl std::fmt::Display for Rejection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Missing => write!(f, "that file does not exist"),
            Self::NotAProgram => write!(f, "that file is not a Windows program"),
            Self::NoChromiumModule => write!(f, "that program does not load a Chromium module (a *_elf.dll)"),
            Self::ModuleMissing(module) => write!(f, "{module} is missing beside that program"),
            Self::NotChromium(module) => write!(f, "{module} does not export what every Chromium build exports"),
        }
    }
}

/// The version folders beside a program, newest first.
fn version_folders(files: &dyn Files, folder: &Path) -> Vec<String> {
    let mut versions: Vec<(Version, String)> = files
        .children(folder)
        .into_iter()
        .filter(|(_, is_folder)| *is_folder)
        .filter_map(|(name, _)| Some((hold::parse_version(&name)?, name)))
        .collect();
    versions.sort_by(|left, right| right.0.cmp(&left.0));
    versions.into_iter().map(|(_, name)| name).collect()
}

/// The Chromium module that proves `program` is a Chromium browser.
pub fn chromium_proof(files: &dyn Files, program: &Path) -> Result<PathBuf, Rejection> {
    if !files.is_file(program) {
        return Err(Rejection::Missing);
    }
    let folder = program.parent().ok_or(Rejection::NotAProgram)?;
    let name = program.file_name().ok_or(Rejection::NotAProgram)?;
    let versions = version_folders(files, folder);
    // A launcher stub may carry no import of its own, so the same-named program in a version
    // folder is looked at too.
    let programs = std::iter::once(program.to_path_buf())
        .chain(versions.iter().map(|version| folder.join(version).join(name)))
        .filter(|candidate| files.is_file(candidate));
    let mut readable = false;
    let mut rejection = Rejection::NoChromiumModule;
    for candidate in programs {
        let Some(found) = files.names(&candidate) else { continue };
        readable = true;
        let home = candidate.parent().unwrap_or(folder);
        for module in found.imports.iter().filter(|import| import.to_ascii_lowercase().ends_with("_elf.dll")) {
            let places = std::iter::once(home.join(module))
                .chain(std::iter::once(folder.join(module)))
                .chain(versions.iter().map(|version| folder.join(version).join(module)));
            let Some(place) = places.into_iter().find(|place| files.is_file(place)) else {
                rejection = Rejection::ModuleMissing(module.clone());
                continue;
            };
            let exports = files.names(&place).map(|names| names.exports).unwrap_or_default();
            if CHROMIUM_EXPORTS.iter().any(|marker| exports.iter().any(|export| export == marker)) {
                return Ok(place);
            }
            rejection = Rejection::NotChromium(module.clone());
        }
    }
    Err(if readable { rejection } else { Rejection::NotAProgram })
}

/// Whether Rivet can apply to a browser build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    /// Rivet has a verified row for the installed build.
    Ready,
    /// The installed build is new to Rivet. A copy of an earlier verified build opens instead.
    Kept { installed: Version, kept: Version },
    /// Rivet cannot apply to it yet, and why.
    NotYet(String),
}

impl Status {
    pub fn can_open(&self) -> bool {
        !matches!(self, Self::NotYet(_))
    }

    /// The plain sentence for the status, shown when a browser is chosen.
    pub fn sentence(&self, name: &str) -> String {
        match self {
            Self::Ready => format!("Rivet is verified for this build of {name}."),
            Self::Kept { installed, kept } => format!(
                "{name} {} is new to Rivet. Rivet opens the verified build {} it kept, for a limited time.",
                hold::format_version(*installed),
                hold::format_version(*kept)
            ),
            Self::NotYet(reason) => reason.clone(),
        }
    }
}

/// The status of a program from its file name, its installed version, the versions with a verified
/// row and the versions Rivet has kept.
pub fn status(file_name: &str, installed: Option<Version>, supported: &[Version], kept: &[Version]) -> Status {
    if hold::module_of(file_name).is_none() {
        return Status::NotYet("Rivet has no verified build of this browser yet, so it cannot apply to it.".to_owned());
    }
    let Some(installed) = installed else {
        return Status::NotYet("Rivet could not read the version of this browser.".to_owned());
    };
    if supported.contains(&installed) {
        return Status::Ready;
    }
    match kept.iter().copied().filter(|version| supported.contains(version)).max() {
        Some(kept) => Status::Kept { installed, kept },
        None => Status::NotYet(format!(
            "Rivet has not verified {} yet and has no earlier build of it to open.",
            hold::format_version(installed)
        )),
    }
}

/// A Chromium browser found on this computer or chosen by hand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Browser {
    pub path: PathBuf,
    pub name: String,
    pub version: String,
    /// The Chromium module that proved it, for people who want to see the evidence.
    pub proof: PathBuf,
    pub status: Status,
}

/// What the program's own version resource says.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Product {
    pub name: Option<String>,
    pub version: Option<String>,
}

impl Browser {
    pub fn new(path: PathBuf, proof: PathBuf, product: Product, status: Status, installed: Option<Version>) -> Self {
        let name = product
            .name
            .filter(|name| !name.trim().is_empty())
            .or_else(|| path.file_stem().map(|stem| stem.to_string_lossy().into_owned()))
            .unwrap_or_else(|| "Browser".to_owned());
        let version = installed.map(hold::format_version).or(product.version).unwrap_or_default();
        Self { path, name, version, proof, status }
    }
}

/// The folders to read for browsers, below each place programs get installed.
pub struct Roots {
    pub programs: Vec<PathBuf>,
    pub local: Option<PathBuf>,
}

impl Roots {
    pub fn from_environment() -> Self {
        let mut programs = Vec::new();
        for variable in ["ProgramFiles", "ProgramW6432", "ProgramFiles(x86)"] {
            if let Some(path) = std::env::var_os(variable).map(PathBuf::from) {
                if !programs.contains(&path) {
                    programs.push(path);
                }
            }
        }
        Self { programs, local: rivet_core::paths::local_app_data() }
    }
}

/// Whether a folder looks like where a Chromium browser keeps its program: a version folder, or an
/// `*_elf.dll` beside the programs. The check only spares the reading of unrelated programs.
fn looks_installed(files: &dyn Files, folder: &Path) -> bool {
    files.children(folder).iter().any(|(name, is_folder)| {
        if *is_folder {
            hold::parse_version(name).is_some()
        } else {
            name.to_ascii_lowercase().ends_with("_elf.dll")
        }
    })
}

fn programs_in(files: &dyn Files, folder: &Path, found: &mut Vec<PathBuf>) {
    if !looks_installed(files, folder) {
        return;
    }
    for (name, is_folder) in files.children(folder) {
        if !is_folder && name.to_ascii_lowercase().ends_with(".exe") {
            found.push(folder.join(name));
        }
    }
}

/// The programs in the places browsers install to: `<root>\<vendor>\<product>\Application` and
/// `<root>\<vendor>\Application`, below Program Files and the user's local application data.
pub fn installed_programs(files: &dyn Files, roots: &Roots) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut places: Vec<(PathBuf, bool)> = roots.programs.iter().map(|root| (root.clone(), false)).collect();
    if let Some(local) = &roots.local {
        places.push((local.clone(), false));
        places.push((local.join("Programs"), true));
    }
    for (root, direct) in places {
        for (vendor, is_folder) in files.children(&root) {
            if !is_folder {
                continue;
            }
            let vendor = root.join(vendor);
            let inside = files.children(&vendor);
            if direct {
                programs_in(files, &vendor, &mut found);
            }
            for (product, is_folder) in inside {
                if !is_folder {
                    continue;
                }
                if product.eq_ignore_ascii_case("Application") {
                    programs_in(files, &vendor.join(&product), &mut found);
                } else {
                    let product = vendor.join(product);
                    if files.children(&product).iter().any(|(name, is_folder)| *is_folder && name.eq_ignore_ascii_case("Application")) {
                        programs_in(files, &product.join("Application"), &mut found);
                    }
                }
            }
        }
    }
    found
}

fn same_path(left: &Path, right: &Path) -> bool {
    left.to_string_lossy().replace('/', "\\").eq_ignore_ascii_case(&right.to_string_lossy().replace('/', "\\"))
}

/// The paths that proved to be Chromium browsers, from the registered programs and the places
/// browsers install to, each once.
pub fn chromium_programs(files: &dyn Files, roots: &Roots, registered: Vec<PathBuf>) -> Vec<(PathBuf, PathBuf)> {
    let mut seen: Vec<PathBuf> = Vec::new();
    let mut found = Vec::new();
    for path in registered.into_iter().chain(installed_programs(files, roots)) {
        if seen.iter().any(|known| same_path(known, &path)) {
            continue;
        }
        seen.push(path.clone());
        if let Ok(proof) = chromium_proof(files, &path) {
            found.push((path, proof));
        }
    }
    found
}

/// The order people see: browsers Rivet can open first, then by name.
pub fn order(browsers: &mut [Browser]) {
    let rank = |browser: &Browser| match browser.status {
        Status::Ready => 0,
        Status::Kept { .. } => 1,
        Status::NotYet(_) => 2,
    };
    browsers.sort_by_key(|browser| (rank(browser), browser.name.to_lowercase()));
}

#[cfg(test)]
pub(crate) mod fake {
    use super::*;
    use std::collections::BTreeMap;

    /// A disk in memory: folders by path and programs with their names.
    #[derive(Default)]
    pub struct Memory {
        /// By lower case path, with the path as written and the names of a program.
        pub files: BTreeMap<String, (String, Option<pe::Names>)>,
    }

    fn key(path: &Path) -> String {
        path.to_string_lossy().replace('/', "\\").to_ascii_lowercase()
    }

    impl Memory {
        pub fn program(&mut self, path: &str, imports: &[&str], exports: &[&str]) {
            let names = pe::Names {
                imports: imports.iter().map(|name| (*name).to_owned()).collect(),
                exports: exports.iter().map(|name| (*name).to_owned()).collect(),
            };
            self.files.insert(key(Path::new(path)), (path.to_owned(), Some(names)));
        }

        pub fn data(&mut self, path: &str) {
            self.files.insert(key(Path::new(path)), (path.to_owned(), None));
        }
    }

    impl Files for Memory {
        fn children(&self, folder: &Path) -> Vec<(String, bool)> {
            let prefix = format!("{}\\", key(folder));
            let mut found: BTreeMap<String, bool> = BTreeMap::new();
            for (lower, (written, _)) in &self.files {
                if !lower.starts_with(&prefix) {
                    continue;
                }
                let rest = &written[prefix.len()..];
                match rest.split_once('\\') {
                    Some((name, _)) => {
                        found.insert(name.to_owned(), true);
                    }
                    None => {
                        found.entry(rest.to_owned()).or_insert(false);
                    }
                }
            }
            found.into_iter().collect()
        }

        fn names(&self, file: &Path) -> Option<pe::Names> {
            self.files.get(&key(file))?.1.as_ref().map(|names| pe::Names {
                imports: names.imports.clone(),
                exports: names.exports.clone(),
            })
        }

        fn is_file(&self, path: &Path) -> bool {
            self.files.contains_key(&key(path))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fake::Memory;
    use super::*;

    const CHROME: &str = r"C:\Program Files\Google\Chrome\Application";

    fn chrome_like(disk: &mut Memory, folder: &str, exe: &str, elf: &str) {
        disk.program(&format!(r"{folder}\{exe}"), &["KERNEL32.dll", elf], &[]);
        disk.program(
            &format!(r"{folder}\1.2.3.4\{elf}"),
            &["KERNEL32.dll"],
            &["GetHandleVerifier", "IsBrowserProcess"],
        );
        disk.data(&format!(r"{folder}\1.2.3.4\icudtl.dat"));
    }

    #[test]
    fn fr1_a_browser_is_known_by_its_chromium_module_not_by_its_name() {
        let mut disk = Memory::default();
        chrome_like(&mut disk, r"C:\Apps\Odd Browser\Application", "oddity.exe", "oddity_elf.dll");
        let proof = chromium_proof(&disk, Path::new(r"C:\Apps\Odd Browser\Application\oddity.exe")).unwrap();
        assert_eq!(proof, Path::new(r"C:\Apps\Odd Browser\Application\1.2.3.4\oddity_elf.dll"));
    }

    #[test]
    fn fr1_a_program_named_chrome_without_the_module_is_not_a_browser() {
        let mut disk = Memory::default();
        disk.program(r"C:\Fake\chrome.exe", &["KERNEL32.dll"], &[]);
        assert_eq!(chromium_proof(&disk, Path::new(r"C:\Fake\chrome.exe")), Err(Rejection::NoChromiumModule));
    }

    #[test]
    fn fr1_a_helper_beside_the_browser_is_not_a_browser() {
        let mut disk = Memory::default();
        chrome_like(&mut disk, CHROME, "chrome.exe", "chrome_elf.dll");
        disk.program(&format!(r"{CHROME}\chrome_proxy.exe"), &["KERNEL32.dll"], &[]);
        assert!(chromium_proof(&disk, Path::new(&format!(r"{CHROME}\chrome.exe"))).is_ok());
        assert_eq!(
            chromium_proof(&disk, Path::new(&format!(r"{CHROME}\chrome_proxy.exe"))),
            Err(Rejection::NoChromiumModule)
        );
    }

    #[test]
    fn fr1_a_module_without_chromium_exports_does_not_prove_a_browser() {
        let mut disk = Memory::default();
        disk.program(r"C:\App\app.exe", &["app_elf.dll"], &[]);
        disk.program(r"C:\App\app_elf.dll", &[], &["SomethingElse"]);
        assert_eq!(
            chromium_proof(&disk, Path::new(r"C:\App\app.exe")),
            Err(Rejection::NotChromium("app_elf.dll".to_owned()))
        );
        disk.program(r"C:\Gone\gone.exe", &["gone_elf.dll"], &[]);
        assert_eq!(
            chromium_proof(&disk, Path::new(r"C:\Gone\gone.exe")),
            Err(Rejection::ModuleMissing("gone_elf.dll".to_owned()))
        );
    }

    #[test]
    fn fr1_a_missing_or_unreadable_file_says_so() {
        let mut disk = Memory::default();
        disk.data(r"C:\Docs\notes.exe");
        assert_eq!(chromium_proof(&disk, Path::new(r"C:\Nowhere\x.exe")), Err(Rejection::Missing));
        assert_eq!(chromium_proof(&disk, Path::new(r"C:\Docs\notes.exe")), Err(Rejection::NotAProgram));
    }

    #[test]
    fn fr1_a_launcher_stub_is_proved_by_the_program_in_its_version_folder() {
        let mut disk = Memory::default();
        disk.program(r"C:\B\Application\stub.exe", &["KERNEL32.dll"], &[]);
        disk.program(r"C:\B\Application\9.9.9.9\stub.exe", &["real_elf.dll"], &[]);
        disk.program(r"C:\B\Application\9.9.9.9\real_elf.dll", &[], &["IsBrowserProcess"]);
        assert!(chromium_proof(&disk, Path::new(r"C:\B\Application\stub.exe")).is_ok());
    }

    #[test]
    fn fr1_detection_reads_registered_and_installed_programs_once_each() {
        let mut disk = Memory::default();
        chrome_like(&mut disk, CHROME, "chrome.exe", "chrome_elf.dll");
        chrome_like(&mut disk, r"C:\Users\me\AppData\Local\Vivaldi\Application", "vivaldi.exe", "vivaldi_elf.dll");
        chrome_like(&mut disk, r"C:\Program Files\Solo\Application", "solo.exe", "solo_elf.dll");
        disk.program(r"C:\Program Files\7-Zip\7zFM.exe", &["KERNEL32.dll"], &[]);
        disk.program(r"C:\Program Files\Editor\Code.exe", &["KERNEL32.dll"], &[]);
        disk.data(r"C:\Program Files\Editor\resources.pak");
        let roots = Roots {
            programs: vec![PathBuf::from(r"C:\Program Files")],
            local: Some(PathBuf::from(r"C:\Users\me\AppData\Local")),
        };
        let registered = vec![
            PathBuf::from(format!(r"{CHROME}\chrome.exe")),
            PathBuf::from(r"C:\Program Files\7-Zip\7zFM.exe"),
            PathBuf::from(r"c:\program files\google\chrome\application\CHROME.EXE"),
        ];
        let found: Vec<_> = chromium_programs(&disk, &roots, registered).into_iter().map(|(path, _)| path).collect();
        assert_eq!(found.len(), 3, "{found:?}");
        assert!(found.contains(&PathBuf::from(r"C:\Program Files\Google\Chrome\Application\chrome.exe")));
        assert!(found.contains(&PathBuf::from(r"C:\Users\me\AppData\Local\Vivaldi\Application\vivaldi.exe")));
        assert!(found.contains(&PathBuf::from(r"C:\Program Files\Solo\Application\solo.exe")));
    }

    #[test]
    fn fr1_status_says_plainly_when_rivet_cannot_apply_yet() {
        let supported = [[156, 0, 8078, 74]];
        assert_eq!(status("chrome.exe", Some([156, 0, 8078, 74]), &supported, &[]), Status::Ready);
        assert!(matches!(
            status("chrome.exe", Some([157, 0, 1, 1]), &supported, &[[156, 0, 8078, 74]]),
            Status::Kept { kept: [156, 0, 8078, 74], .. }
        ));
        let behind = status("chrome.exe", Some([157, 0, 1, 1]), &supported, &[]);
        assert!(matches!(behind, Status::NotYet(_)) && !behind.can_open());
        assert!(behind.sentence("Chrome").contains("157.0.1.1"));
        assert!(matches!(status("aside.exe", Some([1, 0, 1022, 1]), &supported, &[]), Status::NotYet(_)));
        assert!(matches!(status("chrome.exe", None, &supported, &[]), Status::NotYet(_)));
    }

    #[test]
    fn fr1_browsers_rivet_can_open_come_first() {
        let make = |name: &str, status| Browser {
            path: PathBuf::from(name),
            name: name.to_owned(),
            version: String::new(),
            proof: PathBuf::new(),
            status,
        };
        let mut list = vec![
            make("Zed", Status::Ready),
            make("Aside", Status::NotYet(String::new())),
            make("Brave", Status::Ready),
            make("Chrome", Status::Kept { installed: [2, 0, 0, 0], kept: [1, 0, 0, 0] }),
        ];
        order(&mut list);
        let names: Vec<_> = list.iter().map(|browser| browser.name.as_str()).collect();
        assert_eq!(names, ["Brave", "Zed", "Chrome", "Aside"]);
    }

    #[test]
    fn fr1_a_browser_is_named_from_its_file_when_it_has_no_product_name() {
        let named = Browser::new(
            PathBuf::from(r"C:\B\odd.exe"),
            PathBuf::new(),
            Product { name: Some("Odd Browser".into()), version: Some("3.1".into()) },
            Status::Ready,
            None,
        );
        assert_eq!((named.name.as_str(), named.version.as_str()), ("Odd Browser", "3.1"));
        let bare = Browser::new(PathBuf::from(r"C:\B\odd.exe"), PathBuf::new(), Product::default(), Status::Ready, Some([1, 2, 3, 4]));
        assert_eq!((bare.name.as_str(), bare.version.as_str()), ("odd", "1.2.3.4"));
    }
}
