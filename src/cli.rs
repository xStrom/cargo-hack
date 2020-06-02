use anyhow::{bail, format_err, Error};
use std::{env, fmt, mem, rc::Rc, str::FromStr};
use termcolor::ColorChoice;

use crate::{ProcessBuilder, Result};

fn print_version() {
    println!("{0} {1}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"),)
}

// (short flag, long flag, short descriptions, additional descriptions)
const HELP: &[(&str, &str, &str, &[&str])] = &[
    ("-p", "--package <SPEC>...", "Package(s) to check", &[]),
    ("", "--all", "Alias for --workspace", &[]),
    ("", "--workspace", "Perform command for all packages in the workspace", &[]),
    ("", "--exclude <SPEC>...", "Exclude packages from the check", &[]),
    ("", "--manifest-path <PATH>", "Path to Cargo.toml", &[]),
    ("", "--features <FEATURES>", "Space-separated list of features to activate", &[]),
    (
        "",
        "--each-feature",
        "Perform for each feature which includes --no-default-features flag and default features of the package",
        &[],
    ),
    (
        "",
        "--feature-powerset",
        "Perform for the feature powerset which includes --no-default-features flag and default features of the package",
        &[],
    ),
    ("", "--optional-deps", "Use optional dependencies as features", &[
        "This flag can only be used with either --each-feature flag or --feature-powerset flag.",
    ]),
    ("", "--skip <FEATURES>", "Space-separated list of features to skip", &[
        "To skip run of default feature, using value `--skip default`.",
        "This flag can only be used with either --each-feature flag or --feature-powerset flag.",
    ]),
    ("", "--skip-no-default-features", "Skip run of just --no-default-features flag", &[
        "This flag can only be used with either --each-feature flag or --feature-powerset flag.",
    ]),
    ("", "--no-dev-deps", "Perform without dev-dependencies", &[
        "This flag removes dev-dependencies from real `Cargo.toml` while cargo-hack is running and restores it when finished.",
    ]),
    (
        "",
        "--remove-dev-deps",
        "Equivalent to --no-dev-deps flag except for does not restore the original `Cargo.toml` after performed",
        &[],
    ),
    ("", "--ignore-private", "Skip to perform on `publish = false` packages", &[]),
    (
        "",
        "--ignore-unknown-features",
        "Skip passing --features flag to `cargo` if that feature does not exist in the package",
        &[],
    ),
    ("-v", "--verbose", "Use verbose output", &["This flag will be propagated to cargo."]),
    ("", "--color <WHEN>", "Coloring: auto, always, never", &[
        "This flag will be propagated to cargo.",
    ]),
    ("-h", "--help", "Prints help information", &[]),
    ("-V", "--version", "Prints version information", &[]),
];

struct Help {
    long: bool,
    term_size: usize,
}

impl Help {
    fn new(long: bool) -> Self {
        Self { long, term_size: term_size::dimensions().map_or(120, |(w, _)| w) }
    }
}

impl fmt::Display for Help {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fn write(
            f: &mut fmt::Formatter<'_>,
            indent: usize,
            require_first_indent: bool,
            term_size: usize,
            desc: &str,
        ) -> fmt::Result {
            if require_first_indent {
                (0..indent).try_for_each(|_| write!(f, " "))?;
            }
            let mut written = 0;
            let size = term_size - indent;
            for s in desc.split(' ') {
                if written + s.len() + 1 >= size {
                    writeln!(f)?;
                    (0..indent).try_for_each(|_| write!(f, " "))?;
                    write!(f, "{}", s)?;
                    written = s.len();
                } else if written == 0 {
                    write!(f, "{}", s)?;
                    written += s.len();
                } else {
                    write!(f, " {}", s)?;
                    written += s.len() + 1;
                }
            }
            Ok(())
        }

        writeln!(
            f,
            "\
{0} {1}\n{2}\n\n{3}
USAGE:
    cargo hack [OPTIONS] [SUBCOMMAND]\n
Use -h for short descriptions and --help for more details.\n
OPTIONS:",
            env!("CARGO_PKG_NAME"),
            env!("CARGO_PKG_VERSION"),
            env!("CARGO_PKG_AUTHORS"),
            env!("CARGO_PKG_DESCRIPTION")
        )?;

        for &(short, long, desc, additional) in HELP {
            write!(f, "    {:2}{} ", short, if short.is_empty() { " " } else { "," })?;
            if self.long {
                writeln!(f, "{}", long)?;
                write(f, 12, true, self.term_size, desc)?;
                writeln!(f, ".\n")?;
                for desc in additional {
                    write(f, 12, true, self.term_size, desc)?;
                    writeln!(f, "\n")?;
                }
            } else {
                write!(f, "{:25} ", long)?;
                write(f, 34, false, self.term_size, desc)?;
                writeln!(f)?;
            }
        }
        if !self.long {
            writeln!(f)?;
        }

        writeln!(
            f,
            "\
Some common cargo commands are (see all commands with --list):
        build       Compile the current package
        check       Analyze the current package and report errors, but don't build object files
        run         Run a binary or example of the local package
        test        Run the tests
"
        )
    }
}

#[derive(Debug)]
pub(crate) struct Args {
    pub(crate) leading_args: Rc<[String]>,
    pub(crate) trailing_args: Rc<[String]>,

    pub(crate) subcommand: Option<String>,

    /// --manifest-path <PATH>
    pub(crate) manifest_path: Option<String>,
    /// -p, --package <SPEC>...
    pub(crate) package: Vec<String>,
    /// --exclude <SPEC>...
    pub(crate) exclude: Vec<String>,
    /// --workspace, (--all)
    pub(crate) workspace: bool,
    /// --each-feature
    pub(crate) each_feature: bool,
    /// --feature-powerset
    pub(crate) feature_powerset: bool,
    /// --skip <FEATURES>...
    pub(crate) skip: Vec<String>,
    /// --no-dev-deps
    pub(crate) no_dev_deps: bool,
    /// --remove-dev-deps
    pub(crate) remove_dev_deps: bool,
    /// --ignore-private
    pub(crate) ignore_private: bool,
    /// --ignore-unknown-features, (--ignore-non-exist-features)
    pub(crate) ignore_unknown_features: bool,
    /// --optional-deps
    pub(crate) optional_deps: bool,
    /// --skip-no-default-features
    pub(crate) skip_no_default_features: bool,

    // flags that will be propagated to cargo
    /// --features <FEATURES>...
    pub(crate) features: Vec<String>,
    /// --color <WHEN>
    pub(crate) color: Option<Coloring>,
    /// -v, --verbose, -vv
    pub(crate) verbose: bool,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum Coloring {
    Auto,
    Always,
    Never,
}

impl Coloring {
    pub(crate) fn color_choice(color: Option<Coloring>) -> ColorChoice {
        match color {
            Some(Coloring::Auto) | None => ColorChoice::Auto,
            Some(Coloring::Always) => ColorChoice::Always,
            Some(Coloring::Never) => ColorChoice::Never,
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Coloring::Auto => "auto",
            Coloring::Always => "always",
            Coloring::Never => "never",
        }
    }
}

impl FromStr for Coloring {
    type Err = Error;

    fn from_str(name: &str) -> Result<Self> {
        match name {
            "auto" => Ok(Coloring::Auto),
            "always" => Ok(Coloring::Always),
            "never" => Ok(Coloring::Never),
            other => bail!("must be auto, always, or never, but found `{}`", other),
        }
    }
}

pub(crate) fn args(coloring: &mut Option<Coloring>) -> Result<Option<Args>> {
    let mut args = env::args();
    let _ = args.next(); // executable name
    match &args.next() {
        Some(a) if a == "hack" => {}
        Some(_) | None => {
            println!("{}", Help::new(false));
            return Ok(None);
        }
    }

    let mut leading = Vec::new();
    let mut subcommand: Option<String> = None;

    let mut manifest_path = None;
    let mut color = None;

    let mut package = Vec::new();
    let mut exclude = Vec::new();
    let mut features = Vec::new();
    let mut skip = Vec::new();

    let mut workspace = None;
    let mut no_dev_deps = false;
    let mut remove_dev_deps = false;
    let mut each_feature = false;
    let mut feature_powerset = false;
    let mut ignore_private = false;
    let mut ignore_unknown_features = false;
    let mut ignore_non_exist_features = false;
    let mut optional_deps = false;
    let mut skip_no_default_features = false;

    let res = (|| -> Result<()> {
        while let Some(arg) = args.next() {
            // stop at `--`
            // 1. `cargo hack check --no-dev-deps`
            //   first:  `cargo hack check --no-dev-deps` (filtered and passed to `cargo`)
            //   second: (empty)
            // 2. `cargo hack test --each-feature -- --ignored`
            //   first:  `cargo hack test --each-feature` (filtered and passed to `cargo`)
            //   second: `--ignored` (passed directly to `cargo` with `--`)
            if arg == "--" {
                break;
            }

            if !arg.starts_with('-') {
                subcommand.get_or_insert_with(|| arg.clone());
                leading.push(arg);
                continue;
            }

            macro_rules! parse_arg1 {
                ($ident:ident, $propagate:expr, $pat:expr, $help:expr) => {
                    if arg == $pat {
                        if $ident.is_some() {
                            return Err(multi_arg($help, subcommand.as_ref()));
                        }
                        let next =
                            args.next().ok_or_else(|| req_arg($help, subcommand.as_ref()))?;
                        if $propagate {
                            $ident = Some(next.clone());
                            leading.push(arg);
                            leading.push(next);
                        } else {
                            $ident = Some(next);
                        }
                        continue;
                    } else if arg.starts_with(concat!($pat, "=")) {
                        if $ident.is_some() {
                            return Err(multi_arg($help, subcommand.as_ref()));
                        }
                        let next = arg
                            .splitn(2, '=')
                            .nth(1)
                            .ok_or_else(|| req_arg($help, subcommand.as_ref()))?;
                        $ident = Some(next.to_string());
                        if $propagate {
                            leading.push(arg);
                        }
                        continue;
                    }
                };
            }
            macro_rules! parse_arg2 {
                ($ident:ident, $allow_split:expr, $pat:expr, $help:expr) => {
                    if arg == $pat {
                        let arg = args.next().ok_or_else(|| req_arg($help, subcommand.as_ref()))?;
                        if $allow_split {
                            $ident.extend(arg.split(',').map(ToString::to_string));
                        } else {
                            $ident.push(arg);
                        }
                        continue;
                    } else if arg.starts_with(concat!($pat, "=")) {
                        let arg = arg
                            .splitn(2, '=')
                            .nth(1)
                            .ok_or_else(|| req_arg($help, subcommand.as_ref()))?;
                        if $allow_split {
                            $ident.extend(arg.split(',').map(ToString::to_string));
                        } else {
                            $ident.push(arg.to_string());
                        }
                        continue;
                    }
                };
            }

            parse_arg1!(manifest_path, false, "--manifest-path", "--manifest-path <PATH>");
            parse_arg1!(color, true, "--color", "--color <WHEN>");

            parse_arg2!(package, false, "--package", "--package <SPEC>");
            parse_arg2!(package, false, "-p", "--package <SPEC>");
            parse_arg2!(exclude, false, "--exclude", "--exclude <SPEC>");
            parse_arg2!(features, true, "--features", "--features <FEATURES>");
            parse_arg2!(skip, true, "--skip", "--skip <FEATURES>");

            match arg.as_str() {
                "--workspace" | "--all" => {
                    if let Some(arg) = workspace.replace(arg) {
                        return Err(multi_arg(&arg, subcommand.as_ref()));
                    }
                }
                "--no-dev-deps" => {
                    if mem::replace(&mut no_dev_deps, true) {
                        return Err(multi_arg(&arg, subcommand.as_ref()));
                    }
                }
                "--remove-dev-deps" => {
                    if mem::replace(&mut remove_dev_deps, true) {
                        return Err(multi_arg(&arg, subcommand.as_ref()));
                    }
                }
                "--each-feature" => {
                    if mem::replace(&mut each_feature, true) {
                        return Err(multi_arg(&arg, subcommand.as_ref()));
                    }
                }
                "--feature-powerset" => {
                    if mem::replace(&mut feature_powerset, true) {
                        return Err(multi_arg(&arg, subcommand.as_ref()));
                    }
                }
                "--ignore-private" => {
                    if mem::replace(&mut ignore_private, true) {
                        return Err(multi_arg(&arg, subcommand.as_ref()));
                    }
                }
                "--optional-deps" => {
                    if mem::replace(&mut optional_deps, true) {
                        return Err(multi_arg(&arg, subcommand.as_ref()));
                    }
                }
                "--skip-no-default-features" => {
                    if mem::replace(&mut skip_no_default_features, true) {
                        return Err(multi_arg(&arg, subcommand.as_ref()));
                    }
                }
                "--ignore-unknown-features" => {
                    if ignore_unknown_features || ignore_non_exist_features {
                        return Err(multi_arg(&arg, subcommand.as_ref()));
                    }
                    ignore_unknown_features = true;
                }
                "--ignore-non-exist-features" => {
                    if ignore_unknown_features || ignore_non_exist_features {
                        return Err(multi_arg("--ignore-unknown-features", subcommand.as_ref()));
                    }
                    ignore_non_exist_features = true;
                }
                _ => leading.push(arg),
            }
        }

        Ok(())
    })();

    let color = color.map(|c| c.parse()).transpose()?;
    *coloring = color;

    res?;

    if leading.is_empty() && !remove_dev_deps
        || subcommand.is_none() && leading.iter().any(|a| a == "-h")
    {
        println!("{}", Help::new(false));
        return Ok(None);
    } else if subcommand.is_none() && leading.iter().any(|a| a == "--help") {
        println!("{}", Help::new(true));
        return Ok(None);
    } else if leading.iter().any(|a| a == "--version" || a == "-V" || a == "-vV" || a == "-Vv") {
        print_version();
        return Ok(None);
    }

    if !exclude.is_empty() && workspace.is_none() {
        bail!("--exclude can only be used together with --workspace");
    }
    if !each_feature && !feature_powerset {
        if !skip.is_empty() {
            bail!("--skip can only be used with either --each-feature or --feature-powerset");
        } else if optional_deps {
            bail!(
                "--optional-deps can only be used with either --each-feature or --feature-powerset"
            );
        } else if skip_no_default_features {
            bail!(
                "--skip-no-default-features can only be used with either --each-feature or --feature-powerset"
            );
        }
    }

    if let Some(subcommand) = &subcommand {
        if subcommand == "test" || subcommand == "bench" {
            if remove_dev_deps {
                bail!("--remove-dev-deps may not be used together with {} subcommand", subcommand);
            } else if no_dev_deps {
                bail!("--no-dev-deps may not be used together with {} subcommand", subcommand);
            }
        }
    }
    if let Some(pos) = leading.iter().position(|a| match a.as_str() {
        "--example" | "--examples" | "--test" | "--tests" | "--bench" | "--benches"
        | "--all-targets" => true,
        _ => a.starts_with("--example=") || a.starts_with("--test=") || a.starts_with("--bench="),
    }) {
        if remove_dev_deps {
            bail!("--remove-dev-deps may not be used together with {}", leading[pos]);
        } else if no_dev_deps {
            bail!("--no-dev-deps may not be used together with {}", leading[pos]);
        }
    }

    if no_dev_deps && remove_dev_deps {
        bail!("--no-dev-deps may not be used together with --remove-dev-deps");
    }
    if each_feature && feature_powerset {
        bail!("--each-feature may not be used together with --feature-powerset");
    }

    if subcommand.is_none() {
        if leading.iter().any(|a| a == "--list") {
            let mut line = ProcessBuilder::new(crate::cargo_binary());
            line.arg("--list");
            line.exec()?;
            return Ok(None);
        } else if !remove_dev_deps {
            // TODO: improve this
            bail!(
                "\
No subcommand or valid flag specified.

USAGE:
    cargo hack [OPTIONS] [SUBCOMMAND]

For more information try --help
"
            );
        }
    }

    let verbose = leading.iter().any(|a| a == "--verbose" || a == "-v" || a == "-vv");
    if ignore_non_exist_features {
        warn!(
            color,
            "'--ignore-non-exist-features' flag is deprecated, use '--ignore-unknown-features' flag instead"
        );
    }
    if no_dev_deps {
        info!(
            color,
            "`--no-dev-deps` flag removes dev-dependencies from real `Cargo.toml` while cargo-hack is running and restores it when finished"
        )
    }

    Ok(Some(Args {
        leading_args: leading.into(),
        // shared_from_iter requires Rust 1.37
        trailing_args: args.collect::<Vec<_>>().into(),

        subcommand,

        manifest_path,
        package,
        exclude,
        workspace: workspace.is_some(),
        each_feature,
        feature_powerset,
        skip,
        no_dev_deps,
        remove_dev_deps,
        ignore_private,
        ignore_unknown_features: ignore_unknown_features || ignore_non_exist_features,
        optional_deps,
        skip_no_default_features,

        features,
        color,
        verbose,
    }))
}

fn req_arg(arg: &str, subcommand: Option<&String>) -> Error {
    format_err!(
        "\
The argument '{0}' requires a value but none was supplied

USAGE:
    cargo hack{1} {0}

For more information try --help
",
        arg,
        if let Some(subcommand) = subcommand {
            String::from(" ") + subcommand
        } else {
            String::from("")
        }
    )
}

fn multi_arg(arg: &str, subcommand: Option<&String>) -> Error {
    format_err!(
        "\
The argument '{0}' was provided more than once, but cannot be used multiple times

USAGE:
    cargo hack{1} {0}

For more information try --help
",
        arg,
        if let Some(subcommand) = subcommand {
            String::from(" ") + subcommand
        } else {
            String::from("")
        }
    )
}
