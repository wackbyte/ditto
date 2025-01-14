use crate::{common, ninja::get_ninja_exe, pkg, spinner::Spinner, version::Version};
use clap::{Arg, ArgMatches, Command};
use console::Style;
use ditto_config::{read_config, Config, PackageName, CONFIG_FILE_NAME};
use ditto_make::{self as make, BuildNinja, GetWarnings, PackageSources, Sources};
use fs2::FileExt;
use log::{debug, trace};
use miette::{IntoDiagnostic, Result, WrapErr};
use notify::Watcher;
use std::{
    collections::HashMap,
    env::current_exe,
    fs,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{self, ExitStatus, Stdio},
    sync::mpsc,
    time::{Duration, Instant},
};

pub static COMPILE_SUBCOMMAND: &str = "compile";

pub fn command<'a>(name: &str) -> Command<'a> {
    Command::new(name).about("Build a project").arg(
        Arg::new("watch")
            .short('w')
            .long("watch")
            .help("Watch files for changes"),
    )
}

pub async fn run(matches: &ArgMatches, ditto_version: &Version) -> Result<()> {
    if matches.is_present("watch") {
        run_watch(matches, ditto_version).await
    } else {
        let status = run_once(matches, ditto_version).await?;
        process::exit(status.code().unwrap_or(0));
    }
}

struct EventForwarder {
    tx: mpsc::Sender<notify::Result<notify::Event>>,
    debounce_duration: Duration,
    last_run: Option<Instant>,
}

impl EventForwarder {
    fn new(tx: mpsc::Sender<notify::Result<notify::Event>>) -> Self {
        Self {
            tx,
            // Debounce 100ms seems reasonable
            debounce_duration: Duration::from_millis(100),
            last_run: None,
        }
    }
}

impl notify::EventHandler for EventForwarder {
    fn handle_event(&mut self, event: notify::Result<notify::Event>) {
        let now = Instant::now();
        if let Some(last_run) = self.last_run {
            // Debouncing
            if now.duration_since(last_run) > self.debounce_duration {
                if let Err(err) = self.tx.send(event) {
                    log::error!("Error sending notify event: {:?}", err);
                }
                self.last_run = Some(now);
            }
        } else {
            if let Err(err) = self.tx.send(event) {
                log::error!("Error sending notify event: {:?}", err);
            }
            self.last_run = Some(now);
        }
    }
}

pub async fn run_watch(matches: &ArgMatches, ditto_version: &Version) -> Result<()> {
    // Set up the channel
    let (tx, rx) = mpsc::channel();
    let mut watcher = notify::RecommendedWatcher::new(EventForwarder::new(tx)).into_diagnostic()?;

    // Watch ditto.toml and src/**
    // NOTE not watching packages as that seems wasteful...
    // package source isn't going to be touched the majority of the time?
    // We could consider watching packages that are symlinks (i.e. local)
    watcher
        .watch(
            &PathBuf::from(CONFIG_FILE_NAME),
            notify::RecursiveMode::NonRecursive,
        )
        .into_diagnostic()?;
    watcher
        .watch(
            // TODO use src config value
            &PathBuf::from("src"),
            notify::RecursiveMode::Recursive,
        )
        .into_diagnostic()?;

    // Clear screen initially
    // (other watching tools do this)
    clearscreen::clear()
        .into_diagnostic()
        .wrap_err("error clearing screen")?;

    //let print_done = || {
    //    println!("{}", Style::new().green().bold().apply_to("Done"));
    //};

    if let Err(err) = run_once(matches, ditto_version).await {
        // print the error but don't exit!
        eprintln!("{:?}", err);
    }
    //print_done();

    // Listen for changes...
    loop {
        let event = rx.recv().into_diagnostic()?;

        match event {
            Ok(notify::Event {
                kind: notify::EventKind::Modify(_),
                mut paths,
                ..
            }) if paths.len() == 1 => {
                let path = paths.pop().unwrap();
                let event_path_extension = path.extension().and_then(|ext| ext.to_str());
                // Be selective about what we re-run for.
                // I.e. don't re-run for foreign files etc.
                if matches!(
                    event_path_extension,
                    // ditto source file
                    Some("ditto") | 
                    // config file
                    Some("toml")
                ) {
                    clearscreen::clear()
                        .into_diagnostic()
                        .wrap_err("error clearing screen")?;
                    if let Err(err) = run_once(matches, ditto_version).await {
                        // print the error but don't exit!
                        eprintln!("{:?}", err);
                    }
                    //print_done();
                }
            }
            other => {
                log::trace!("Ignoring notify event: {:?}", other);
            }
        }
    }
}

pub async fn run_once(_matches: &ArgMatches, ditto_version: &Version) -> Result<ExitStatus> {
    let config_path: PathBuf = [".", CONFIG_FILE_NAME].iter().collect();
    let config = read_config(&config_path)?;

    // Need to acquire a lock on the build directory as lots of `ditto make`
    // processes running concurrently will cause problems!
    let lock = acquire_lock(&config)?;
    debug!("Lock acquired");

    // Install/remove packages as needed
    // (this is a nicer pattern than requiring a run of a separate CLI command, IMO)
    if !config.dependencies.is_empty() {
        pkg::check_packages_up_to_date(&config)
            .await
            .wrap_err("error checking packages are up to date")?;
    }

    let now = Instant::now(); // for timing

    // Do the work
    let status = make(&config_path, &config, ditto_version)
        .await
        .wrap_err("error running make")?;

    lock.unlock()
        .into_diagnostic()
        .wrap_err("error releasing lock")?;

    debug!("make ran in {}ms", now.elapsed().as_millis());

    Ok(status)
}

async fn make(config_path: &Path, config: &Config, ditto_version: &Version) -> Result<ExitStatus> {
    let (build_ninja, get_warnings) = generate_build_ninja(config_path, config, ditto_version)
        .wrap_err("error generating build.ninja")?;

    trace!("build.ninja generated");

    let mut build_ninja_path = config.ditto_dir.to_path_buf();
    build_ninja_path.push("build");
    build_ninja_path.set_extension("ninja");

    {
        if !config.ditto_dir.exists() {
            fs::create_dir_all(&config.ditto_dir)
                .into_diagnostic()
                .wrap_err(format!(
                    "error creating {}",
                    config.ditto_dir.to_string_lossy()
                ))?;
        }

        let mut handle = fs::File::create(&build_ninja_path)
            .into_diagnostic()
            .wrap_err(format!(
                "error creating ninja build file: {:?}",
                build_ninja_path.to_string_lossy()
            ))?;

        handle
            .write_all(build_ninja.into_syntax().as_bytes())
            .into_diagnostic()
            .wrap_err(format!(
                "error writing {:?}",
                build_ninja_path.to_string_lossy()
            ))?;

        debug!(
            "build.ninja written to {:?}",
            build_ninja_path.to_string_lossy()
        );
    }

    static NINJA_STATUS_MESSAGE: &str = "__NINJA";

    let ninja_exe = get_ninja_exe().await?;
    let mut child = process::Command::new(&ninja_exe)
        .arg("-f")
        .arg(&build_ninja_path)
        .stdout(Stdio::piped())
        // Mark ninja status messages so we can push them to our own progress spinner
        .env("NINJA_STATUS", NINJA_STATUS_MESSAGE)
        // Don't strip color codes, we'll handle that
        // https://github.com/ninja-build/ninja/commit/bf7107bb864d0383028202e3f4a4228c02302961
        .env("CLICOLOR_FORCE", "1")
        // Pass `is_plain` logic down to CLI calls made by ninja
        .env("DITTO_PLAIN", common::is_plain().to_string())
        .spawn()
        .into_diagnostic()
        .wrap_err(format!(
            "error running ninja: {} -f {}",
            ninja_exe,
            build_ninja_path.to_string_lossy()
        ))?;

    let stdout = child.stdout.as_mut().unwrap();
    let stdout_reader = BufReader::new(stdout);
    let mut stdout_lines = stdout_reader.lines();
    if let Some(Ok(first_line)) = stdout_lines.next() {
        // NOTE relying on the format of ninja messages like this could break
        // if DITTO_NINJA is set to a ninja version with a different format
        if first_line.starts_with("ninja: no work to do") {
            // Nothing to do,
            // still need to print warnings though
            let warnings = get_warnings()?;
            if !warnings.is_empty() {
                let warnings_len = warnings.len();
                for (i, warning) in warnings.into_iter().enumerate() {
                    if i == warnings_len - 1 {
                        eprintln!("{:?}", warning);
                    } else {
                        eprint!("{:?}", warning);
                    }
                }
            } else {
                println!("{}", Style::new().white().dim().apply_to("Nothing to do"));
            }
            child
                .wait()
                .into_diagnostic()
                .wrap_err("ninja wasn't running?")
        } else {
            let mut spinner = Spinner::new();
            spinner.set_message(
                first_line
                    .trim_start_matches(NINJA_STATUS_MESSAGE)
                    .to_owned(),
            );

            // Our error/warning reports generally start with a blank line,
            // so we need to replicate that behavior when forwarding ninja
            // output for a consistent experience.
            let mut printed_initial_newline = false;
            while let Some(Ok(line)) = stdout_lines.next() {
                if line.starts_with(NINJA_STATUS_MESSAGE) {
                    spinner.set_message(line.trim_start_matches(NINJA_STATUS_MESSAGE).to_owned());
                } else if line.starts_with("ninja: build stopped: subcommand failed") {
                } else if console::strip_ansi_codes(&line).starts_with("FAILED") {
                    // The following line prints the command that was run (and failed)
                    // so swallow it
                    stdout_lines.next();
                } else {
                    if !printed_initial_newline {
                        spinner.println("\n");
                        printed_initial_newline = true
                    }
                    spinner.println(line);
                }
            }

            let status = child.wait().expect("error waiting for ninja to exit");
            spinner.finish();
            if status.success() {
                // Only print warnings if there wasn't an error
                let warnings = get_warnings()?;
                if !warnings.is_empty() {
                    let warnings_len = warnings.len();
                    for (i, warning) in warnings.into_iter().enumerate() {
                        if i == warnings_len - 1 {
                            eprintln!("{:?}", warning);
                        } else {
                            eprint!("{:?}", warning);
                        }
                    }
                }
            }
            Ok(status)
        }
    } else {
        unreachable!()
    }
}

fn generate_build_ninja(
    config_path: &Path,
    config: &Config,
    ditto_version: &Version,
) -> Result<(BuildNinja, GetWarnings)> {
    let mut build_dir = config.ditto_dir.to_path_buf();
    build_dir.push("build");
    build_dir.push(&ditto_version.semversion.to_string());

    let ditto_bin = current_exe()
        .into_diagnostic()
        .wrap_err("error getting current executable")?;

    let ditto_sources = find_ditto_files(&config.src_dir)?;

    let sources = Sources {
        config: config_path.to_path_buf(),
        ditto: ditto_sources,
    };

    let package_sources =
        get_package_sources(config).wrap_err("error finding ditto files in packages")?;

    let result = make::generate_build_ninja(
        build_dir,
        ditto_bin,
        &ditto_version.semversion,
        COMPILE_SUBCOMMAND,
        sources,
        package_sources,
    );
    if let Err(ref report) = result {
        // This is a bit brittle, but we want parse errors encountered during
        // build planning to be indistinguishable from parse errors encountered
        // during the actual build
        if report.root_cause().to_string() == "syntax error" {
            //                                  ^^ BEWARE relying on this string is brittle,
            eprintln!("{:?}", report);
            std::process::exit(1);
        }
    }
    result
}

fn get_package_sources(config: &Config) -> Result<PackageSources> {
    let mut package_sources = HashMap::new();
    for path in pkg::list_installed_packages(&pkg::mk_packages_dir(config))? {
        let package_name =
            PackageName::new_unchecked(path.file_name().unwrap().to_string_lossy().into_owned());
        let sources = get_sources_for_dir(&path)?;
        package_sources.insert(package_name, sources);
    }
    Ok(package_sources)
}

fn get_sources_for_dir(dir: &Path) -> Result<Sources> {
    let mut config_path = dir.to_path_buf();
    config_path.push(CONFIG_FILE_NAME);
    let config = read_config(&config_path)?;

    let mut src_dir = dir.to_path_buf();
    src_dir.push(config.src_dir);

    let ditto_sources = find_ditto_files(src_dir)?;
    Ok(Sources {
        config: config_path,
        ditto: ditto_sources,
    })
}

fn find_ditto_files<P: AsRef<Path>>(root: P) -> Result<Vec<PathBuf>> {
    make::find_ditto_files(root.as_ref())
        .into_diagnostic()
        .wrap_err(format!(
            "error finding ditto files in {}",
            root.as_ref().to_string_lossy()
        ))
}

static LOCK_FILE: &str = "_lock";

fn acquire_lock(config: &Config) -> Result<impl FileExt> {
    if !config.ditto_dir.exists() {
        debug!(
            "{} doesn't exist, creating",
            config.ditto_dir.to_string_lossy()
        );

        fs::create_dir_all(&config.ditto_dir)
            .into_diagnostic()
            .wrap_err(format!(
                "error creating {}",
                config.ditto_dir.to_string_lossy()
            ))?;
    }

    let mut lock_file = config.ditto_dir.to_path_buf();
    lock_file.push(LOCK_FILE);

    debug!("Opening lock file at {}", lock_file.to_string_lossy());
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(&lock_file)
        .into_diagnostic()
        .wrap_err(format!(
            "error opening lock file {}",
            lock_file.to_string_lossy()
        ))?;

    if file.try_lock_exclusive().is_ok() {
        Ok(file)
    } else {
        println!("Waiting for lock...");
        file.lock_exclusive()
            .into_diagnostic()
            .wrap_err("error waiting for lock")?;
        Ok(file)
    }
}
