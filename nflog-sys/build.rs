use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const C_FILES: &[&str] = &["libnetfilter_log.c"];
const HEADER_FILES: &[&str] = &["libipulog.h", "libnetfilter_log.h", "linux_nfnetlink_log.h"];

fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let lib = out_dir.join("lib");
    let include = out_dir.join("include");

    let dir = Path::new("src/libnetfilter_log");
    let include_dir = dir.join("include/libnetfilter_log");
    fs::create_dir_all(&include).unwrap();
    for &header in HEADER_FILES {
        fs::copy(include_dir.join(header), include.join(header)).unwrap();
    }

    let src = dir.join("src");
    let mut cfg = cc::Build::new();
    cfg.out_dir(&lib)
        .warnings(false)
        .flag("-lnfnetlink")
        .include(dir.join("include"));

    for &file in C_FILES {
        cfg.file(src.join(file));
    }

    if let Some(nfnetlink_include) = env::var_os("DEP_NFNETLINK_INCLUDE") {
        cfg.include(&nfnetlink_include);
    }

    cfg.compile("netfilter_log");

    println!("cargo:root={}", out_dir.display());
    println!("cargo:include={}", include.display());
}
