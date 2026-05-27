fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=migrations");
    println!("cargo:rerun-if-changed=build.rs");

    Ok(())
}
