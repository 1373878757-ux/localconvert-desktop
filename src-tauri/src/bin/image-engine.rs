use image::{
    codecs::{
        jpeg::JpegEncoder,
        png::{CompressionType as PngCompressionType, FilterType as PngFilterType, PngEncoder},
    },
    imageops::FilterType as ResizeFilterType,
    DynamicImage, GenericImageView, ImageDecoder, ImageFormat, ImageReader, Rgb, RgbImage, Rgba,
    RgbaImage,
};
use serde::Serialize;
use std::{
    ffi::{OsStr, OsString},
    fs::{self, OpenOptions},
    io::{BufWriter, Cursor, Seek, Write},
    path::{Path, PathBuf},
};

const IMAGE_ENGINE_VERSION: &str = "0.4.0-preview.0";
const SELF_CHECK_MESSAGE: &str = "LocalConvert image-engine self-check ok";
const MAX_RESIZE_DIMENSION: u32 = 16_384;
const MAX_RESIZE_PIXELS: u64 = 64_000_000;
const MIN_COMPRESSION_QUALITY: u8 = 40;
const MAX_COMPRESSION_QUALITY: u8 = 95;
const DEFAULT_JPEG_QUALITY: u8 = 82;
const DEFAULT_WEBP_QUALITY: u8 = 80;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ResizeMode {
    Fit,
    Width,
    Height,
}

#[derive(Debug, PartialEq, Eq)]
struct ResizeOptions {
    input: PathBuf,
    output: PathBuf,
    mode: ResizeMode,
    max_width: Option<u32>,
    max_height: Option<u32>,
}

#[derive(Debug, PartialEq, Eq)]
struct CompressOptions {
    input: PathBuf,
    output: PathBuf,
    quality: Option<u8>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct ResizeReport {
    operation: &'static str,
    mode: &'static str,
    source_width: u32,
    source_height: u32,
    output_width: u32,
    output_height: u32,
    resized: bool,
    upscaled: bool,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct CompressionReport {
    operation: &'static str,
    format: &'static str,
    quality: Option<u8>,
    lossless: bool,
    source_width: u32,
    source_height: u32,
    output_width: u32,
    output_height: u32,
    source_bytes: u64,
    output_bytes: u64,
}

#[derive(Debug, PartialEq, Eq)]
enum CliCommand {
    Version,
    SelfCheck,
    Convert(ConvertOptions),
    Resize(ResizeOptions),
    Compress(CompressOptions),
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
        CliCommand::Resize(options) => {
            let report = resize_image(&options)?;
            serde_json::to_string(&report)
                .map_err(|error| format!("Unable to serialize resize result: {error}"))
        }
        CliCommand::Compress(options) => {
            let report = compress_image(&options)?;
            serde_json::to_string(&report)
                .map_err(|error| format!("Unable to serialize compression result: {error}"))
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
        Some("resize") => parse_resize_args(args).map(CliCommand::Resize),
        Some("compress") => parse_compress_args(args).map(CliCommand::Compress),
        _ => Err(usage_error("Unsupported image-engine command.")),
    }
}

fn parse_compress_args<I>(args: I) -> Result<CompressOptions, String>
where
    I: IntoIterator<Item = OsString>,
{
    let mut args = args.into_iter();
    let mut input = None;
    let mut output = None;
    let mut quality = None;

    while let Some(flag) = args.next() {
        let value = args.next().ok_or_else(|| {
            usage_error(&format!("Missing value for {}.", flag.to_string_lossy()))
        })?;

        match flag.to_str() {
            Some("--input") if input.is_none() => input = Some(PathBuf::from(value)),
            Some("--output") if output.is_none() => output = Some(PathBuf::from(value)),
            Some("--quality") if quality.is_none() => {
                quality = Some(parse_compression_quality(&value)?)
            }
            Some("--input" | "--output" | "--quality") => {
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

    Ok(CompressOptions {
        input: input.ok_or_else(|| usage_error("--input is required."))?,
        output: output.ok_or_else(|| usage_error("--output is required."))?,
        quality,
    })
}

fn parse_compression_quality(value: &OsStr) -> Result<u8, String> {
    let value = value
        .to_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| compression_quality_error("Compression quality is required."))?;
    let quality = value
        .parse::<u8>()
        .map_err(|_| compression_quality_error("Compression quality must be a whole number."))?;
    validate_compression_quality(quality)?;
    Ok(quality)
}

fn validate_compression_quality(quality: u8) -> Result<(), String> {
    if !(MIN_COMPRESSION_QUALITY..=MAX_COMPRESSION_QUALITY).contains(&quality) {
        return Err(compression_quality_error(
            "Compression quality is outside the supported range.",
        ));
    }
    Ok(())
}

fn compression_quality_error(message: &str) -> String {
    format!(
        "{message} Use a value from {MIN_COMPRESSION_QUALITY} through {MAX_COMPRESSION_QUALITY}."
    )
}

fn parse_resize_args<I>(args: I) -> Result<ResizeOptions, String>
where
    I: IntoIterator<Item = OsString>,
{
    let mut args = args.into_iter();
    let mut input = None;
    let mut output = None;
    let mut mode = None;
    let mut max_width = None;
    let mut max_height = None;

    while let Some(flag) = args.next() {
        let value = args.next().ok_or_else(|| {
            usage_error(&format!("Missing value for {}.", flag.to_string_lossy()))
        })?;

        match flag.to_str() {
            Some("--input") if input.is_none() => input = Some(PathBuf::from(value)),
            Some("--output") if output.is_none() => output = Some(PathBuf::from(value)),
            Some("--mode") if mode.is_none() => mode = Some(parse_resize_mode(&value)?),
            Some("--max-width") if max_width.is_none() => {
                max_width = Some(parse_resize_dimension(&value, "--max-width")?)
            }
            Some("--max-height") if max_height.is_none() => {
                max_height = Some(parse_resize_dimension(&value, "--max-height")?)
            }
            Some("--input" | "--output" | "--mode" | "--max-width" | "--max-height") => {
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

    let mode = mode.ok_or_else(|| usage_error("--mode is required."))?;
    validate_resize_request(mode, max_width, max_height)?;

    Ok(ResizeOptions {
        input: input.ok_or_else(|| usage_error("--input is required."))?,
        output: output.ok_or_else(|| usage_error("--output is required."))?,
        mode,
        max_width,
        max_height,
    })
}

fn parse_resize_mode(value: &OsStr) -> Result<ResizeMode, String> {
    match value.to_str().map(str::trim) {
        Some("fit") => Ok(ResizeMode::Fit),
        Some("width") => Ok(ResizeMode::Width),
        Some("height") => Ok(ResizeMode::Height),
        _ => Err("Resize mode must be fit, width, or height.".to_string()),
    }
}

fn parse_resize_dimension(value: &OsStr, option: &str) -> Result<u32, String> {
    let value = value
        .to_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("{option} must be a positive whole number."))?;
    let dimension = value
        .parse::<u32>()
        .map_err(|_| format!("{option} must be a positive whole number."))?;
    validate_dimension_limit(dimension, option)?;
    Ok(dimension)
}

fn validate_resize_request(
    mode: ResizeMode,
    max_width: Option<u32>,
    max_height: Option<u32>,
) -> Result<(), String> {
    if let Some(width) = max_width {
        validate_dimension_limit(width, "--max-width")?;
    }
    if let Some(height) = max_height {
        validate_dimension_limit(height, "--max-height")?;
    }

    match (mode, max_width, max_height) {
        (ResizeMode::Fit, Some(width), Some(height)) => {
            if u64::from(width) * u64::from(height) > MAX_RESIZE_PIXELS {
                return Err(format!(
                    "Requested resize box exceeds the {MAX_RESIZE_PIXELS}-pixel safety limit."
                ));
            }
            Ok(())
        }
        (ResizeMode::Width, Some(_), None) | (ResizeMode::Height, None, Some(_)) => Ok(()),
        (ResizeMode::Fit, _, _) => {
            Err("Fit mode requires both --max-width and --max-height.".to_string())
        }
        (ResizeMode::Width, _, _) => Err("Width mode requires only --max-width.".to_string()),
        (ResizeMode::Height, _, _) => Err("Height mode requires only --max-height.".to_string()),
    }
}

fn validate_dimension_limit(dimension: u32, option: &str) -> Result<(), String> {
    if dimension == 0 {
        return Err(format!("{option} must be greater than zero."));
    }
    if dimension > MAX_RESIZE_DIMENSION {
        return Err(format!(
            "{option} must not exceed {MAX_RESIZE_DIMENSION} pixels."
        ));
    }
    Ok(())
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
        "{message} Usage: image-engine --version | --self-check | convert --input <path> --output <path> --format <jpg|png|webp> | resize --input <path> --output <path> --mode <fit|width|height> [--max-width <pixels>] [--max-height <pixels>] | compress --input <path> --output <path> [--quality <40-95>]"
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

impl ResizeMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Fit => "fit",
            Self::Width => "width",
            Self::Height => "height",
        }
    }
}

fn convert_image(options: &ConvertOptions) -> Result<(), String> {
    let expected_source_format = source_format(&options.input)?;
    if expected_source_format == options.target_format {
        return Err("Source and target image formats must differ.".to_string());
    }

    validate_input_file(&options.input)?;
    validate_output_path(&options.output, options.target_format)?;
    let image = decode_oriented_image(&options.input, expected_source_format)?;

    write_image_create_new(&image, &options.output, options.target_format)
}

fn resize_image(options: &ResizeOptions) -> Result<ResizeReport, String> {
    validate_resize_request(options.mode, options.max_width, options.max_height)?;
    let source_format = source_format(&options.input)?;
    validate_input_file(&options.input)?;
    validate_output_path(&options.output, source_format)?;

    let image = decode_oriented_image(&options.input, source_format)?;
    let (source_width, source_height) = image.dimensions();
    let (output_width, output_height) = calculate_resize_dimensions(
        source_width,
        source_height,
        options.mode,
        options.max_width,
        options.max_height,
    )?;
    let resized = (output_width, output_height) != (source_width, source_height);
    let output_image = if resized {
        image.resize_exact(output_width, output_height, ResizeFilterType::Lanczos3)
    } else {
        image
    };

    write_image_create_new(&output_image, &options.output, source_format)?;

    Ok(ResizeReport {
        operation: "resize",
        mode: options.mode.as_str(),
        source_width,
        source_height,
        output_width,
        output_height,
        resized,
        upscaled: false,
    })
}

fn compress_image(options: &CompressOptions) -> Result<CompressionReport, String> {
    let source_format = source_format(&options.input)?;
    let quality = compression_quality_for_format(source_format, options.quality)?;
    validate_input_file(&options.input)?;
    validate_output_path(&options.output, source_format)?;

    let source_bytes = fs::metadata(&options.input)
        .map_err(|error| format!("Unable to inspect input image size: {error}"))?
        .len();
    let image = decode_oriented_image(&options.input, source_format)?;
    let (source_width, source_height) = image.dimensions();
    write_compressed_image_create_new(&image, &options.output, source_format, quality)?;
    let output_bytes = fs::metadata(&options.output)
        .map_err(|error| format!("Unable to inspect compressed image size: {error}"))?
        .len();

    Ok(CompressionReport {
        operation: "compress",
        format: source_format.extension(),
        quality,
        lossless: source_format == TargetFormat::Png,
        source_width,
        source_height,
        output_width: source_width,
        output_height: source_height,
        source_bytes,
        output_bytes,
    })
}

fn compression_quality_for_format(
    source_format: TargetFormat,
    requested_quality: Option<u8>,
) -> Result<Option<u8>, String> {
    match source_format {
        TargetFormat::Jpeg => {
            let quality = requested_quality.unwrap_or(DEFAULT_JPEG_QUALITY);
            validate_compression_quality(quality)?;
            Ok(Some(quality))
        }
        TargetFormat::WebP => {
            let quality = requested_quality.unwrap_or(DEFAULT_WEBP_QUALITY);
            validate_compression_quality(quality)?;
            Ok(Some(quality))
        }
        TargetFormat::Png if requested_quality.is_some() => {
            Err("PNG optimization is lossless and does not accept --quality.".to_string())
        }
        TargetFormat::Png => Ok(None),
    }
}

fn validate_input_file(input: &Path) -> Result<(), String> {
    let source_metadata = fs::metadata(input).map_err(|error| {
        format!(
            "Input image does not exist or cannot be inspected: {}: {error}",
            input.to_string_lossy()
        )
    })?;
    if !source_metadata.is_file() {
        return Err(format!(
            "Input image path is not a file: {}",
            input.to_string_lossy()
        ));
    }
    Ok(())
}

fn decode_oriented_image(
    input: &Path,
    expected_source_format: TargetFormat,
) -> Result<DynamicImage, String> {
    let reader = ImageReader::open(input)
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

    let mut decoder = reader
        .into_decoder()
        .map_err(|error| format!("Unable to decode input image: {error}"))?;
    let mut limits = image::Limits::default();
    limits
        .reserve(decoder.total_bytes())
        .map_err(|error| format!("Unable to decode input image: {error}"))?;
    decoder
        .set_limits(limits)
        .map_err(|error| format!("Unable to decode input image: {error}"))?;
    let orientation = decoder
        .orientation()
        .map_err(|error| format!("Unable to read input image orientation: {error}"))?;
    let mut image = DynamicImage::from_decoder(decoder)
        .map_err(|error| format!("Unable to decode input image: {error}"))?;
    image.apply_orientation(orientation);
    Ok(image)
}

fn calculate_resize_dimensions(
    source_width: u32,
    source_height: u32,
    mode: ResizeMode,
    max_width: Option<u32>,
    max_height: Option<u32>,
) -> Result<(u32, u32), String> {
    if source_width == 0 || source_height == 0 {
        return Err("Input image dimensions must be greater than zero.".to_string());
    }
    validate_resize_request(mode, max_width, max_height)?;

    let dimensions = match mode {
        ResizeMode::Fit => {
            let width = max_width.expect("validated fit width");
            let height = max_height.expect("validated fit height");
            if source_width <= width && source_height <= height {
                (source_width, source_height)
            } else if u64::from(width) * u64::from(source_height)
                <= u64::from(height) * u64::from(source_width)
            {
                (width, scaled_dimension(source_height, width, source_width))
            } else {
                (
                    scaled_dimension(source_width, height, source_height),
                    height,
                )
            }
        }
        ResizeMode::Width => {
            let width = max_width.expect("validated width-only width");
            if source_width <= width {
                (source_width, source_height)
            } else {
                (width, scaled_dimension(source_height, width, source_width))
            }
        }
        ResizeMode::Height => {
            let height = max_height.expect("validated height-only height");
            if source_height <= height {
                (source_width, source_height)
            } else {
                (
                    scaled_dimension(source_width, height, source_height),
                    height,
                )
            }
        }
    };

    validate_result_dimensions(dimensions.0, dimensions.1)?;
    Ok(dimensions)
}

fn scaled_dimension(source: u32, target: u32, source_reference: u32) -> u32 {
    ((u64::from(source) * u64::from(target)) / u64::from(source_reference))
        .max(1)
        .try_into()
        .expect("scaled image dimension should fit in u32")
}

fn validate_result_dimensions(width: u32, height: u32) -> Result<(), String> {
    if width == 0 || height == 0 {
        return Err("Calculated resize dimensions must be greater than zero.".to_string());
    }
    if width > MAX_RESIZE_DIMENSION || height > MAX_RESIZE_DIMENSION {
        return Err(format!(
            "Calculated resize dimensions must not exceed {MAX_RESIZE_DIMENSION} pixels per edge."
        ));
    }
    if u64::from(width) * u64::from(height) > MAX_RESIZE_PIXELS {
        return Err(format!(
            "Calculated resize output exceeds the {MAX_RESIZE_PIXELS}-pixel safety limit."
        ));
    }
    Ok(())
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
    // The backend owns failure cleanup through its task-scoped workspace.
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
}

fn write_compressed_image_create_new(
    image: &DynamicImage,
    output: &Path,
    source_format: TargetFormat,
    quality: Option<u8>,
) -> Result<(), String> {
    let output_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .map_err(|error| format!("Unable to create compressed image: {error}"))?;
    let mut writer = BufWriter::new(output_file);

    match source_format {
        TargetFormat::Jpeg => {
            let quality = quality.expect("validated JPEG quality");
            let flattened = flatten_onto_white(image);
            flattened
                .write_with_encoder(JpegEncoder::new_with_quality(&mut writer, quality))
                .map_err(|error| format!("Unable to encode compressed JPEG: {error}"))?;
        }
        TargetFormat::Png => {
            image
                .write_with_encoder(PngEncoder::new_with_quality(
                    &mut writer,
                    PngCompressionType::Best,
                    PngFilterType::Adaptive,
                ))
                .map_err(|error| format!("Unable to optimize PNG losslessly: {error}"))?;
        }
        TargetFormat::WebP => {
            let quality = quality.expect("validated WebP quality");
            let rgba = image.to_rgba8();
            let encoded = webpx::Encoder::new_rgba(rgba.as_raw(), rgba.width(), rgba.height())
                .quality(f32::from(quality))
                .encode_owned(webpx::Unstoppable)
                .map_err(|error| format!("Unable to encode compressed WebP: {error}"))?;
            writer
                .write_all(&encoded)
                .map_err(|error| format!("Unable to write compressed WebP: {error}"))?;
        }
    }

    writer
        .flush()
        .map_err(|error| format!("Unable to finish compressed image: {error}"))?;
    Ok(())
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

    let rgba = sample.to_rgba8();
    let lossy_webp = webpx::Encoder::new_rgba(rgba.as_raw(), rgba.width(), rgba.height())
        .quality(f32::from(DEFAULT_WEBP_QUALITY))
        .encode_owned(webpx::Unstoppable)
        .map_err(|error| format!("lossy WebP codec self-check encode failed: {error}"))?;
    let decoded = image::load_from_memory_with_format(&lossy_webp, ImageFormat::WebP)
        .map_err(|error| format!("lossy WebP codec self-check decode failed: {error}"))?;
    if decoded.dimensions() != (2, 2) {
        return Err("lossy WebP codec self-check returned unexpected dimensions.".to_string());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{codecs::jpeg::JpegEncoder, metadata::Orientation};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[derive(Clone, Copy, Debug)]
    enum ExpectedColor {
        Red,
        Green,
        Blue,
        Yellow,
    }

    fn temp_case_dir(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        std::env::temp_dir()
            .join(format!("localconvert-image-engine-cli-{name}"))
            .join(unique.to_string())
    }

    fn jpeg_exif_orientation_segment(orientation: u8) -> Vec<u8> {
        assert!((1..=8).contains(&orientation));

        let tiff_data = [
            0x4d,
            0x4d,
            0x00,
            0x2a, // Big-endian TIFF header.
            0x00,
            0x00,
            0x00,
            0x08, // Offset to the first IFD.
            0x00,
            0x01, // One IFD entry.
            0x01,
            0x12, // Orientation tag.
            0x00,
            0x03, // SHORT value.
            0x00,
            0x00,
            0x00,
            0x01, // One value.
            0x00,
            orientation,
            0x00,
            0x00, // Orientation value and padding.
            0x00,
            0x00,
            0x00,
            0x00, // No next IFD.
        ];
        let mut payload = b"Exif\0\0".to_vec();
        payload.extend_from_slice(&tiff_data);
        let segment_length =
            u16::try_from(payload.len() + 2).expect("test EXIF segment should fit in JPEG APP1");

        let mut segment = vec![0xff, 0xe1];
        segment.extend_from_slice(&segment_length.to_be_bytes());
        segment.extend_from_slice(&payload);
        segment
    }

    fn write_oriented_jpeg(path: &Path, orientation: u8) {
        let mut pixels = RgbImage::new(32, 24);
        for (x, y, pixel) in pixels.enumerate_pixels_mut() {
            *pixel = match (x < 16, y < 12) {
                (true, true) => Rgb([255, 0, 0]),
                (false, true) => Rgb([0, 255, 0]),
                (true, false) => Rgb([0, 0, 255]),
                (false, false) => Rgb([255, 255, 0]),
            };
        }

        let mut jpeg = Vec::new();
        JpegEncoder::new_with_quality(&mut jpeg, 100)
            .encode_image(&DynamicImage::ImageRgb8(pixels))
            .expect("test JPEG should encode");
        assert!(jpeg.starts_with(&[0xff, 0xd8]));

        let mut oriented_jpeg = Vec::with_capacity(jpeg.len() + 36);
        oriented_jpeg.extend_from_slice(&jpeg[..2]);
        oriented_jpeg.extend_from_slice(&jpeg_exif_orientation_segment(orientation));
        oriented_jpeg.extend_from_slice(&jpeg[2..]);
        fs::write(path, oriented_jpeg).expect("oriented test JPEG should be written");
    }

    fn read_orientation(path: &Path) -> Orientation {
        let reader = ImageReader::open(path)
            .expect("test image should open")
            .with_guessed_format()
            .expect("test image format should be detected");
        let mut decoder = reader
            .into_decoder()
            .expect("test decoder should initialize");
        decoder
            .orientation()
            .expect("test image orientation should be readable")
    }

    fn write_test_image(path: &Path, width: u32, height: u32, format: ImageFormat) -> Vec<u8> {
        let image = DynamicImage::ImageRgba8(RgbaImage::from_pixel(
            width,
            height,
            Rgba([30, 110, 190, 220]),
        ));
        image
            .save_with_format(path, format)
            .expect("test image should be written");
        fs::read(path).expect("test image bytes should be readable")
    }

    fn assert_corner_color(pixel: Rgba<u8>, expected: ExpectedColor) {
        let [red, green, blue, _alpha] = pixel.0;
        let matches = match expected {
            ExpectedColor::Red => red > 180 && green < 80 && blue < 80,
            ExpectedColor::Green => green > 180 && red < 80 && blue < 80,
            ExpectedColor::Blue => blue > 180 && red < 80 && green < 80,
            ExpectedColor::Yellow => red > 180 && green > 180 && blue < 80,
        };
        assert!(
            matches,
            "expected {expected:?}, found RGB({red}, {green}, {blue})"
        );
    }

    fn assert_jpeg_orientation_conversion(
        exif_orientation: u8,
        target_format: TargetFormat,
        expected_dimensions: (u32, u32),
        expected_corners: [ExpectedColor; 4],
    ) {
        let case_dir = temp_case_dir(&format!("jpeg-orientation-{exif_orientation}"));
        let converted_dir = case_dir.join("converted");
        fs::create_dir_all(&converted_dir).expect("converted test directory should be created");
        let source = case_dir.join("phone photo.jpg");
        let output = converted_dir.join(format!("phone photo.{}", target_format.extension()));
        write_oriented_jpeg(&source, exif_orientation);
        let source_before = fs::read(&source).expect("source bytes should be readable");
        let expected_source_orientation =
            Orientation::from_exif(exif_orientation).expect("test orientation should be valid");
        assert_eq!(read_orientation(&source), expected_source_orientation);

        convert_image(&ConvertOptions {
            input: source.clone(),
            output: output.clone(),
            target_format,
        })
        .expect("oriented JPEG conversion should succeed");

        let output_image = image::open(&output).expect("converted image should decode");
        assert_eq!(output_image.dimensions(), expected_dimensions);
        let (width, height) = expected_dimensions;
        let sample_points = [
            (2, 2),
            (width - 3, 2),
            (2, height - 3),
            (width - 3, height - 3),
        ];
        for ((x, y), expected) in sample_points.into_iter().zip(expected_corners) {
            assert_corner_color(output_image.get_pixel(x, y), expected);
        }
        assert_eq!(read_orientation(&output), Orientation::NoTransforms);
        assert_eq!(
            fs::read(&source).expect("source bytes should remain readable"),
            source_before
        );

        let _ = fs::remove_dir_all(case_dir);
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
    fn parses_resize_arguments_as_structured_data() {
        let command = parse_cli_args([
            OsString::from("resize"),
            OsString::from("--input"),
            OsString::from("/Users/mac/客户 图片/input one.jpeg"),
            OsString::from("--output"),
            OsString::from("/Users/mac/客户 图片/converted/input one.jpeg"),
            OsString::from("--mode"),
            OsString::from("fit"),
            OsString::from("--max-width"),
            OsString::from("1600"),
            OsString::from("--max-height"),
            OsString::from("1200"),
        ])
        .expect("resize arguments should parse");

        assert_eq!(
            command,
            CliCommand::Resize(ResizeOptions {
                input: PathBuf::from("/Users/mac/客户 图片/input one.jpeg"),
                output: PathBuf::from("/Users/mac/客户 图片/converted/input one.jpeg"),
                mode: ResizeMode::Fit,
                max_width: Some(1600),
                max_height: Some(1200),
            })
        );
    }

    #[test]
    fn parses_compress_arguments_and_validates_quality() {
        let command = parse_cli_args([
            OsString::from("compress"),
            OsString::from("--input"),
            OsString::from("/Users/mac/客户 图片/input one.webp"),
            OsString::from("--output"),
            OsString::from("/Users/mac/客户 图片/converted/input one compressed.webp"),
            OsString::from("--quality"),
            OsString::from("80"),
        ])
        .expect("compression arguments should parse");

        assert_eq!(
            command,
            CliCommand::Compress(CompressOptions {
                input: PathBuf::from("/Users/mac/客户 图片/input one.webp"),
                output: PathBuf::from("/Users/mac/客户 图片/converted/input one compressed.webp"),
                quality: Some(80),
            })
        );

        for value in ["39", "96", "-1", "80.5", "quality"] {
            let result = parse_cli_args([
                OsString::from("compress"),
                OsString::from("--input"),
                OsString::from("sample.jpg"),
                OsString::from("--output"),
                OsString::from("converted/sample compressed.jpg"),
                OsString::from("--quality"),
                OsString::from(value),
            ]);
            assert!(result.is_err(), "quality {value} should be rejected");
        }
    }

    #[test]
    fn rejects_invalid_resize_dimensions_and_mode_shapes() {
        for value in ["0", "-1", "1.5", "large", "16385"] {
            let result = parse_cli_args([
                OsString::from("resize"),
                OsString::from("--input"),
                OsString::from("sample.png"),
                OsString::from("--output"),
                OsString::from("converted/sample.png"),
                OsString::from("--mode"),
                OsString::from("width"),
                OsString::from("--max-width"),
                OsString::from(value),
            ]);
            assert!(result.is_err(), "{value} should be rejected");
        }

        assert!(validate_resize_request(ResizeMode::Fit, Some(8000), Some(8000)).is_ok());
        assert!(validate_resize_request(ResizeMode::Fit, Some(10_000), Some(10_000)).is_err());
        assert!(validate_resize_request(ResizeMode::Fit, Some(100), None).is_err());
        assert!(validate_resize_request(ResizeMode::Width, Some(100), Some(100)).is_err());
        assert!(validate_resize_request(ResizeMode::Height, Some(100), None).is_err());
    }

    #[test]
    fn calculates_aspect_ratio_preserving_dimensions_without_upscale() {
        assert_eq!(
            calculate_resize_dimensions(400, 200, ResizeMode::Fit, Some(100), Some(100)),
            Ok((100, 50))
        );
        assert_eq!(
            calculate_resize_dimensions(400, 200, ResizeMode::Width, Some(120), None),
            Ok((120, 60))
        );
        assert_eq!(
            calculate_resize_dimensions(200, 400, ResizeMode::Height, None, Some(120)),
            Ok((60, 120))
        );
        assert_eq!(
            calculate_resize_dimensions(100, 50, ResizeMode::Fit, Some(1000), Some(1000)),
            Ok((100, 50))
        );
        assert!(
            calculate_resize_dimensions(16_000, 80_000, ResizeMode::Width, Some(16_000), None)
                .is_err()
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
    fn resizes_jpeg_to_fit_within_box_and_preserves_source() {
        let case_dir = temp_case_dir("resize-jpeg-fit");
        let converted_dir = case_dir.join("converted");
        fs::create_dir_all(&converted_dir).expect("converted test directory should be created");
        let source = case_dir.join("office photo.jpg");
        let output = converted_dir.join("office photo.jpg");
        let source_before = write_test_image(&source, 400, 200, ImageFormat::Jpeg);

        let report = resize_image(&ResizeOptions {
            input: source.clone(),
            output: output.clone(),
            mode: ResizeMode::Fit,
            max_width: Some(100),
            max_height: Some(100),
        })
        .expect("JPEG fit resize should succeed");

        assert_eq!((report.output_width, report.output_height), (100, 50));
        assert!(report.resized);
        assert!(!report.upscaled);
        assert_eq!(
            image::open(&output)
                .expect("resized JPEG should decode")
                .dimensions(),
            (100, 50)
        );
        assert_eq!(
            fs::read(&source).expect("source should remain"),
            source_before
        );

        let _ = fs::remove_dir_all(case_dir);
    }

    #[test]
    fn resizes_png_by_width_and_webp_by_height() {
        let case_dir = temp_case_dir("resize-width-height");
        let converted_dir = case_dir.join("converted");
        fs::create_dir_all(&converted_dir).expect("converted test directory should be created");

        let png_source = case_dir.join("wide image.png");
        let png_output = converted_dir.join("wide image.png");
        write_test_image(&png_source, 400, 200, ImageFormat::Png);
        let png_report = resize_image(&ResizeOptions {
            input: png_source,
            output: png_output.clone(),
            mode: ResizeMode::Width,
            max_width: Some(100),
            max_height: None,
        })
        .expect("PNG width-only resize should succeed");
        assert_eq!(
            (png_report.output_width, png_report.output_height),
            (100, 50)
        );
        assert_eq!(
            image::open(&png_output)
                .expect("resized PNG should decode")
                .dimensions(),
            (100, 50)
        );

        let webp_source = case_dir.join("tall image.webp");
        let webp_output = converted_dir.join("tall image.webp");
        write_test_image(&webp_source, 200, 400, ImageFormat::WebP);
        let webp_report = resize_image(&ResizeOptions {
            input: webp_source,
            output: webp_output.clone(),
            mode: ResizeMode::Height,
            max_width: None,
            max_height: Some(100),
        })
        .expect("WebP height-only resize should succeed");
        assert_eq!(
            (webp_report.output_width, webp_report.output_height),
            (50, 100)
        );
        assert_eq!(
            image::open(&webp_output)
                .expect("resized WebP should decode")
                .dimensions(),
            (50, 100)
        );

        let _ = fs::remove_dir_all(case_dir);
    }

    #[test]
    fn applies_orientation_before_resize_and_clears_output_orientation() {
        let case_dir = temp_case_dir("resize-oriented-jpeg");
        let converted_dir = case_dir.join("converted");
        fs::create_dir_all(&converted_dir).expect("converted test directory should be created");
        let source = case_dir.join("手机 照片.jpg");
        let output = converted_dir.join("手机 照片.jpg");
        write_oriented_jpeg(&source, 6);

        let report = resize_image(&ResizeOptions {
            input: source,
            output: output.clone(),
            mode: ResizeMode::Fit,
            max_width: Some(16),
            max_height: Some(16),
        })
        .expect("oriented JPEG resize should succeed");

        assert_eq!((report.source_width, report.source_height), (24, 32));
        assert_eq!((report.output_width, report.output_height), (12, 16));
        assert_eq!(read_orientation(&output), Orientation::NoTransforms);

        let _ = fs::remove_dir_all(case_dir);
    }

    #[test]
    fn no_upscale_keeps_dimensions_and_emits_structured_report() {
        let case_dir = temp_case_dir("resize-no-upscale");
        let converted_dir = case_dir.join("converted");
        fs::create_dir_all(&converted_dir).expect("converted test directory should be created");
        let source = case_dir.join("small.png");
        let output = converted_dir.join("small.png");
        write_test_image(&source, 100, 50, ImageFormat::Png);

        let stdout = run([
            OsString::from("resize"),
            OsString::from("--input"),
            source.as_os_str().to_os_string(),
            OsString::from("--output"),
            output.as_os_str().to_os_string(),
            OsString::from("--mode"),
            OsString::from("fit"),
            OsString::from("--max-width"),
            OsString::from("1000"),
            OsString::from("--max-height"),
            OsString::from("1000"),
        ])
        .expect("no-upscale resize should succeed");
        let report: serde_json::Value =
            serde_json::from_str(&stdout).expect("resize output should be JSON");

        assert_eq!(report["sourceWidth"], 100);
        assert_eq!(report["sourceHeight"], 50);
        assert_eq!(report["outputWidth"], 100);
        assert_eq!(report["outputHeight"], 50);
        assert_eq!(report["resized"], false);
        assert_eq!(report["upscaled"], false);

        let _ = fs::remove_dir_all(case_dir);
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
    fn preserves_normal_jpeg_orientation() {
        assert_jpeg_orientation_conversion(
            1,
            TargetFormat::Png,
            (32, 24),
            [
                ExpectedColor::Red,
                ExpectedColor::Green,
                ExpectedColor::Blue,
                ExpectedColor::Yellow,
            ],
        );
    }

    #[test]
    fn applies_jpeg_rotate_90_orientation_before_webp_encoding() {
        assert_jpeg_orientation_conversion(
            6,
            TargetFormat::WebP,
            (24, 32),
            [
                ExpectedColor::Blue,
                ExpectedColor::Red,
                ExpectedColor::Yellow,
                ExpectedColor::Green,
            ],
        );
    }

    #[test]
    fn applies_jpeg_rotate_180_orientation() {
        assert_jpeg_orientation_conversion(
            3,
            TargetFormat::Png,
            (32, 24),
            [
                ExpectedColor::Yellow,
                ExpectedColor::Blue,
                ExpectedColor::Green,
                ExpectedColor::Red,
            ],
        );
    }

    #[test]
    fn applies_jpeg_rotate_270_orientation() {
        assert_jpeg_orientation_conversion(
            8,
            TargetFormat::Png,
            (24, 32),
            [
                ExpectedColor::Green,
                ExpectedColor::Yellow,
                ExpectedColor::Red,
                ExpectedColor::Blue,
            ],
        );
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

    #[test]
    fn compresses_jpeg_and_webp_with_quality_without_modifying_sources() {
        let case_dir = temp_case_dir("compress-jpeg-webp");
        let converted_dir = case_dir.join("converted");
        fs::create_dir_all(&converted_dir).expect("converted directory should be created");

        for (format, extension, quality) in [
            (ImageFormat::Jpeg, "jpg", DEFAULT_JPEG_QUALITY),
            (ImageFormat::WebP, "webp", DEFAULT_WEBP_QUALITY),
        ] {
            let source = case_dir.join(format!("客户 图片 source.{extension}"));
            let output = converted_dir.join(format!("客户 图片 compressed.{extension}"));
            let source_before = write_test_image(&source, 64, 48, format);

            let report = compress_image(&CompressOptions {
                input: source.clone(),
                output: output.clone(),
                quality: Some(quality),
            })
            .expect("quality compression should succeed");

            assert_eq!(report.quality, Some(quality));
            assert!(!report.lossless);
            assert_eq!((report.output_width, report.output_height), (64, 48));
            assert!(report.output_bytes > 0);
            assert_eq!(
                image::open(&output)
                    .expect("compressed output should decode")
                    .dimensions(),
                (64, 48)
            );
            assert_eq!(
                fs::read(&source).expect("source should remain readable"),
                source_before
            );
        }

        let _ = fs::remove_dir_all(case_dir);
    }

    #[test]
    fn optimizes_png_losslessly_and_preserves_pixels() {
        let case_dir = temp_case_dir("compress-png-lossless");
        let converted_dir = case_dir.join("converted");
        fs::create_dir_all(&converted_dir).expect("converted directory should be created");
        let source = case_dir.join("lossless source.png");
        let output = converted_dir.join("lossless source compressed.png");
        let source_before = write_test_image(&source, 31, 27, ImageFormat::Png);
        let source_pixels = image::open(&source)
            .expect("source PNG should decode")
            .to_rgba8();

        let report = compress_image(&CompressOptions {
            input: source.clone(),
            output: output.clone(),
            quality: None,
        })
        .expect("lossless PNG optimization should succeed");

        assert!(report.lossless);
        assert_eq!(report.quality, None);
        assert_eq!(
            image::open(&output)
                .expect("optimized PNG should decode")
                .to_rgba8(),
            source_pixels
        );
        assert_eq!(fs::read(&source).unwrap(), source_before);
        assert!(compress_image(&CompressOptions {
            input: source,
            output: converted_dir.join("invalid-quality.png"),
            quality: Some(80),
        })
        .is_err());

        let _ = fs::remove_dir_all(case_dir);
    }

    #[test]
    fn applies_jpeg_orientation_before_compression() {
        let case_dir = temp_case_dir("compress-oriented-jpeg");
        let converted_dir = case_dir.join("converted");
        fs::create_dir_all(&converted_dir).expect("converted directory should be created");
        let source = case_dir.join("手机 照片.jpg");
        let output = converted_dir.join("手机 照片 compressed.jpg");
        write_oriented_jpeg(&source, 6);
        let source_before = fs::read(&source).expect("source should be readable");

        let report = compress_image(&CompressOptions {
            input: source.clone(),
            output: output.clone(),
            quality: Some(82),
        })
        .expect("oriented JPEG compression should succeed");

        assert_eq!((report.output_width, report.output_height), (24, 32));
        assert_eq!(
            image::open(&output)
                .expect("compressed JPEG should decode")
                .dimensions(),
            (24, 32)
        );
        assert_eq!(read_orientation(&output), Orientation::NoTransforms);
        assert_eq!(fs::read(&source).unwrap(), source_before);

        let _ = fs::remove_dir_all(case_dir);
    }
}
