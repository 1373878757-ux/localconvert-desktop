use image::{
    DynamicImage, GenericImageView, ImageFormat, ImageReader, Rgb, RgbImage, Rgba, RgbaImage,
};
use std::{
    ffi::{OsStr, OsString},
    fs::{self, OpenOptions},
    io::{BufWriter, Cursor, Seek, Write},
    path::{Path, PathBuf},
};

const IMAGE_ENGINE_VERSION: &str = "0.2.0-preview.1";
const SELF_CHECK_MESSAGE: &str = "LocalConvert image-engine self-check ok";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TargetFormat {
    Jpeg,
    Png,
    WebP,
}

#[derive(Debug, PartialEq, Eq)]
struct ConvertOptions {
    input: PathBuf,
    output: PathBuf,
    target_format: TargetFormat,
}

#[derive(Debug, PartialEq, Eq)]
enum CliCommand {
    Version,
    SelfCheck,
    Convert(ConvertOptions),
}

fn main() {
    match run(std::env::args_os().skip(1)) {
        Ok(message) => println!("{message}"),
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(2);
        }
    }
}

fn run<I>(args: I) -> Result<String, String>
where
    I: IntoIterator<Item = OsString>,
{
    match parse_cli_args(args)? {
        CliCommand::Version => Ok(format!("LocalConvert image-engine {IMAGE_ENGINE_VERSION}")),
        CliCommand::SelfCheck => {
            run_codec_self_check()?;
            Ok(SELF_CHECK_MESSAGE.to_string())
        }
        CliCommand::Convert(options) => {
            convert_image(&options)?;
            Ok(format!(
                "LocalConvert image-engine convert ok: {} -> {}",
                source_format(&options.input)?.extension(),
                options.target_format.extension()
            ))
        }
    }
}

fn parse_cli_args<I>(args: I) -> Result<CliCommand, String>
where
    I: IntoIterator<Item = OsString>,
{
    let mut args = args.into_iter();
    let command = args
        .next()
        .ok_or_else(|| usage_error("A command is required."))?;

    match command.to_str() {
        Some("--version") => {
            reject_extra_args(args)?;
            Ok(CliCommand::Version)
        }
        Some("--self-check") => {
            reject_extra_args(args)?;
            Ok(CliCommand::SelfCheck)
        }
        Some("convert") => parse_convert_args(args).map(CliCommand::Convert),
        _ => Err(usage_error("Unsupported image-engine command.")),
    }
}

fn parse_convert_args<I>(args: I) -> Result<ConvertOptions, String>
where
    I: IntoIterator<Item = OsString>,
{
    let mut args = args.into_iter();
    let mut input = None;
    let mut output = None;
    let mut target_format = None;

    while let Some(flag) = args.next() {
        let value = args.next().ok_or_else(|| {
            usage_error(&format!("Missing value for {}.", flag.to_string_lossy()))
        })?;

        match flag.to_str() {
            Some("--input") if input.is_none() => input = Some(PathBuf::from(value)),
            Some("--output") if output.is_none() => output = Some(PathBuf::from(value)),
            Some("--format") if target_format.is_none() => {
                target_format = Some(parse_target_format(&value)?)
            }
            Some("--input" | "--output" | "--format") => {
                return Err(usage_error(&format!(
                    "Duplicate option: {}.",
                    flag.to_string_lossy()
                )))
            }
            _ => {
                return Err(usage_error(&format!(
                    "Unsupported option: {}.",
                    flag.to_string_lossy()
                )))
            }
        }
    }

    Ok(ConvertOptions {
        input: input.ok_or_else(|| usage_error("--input is required."))?,
        output: output.ok_or_else(|| usage_error("--output is required."))?,
        target_format: target_format.ok_or_else(|| usage_error("--format is required."))?,
    })
}

fn reject_extra_args<I>(mut args: I) -> Result<(), String>
where
    I: Iterator<Item = OsString>,
{
    if args.next().is_some() {
        Err(usage_error(
            "This command does not accept additional arguments.",
        ))
    } else {
        Ok(())
    }
}

fn usage_error(message: &str) -> String {
    format!(
        "{message} Usage: image-engine --version | --self-check | convert --input <path> --output <path> --format <jpg|png|webp>"
    )
}

fn parse_target_format(value: &OsStr) -> Result<TargetFormat, String> {
    match value.to_str().map(str::trim).map(str::to_ascii_lowercase) {
        Some(value) if value == "jpg" || value == "jpeg" => Ok(TargetFormat::Jpeg),
        Some(value) if value == "png" => Ok(TargetFormat::Png),
        Some(value) if value == "webp" => Ok(TargetFormat::WebP),
        _ => Err("Target format must be jpg, png, or webp.".to_string()),
    }
}

fn source_format(path: &Path) -> Result<TargetFormat, String> {
    match normalized_extension(path).as_deref() {
        Some("jpg" | "jpeg") => Ok(TargetFormat::Jpeg),
        Some("png") => Ok(TargetFormat::Png),
        Some("webp") => Ok(TargetFormat::WebP),
        _ => Err(format!(
            "Input image must use a .jpg, .jpeg, .png, or .webp extension: {}",
            path.to_string_lossy()
        )),
    }
}

fn normalized_extension(path: &Path) -> Option<String> {
    path.extension()
        .and_then(OsStr::to_str)
        .map(str::trim)
        .filter(|extension| !extension.is_empty())
        .map(str::to_ascii_lowercase)
}

impl TargetFormat {
    fn extension(self) -> &'static str {
        match self {
            Self::Jpeg => "jpg",
            Self::Png => "png",
            Self::WebP => "webp",
        }
    }

    fn image_format(self) -> ImageFormat {
        match self {
            Self::Jpeg => ImageFormat::Jpeg,
            Self::Png => ImageFormat::Png,
            Self::WebP => ImageFormat::WebP,
        }
    }
}

fn convert_image(options: &ConvertOptions) -> Result<(), String> {
    let expected_source_format = source_format(&options.input)?;
    if expected_source_format == options.target_format {
        return Err("Source and target image formats must differ.".to_string());
    }

    let source_metadata = fs::metadata(&options.input).map_err(|error| {
        format!(
            "Input image does not exist or cannot be inspected: {}: {error}",
            options.input.to_string_lossy()
        )
    })?;
    if !source_metadata.is_file() {
        return Err(format!(
            "Input image path is not a file: {}",
            options.input.to_string_lossy()
        ));
    }

    validate_output_path(&options.output, options.target_format)?;

    let reader = ImageReader::open(&options.input)
        .map_err(|error| format!("Unable to open input image: {error}"))?
        .with_guessed_format()
        .map_err(|error| format!("Unable to detect input image format: {error}"))?;
    let detected_format = reader
        .format()
        .ok_or_else(|| "Unable to detect input image format.".to_string())?;
    if detected_format != expected_source_format.image_format() {
        return Err(format!(
            "Input image content does not match its .{} extension.",
            expected_source_format.extension()
        ));
    }

    let image = reader
        .decode()
        .map_err(|error| format!("Unable to decode input image: {error}"))?;
    write_image_create_new(&image, &options.output, options.target_format)
}

fn validate_output_path(output: &Path, target_format: TargetFormat) -> Result<(), String> {
    let output_extension = normalized_extension(output)
        .ok_or_else(|| "Output image must include a supported extension.".to_string())?;
    let extension_matches = match target_format {
        TargetFormat::Jpeg => output_extension == "jpg" || output_extension == "jpeg",
        TargetFormat::Png => output_extension == "png",
        TargetFormat::WebP => output_extension == "webp",
    };
    if !extension_matches {
        return Err(format!(
            "Output image extension must match target format {}.",
            target_format.extension()
        ));
    }

    let parent = output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| "Output image must have a parent folder.".to_string())?;
    if !parent.is_dir() {
        return Err(format!(
            "Output image parent folder does not exist: {}",
            parent.to_string_lossy()
        ));
    }
    if output.exists() {
        return Err(format!(
            "Output image already exists and will not be overwritten: {}",
            output.to_string_lossy()
        ));
    }

    Ok(())
}

fn write_image_create_new(
    image: &DynamicImage,
    output: &Path,
    target_format: TargetFormat,
) -> Result<(), String> {
    let write_result = (|| -> Result<(), String> {
        let output_file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output)
            .map_err(|error| format!("Unable to create output image: {error}"))?;
        let mut writer = BufWriter::new(output_file);
        write_image(image, &mut writer, target_format)
            .map_err(|error| format!("Unable to encode output image: {error}"))?;
        writer
            .flush()
            .map_err(|error| format!("Unable to finish output image: {error}"))?;
        Ok(())
    })();

    if write_result.is_err() {
        let _ = fs::remove_file(output);
    }

    write_result
}

fn write_image<W>(
    image: &DynamicImage,
    writer: &mut W,
    target_format: TargetFormat,
) -> image::ImageResult<()>
where
    W: Write + Seek,
{
    if target_format == TargetFormat::Jpeg {
        flatten_onto_white(image).write_to(writer, target_format.image_format())
    } else {
        image.write_to(writer, target_format.image_format())
    }
}

fn flatten_onto_white(image: &DynamicImage) -> DynamicImage {
    let rgba = image.to_rgba8();
    let mut rgb = RgbImage::new(rgba.width(), rgba.height());

    for (x, y, pixel) in rgba.enumerate_pixels() {
        let alpha = u16::from(pixel[3]);
        let inverse_alpha = 255 - alpha;
        let channel =
            |value: u8| ((u16::from(value) * alpha + 255 * inverse_alpha + 127) / 255) as u8;
        rgb.put_pixel(
            x,
            y,
            Rgb([channel(pixel[0]), channel(pixel[1]), channel(pixel[2])]),
        );
    }

    DynamicImage::ImageRgb8(rgb)
}

fn run_codec_self_check() -> Result<(), String> {
    let sample = DynamicImage::ImageRgba8(RgbaImage::from_pixel(2, 2, Rgba([32, 120, 200, 180])));

    for target_format in [TargetFormat::Jpeg, TargetFormat::Png, TargetFormat::WebP] {
        let mut writer = Cursor::new(Vec::new());
        write_image(&sample, &mut writer, target_format).map_err(|error| {
            format!(
                "{} codec self-check encode failed: {error}",
                target_format.extension()
            )
        })?;
        let bytes = writer.into_inner();
        if bytes.is_empty() {
            return Err(format!(
                "{} codec self-check produced an empty image.",
                target_format.extension()
            ));
        }
        let decoded = image::load_from_memory_with_format(&bytes, target_format.image_format())
            .map_err(|error| {
                format!(
                    "{} codec self-check decode failed: {error}",
                    target_format.extension()
                )
            })?;
        if decoded.dimensions() != (2, 2) {
            return Err(format!(
                "{} codec self-check returned unexpected dimensions.",
                target_format.extension()
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_case_dir(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        std::env::temp_dir()
            .join(format!("localconvert-image-engine-cli-{name}"))
            .join(unique.to_string())
    }

    #[test]
    fn parses_convert_arguments_as_structured_data() {
        let command = parse_cli_args([
            OsString::from("convert"),
            OsString::from("--input"),
            OsString::from("/Users/mac/客户 图片/input one.png"),
            OsString::from("--output"),
            OsString::from("/Users/mac/客户 图片/converted/input one.webp"),
            OsString::from("--format"),
            OsString::from("webp"),
        ])
        .expect("convert arguments should parse");

        assert_eq!(
            command,
            CliCommand::Convert(ConvertOptions {
                input: PathBuf::from("/Users/mac/客户 图片/input one.png"),
                output: PathBuf::from("/Users/mac/客户 图片/converted/input one.webp"),
                target_format: TargetFormat::WebP,
            })
        );
    }

    #[test]
    fn rejects_unknown_or_incomplete_commands() {
        assert!(parse_cli_args([OsString::from("convert")]).is_err());
        assert!(parse_cli_args([
            OsString::from("convert"),
            OsString::from("--input"),
            OsString::from("sample.png"),
            OsString::from("--output"),
            OsString::from("converted/sample.gif"),
            OsString::from("--format"),
            OsString::from("gif"),
        ])
        .is_err());
        assert!(parse_cli_args([OsString::from("resize")]).is_err());
    }

    #[test]
    fn codec_self_check_exercises_all_enabled_formats() {
        run_codec_self_check().expect("enabled codecs should pass the in-memory self-check");
    }

    #[test]
    fn converts_png_to_webp_without_modifying_source() {
        let case_dir = temp_case_dir("png-to-webp");
        let converted_dir = case_dir.join("converted");
        fs::create_dir_all(&converted_dir).expect("converted test directory should be created");
        let source = case_dir.join("客户 图片.png");
        let output = converted_dir.join("客户 图片.webp");
        let source_image =
            DynamicImage::ImageRgba8(RgbaImage::from_pixel(3, 2, Rgba([20, 80, 160, 200])));
        source_image
            .save_with_format(&source, ImageFormat::Png)
            .expect("source PNG should be written");
        let source_before = fs::read(&source).expect("source bytes should be readable");

        convert_image(&ConvertOptions {
            input: source.clone(),
            output: output.clone(),
            target_format: TargetFormat::WebP,
        })
        .expect("PNG to WebP conversion should succeed");

        assert!(fs::metadata(&output).expect("output should exist").len() > 0);
        assert_eq!(
            image::open(&output)
                .expect("output should decode")
                .dimensions(),
            (3, 2)
        );
        assert_eq!(
            fs::read(&source).expect("source bytes should remain readable"),
            source_before
        );

        let _ = fs::remove_dir_all(case_dir);
    }

    #[test]
    fn refuses_existing_output() {
        let case_dir = temp_case_dir("overwrite");
        fs::create_dir_all(&case_dir).expect("test directory should be created");
        let output = case_dir.join("existing.jpg");
        fs::write(&output, b"keep").expect("existing output should be created");

        let error = validate_output_path(&output, TargetFormat::Jpeg)
            .expect_err("existing output must be refused");
        assert!(error.contains("will not be overwritten"));
        assert_eq!(
            fs::read(&output).expect("existing output should remain"),
            b"keep"
        );

        let _ = fs::remove_dir_all(case_dir);
    }

    #[test]
    fn transparent_pixels_are_flattened_onto_white_for_jpeg() {
        let transparent = DynamicImage::ImageRgba8(RgbaImage::from_pixel(1, 1, Rgba([0, 0, 0, 0])));
        let flattened = flatten_onto_white(&transparent).to_rgb8();

        assert_eq!(flattened.get_pixel(0, 0), &Rgb([255, 255, 255]));
    }
}
