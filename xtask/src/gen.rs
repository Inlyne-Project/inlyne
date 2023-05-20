#![allow(dead_code)]

include!("../../src/opts/cli.rs");

use std::fs;

use clap_complete::{generate_to, Shell};

pub fn gen() {
    gen_completions();
}

fn gen_completions() {
    let out_dir = "completions";
    fs::create_dir_all(out_dir).unwrap();

    let mut cmd = command();
    for &shell in Shell::value_variants() {
        generate_to(shell, &mut cmd, "inlyne", out_dir).unwrap();
    }
}
