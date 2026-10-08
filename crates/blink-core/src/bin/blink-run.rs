//! Headless project runner. Exit 0: passed; 1: failed run; 2: input/IO error.

use blink_core::collection_runner::{RunControl, RunOptions, group_order, parse_dataset, run};
use blink_core::engine::{Engine, paths::Paths};
use blink_core::workspace_state::Workspace;
use std::path::PathBuf;

const USAGE: &str = "Usage: blink-run PROJECT [--group NAME] [--environment NAME] [--dataset FILE.csv|FILE.json] [--report FILE.json] [--stop-on-failure] [--allow-protected]\nExit codes: 0 passed, 1 failed/canceled/skipped, 2 input or IO error.\nReport goes to stdout unless --report is set. Tokens can use {{!ENV_VAR}} for CI secrets.";

fn main() {
    match execute() {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    }
}

fn execute() -> Result<i32, String> {
    let mut args = std::env::args().skip(1);
    let Some(project) = args.next() else {
        return Err(USAGE.into());
    };
    if project == "--help" || project == "-h" {
        println!("{USAGE}");
        return Ok(0);
    }
    let mut options = RunOptions::default();
    let mut group = None;
    let mut environment = None;
    let mut report_path = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--stop-on-failure" => options.stop_on_failure = true,
            "--allow-protected" => options.allow_protected = true,
            "--group" | "--environment" | "--dataset" | "--report" => {
                let value = args
                    .next()
                    .ok_or_else(|| format!("{arg} requires a value."))?;
                match arg.as_str() {
                    "--group" => group = Some(value),
                    "--environment" => environment = Some(value),
                    "--report" => report_path = Some(PathBuf::from(value)),
                    _ => {
                        let path = PathBuf::from(value);
                        let json = match path.extension().and_then(|x| x.to_str()) {
                            Some("json") => true,
                            Some("csv") => false,
                            _ => return Err("Dataset must have a .csv or .json extension.".into()),
                        };
                        let text = std::fs::read_to_string(&path)
                            .map_err(|e| format!("Cannot read dataset: {e}"))?;
                        options.dataset = parse_dataset(&text, json)?;
                    }
                }
            }
            _ => return Err(format!("Unknown option: {arg}\n{USAGE}")),
        }
    }
    // CI never reads or changes the desktop workspace and local credential store.
    let temp = tempfile::tempdir().map_err(|e| e.to_string())?;
    let engine = Engine::new(Paths::with_base(temp.path().to_path_buf()))?;
    let (path, document) = futures::executor::block_on(engine.open_project(project.into()))?;
    let mut workspace = Workspace::new();
    let root = workspace.attach_project(path, document)?;
    if let Some(name) = environment {
        let matches: Vec<_> = workspace
            .group(root)
            .unwrap()
            .environments
            .as_deref()
            .unwrap_or_default()
            .iter()
            .filter(|e| e.name == name)
            .map(|e| e.id)
            .collect();
        if matches.len() != 1 {
            return Err(format!(
                "Environment name must match exactly one environment: {name}"
            ));
        }
        workspace.set_active_environment(root, Some(matches[0]));
    }
    let group = match group {
        None => root,
        Some(name) => {
            let matches: Vec<_> = workspace
                .groups
                .iter()
                .filter(|g| g.name == name)
                .map(|g| g.id)
                .collect();
            if matches.len() != 1 {
                return Err(format!("Group name must match exactly one group: {name}"));
            }
            matches[0]
        }
    };
    options.request_ids = group_order(&workspace, group);
    let request_ids = workspace.projects[0].request_ids.clone();
    let mut report =
        futures::executor::block_on(run(engine, workspace, options, RunControl::default()))?;
    // Stable project IDs make reports comparable across CI invocations.
    for result in &mut report.results {
        if let Some((disk, _)) = request_ids
            .iter()
            .find(|(_, runtime)| **runtime == result.request_id)
        {
            result.request_id = *disk;
        }
    }
    let json = report.to_json()?;
    match report_path {
        Some(path) => std::fs::write(path, format!("{json}\n"))
            .map_err(|e| format!("Cannot write report: {e}"))?,
        None => println!("{json}"),
    }
    Ok(if report.passed() { 0 } else { 1 })
}
