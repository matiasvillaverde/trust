fn main() {
    // diesel_migrations::embed_migrations!() reads SQL from the `migrations/` directory at compile
    // time. Ensure changes to migration SQL trigger a rebuild so tests and local runs always
    // execute the current migration set.
    println!("cargo:rerun-if-changed=migrations");
}
