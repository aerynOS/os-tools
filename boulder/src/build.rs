// SPDX-FileCopyrightText: 2024 AerynOS Developers
// SPDX-License-Identifier: MPL-2.0

use std::{
    collections::BTreeSet,
    io::{self, Write},
    os::unix::process::ExitStatusExt,
    path::{Path, PathBuf},
    process, thread,
};

use ::container::Container;
use fs_err as fs;
use futures_util::TryStreamExt;
use itertools::Itertools;
use moss::{environment, repository, runtime, util};
use nix::{
    sys::signal::Signal,
    unistd::{Pid, getpgrp, setpgid},
};
use thiserror::Error;
use tui::{ProgressBar, ProgressStyle, Styled};

use self::job::{Job, pgo_dir};
use crate::{
    Env, Macros, Paths, Recipe, Timing,
    architecture::BuildTarget,
    build::script::ScriptBundle,
    container, macros, profile, recipe, timing, ui,
    upstream::{self, Upstream, update_git_upstream_refs},
};

pub mod job;
pub mod pgo;
mod root;
mod script;

pub struct Builder {
    pub targets: Vec<Target>,
    pub recipe: Recipe,
    pub paths: Paths,
    pub macros: Macros,
    pub ccache: bool,
    pub env: Env,
    upstreams: Vec<Upstream>,
    repos: repository::Map,
}

pub struct Target {
    pub build_target: BuildTarget,
    pub jobs: Vec<Job>,
}

impl Builder {
    pub fn new(
        recipe_path: &Path,
        verify_against_manifest: Option<PathBuf>,
        env: Env,
        profile: profile::Id,
        ccache: bool,
        output_dir: impl Into<PathBuf>,
    ) -> Result<Self, Error> {
        let recipe = Recipe::load(recipe_path)?;

        let macros = Macros::load(&env)?;

        let paths = Paths::new(&recipe, verify_against_manifest, &env.cache_dir, "/mason", output_dir)?;

        let build_targets = recipe.build_targets();

        if build_targets.is_empty() {
            return Err(Error::NoBuildTargets);
        }

        let targets = build_targets
            .into_iter()
            .map(|build_target| {
                let stages = pgo::stages(&recipe, build_target)
                    .map(|stages| stages.into_iter().map(Some).collect::<Vec<_>>())
                    .unwrap_or_else(|| vec![None]);

                let jobs = stages
                    .into_iter()
                    .map(|stage| Job::new(build_target, stage, &recipe, &paths, &macros, ccache))
                    .collect::<Result<Vec<_>, _>>()?;

                Ok(Target { build_target, jobs })
            })
            .collect::<Result<Vec<_>, job::Error>>()?;

        let upstreams = upstream::parse_recipe(&recipe)?;

        let profiles = profile::Manager::new(&env);
        let repos = profiles.repositories(&profile)?.clone();

        Ok(Self {
            targets,
            recipe,
            paths,
            macros,
            ccache,
            env,
            upstreams,
            repos,
        })
    }

    pub fn extra_deps(&self) -> Result<Vec<String>, Error> {
        let mut dependencies = BTreeSet::new();
        for target in &self.targets {
            for job in &target.jobs {
                for script in job.phases.values() {
                    dependencies.extend(script.dependencies().iter().cloned());
                }
            }
        }
        Ok(dependencies.into_iter().collect())
    }

    pub fn setup(
        &mut self,
        timing: &mut Timing,
        initialize_timer: timing::Timer,
        update_repos: bool,
        cont: &mut Container,
    ) -> Result<Vec<upstream::Stored>, Error> {
        // Recreate artifacts
        util::recreate_dir(&self.paths.artefacts().host).map_err(Error::RecreateArtefactsDir)?;

        // Recreate rootfs
        root::recreate(self)?;

        // Get extra deps
        let extra_deps = self.extra_deps()?;

        // Populate rootfs
        root::populate(
            self,
            self.repos.clone(),
            timing,
            initialize_timer,
            update_repos,
            extra_deps,
        )?;

        let timer = timing.begin(timing::Kind::Fetch);

        let stored = runtime::block_on(
            ui::run_parallel(
                environment::MAX_NETWORK_CONCURRENCY,
                self.upstreams.iter(),
                async |ups, pb| store_upstream(ups, &self.paths.upstreams().host, pb).await,
            )
            .try_collect::<Vec<_>>(),
        )?;

        self.share_upstreams(cont, &stored)?;

        // FIXME: When building, we should open the recipe file as R/O.
        // Updating refs should be performed in `recipe new` or `recipe update` only.
        if let Some(updated_yaml) = update_git_upstream_refs(&self.recipe.source, &stored) {
            fs::write(&self.recipe.path, updated_yaml)?;
            println!(
                concat!(
                    "{} | Git references resolved to commit hashes and saved to stone.yaml.",
                    "This ensures reproducible builds since tags and branches can move over time."
                ),
                "Warning".yellow()
            );
        }

        timing.finish(timer);

        Ok(stored)
    }

    pub fn cleanup(&self) -> Result<(), Error> {
        // Remove rootfs
        root::remove(self)?;

        // Remove artifacts
        if self.paths.artefacts().host.exists() {
            fs::remove_dir_all(&self.paths.artefacts().host)?;
        }

        // Remove build dir
        if self.paths.build().host.exists() {
            fs::remove_dir_all(&self.paths.build().host)?;
        }

        // Remove downloaded upstreams
        upstream::remove(&self.paths.upstreams().host, &self.upstreams)?;

        // Prune moss cache, retaining stones from the repos defined
        // by our boulder profile
        moss::Client::builder("boulder", moss::Installation::open(&self.env.moss_dir, None)?)
            .repositories(self.repos.clone())
            .build()?
            .prune_cache()?;

        Ok(())
    }

    pub fn build(&self, timing: &mut Timing) -> Result<(), Error> {
        for (i, target) in self.targets.iter().enumerate() {
            println!("{}", build_target_prefix(target.build_target, i));

            // Set ourselves into our own process group
            // and set it as fg term
            //
            // This is so we can restore this process back as
            // the fg term after using `bash` for chroot below
            // so we can reestablish SIGINT forwarding to scripts
            setpgid(Pid::from_raw(0), Pid::from_raw(0))?;
            let pgid = getpgrp();
            ::container::set_term_fg(pgid)?;

            let err = self.build_target(timing, target);
            if let Err(e) = &err {
                if matches!(e, Error::BreakpointExit) {
                    // Restore ourselves as fg term since bash steals it
                    ::container::set_term_fg(pgid)?;
                }
                return err;
            }

            println!();
        }
        Ok(())
    }

    fn share_upstreams(&self, cont: &mut Container, stored: &[upstream::Stored]) -> Result<(), Error> {
        let pb = &ProgressBar::new(0).with_style(
            ProgressStyle::with_template(" {spinner} {wide_msg} ")
                .unwrap()
                .tick_chars(ui::PROGRESS_CHARS),
        );

        println!("\nSharing {} upstream(s) with the build container:", stored.len());
        runtime::block_on(async {
            for upstream in stored {
                pb.set_message(format!("{} {}", "Copying".yellow(), upstream.name().bold()));
                for target in &self.targets {
                    let dest = &self.paths.build().host.join(target.build_target.to_string());
                    upstream.share(cont, dest).await?;

                    let cached_tag = upstream
                        .was_cached()
                        .then_some(format!("{}", " (cached)".dim()))
                        .unwrap_or_default();
                    pb.suspend(|| println!("{} {}{cached_tag}", "Shared".green(), upstream.name().bold()));
                }
            }
            Ok(())
        })
    }

    fn build_target(&self, timing: &mut Timing, target: &Target) -> Result<(), Error> {
        // Clean out any pre-existing profiling data from PGO runs
        if let Some(job) = target.jobs.iter().find(|job| job.pgo_stage.is_some()) {
            util::recreate_dir(&pgo_dir(&job.build_dir))?;
        }

        for (i, job) in target.jobs.iter().enumerate() {
            let is_pgo = job.pgo_stage.is_some();

            if let Some(stage) = job.pgo_stage {
                println!("{}", pgo_stage_prefix(stage, i));
            }

            for (i, (phase, script)) in job.phases.iter().enumerate() {
                println!("{}", phase_prefix(*phase, is_pgo, i));

                let build_dir = &job.build_dir;
                let work_dir = &job.work_dir;
                let current_dir = if work_dir.exists() { &work_dir } else { &build_dir };

                let timer = timing.begin(timing::Kind::Build(timing::Build {
                    target: job.target,
                    pgo_stage: job.pgo_stage,
                    phase: *phase,
                }));

                for command in script.commands() {
                    match command {
                        &stone_script::Command::Breakpoint { line_num, exit } => {
                            let line_num = breakpoint_line(line_num, &self.recipe, job.target, *phase)
                                .map(|line_num| format!(" at line {line_num}"))
                                .unwrap_or_default();

                            println!(
                                "\n{}{line_num} {}",
                                "Breakpoint".bold(),
                                if exit { "(exit)".dim() } else { "(continue)".dim() },
                            );

                            // Write env to $HOME/.profile
                            let profile = format_profile(script)?;
                            fs::write(build_dir.join(".profile"), profile)?;

                            let mut command = process::Command::new("/usr/bin/bash")
                                .arg("--login")
                                .env_clear()
                                .env("HOME", build_dir)
                                .env("PATH", "/usr/bin:/usr/sbin")
                                .env("TERM", "xterm-256color")
                                .current_dir(current_dir)
                                .spawn()?;

                            command.wait()?;

                            if exit {
                                return Err(Error::BreakpointExit);
                            }
                        }
                        stone_script::Command::Output { output: content } => {
                            // TODO: Proper temp file
                            let script_path = "/tmp/script";

                            {
                                let mut file = fs::File::create(script_path)?;
                                writeln!(file, "{content}\n")?;
                            }

                            let result = logged(*phase, is_pgo, "/usr/bin/bash", |command| {
                                command
                                    .arg(script_path)
                                    .env_clear()
                                    .env("HOME", build_dir)
                                    .env("PATH", "/usr/bin:/usr/sbin")
                                    .current_dir(current_dir)
                            })?;

                            if !result.success() {
                                match result.code() {
                                    Some(code) => {
                                        return Err(Error::Code(code));
                                    }
                                    None => {
                                        if let Some(signal) = result
                                            .signal()
                                            .or_else(|| result.stopped_signal())
                                            .and_then(|i| Signal::try_from(i).ok())
                                        {
                                            return Err(Error::Signal(signal));
                                        } else {
                                            return Err(Error::UnknownSignal);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                timing.finish(timer);
            }
        }
        Ok(())
    }
}

pub fn build_target_prefix(target: BuildTarget, i: usize) -> String {
    let newline = if i > 0 { "\n".into() } else { String::default() };

    format!("{newline}{}", target.to_string().dim())
}

pub fn pgo_stage_prefix(stage: pgo::Stage, i: usize) -> String {
    let newline = if i > 0 {
        format!("{}\n", "│".dim())
    } else {
        String::default()
    };

    format!("{newline}{}", format!("│pgo-{stage}").dim())
}

pub fn phase_prefix(phase: job::Phase, is_pgo: bool, i: usize) -> String {
    let pipes = if is_pgo { "││".dim() } else { "│".dim() };
    let newline = if i > 0 { format!("{pipes}\n") } else { String::default() };

    format!("{newline}{pipes}{}", phase.styled(phase))
}

fn logged(
    phase: job::Phase,
    is_pgo: bool,
    command: &str,
    f: impl FnOnce(&mut process::Command) -> &mut process::Command,
) -> io::Result<process::ExitStatus> {
    let mut command = process::Command::new(command);

    f(&mut command);

    let mut child = command
        .stdout(process::Stdio::piped())
        .stderr(process::Stdio::piped())
        .spawn()?;

    // Log stdout and stderr
    let stdout_log = log(phase, is_pgo, child.stdout.take().unwrap());
    let stderr_log = log(phase, is_pgo, child.stderr.take().unwrap());

    // Forward SIGINT to this process
    ::container::forward_sigint(Pid::from_raw(child.id() as i32))?;

    let result = child.wait()?;

    let _ = stdout_log.join();
    let _ = stderr_log.join();

    Ok(result)
}

fn log<R>(phase: job::Phase, is_pgo: bool, pipe: R) -> thread::JoinHandle<()>
where
    R: io::Read + Send + 'static,
{
    use std::io::BufRead;

    thread::spawn(move || {
        let pgo = if is_pgo { "│" } else { "" }.dim();
        let kind = phase.styled(format!("{}│", phase.abbrev()));
        let tag = format!("{}{pgo}{kind}", "│".dim());

        let mut lines = io::BufReader::new(pipe).lines();

        while let Some(Ok(line)) = lines.next() {
            println!("{tag} {line}");
        }
    })
}

pub fn format_profile(script: &ScriptBundle) -> Result<String, Error> {
    let mut profile = script
        .prefix()
        .lines()
        .filter(|line| !line.starts_with("#!") && !line.starts_with("set -") && !line.starts_with("TERM="))
        .join("\n");

    let env = script.env();

    for (identifier, action) in &env.actions {
        let mut ctx = stone_script::ScriptContext::new().with_toplevel_builtins();
        ctx.eval(env, &action.value)?;
        profile.push_str(&format!(
            "a_{identifier}() {{\n{}\n}}\nexport -f a_{identifier}\n",
            ctx.flush_to_string()
        ));
    }

    for (identifier, definition) in &env.definitions {
        let mut ctx = stone_script::ScriptContext::new().with_toplevel_builtins();
        ctx.eval(env, &definition.value)?;
        profile.push_str(&format!(
            "d_{identifier}=\"{}\"; export d_{identifier}\n",
            ctx.flush_to_string()
        ));
    }

    Ok(profile)
}

fn breakpoint_line(line_num: usize, recipe: &Recipe, build_target: BuildTarget, phase: job::Phase) -> Option<usize> {
    let profile = recipe.build_target_profile_key(build_target);

    let has_key = |line: &str, key: &str| {
        line.split_once(':')
            .is_some_and(|(leading, _)| leading.trim().ends_with(key))
    };

    let mut lines = recipe
        .source
        .lines()
        .enumerate()
        // If no profile, we care about root keys (no leading whitespace),
        // otherwise it will be indented
        .filter(|(_, line)| {
            let indented = line.trim().chars().next() != line.chars().next();

            if profile.is_none() { !indented } else { indented }
        })
        // Skip lines occurring before profile, otherwise it's the
        // root profile
        .skip_while(|(_, line)| {
            if let Some(profile) = &profile {
                !has_key(line, profile)
            } else {
                false
            }
        });

    let phase = match phase {
        // Internal phase, no breakpoint will occur
        job::Phase::Prepare => return None,
        job::Phase::Setup => "setup",
        job::Phase::Build => "build",
        job::Phase::Install => "install",
        job::Phase::Check => "check",
        job::Phase::Workload => "workload",
    };

    lines.find_map(|(mut line_num_, line)| {
        if has_key(line, phase) {
            // 0 based to 1 based
            line_num_ += 1;

            let (_, rest) = line.split_once(':').expect("line contains :");

            // If block, string starts on next line
            if rest.trim().starts_with('|') || rest.trim().starts_with('>') {
                line_num_ += 1;
            }

            Some(line_num + line_num_)
        } else {
            None
        }
    })
}

async fn store_upstream(ups: &Upstream, storage_dir: &Path, pb: &ProgressBar) -> Result<upstream::Stored, Error> {
    pb.set_message(format!("{} {}", "Downloading".blue(), ups.name().bold()));

    let stored = ups.store(storage_dir, pb).await.map_err(Error::from)?;
    let cached_tag = stored
        .was_cached()
        .then_some(format!("{}", " (cached)".dim()))
        .unwrap_or_default();
    pb.suspend(|| println!("{} {}{cached_tag}", "Downloaded".green(), ups.name().bold()));
    Ok(stored)
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("no supported build targets for recipe")]
    NoBuildTargets,
    #[error("macros")]
    Macros(#[from] macros::Error),
    #[error("job")]
    Job(#[from] job::Error),
    #[error("profile")]
    Profile(#[from] profile::Error),
    #[error("root")]
    Root(#[from] root::Error),
    #[error("upstream")]
    Upstream(#[from] upstream::Error),
    #[error("container")]
    Container(#[from] container::Error),
    #[error("recipe")]
    Recipe(#[from] recipe::Error),
    #[error("failed with status code {0}")]
    Code(i32),
    #[error("stopped by signal {}", .0.as_str())]
    Signal(Signal),
    #[error("stopped by unknown signal")]
    UnknownSignal,
    #[error("nix")]
    Nix(#[from] nix::Error),
    #[error("io")]
    Io(#[from] io::Error),
    #[error("recreate artefacts dir")]
    RecreateArtefactsDir(#[source] io::Error),
    #[error("moss client")]
    MossClient(#[from] moss::client::Error),
    #[error("moss installation")]
    MossInstallation(#[from] moss::installation::Error),
    #[error("script")]
    Script(#[from] stone_script::Error),
    #[error("breakpoint with exit action reached")]
    BreakpointExit,
}
