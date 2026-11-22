//! Build script: Cargo runs it before compiling the program.
//! It compiles every GLSL shader in `shaders_uncompiled/` to SPIR-V with `glslc`.

use std::process::Command;

fn main() {
    // Only re-run this script when a shader changes
    println!("cargo:rerun-if-changed=./shaders_uncompiled/");

    // Compile each shader file into shaders/<name>.spv
    for entry in std::fs::read_dir("shaders_uncompiled").unwrap() {
        let path = entry.unwrap().path();

        let name = path.file_name().unwrap().to_str().unwrap();
        let output = format!("shaders/{name}.spv");

        let status = Command::new("glslc")
            .arg(&path)
            .arg("-o")
            .arg(&output)
            .status()
            .expect("could not run glslc");

        // Stop the build if a shader has an error
        if !status.success() {
            panic!("glslc failed on {name}");
        }

        // A build script can only print through cargo warnings
        println!("cargo:warning=Compiled {name}");
    }
}
