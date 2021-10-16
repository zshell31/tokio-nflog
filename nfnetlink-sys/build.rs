use std::convert::AsRef;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const C_FILES: &[&str] = &["iftable.c", "libnfnetlink.c", "rtnl.c"];
const HEADER_FILES: &[&str] = &[
    "libnfnetlink.h",
    "linux_nfnetlink_compat.h",
    "linux_nfnetlink.h",
];

fn main() {
    // TODO: check linux target
    // TODO: git submodule update --init
    // TODO: pkg_config
    // TODO: generating bindings

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let lib = out_dir.join("lib");
    let include = out_dir.join("include");

    let nfnetlink_dir = Path::new("src/libnfnetlink");
    let nfnetlink_include = nfnetlink_dir.join("include/libnfnetlink");
    fs::create_dir_all(&include).unwrap();
    for &header in HEADER_FILES {
        fs::copy(nfnetlink_include.join(header), &include.join(header)).unwrap();
    }

    let src = nfnetlink_dir.join("src");
    let mut cfg = cc::Build::new();
    cfg.out_dir(&lib)
        .warnings(false)
        .include(nfnetlink_dir.join("include"))
        .flag("-fvisibility=hidden")
        .define("NFNL_EXPORT", "__attribute__((visibility(\"default\")))");

    for &file in C_FILES {
        cfg.file(src.join(file));
    }

    cfg.compile("nfnetlink");

    println!("cargo:root={}", out_dir.display());
    println!("cargo:include={}", include.display());

    rerun_if("src/libnfnetlink");
}

fn rerun_if<P: AsRef<Path>>(path: P) {
    if path.as_ref().is_dir() {
        for entry in fs::read_dir(path).expect("read_dir") {
            rerun_if(&entry.expect("entry").path());
        }
    } else {
        println!("cargo:rerun-if-changed={}", path.as_ref().display());
    }
}
