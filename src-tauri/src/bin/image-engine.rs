const IMAGE_ENGINE_VERSION: &str = "0.2.0-preview.0";
const SELF_CHECK_MESSAGE: &str = "LocalConvert image-engine self-check ok";

fn main() {
    let mut args = std::env::args().skip(1);
    match (args.next().as_deref(), args.next()) {
        (Some("--version"), None) => {
            println!("LocalConvert image-engine {IMAGE_ENGINE_VERSION}");
        }
        (Some("--self-check"), None) => {
            println!("{SELF_CHECK_MESSAGE}");
        }
        (None, None) => {
            eprintln!("LocalConvert image-engine expects --version or --self-check.");
            std::process::exit(2);
        }
        _ => {
            eprintln!("Unsupported image-engine command. Use --version or --self-check.");
            std::process::exit(2);
        }
    }
}
