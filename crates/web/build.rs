use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=style/input.css");
    println!("cargo:rerun-if-changed=src/");

    // Compile Tailwind CSS during `cargo build` if the CLI is available.
    // In CI the CSS is compiled as a separate step; this is a convenience for local dev.
    if std::env::var("SKIP_TAILWIND").is_ok() {
        return;
    }

    let output = Command::new("tailwindcss")
        .args([
            "-i",
            "style/input.css",
            "-o",
            "style/output.css",
            "--minify",
        ])
        .output();

    match output {
        Ok(o) if o.status.success() => {
            println!("cargo:warning=Tailwind CSS compiled to style/output.css");
        }
        Ok(o) => {
            eprintln!(
                "tailwindcss failed ({}): {}",
                o.status,
                String::from_utf8_lossy(&o.stderr)
            );
        }
        Err(_) => {
            // Tailwind CLI not installed — warn but don't fail the build.
            // Install with: npm install -g tailwindcss (or npx tailwindcss ...)
            println!(
                "cargo:warning=tailwindcss CLI not found; \
                 run `npx tailwindcss -i crates/web/style/input.css \
                 -o crates/web/style/output.css` manually."
            );
        }
    }
}
