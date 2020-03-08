#[macro_use]
extern crate clap;
extern crate postgres;
#[macro_use]
extern crate log;

use clap::App;
use postgres::error::SqlState;
use postgres::{Client, NoTls};
use std::env;
use std::fs::File;
use std::io::{BufRead, BufReader};

#[derive(Debug)]
struct Root {
    word: String,
    root: String,
    index: isize, // can be >0 for words with multiple roots
}

fn main() {
    env_logger::init();
    let yaml = load_yaml!("cli.yml");
    let matches = App::from_yaml(yaml).get_matches();
    let connection_string = matches.value_of("connection-string").unwrap();
    let input_file = matches.value_of("INPUT").unwrap();
    let quality = matches.value_of("quality").unwrap();
    let kind = matches.value_of("kind").unwrap();

    let mut client =
        Client::connect(&connection_string, NoTls).expect("Cannot connect to the database");

    let reader = BufReader::new(File::open(input_file).expect("Cannot open input file"));

    let insert = client
        .prepare("INSERT INTO roots (word, root, index, quality) VALUES ($1, $2, $3, $4)")
        .unwrap();
    for line in reader.lines() {
        // TODO handle non-UTF8 (for example, panics on CP1251)
        let line = line.unwrap();
        let line = line.trim();
        info!("{}", line);
        for root in get_roots_from_string(line, kind) {
            match client.execute(
                &insert,
                &[&root.word, &root.root, &(root.index as i32), &quality],
            ) {
                Ok(_) => (),
                Err(err) => {
                    if err.code() == Some(&SqlState::UNIQUE_VIOLATION) {
                        warn!("Duplicate key {:?}", root);
                        debug!("{}", err)
                    } else {
                        panic!("{}", err)
                    }
                }
            }
        }
    }
}

fn get_roots_from_string(string: &str, kind: &str) -> Vec<Root> {
    match kind {
        "morphemes" => get_roots_from_morpheme_string(string),
        "psql" => get_roots_from_psql_string(string),
        &_ => unreachable!("kind must be set either to 'morphemes' or to 'psql'"),
    }
}

fn get_roots_from_morpheme_string(string: &str) -> Vec<Root> {
    let mut vec = Vec::new();
    let parts: Vec<&str> = string.split('\t').collect();
    let word = parts[0];
    parts[1]
        .split('/')
        .filter_map(|p| {
            let pieces: Vec<&str> = p.split(':').collect();
            if pieces[1] == "ROOT" {
                Some(pieces[0])
            } else {
                None
            }
        })
        .enumerate()
        .for_each(|x| {
            let (index, root) = x;
            vec.push(Root {
                word: word.to_owned(),
                root: root.to_owned(),
                index: index as isize,
            })
        });
    vec
}

fn get_roots_from_psql_string(string: &str) -> Vec<Root> {
    let mut vec = Vec::new();
    let parts: Vec<&str> = string.split('|').collect();
    let roots = parts[0].trim();
    let words = parts[1].trim().trim_matches('{').trim_matches('}');
    words.split(',').for_each(|word| {
        vec.push(Root {
            word: word.to_owned(),
            root: roots.to_owned(),
            index: -1,
        })
    });
    vec
}

fn default_var(key: &str, default: &str) -> String {
    match env::var(key) {
        Ok(val) => val,
        Err(err) => match err {
            env::VarError::NotPresent => default.to_owned(),
            env::VarError::NotUnicode(_) => panic!(err),
        },
    }
}
