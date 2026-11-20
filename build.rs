use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=./shaders_uncompiled/");

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

        if !status.success() {
            panic!("glslc failed on {name}");
        }

        println!("cargo:warning=Compiled {name}");
    }
}
