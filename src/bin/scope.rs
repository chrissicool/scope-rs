use std::collections::VecDeque;
use std::error::Error;
use std::ffi::OsString;
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};
use std::thread;

extern crate clap;
use clap::Parser;

use scope_rs::{
    Driver,
    DriverList,
    FileCrawler,
    TagFileCreator,
};


/// Parse jobs parameter into a `usize`.
///
/// The validation includes that there is at least one job.
fn jobs_parser(jobs: &str) -> Result<usize, clap::error::Error> {
    let err = clap::error::Error::new(clap::error::ErrorKind::ValueValidation);
    match jobs.parse::<usize>() {
        Ok(result) => {
            if result > 0 {
                Ok(result)
            } else {
                Err(err)
            }
        },
        _ => Err(err),
    }
}

/// Command line arguments.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Test file and print if it would be scoped.
    /// This option also prints the driver in use.
    #[arg(short, long, default_value_t = false)]
    inspect: bool,

    /// Specify *list* to get all usable MIME drivers in order of preference.
    /// Specify a *driver* from a previous *list* operation as MIME driver.
    #[arg(short, long)]
    driver: Option<OsString>,

    /// Run in verbose mode.
    #[arg(short, long, action, default_value_t = false)]
    verbose: bool,

    /// Number of parallel jobs to use.
    #[arg(short, long, action,
        // SAFETY: unwrap() does not panic with known-good value in constructor.
        default_value_t = thread::available_parallelism()
                                .unwrap_or(NonZeroUsize::new(1).unwrap()).get(),
        value_parser = jobs_parser,
    )]
    jobs: usize,

    /// Files and directories to exclude.
    #[arg(short = 'x', long, value_delimiter = ',')]
    excludes: Option<Vec<String>>,

    #[arg(last = true, default_value = ".")]
    dir: Vec<PathBuf>,
}

/// Make a list of excludes from an optional list of excludes.
///
/// Also add the default list of excludes to the result.
fn make_excludes(excludes: Option<Vec<String>>) -> Vec<String> {
    let mut result: Vec<String> = vec![];
    // XXX Too Unixy.
    const EXCLUDES: &[&str] = &[
        "/.git/",
        "/.svn/",
        "/CVS/",
    ];

    if let Some(x) = excludes {
        result = x;
    }
    for x in EXCLUDES {
        result.push((**x).to_string());
    }

    result
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    //println!("{:#?}", args);
    if args.driver.clone().unwrap_or_default() == "list" {
        println!("{}", DriverList::new(None, false));
        return Ok(());
    }

    let driver = Arc::new(DriverList::new(args.driver, args.inspect));
    if ! driver.usable() {
        return Err("No usable driver found.".into());
    }

    if args.inspect {
        println!("Driver: {}", driver.name());
    }

    let files_to_scan = Arc::new(Mutex::new(VecDeque::new()));
    let tags_creator = Arc::new(Mutex::new(TagFileCreator::new()?));
    let running = Arc::new(RwLock::new(true));

    let crawler = FileCrawler::new(
        args.dir,
        make_excludes(args.excludes),
        Arc::clone(&files_to_scan), // Producer
    );

    let mut threads = Vec::with_capacity(args.jobs);
    (0..args.jobs).for_each(|_| {
        let files_to_scan = Arc::clone(&files_to_scan); // Consumer
        let tags_creator = Arc::clone(&tags_creator);
        let running = Arc::clone(&running);
        let driver = Arc::clone(&driver);
        threads.push(thread::spawn(move|| {
            loop {
                let mut files = files_to_scan.lock().unwrap();
                if let Some(path) = files.pop_front() {
                    drop(files); // XXX .lock().unwrap().pop_front() is slower
                    if driver.by_extension(&path) {
                        driver.inspect("Include [.ext]",
                                        &path, None, args.verbose);
                        if ! args.inspect {
                            tags_creator.lock().unwrap().writeln(&path).unwrap();
                        }
                    } else if let Ok(mime) = driver.run(&path) {
                        if driver.by_mime(&path, &mime) {
                            driver.inspect("Include [mime]",
                                            &path, Some(&mime), args.verbose);
                            if ! args.inspect {
                                tags_creator.lock().unwrap().writeln(&path).unwrap();
                            }
                        } else {
                            driver.inspect("Exclude [----]",
                                            &path, Some(&mime), false);
                        }
                    } else {
                        eprintln!("Cannot determine MIME type for {}",
                            path.display());
                    }
                } else {
                    drop(files);
                    if ! *running.read().unwrap() {
                        break;
                    }
                }
            }
        }));
    });

    crawler.run()?;
    *Arc::clone(&running).write().unwrap() = false;

    threads.into_iter().for_each(|t| {
        t.join().expect("Thread creation or execution failed.");
    });

    Ok(())
}
