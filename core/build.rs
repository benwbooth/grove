// The C shim for MuPDF, djvulibre and Ghostscript (csrc/docs.c): their
// errors unwind with setjmp/longjmp, which has to stay in C.
fn main() {
    println!("cargo::rerun-if-changed=csrc/docs.c");
    println!("cargo::rerun-if-changed=csrc/burn.c");
    println!("cargo::rerun-if-env-changed=GROVE_LIBGS");
    let mut b = cc::Build::new();
    b.file("csrc/docs.c").file("csrc/burn.c").warnings(false);
    // Ghostscript is dlopen'ed privately (see docs.c): its full path.
    if let Ok(gs) = std::env::var("GROVE_LIBGS") {
        b.define("GROVE_LIBGS", Some(format!("\"{gs}\"").as_str()));
    }
    b.compile("grove_docs");
    println!("cargo::rustc-link-lib=dylib=dl");
    for lib in ["mupdf", "djvulibre", "isofs", "burn"] {
        println!("cargo::rustc-link-lib=dylib={lib}");
    }
}
