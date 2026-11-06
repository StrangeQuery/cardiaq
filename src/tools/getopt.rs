//! The command line the GNU way, for burn and carc: short options (`-o FILE`, `-oFILE`, several flags in one `-hq`),
//! long ones (`--output=FILE`, `--output FILE`, or any unambiguous start of the name, `--out`), `--` ending the
//! options, and `-` an operand (standard input or output). Mistakes are reported as getopt reports them, with
//! `Try 'PROG --help'`, and exit 2.

/// The name the program goes by in its messages and usage: `burn`, say, or `cargo cardiac run`. Set by each
/// program's `main` as it starts.
static PROG: std::sync::Mutex<&'static str> = std::sync::Mutex::new("");

/// Name the program `prog` (see `prog`).
pub fn set_prog(prog: &str) {
    *PROG.lock().unwrap_or_else(|e| e.into_inner()) = Box::leak(prog.to_string().into_boxed_str());
}

/// The name the program goes by, as `set_prog` gave it.
pub fn prog() -> &'static str {
    *PROG.lock().unwrap_or_else(|e| e.into_inner())
}

/// An option: its letter (if it has one), its long name, and whether it takes an argument.
pub struct Opt {
    pub short: Option<char>,
    pub long: &'static str,
    pub arg: bool,
}

/// `PROG: WHY` on standard error, and exit 1: a program that can't be read, a file that can't be written.
pub fn die(prog: &str, why: impl std::fmt::Display) -> ! {
    eprintln!("{prog}: {why}");
    std::process::exit(1)
}

/// A mistake on the command line, as getopt reports one, and exit 2.
pub fn usage_error(prog: &str, why: impl std::fmt::Display) -> ! {
    eprintln!("{prog}: {why}\nTry '{prog} --help' for more information.");
    std::process::exit(2)
}

/// What the OS says went wrong, as a C program says it: "No such file or directory", without Rust's "(os error 2)".
pub fn os_error(e: &std::io::Error) -> String {
    let s = e.to_string();
    match s.find(" (os error ") {
        Some(i) => s[..i].to_string(),
        None => s,
    }
}

/// `--version`: the program, its package, its version; the copyright and the licence.
pub fn version(prog: &str) {
    println!(
        "{prog} (heartburn) {}\nCopyright (c) 2026 Strange Query\nLicense MIT: <https://opensource.org/licenses/MIT>\n\
         This is free software: you are free to change and redistribute it.\n\
         There is NO WARRANTY, to the extent permitted by law.",
        env!("CARGO_PKG_VERSION")
    );
}

/// The options given (each by its long name, with its argument) and the operands, in order.
pub type Parsed = (Vec<(&'static str, Option<String>)>, Vec<String>);

/// The options given and the operands, as `try_parse` reads them; a mistake reported as getopt reports it, and
/// exit 2.
pub fn parse(prog: &str, opts: &[Opt], args: impl IntoIterator<Item = String>, first_operand_ends: bool) -> Parsed {
    try_parse(opts, args, first_operand_ends).unwrap_or_else(|why| usage_error(prog, why))
}

/// The options given (each by its long name, with its argument) and the operands, in order; or the mistake, as
/// getopt words it. Options may come anywhere among the operands, as gcc takes them; or, with
/// `first_operand_ends`, only before the first, the rest belonging to it (as `env` and `gdb --args` take them).
pub fn try_parse(
    opts: &[Opt],
    args: impl IntoIterator<Item = String>,
    first_operand_ends: bool,
) -> Result<Parsed, String> {
    let (mut given, mut operands) = (vec![], vec![]);
    let mut args = args.into_iter();
    while let Some(a) = args.next() {
        if a == "--" {
            operands.extend(args.by_ref());
        } else if let Some(body) = a.strip_prefix("--") {
            let (name, value) = match body.split_once('=') {
                Some((n, v)) => (n, Some(v.to_string())),
                None => (body, None),
            };
            let o = match opts.iter().find(|o| o.long == name) {
                Some(o) => o,
                None => match opts
                    .iter()
                    .filter(|o| !name.is_empty() && o.long.starts_with(name))
                    .collect::<Vec<_>>()[..]
                {
                    [o] => o,
                    [] => return Err(format!("unrecognized option '{a}'")),
                    ref several => {
                        let names: Vec<String> = several.iter().map(|o| format!("'--{}'", o.long)).collect();
                        return Err(format!(
                            "option '--{name}' is ambiguous; possibilities: {}",
                            names.join(" ")
                        ));
                    }
                },
            };
            let value = match (o.arg, value) {
                (false, Some(_)) => return Err(format!("option '--{}' doesn't allow an argument", o.long)),
                (true, None) => Some(
                    args.next()
                        .ok_or_else(|| format!("option '--{}' requires an argument", o.long))?,
                ),
                (_, v) => v,
            };
            given.push((o.long, value));
        } else if a.len() > 1 && a.starts_with('-') {
            for (i, c) in a[1..].char_indices() {
                let o = opts
                    .iter()
                    .find(|o| o.short == Some(c))
                    .ok_or_else(|| format!("invalid option -- '{c}'"))?;
                if !o.arg {
                    given.push((o.long, None));
                    continue;
                }
                // the rest of the word is its argument, or else the next word is
                let rest = &a[1 + i + c.len_utf8()..];
                let value = match rest {
                    "" => args
                        .next()
                        .ok_or_else(|| format!("option requires an argument -- '{c}'"))?,
                    r => r.to_string(),
                };
                given.push((o.long, Some(value)));
                break;
            }
        } else {
            operands.push(a);
            if first_operand_ends {
                operands.extend(args.by_ref());
            }
        }
    }
    Ok((given, operands))
}
