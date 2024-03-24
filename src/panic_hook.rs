//! `human_panic` tailored more to our needs
//!
//! We provide the report data in markdown so that it can be pasted into a github issue and provide
//! actionable information on how to find and submit issues

// We need to display info to `stderr`, and the macro makes it harder to use a more local `allow`
#![allow(clippy::print_stderr)]

use std::{
    fmt::Write,
    hash::Hasher,
    io,
    panic::PanicInfo,
    path::{Path, PathBuf},
};

use human_panic::{report::Method, Metadata};
use serde::Deserialize;

#[macro_export]
macro_rules! setup_panic {
    () => {
        match ::human_panic::PanicStyle::default() {
            ::human_panic::PanicStyle::Debug => {}
            ::human_panic::PanicStyle::Human => {
                let meta = ::human_panic::metadata!();

                ::std::panic::set_hook(::std::boxed::Box::new(
                    move |info: &::std::panic::PanicInfo| {
                        eprintln!("{info}");
                        let file_path = $crate::panic_hook::handle_dump(&meta, info);
                        $crate::panic_hook::print_msg(file_path.as_deref(), &meta).unwrap();
                    },
                ));
            }
        }
    };
}

#[derive(Deserialize)]
struct Report {
    name: String,
    operating_system: String,
    crate_version: String,
    explanation: String,
    cause: String,
    backtrace: String,
}

impl Report {
    fn new(name: &str, version: &str, method: Method, explanation: String, cause: String) -> Self {
        human_panic::report::Report::new(name, version, method, explanation, cause).into()
    }

    fn serialize(&self) -> Option<String> {
        let Self {
            name,
            operating_system,
            crate_version,
            explanation,
            cause,
            backtrace,
        } = self;

        let explanation = explanation.trim();

        let mut buf = String::new();
        write!(
            buf,
            "\
# Crash Report

| Name | `{name}` |
| ---: | :--- |
| Version | `{crate_version}` |
| Operating System | {operating_system} |

`````text
Cause: {cause}

Explanation:
{explanation}
`````

<details>
<summary>(backtrace)</summary>

`````text{backtrace}
`````

</details>

---

<!-- Add any relevant info below vv -->"
        )
        .ok()?;
        Some(buf)
    }

    fn persist(&self) -> Option<PathBuf> {
        let contents = self.serialize()?;
        let tmp_dir = std::env::temp_dir();
        let report_uid = {
            let mut hasher = twox_hash::XxHash64::default();
            hasher.write(contents.as_bytes());
            hasher.finish()
        };
        let report_filename = format!("inlyne-report-{report_uid:x}.md");
        let report_path = tmp_dir.join(report_filename);
        std::fs::write(&report_path, &contents).ok()?;

        Some(report_path)
    }
}

impl From<human_panic::report::Report> for Report {
    fn from(report: human_panic::report::Report) -> Self {
        let toml_text = toml::to_string(&report).unwrap();
        toml::from_str(&toml_text).unwrap()
    }
}

pub fn handle_dump(meta: &Metadata, panic_info: &PanicInfo) -> Option<PathBuf> {
    let mut expl = String::new();

    let message = match (
        panic_info.payload().downcast_ref::<&str>(),
        panic_info.payload().downcast_ref::<String>(),
    ) {
        (Some(s), _) => Some(s.to_string()),
        (_, Some(s)) => Some(s.to_string()),
        (None, None) => None,
    };

    let cause = match message {
        Some(m) => m,
        None => "Unknown".into(),
    };

    match panic_info.location() {
        Some(location) => {
            let file = location.file();
            let line = location.line();
            expl.push_str(&format!("Panic occurred in file '{file}' at line {line}\n",))
        }
        None => expl.push_str("Panic location unknown.\n"),
    }

    let report = Report::new(&meta.name, &meta.version, Method::Panic, expl, cause);
    let maybe = report.persist();
    if maybe.is_none() {
        eprintln!("{}", report.serialize().unwrap());
    }

    maybe
}

pub fn print_msg(file_path: Option<&Path>, meta: &Metadata) -> Option<()> {
    use io::Write as _;

    let stderr = anstream::stderr();
    let mut stderr = stderr.lock();

    write!(stderr, "{}", anstyle::AnsiColor::Red.render_fg()).ok()?;
    write_msg(&mut stderr, file_path, meta)?;
    write!(stderr, "{}", anstyle::Reset.render()).ok()?;

    Some(())
}

fn write_msg(buffer: &mut impl io::Write, file_path: Option<&Path>, meta: &Metadata) -> Option<()> {
    let name = &meta.name;
    let report_path = match file_path {
        Some(fp) => format!("{}", fp.display()),
        None => "<Failed to store file to disk>".to_string(),
    };

    write!(
        buffer,
        "\
Well, this is embarrassing.

{name} had a problem and crashed. To help us diagnose the problem you can send us a crash report.
We have generated a report file at \"{report_path}\". You can search
for issues with similar explanations at the following url:

- https://github.com/Inlyne-Project/inlyne/issues?q=label%3AC-crash-report

and you can submit a new crash report using the report file as a template if there are no existing
issues matching your own (the following link has the crash report label)

- https://github.com/Inlyne-Project/inlyne/issues/new?labels=C-crash-report

We take privacy seriously, and do not preform any auotmated error collection. In order to improve
the software we, rely on people to submit reports.

Thank you kindly!"
    )
    .ok()?;

    Some(())
}
