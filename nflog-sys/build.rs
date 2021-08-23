use std::env;
use std::path::{Path, PathBuf};

const C_FILES: &[&str] = &["libnetfilter_log.c"];

fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let lib = out_dir.join("lib");

    let dir = Path::new("src/libnetfilter_log");
    let src = dir.join("src");
    let mut cfg = cc::Build::new();
    cfg.out_dir(&lib)
        .warnings(false)
        .flag("-lnfnetlink")
        .include(dir.join("include"));

    for &file in C_FILES {
        cfg.file(src.join(file));
    }

    println!("here: {:#?}", env::vars().into_iter().collect::<Vec<_>>());
    if let Ok(nfnetlink_include) = env::var("DEP_NFNETLINK_INCLUDE") {
        println!("got env");
        cfg.include(&nfnetlink_include);
    }

    cfg.compile("netfilter_log");

    println!("cargo:rustc-link-lib=static=nfnetlink");
}
