fn main () {
    cxx_build::bridge("src/main.rs")
        .file("src/outlines.cpp")
        .compile("cxx-outlines");

    println!("cargo:rerun-if-changed=src/outlines.cpp");
    println!("cargo:rerun-if-changed=src/outlines.h");    
}
