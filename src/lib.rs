#![doc = include_str!("../README.md")]

use std::collections::VecDeque;
use std::error::Error;
use std::ffi::OsString;
use std::fmt;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};

/// Generic driver abstraction.
///
/// Each mime-type driver needs to implement this trait.
pub trait Driver
{
    fn name(&self) -> &str;
    fn usable(&self) -> bool;
    fn run(&self, path: &Path) -> Result<String, Box<dyn Error>>;
}

/// A driver that uses the file(1) tool for mime type checks.
#[derive(Debug, Clone, Copy)]
struct FileDriver {}

impl FileDriver {
    #[inline]
    pub fn new() -> Self {
        FileDriver {}
    }
}

impl Driver for FileDriver {
    #[inline]
    fn name(&self) -> &str {
        "file"
    }

    fn usable(&self) -> bool {
        if let Ok(out) = Command::new("file").arg("-h").output() {
            let s = String::from_utf8(out.stderr).unwrap_or_default();
            if s.contains("--mime-type") {
                return true;
            }
        }
        false
    }

    fn run(&self, path: &Path) -> Result<String, Box<dyn Error>> {
        let mut cmd = Command::new("file");
        cmd.args(["-b", "--mime-type"]);
        let out = cmd.arg(path).output()?;
        let s = String::from_utf8(out.stdout)?;
        Ok(s.trim().into())
    }
}

/// A driver that uses the xdg-mime(1) tool for mime type checks.
#[derive(Debug, Clone, Copy)]
struct MimeDriver {}

impl MimeDriver {
    #[inline]
    pub fn new() -> Self {
        MimeDriver {}
    }
}

impl Driver for MimeDriver {
    #[inline]
    fn name(&self) -> &str {
        "xdg-mime"
    }

    fn usable(&self) -> bool {
        let mut cmd = Command::new("xdg-mime");
        cmd.args(["query", "filetype"]);
        cmd
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn().is_ok()
    }

    fn run(&self, path: &Path) -> Result<String, Box<dyn Error>> {
        let mut cmd = Command::new("xdg-mime");
        cmd.args(["query", "filetype"]);
        let out = cmd.arg(path).output()?;
        let s = String::from_utf8(out.stdout)?;
        Ok(s.trim().into())
    }
}


// A generic driver that abstracts all available drivers.
//
// This is the basis for a thread-safe approach to a List of Driver implementations.
// Dynamic traits will not do this. So bite the bullet and add a new Enum value for each driver.
// That also means to forward the interface accordingly.
#[derive(Debug, Clone, Copy)]
enum GenericDriver {
    MimeDriver(MimeDriver),
    FileDriver(FileDriver),
}

impl Driver for GenericDriver {
    #[inline]
    fn name(&self) -> &str {
        match self {
            GenericDriver::MimeDriver(driver) => driver.name(),
            GenericDriver::FileDriver(driver) => driver.name(),
        }
    }

    #[inline]
    fn usable(&self) -> bool {
        match self {
            GenericDriver::MimeDriver(driver) => driver.usable(),
            GenericDriver::FileDriver(driver) => driver.usable(),
        }
    }

    #[inline]
    fn run(&self, path: &Path) -> Result<String, Box<dyn Error>> {
        match self {
            GenericDriver::MimeDriver(driver) => driver.run(path),
            GenericDriver::FileDriver(driver) => driver.run(path),
        }
    }
}

impl From<FileDriver> for GenericDriver {
    #[inline]
    fn from(driver: FileDriver) -> GenericDriver {
        GenericDriver::FileDriver(driver)
    }
}

impl From<MimeDriver> for GenericDriver {
    #[inline]
    fn from(driver: MimeDriver) -> GenericDriver {
        GenericDriver::MimeDriver(driver)
    }
}


/// A collection of all available drivers.
///
/// The collection implements Driver itself and exposes the best
/// candidate to the user.
#[derive(Debug, Clone)]
pub struct DriverList {
    drivers: Vec<GenericDriver>,
    current: GenericDriver,
    inspect: bool,
}

impl DriverList {
    pub fn new(select: Option<OsString>, inspect: bool) -> Self {
        let mut current: GenericDriver = MimeDriver::new().into();
        // Push order determines preference.
        let drivers = vec![current, FileDriver::new().into()];
        for d in drivers.iter() {
            match select {
                None => {
                    if d.usable() {
                        current = *d;
                        break;
                    }
                }
                Some(ref name) => {
                    if d.name() == name {
                        current = *d;
                        break;
                    }
                }
            }
        }

        DriverList { drivers, current, inspect, }
    }

    pub fn by_extension(&self, path: &Path) -> bool {
        const EXTENSIONS: &[&str] = &[
            "asm",
            "c",
            "cc",
            "cpp",
            "cs",
            "cxx",
            "erl",
            "go",
            "h",
            "hpp",
            "hxx",
            "java",
            "js",
            "lua",
            "php",
            "pl",
            "pm",
            "py",
            "rb",
            "rs",
            "s",
            "sh",
            "S",
            "tcl",
        ];

        if let Some(ext) = path.extension() {
            for e in EXTENSIONS.iter() {
                if *e == ext.to_string_lossy() {
                    return true;
                }
            }
        }

        false
    }

    pub fn by_mime(&self, _path: &Path, mime: &str) -> bool {
        const MIMETYPES: &[&str] = &[
            // from shared-mime-info
            "rust",
            "x-c++",
            "x-c++src",
            "x-c++hdr",
            "x-chdr",
            "x-csharp",
            "x-csrc",
            "x-erlang",
            "x-java",
            "x-javascript",
            "x-lua",
            "x-perl",
            "x-php",
            "x-python",
            "x-ruby",
            "x-shellscript",
            "x-tcl",
            // from GNU file(1), where different
            "x-c",
        ];

        for m in MIMETYPES.iter() {
            if mime.ends_with(m) {
                return true;
            }
        }

        false
    }

    pub fn inspect(&self,
        reason: &str,
        path: &Path,
        mime: Option<&String>,
        verbose: bool,
    ) {
        if verbose {
            println!("{}", path.display());
        } else if self.inspect {
            if let Some(mime) = mime {
                println!("{}: {:29} {}", reason, mime, path.display());
            } else {
                println!("{}: {:29} {}", reason, " ".to_string(), path.display());
            }
        }
    }
}

impl Driver for DriverList {
    #[inline]
    fn name(&self) -> &str {
        if self.usable() {
            self.current.name()
        } else {
            "<none>"
        }
    }

    #[inline]
    fn usable(&self) -> bool {
        self.current.usable()
    }

    fn run(&self, path: &Path) -> Result<String, Box<dyn Error>> {
        if self.usable() {
            self.current.run(path)
        } else {
            Err("No usable driver found.".into())
        }
    }
}

impl fmt::Display for DriverList {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for (i, d) in self.drivers.iter().enumerate() {
            write!(f, "[{}] {}", i, d.name())?;
            if ! d.usable() {
                write!(f, " (!)")?;
            } else if d.name() == self.current.name() {
                write!(f, " (*)")?;
            }
            writeln!(f)?;
        }
        Ok(())
    }
}


/// File crawler that populates the list of files to scan.
///
/// After creation, feels like a std::thread.
pub struct FileCrawler {
    paths: Vec<PathBuf>,
    excludes: Vec<String>,
    files: Arc<Mutex<VecDeque<PathBuf>>>,
}

impl FileCrawler {
    pub fn new(
        paths: Vec<PathBuf>,
        excludes: Vec<String>,
        files: Arc<Mutex<VecDeque<PathBuf>>>,
    ) -> Self {
        FileCrawler { paths, excludes, files, }
    }

    pub fn run(&self) -> Result<(), Box<dyn Error>> {
        for path in &self.paths {
            self.crawl(path)?;
        };
        Ok(())
    }

    fn crawl(&self, path: &Path) -> Result<(), Box<dyn Error>> {
        if path.exists() {
            if self.excludes.iter().any(|x| {
                path.display().to_string().contains(x)
            }) {
                return Ok(());
            }
            self.files.lock().unwrap().push_back(path.to_path_buf().clone());
            if path.is_dir() {
                for entry in fs::read_dir(path)? {
                    let path = entry?.path();
                    self.crawl(&path)?;
                }
            }
        }

        Ok(())
    }
}


/// Tag file creator for Ctags and Cscope databases.
///
/// Create the tags databases for ctags and cscope in parallel
/// for each file comming in from the `scanned_files` queue.
pub struct TagFileCreator {
    cscope: Option<Child>,
    ctags: Option<Child>,
}

impl TagFileCreator {
    pub fn new() -> Result<Self, Box<dyn Error>> {
        let cscope = Command::new("cscope")
            .args(["-bqki", "-"])
            .stdin(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok();

        let ctags = TagFileCreator::find_ctags()?
            .args(["-L", "-", "--extra=+q", "--fields=+i"])
            .stdin(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok();

        if cscope.is_none() {
            eprintln!("Cannot run cscope.");
        }
        if ctags.is_none() {
            eprintln!("Cannot run Exuberant ctags.");
        }
        if ctags.is_none() && cscope.is_none() {
            return Err("Cannot create any tag file database.".into());
        }

        Ok(TagFileCreator { cscope, ctags, })
    }

    /// Find a working Exuberant Ctags variant.
    fn find_ctags() -> Result<Command, Box<dyn Error>> {
        let mut ctags: Option<&str> = None;

        for c in ["uctags", "ectags", "ctags"] {
            if let Ok(out) = Command::new(c)
                        .arg("--help")
                        .stderr(Stdio::null())
                        .output() {
                let s = String::from_utf8(out.stdout)?;
                if s.contains("Exuberant") {
                    ctags = Some(c);
                    break;
                }
            }
        };

        match ctags {
            Some(ctags) => Ok(Command::new(ctags)),
            None        => Err("Cannot find Exuberant Ctags.".into()),
        }
    }

    pub fn writeln(&mut self, path: &Path) -> Result<(), Box<dyn Error>> {
        let mut write_vec: Vec<u8> = vec!();
        let mut write: Box<&mut dyn Write> = Box::new(&mut write_vec);
        writeln!(write, "{}", path.display())?;

        if let Some(ref mut cscope) = self.cscope {
            let cscope_stdin = cscope.stdin.as_mut().ok_or("Cscope died.")?;
            cscope_stdin.write_all(write_vec.as_slice())?;
        }

        if let Some(ref mut ctags) = self.ctags {
            let ctags_stdin = ctags.stdin.as_mut().ok_or("Ctags died.")?;
            ctags_stdin.write_all(write_vec.as_slice())?;
        }
        Ok(())
    }
}

/// Destructor for TagFileCreator.
///
/// Close stdin for ctags and cscope and wait for their termination.
impl Drop for TagFileCreator {
    fn drop(&mut self) {
        if let Some(ref mut cscope) = self.cscope {
            let mut stdin = cscope.stdin.take().unwrap();
            stdin.flush().unwrap_or_default();
        }
        if let Some(ref mut ctags) = self.ctags {
            let mut stdin = ctags.stdin.take().unwrap();
            stdin.flush().unwrap_or_default();
        }

        if let Some(ref mut cscope) = self.cscope {
            cscope.wait().unwrap_or_default();
        }
        if let Some(ref mut ctags) = self.ctags {
            ctags.wait().unwrap_or_default();
        }
    }
}
