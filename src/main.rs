// Shurl - Small utility to manage short URLs in a Git repository
// Copyright (C) 2023  MD Gaziur Rahman Noor
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <http://www.gnu.org/licenses/>.

use clap::Parser;
use owo_colors::OwoColorize;
use rand::Rng;
use serde::{Deserialize, Serialize};
use shellexpand::tilde;
use std::fs;
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use toml::to_string_pretty;
use url::Url;

#[derive(Serialize, Deserialize)]
#[serde(default)]
struct ShurlConfig {
    repo_path: PathBuf,
    name: String,
    email: String,
}

impl Default for ShurlConfig {
    fn default() -> Self {
        Self {
            repo_path: PathBuf::from("/path_to_valid_and_empty_git_repo"),
            name: "shurl".to_string(),
            email: "example@example.com".to_string(),
        }
    }
}

#[derive(Parser)]
struct Args {
    url: String,
    short_name: Option<String>,
}

fn create_name() -> String {
    let mut name = String::new();
    let mut rng = rand::thread_rng();
    for _ in 0..5 {
        name.push(rng.gen_range(b'a'..b'z') as char);
    }
    name
}

fn main() {
    let mut cfg_content = String::new();
    let mut cfg_file = match OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .append(false)
        .open(tilde("~/.config/shurl_config.toml").as_ref())
    {
        Ok(file) => file,
        Err(e) => {
            println!(
                "{} {} {}",
                "Error:".red(),
                "failed to create config file:".bold(),
                e.to_string()
            );
            return;
        }
    };

    if let Err(e) = cfg_file.read_to_string(&mut cfg_content) {
        println!(
            "{} {} {}",
            "Error:".red(),
            "failed to read config file:".bold(),
            e.to_string()
        );
        return;
    }

    if cfg_content.is_empty() {
        cfg_file
            .write(to_string_pretty(&ShurlConfig::default()).unwrap().as_ref())
            .expect("failed to write config file");
        println!(
            "{} {}",
            "Info:".green(),
            "created config file. Set the default repository path and \
            run the command again."
                .bold()
        );
    } else {
        let Ok(cfg) = toml::from_str::<ShurlConfig>(&cfg_content) else {
            println!(
                "{} {}",
                "Error:".red(),
                "failed to parse config file".bold()
            );
            return;
        };
        let args = Args::parse();

        let url = match Url::parse(&args.url) {
            Ok(url) => url,
            Err(e) => {
                eprintln!(
                    "{} {} {}",
                    "Error:".red(),
                    "failed to parse url:".bold(),
                    e.to_string()
                );
                return;
            }
        };

        let expanded_repo_path = tilde(cfg.repo_path.to_str().unwrap()).to_string();
        let repo_path = Path::new(&expanded_repo_path);
        let repo = match git2::Repository::open(repo_path) {
            Ok(repo) => repo,
            Err(e) => {
                eprintln!(
                    "{} {} {}",
                    "Error:".red(),
                    "failed to open repository:".bold(),
                    e.to_string()
                );
                return;
            }
        };

        let file_content = format!(
            "<html>
    <head>
        <meta http-equiv=\"refresh\" content=\"0; URL={url}\" />
    </head>
    <body>
        <p>Redirecting...</p>
        <p>If you are not redirected automatically, follow the <a href=\"{url}\">link</a></p>
    </body>
</html>"
        );
        let file_name = match args.short_name {
            Some(name) => repo_path.join(name + ".html"),
            None => {
                // We're using 5 characters long short names. May clash?
                let mut possible_file_name = repo_path.join(&(create_name() + ".html"));
                while possible_file_name.exists() {
                    possible_file_name = repo_path.join(&(create_name() + ".html"))
                }
                possible_file_name
            }
        };

        fs::write(&file_name, file_content).expect("Failed to write file for redirection to url");

        let mut index_file = match OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .append(true)
            .open(repo_path.join("index.html"))
        {
            Ok(file) => file,
            Err(e) => {
                println!(
                    "{} {} {}",
                    "Error:".red(),
                    "failed to create config file:".bold(),
                    e.to_string()
                );
                return;
            }
        };

        let file_name = file_name.iter().last().unwrap().to_str().unwrap();
        index_file
            .write(format!("\n{url}: <a href=\"./{file_name}\">./{file_name}</a><br/>",).as_ref())
            .expect("Failed to write to index.html");

        let mut index = repo.index().unwrap();
        index
            .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
            .unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let head = repo.head();
        let parent_commit;

        let object_id = repo
            .commit(
                Some("HEAD"),
                &git2::Signature::now(&cfg.name, &cfg.email).unwrap(),
                &git2::Signature::now(&cfg.name, &cfg.email).unwrap(),
                format!("Add redirect to {}", url).as_ref(),
                &tree,
                &match head {
                    Ok(head) => {
                        parent_commit = head.peel_to_commit().unwrap();
                        vec![&parent_commit]
                    },
                    Err(_) => vec![],
                },
            )
            .expect("Failed to create commit");

        println!("Created commit with object id: {}", object_id);

        // HACK: easier way to push to upstream
        Command::new("git")
            .arg("push")
            .arg("origin")
            .arg("master")
            .current_dir(repo_path)
            .status()
            .expect("Failed to push to upstream: try running `git push` manually");
    }
}
