use std::collections::VecDeque;
use std::error::Error;
use std::ffi::OsString;
use std::fmt;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;

/// Generic driver abstraction.
pub trait Driver
{
    fn name(&self) -> &str;
    fn usable(&self) -> bool;
    fn run(&self, path: &PathBuf) -> Result<String, Box<dyn Error>>;
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
        let mut cmd = Command::new("file");
        cmd.args(["-b", "--mime-type"]);
        cmd
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn().is_ok()
    }

    fn run(&self, path: &PathBuf) -> Result<String, Box<dyn Error>> {
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

    fn run(&self, path: &PathBuf) -> Result<String, Box<dyn Error>> {
        let mut cmd = Command::new("xdg-mime");
        cmd.args(["query", "filetype"]);
        let out = cmd.arg(path).output()?;
        let s = String::from_utf8(out.stdout)?;
        Ok(s.trim().into())
    }
}


#[derive(Debug, Clone, Copy)]
// A generic driver that abstracts all available drivers.
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
    fn run(&self, path: &PathBuf) -> Result<String, Box<dyn Error>> {
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
        let mut drivers = Vec::new();
        drivers.push(current.clone());
        drivers.push(FileDriver::new().into());
        for d in drivers.iter() {
            match select {
                None => {
                    if d.usable() {
                        current = d.clone();
                        break;
                    }
                }
                Some(ref name) => {
                    if d.name() == name {
                        current = d.clone();
                        break;
                    }
                }
            }
        }

        DriverList {
            drivers: drivers,
            current: current,
            inspect: inspect,
        }
    }

    pub fn by_extension(&self, path: &PathBuf) -> bool {
        const EXTENSIONS: &'static [&'static str] = &[
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
                if e.to_string() == ext.to_string_lossy() {
                    return true;
                }
            }
        }

        false
    }

    pub fn by_mime(&self, _path: &PathBuf, mime: &String) -> bool {
        const MIMETYPES: &'static [&'static str] = &[
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
        path: &PathBuf,
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

#[derive(Debug, Clone)]
struct DriverUnusableError;

impl fmt::Display for DriverUnusableError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "No usable driver found.")
    }
}

impl Error for DriverUnusableError {}

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

    fn run(&self, path: &PathBuf) -> Result<String, Box<dyn Error>> {
        if self.usable() {
            self.current.run(path)
        } else {
            Err(DriverUnusableError.into())
        }
    }
}

impl fmt::Display for DriverList {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut i = 0;
        for d in self.drivers.iter() {
            write!(f, "[{}] {}", i, d.name())?;
            if ! d.usable() {
                write!(f, " (!)")?;
            } else if d.name() == self.current.name() {
                write!(f, " (*)")?;
            }
            writeln!(f, "")?;
            i += 1;
        }
        Ok(())
    }
}

/// File crawler thread that populates the list of files to scan.
pub struct FileCrawlerThread {
    thread: thread::JoinHandle<()>,
}

impl FileCrawlerThread {
    pub fn new(
        paths: Vec<PathBuf>,
        excludes: Vec<String>,
        files: Arc<Mutex<VecDeque<PathBuf>>>,
    ) -> Self {
        let handle = thread::spawn(move|| {
            for path in paths {
                FileCrawlerThread::crawl(&path, &files, &excludes).unwrap();
            }
        });
        FileCrawlerThread {
            thread: handle,
        }
    }

    pub fn is_finished(&self) -> bool { self.thread.is_finished() }
    pub fn join(self) -> thread::Result<()> { self.thread.join() }

    fn crawl(
        path: &PathBuf,
        files: &Arc<Mutex<VecDeque<PathBuf>>>,
        excludes: &Vec<String>,
    ) -> Result<(), Box<dyn Error>> {
        if path.exists() {
            if excludes.iter().any(|x| {
                path.display().to_string().contains(x)
            }) {
                return Ok(());
            }
            files.lock().unwrap().push_back(path.clone());
            if path.is_dir() {
                for entry in fs::read_dir(path)? {
                    let path = entry?.path();
                    FileCrawlerThread::crawl(&path, files, excludes)?;
                }
            }
        }
        Ok(())
    }
}


/// Tag file creator for Ctags and Cscope.
pub struct TagFileCreator {
    scanned_files: Arc<Mutex<VecDeque<PathBuf>>>,
    cscope: Child,
    ctags: Child,
}

impl TagFileCreator {
    pub fn new(
        scanned_files: Arc<Mutex<VecDeque<PathBuf>>>,
    ) -> Result<Self, Box<dyn Error>> {
        let cscope = Command::new("cscope")
            .args(["-bqki", "-"])
            .stdin(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;

        let ctags = TagFileCreator::find_ctags()?
            .args(["-L", "-", "--extra=+q", "--fields=+i"])
            .stdin(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;

        Ok(TagFileCreator {
            scanned_files: scanned_files,
            cscope: cscope,
            ctags: ctags,
        })
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

    pub fn run(&mut self) -> Result<(), Box<dyn Error>> {
        while let Some(file) = self.scanned_files.lock().unwrap().pop_front() {
            let mut write_vec: Vec<u8> = vec!();
            let mut write: Box<&mut dyn Write> = Box::new(&mut write_vec);
            writeln!(write, "{}", file.display())?;

            let cscope_stdin = self.cscope.stdin.as_mut().ok_or("Cscope died.")?;
            cscope_stdin.write_all(write_vec.as_slice())?;

            let ctags_stdin = self.ctags.stdin.as_mut().ok_or("Ctags died.")?;
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
        {
            let mut stdin = self.cscope.stdin.take().unwrap();
            stdin.flush().unwrap_or_default();
            let mut stdin = self.ctags.stdin.take().unwrap();
            stdin.flush().unwrap_or_default();
        }

        self.cscope.wait().unwrap_or_default();
        self.ctags.wait().unwrap_or_default();
    }
}
