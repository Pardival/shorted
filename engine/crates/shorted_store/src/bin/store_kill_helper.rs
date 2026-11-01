//! Test helper: edits a project as fast as it can, printing each acknowledged revision, until the
//! kill test terminates it. Usage: store_kill_helper <root> <project-id>

#![allow(clippy::expect_used)] // Test tool: failing loudly is the right behaviour.

use std::io::Write;
use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

use shorted_core::command::Command;
use shorted_store::session::ProjectSession;

fn main() {
    let mut args = std::env::args().skip(1);
    let root = args.next().expect("root");
    let id = args.next().expect("id").parse().expect("project id");
    let (status, _keep_open) = mpsc::channel();
    let mut session = ProjectSession::open(Path::new(&root), id, status).expect("open");
    let mut output = std::io::stdout();
    let mut step = 0u64;
    loop {
        step += 1;
        session
            .execute(&Command::Rename {
                title: step.to_string(),
            })
            .expect("execute");
        writeln!(output, "{}", session.revision()).expect("print");
        output.flush().expect("flush");
        std::thread::sleep(Duration::from_millis(2));
    }
}
