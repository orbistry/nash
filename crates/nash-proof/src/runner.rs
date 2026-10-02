use crate::{ProofProgram, lakefile, render};
use nash_source::Expect;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[derive(Debug)]
pub struct Config {
    /// An existing Lake project with the three pinned dependencies built.
    pub lean_project: PathBuf,
    pub fuel: u32,
    pub postcondition_fuel: u32,
    pub solver_timeout: u32,
    pub wall_timeout: Duration,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Verified,
    VerifiedPartial,
    PostconditionExhausted,
    Counterexample,
    Witness,
    NoWitness,
    Unknown,
    BackendError,
    Timeout,
}

#[derive(Debug, Serialize)]
pub struct Outcome {
    pub module: String,
    pub name: String,
    pub status: Status,
    pub fuel: u32,
    pub postcondition_fuel: u32,
    pub trust: &'static str,
    pub counterexample: Vec<String>,
    pub diagnostics: String,
}
impl Outcome {
    pub fn passed(&self) -> bool {
        matches!(
            self.status,
            Status::Verified | Status::VerifiedPartial | Status::Witness
        )
    }
}

#[derive(Deserialize)]
struct Reply {
    query: String,
    status: String,
    counterexample: Vec<String>,
}

/// Write a portable, inspectable Lake project. No solver or network is invoked.
pub fn export(
    programs: &[ProofProgram],
    destination: &Path,
    fuel: u32,
    postcondition_fuel: u32,
    timeout: u32,
) -> std::io::Result<Vec<PathBuf>> {
    // Only reuse a project we created, with the unchanged pinned configuration.
    match fs::create_dir(destination) {
        Ok(()) => {
            fs::write(destination.join("lakefile.lean"), lakefile())?;
            fs::write(
                destination.join("lean-toolchain"),
                "leanprover/lean4:v4.24.0\n",
            )?;
            fs::write(destination.join(".nash-proof-project"), "1\n")?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            if fs::symlink_metadata(destination)?.file_type().is_symlink()
                || fs::read_to_string(destination.join(".nash-proof-project"))? != "1\n"
                || fs::read_to_string(destination.join("lakefile.lean"))? != lakefile()
                || fs::read_to_string(destination.join("lean-toolchain"))?
                    != "leanprover/lean4:v4.24.0\n"
            {
                return Err(std::io::Error::other(
                    "output is not an unchanged Nash proof project; choose another --output",
                ));
            }
        }
        Err(error) => return Err(error),
    }
    // Each run is immutable; the Lake dependency cache stays in the parent project.
    let run_id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(std::io::Error::other)?
        .as_nanos();
    let run = destination.join(format!("run-{}-{run_id}", std::process::id()));
    fs::create_dir(&run)?;
    let mut files = Vec::new();
    let mut manifest = Vec::new();
    for (i, program) in programs.iter().enumerate() {
        let filename = format!("Proof{i}.lean");
        let file = run.join(&filename);
        fs::write(&file, render(program, fuel, postcondition_fuel, timeout))?;
        manifest.push(serde_json::json!({"file": filename, "module": program.module, "name": program.name,
            "fuel": fuel, "postcondition_fuel": postcondition_fuel, "kind": if program.postcondition.is_some() { "partial_correctness" } else { "execution" }, "trust": "smt_verified", "plutus_version": format!("{:?}", program.plutus_version)}));
        files.push(file);
    }
    fs::write(
        run.join("proofs.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )?;
    Ok(files)
}

pub fn run(program: &ProofProgram, source: &Path, config: &Config) -> std::io::Result<Outcome> {
    let stdout = source.with_extension("stdout");
    let stderr = source.with_extension("stderr");
    let source = source.canonicalize()?;
    let mut command = Command::new("lake");
    command
        .current_dir(&config.lean_project)
        .args(["env", "lean"])
        .arg(&source)
        .stdout(Stdio::from(fs::File::create(&stdout)?))
        .stderr(Stdio::from(fs::File::create(&stderr)?));
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command.spawn()?;
    let start = Instant::now();
    let (success, timed_out) = loop {
        if let Some(status) = child.try_wait()? {
            break (status.success(), false);
        }
        if start.elapsed() >= config.wall_timeout {
            #[cfg(unix)]
            unsafe {
                libc::kill(-(child.id() as i32), libc::SIGKILL);
            }
            #[cfg(not(unix))]
            child.kill()?;
            child.wait()?;
            break (false, true);
        }
        std::thread::sleep(Duration::from_millis(25));
    };
    let out = fs::read_to_string(stdout)?;
    let err = fs::read_to_string(stderr)?;
    let (status, counterexample) = classify_mode(
        program.expect,
        program.postcondition.is_some(),
        success,
        timed_out,
        &out,
    );
    Ok(Outcome {
        module: program.module.clone(),
        name: program.name.clone(),
        status,
        fuel: config.fuel,
        postcondition_fuel: config.postcondition_fuel,
        trust: "smt_verified",
        counterexample,
        diagnostics: format!("{out}{err}"),
    })
}

#[cfg(test)]
fn classify(expect: Expect, success: bool, timed_out: bool, output: &str) -> (Status, Vec<String>) {
    classify_mode(expect, false, success, timed_out, output)
}

fn classify_mode(
    expect: Expect,
    partial: bool,
    success: bool,
    timed_out: bool,
    output: &str,
) -> (Status, Vec<String>) {
    if timed_out {
        return (Status::Timeout, vec![]);
    }
    if !success {
        return (Status::BackendError, vec![]);
    }
    let mut replies = Vec::new();
    for line in output.lines() {
        if let Some(json) = line.strip_prefix("@@NASH_PROOF@@") {
            let Ok(reply) = serde_json::from_str::<Reply>(json) else {
                return (Status::BackendError, vec![]);
            };
            if !matches!(reply.status.as_str(), "valid" | "falsified" | "unknown") {
                return (Status::BackendError, vec![]);
            }
            replies.push(reply);
        }
    }
    let existential = expect == Expect::FailOnce;
    let queries: &[&str] = if partial {
        &["postcondition_completion", "property"]
    } else {
        &["property"]
    };
    if !replies
        .iter()
        .map(|r| r.query.as_str())
        .eq(queries.iter().copied())
    {
        return (Status::BackendError, vec![]);
    }
    if partial {
        if expect != Expect::Pass {
            return (Status::BackendError, vec![]);
        }
        match replies[0].status.as_str() {
            "valid" => {}
            "falsified" => {
                return (
                    Status::PostconditionExhausted,
                    replies.remove(0).counterexample,
                );
            }
            _ => return (Status::Unknown, vec![]),
        }
    }
    let property = replies.pop().unwrap();
    let status = match (existential, property.status.as_str()) {
        (false, "valid") if partial => Status::VerifiedPartial,
        (false, "valid") => Status::Verified,
        (false, "falsified") => Status::Counterexample,
        (true, "falsified") => Status::Witness,
        // All inputs succeed within the configured execution limit.
        (true, "valid") => Status::NoWitness,
        _ => Status::Unknown,
    };
    (status, property.counterexample)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn reply(query: &str, status: &str) -> String {
        format!(
            "@@NASH_PROOF@@{{\"query\":\"{query}\",\"status\":\"{status}\",\"counterexample\":[]}}\n"
        )
    }
    #[test]
    fn unknown_and_crashed_runs_never_pass() {
        let output = reply("property", "unknown");
        assert_eq!(
            classify(Expect::Pass, true, false, &output).0,
            Status::Unknown
        );
        assert_eq!(
            classify(Expect::Pass, false, false, &output).0,
            Status::BackendError
        );
        assert_eq!(
            classify(Expect::Pass, true, false, "✅ Valid").0,
            Status::BackendError
        );
    }
    #[test]
    fn existential_failure_requires_an_actual_witness() {
        assert_eq!(
            classify(
                Expect::FailOnce,
                true,
                false,
                &reply("property", "falsified")
            )
            .0,
            Status::Witness
        );
        assert_eq!(
            classify(Expect::FailOnce, true, false, &reply("property", "unknown")).0,
            Status::Unknown
        );
    }
    #[test]
    fn malformed_duplicate_and_missing_replies_fail_closed() {
        for output in [
            String::new(),
            reply("completion", "valid")
                + &reply("property", "valid")
                + &reply("property", "valid"),
            reply("completion", "valid") + "@@NASH_PROOF@@bad\n",
        ] {
            assert_eq!(
                classify(Expect::Pass, true, false, &output).0,
                Status::BackendError
            );
        }
    }
    #[test]
    fn partial_correctness_requires_a_completed_postcondition() {
        for (completion, property, expected) in [
            ("valid", "valid", Status::VerifiedPartial),
            ("valid", "falsified", Status::Counterexample),
            ("falsified", "valid", Status::PostconditionExhausted),
            ("unknown", "valid", Status::Unknown),
            ("valid", "unknown", Status::Unknown),
        ] {
            let output =
                reply("postcondition_completion", completion) + &reply("property", property);
            assert_eq!(
                classify_mode(Expect::Pass, true, true, false, &output).0,
                expected
            );
        }
        assert_eq!(
            classify_mode(Expect::Pass, true, true, false, &reply("property", "valid")).0,
            Status::BackendError
        );
    }
}
