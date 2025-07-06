use std::{
    fs,
    path::PathBuf,
    sync::{Arc, atomic::AtomicI32},
};

use anstyle::{AnsiColor, Color, Style};
use dissimilar::Chunk;
use futures::future::join_all;
use ropey::Rope;

use crate::{QUERY_LANGUAGE, handlers::formatting, util::get_scm_files};

pub async fn format_directories(directories: &[PathBuf], check: bool) -> i32 {
    if directories.is_empty() {
        eprintln!("No directories were specified to be formatted. No work was done.");
        return 1;
    }

    let scm_files = get_scm_files(directories);
    let exit_code = Arc::new(AtomicI32::new(0));

    let tasks = scm_files.into_iter().map(|path| {
        let exit_code = exit_code.clone();
        tokio::spawn(async move {
            let Ok(contents) = fs::read_to_string(&path) else {
                eprintln!("Failed to read {:?}", path.canonicalize().unwrap());
                exit_code.store(1, std::sync::atomic::Ordering::Relaxed);
                return;
            };
            let mut parser = tree_sitter::Parser::new();
            parser
                .set_language(&QUERY_LANGUAGE)
                .expect("Error loading Query grammar");
            let tree = parser.parse(contents.as_str(), None).unwrap();
            let rope = Rope::from(contents.as_str());
            let Some(formatted) = formatting::format_document(&rope, &tree.root_node()) else {
                return;
            };
            if check {
                let edits = formatting::diff(&contents, &formatted, &rope);
                if !edits.is_empty() {
                    exit_code.store(1, std::sync::atomic::Ordering::Relaxed);
                    eprintln!(
                        "Improper formatting detected for {:?}",
                        path.canonicalize().unwrap()
                    );
                    let chunks = dissimilar::diff(&contents, &formatted);
                    let mut chunks = chunks.iter().peekable();
                    let mut old_line_num = 1;
                    let mut new_line_num = 1;
                    let mut display_eq = false;
                    while let Some(chunk) = chunks.next() {
                        // Only display the next equal chunk if the previous or next chunk is an
                        // actual edit
                        display_eq = display_eq || !matches!(chunks.peek(), Some(Chunk::Equal(_)));
                        match chunk {
                            Chunk::Equal(eq) => {
                                old_line_num += eq.lines().count();
                                new_line_num += eq.lines().count();
                                // if display_eq {
                                //     for line in eq.lines() {
                                //         eprintln!("{line}");
                                //     }
                                // }
                                display_eq = false;
                            }
                            Chunk::Delete(del) => {
                                eprintln!(
                                    "{}",
                                    paint(
                                        Some(Color::Ansi(AnsiColor::BrightCyan)),
                                        &format!(
                                            "@@ -{old_line_num},{} +{new_line_num},{} @@",
                                            old_line_num + del.lines().count() - 1,
                                            new_line_num
                                        )
                                    ),
                                );
                                for line in del.lines() {
                                    eprintln!(
                                        "{}",
                                        paint(
                                            Some(Color::Ansi(AnsiColor::Red)),
                                            &format!("-{line}")
                                        )
                                    );
                                }
                                old_line_num += del.lines().count() - 1;
                                display_eq = true;
                            }
                            Chunk::Insert(ins) => {
                                println!("WTF: {:?}", ins);
                                eprintln!(
                                    "{}",
                                    paint(
                                        Some(Color::Ansi(AnsiColor::BrightCyan)),
                                        &format!(
                                            "@@ -{old_line_num},{} +{new_line_num},{} @@",
                                            old_line_num,
                                            new_line_num + ins.lines().count()
                                        )
                                    ),
                                );
                                for line in ins.lines() {
                                    eprintln!(
                                        "{}",
                                        paint(
                                            Some(Color::Ansi(AnsiColor::Green)),
                                            &format!("+{line}")
                                        )
                                    );
                                }
                                new_line_num += ins.lines().count();
                                display_eq = true;
                            }
                        }
                    }
                }
            } else if fs::write(&path, formatted).is_err() {
                exit_code.store(1, std::sync::atomic::Ordering::Relaxed);
                eprint!("Failed to write to {:?}", path.canonicalize().unwrap())
            }
        })
    });
    join_all(tasks).await;
    exit_code.load(std::sync::atomic::Ordering::Relaxed)
}

fn paint(color: Option<impl Into<Color>>, text: &str) -> String {
    let style = Style::new().fg_color(color.map(Into::into));
    format!("{style}{text}{style:#}")
}
