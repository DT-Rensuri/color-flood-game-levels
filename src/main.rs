use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const DEFAULT_WIDTH: usize = 10;
const DEFAULT_HEIGHT: usize = 8;
const DEFAULT_REPEATS: usize = 6;
const DEFAULT_TARGET_COLOR: u8 = 4;

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Level {
    level: u32,
    max_moves: usize,
    target_color: u8,
    colors: BTreeMap<String, String>,
    grid: Vec<Vec<u8>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    schema_version: u32,
    catalog_version: u32,
    levels: Vec<ManifestLevel>,
}

#[derive(Serialize)]
struct ManifestLevel {
    id: String,
    order: u32,
    url: String,
    sha256: String,
    size: usize,
    enabled: bool,
}

#[derive(Clone, Copy)]
struct Cell {
    x: usize,
    y: usize,
}

struct Template {
    cells: Vec<Cell>,
    width: usize,
    height: usize,
    path: PathBuf,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = Options::from_args()?;
    if options.manifest_only {
        update_manifest(
            &options.levels_dir,
            &options.manifest,
            &options.base_url,
            options.catalog_version,
        )?;
        return Ok(());
    }
    let templates = load_templates(&options.templates_dir)?;
    if let Some(invalid_template) = templates
        .iter()
        .find(|t| t.width > options.width || t.height > options.height)
    {
        return Err(format!(
            "Template '{}' ({x}x{y}) in {} is larger than the {}x{} grid",
            invalid_template.path.display(),
            options.templates_dir.display(),
            options.width,
            options.height,
            x = invalid_template.width,
            y = invalid_template.height,
        )
        .into());
    }
    let seed = options.seed.unwrap_or_else(random_seed);
    let mut rng = StdRng::seed_from_u64(seed);
    let mut grid: Vec<Vec<u8>> = vec![vec![options.target_color; options.width]; options.height];
    let available_colors: Vec<u8> = (1..=4)
        .filter(|color| *color != options.target_color)
        .collect();

    for _ in 0..options.repeats {
        let template = &templates[rng.random_range(0..templates.len())];
        let color = available_colors[rng.random_range(0..available_colors.len())];
        paint_template(
            &mut grid,
            &template.cells,
            color,
            &mut rng,
            options.width,
            options.height,
            template.width,
            template.height,
        );
    }

    let level = Level {
        level: options.level,
        max_moves: options.repeats + 1,
        target_color: options.target_color,
        colors: color_palette(),
        grid,
    };
    if let Some(parent) = options.output.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    fs::write(&options.output, serde_json::to_vec_pretty(&level)?)?;
    update_manifest(
        &options.levels_dir,
        &options.manifest,
        &options.base_url,
        options.catalog_version,
    )?;
    println!(
        "Generated {} and updated {} (level {}, seed {}, maxMoves {})",
        options.output.display(),
        options.manifest.display(),
        options.level,
        seed,
        level.max_moves
    );
    Ok(())
}

struct Options {
    output: PathBuf,
    level: u32,
    width: usize,
    height: usize,
    repeats: usize,
    target_color: u8,
    seed: Option<u64>,
    templates_dir: PathBuf,
    levels_dir: PathBuf,
    manifest: PathBuf,
    base_url: String,
    catalog_version: u32,
    manifest_only: bool,
    output_explicit: bool,
    level_explicit: bool,
}

impl Options {
    fn from_args() -> Result<Self, Box<dyn std::error::Error>> {
        let mut options = Self {
            output: PathBuf::from("levels/level_1.json"),
            level: 0,
            width: DEFAULT_WIDTH,
            height: DEFAULT_HEIGHT,
            repeats: DEFAULT_REPEATS,
            target_color: DEFAULT_TARGET_COLOR,
            seed: None,
            templates_dir: PathBuf::from("templates"),
            levels_dir: PathBuf::from("levels"),
            manifest: PathBuf::from("manifest.json"),
            base_url: String::from(
                "https://raw.githubusercontent.com/DT-Rensuri/color-flood-game-levels/main/levels",
            ),
            catalog_version: 1,
            manifest_only: false,
            output_explicit: false,
            level_explicit: false,
        };
        let mut args = env::args().skip(1);
        while let Some(argument) = args.next() {
            let mut value = || -> Result<String, String> {
                args.next()
                    .ok_or_else(|| format!("Missing value for {argument}"))
            };
            match argument.as_str() {
                "--output" => {
                    options.output = value()?.into();
                    options.output_explicit = true;
                }
                "--level" => {
                    options.level = value()?.parse()?;
                    options.level_explicit = true;
                }
                "--width" => options.width = value()?.parse()?,
                "--height" => options.height = value()?.parse()?,
                "--repeats" => options.repeats = value()?.parse()?,
                "--target-color" => options.target_color = value()?.parse()?,
                "--seed" => options.seed = Some(value()?.parse()?),
                "--templates-dir" => options.templates_dir = value()?.into(),
                "--levels-dir" => options.levels_dir = value()?.into(),
                "--manifest" => options.manifest = value()?.into(),
                "--base-url" => options.base_url = value()?.trim_end_matches('/').to_owned(),
                "--catalog-version" => options.catalog_version = value()?.parse()?,
                "--manifest-only" => options.manifest_only = true,
                "--help" | "-h" => {
                    print_help();
                    std::process::exit(0);
                }
                unknown => return Err(format!("Unknown argument: {unknown}").into()),
            }
        }
        if options.width == 0 || options.height == 0 {
            return Err("width and height must be greater than zero".into());
        }
        if !(1..=4).contains(&options.target_color) {
            return Err("target-color must be between 1 and 4".into());
        }
        if options.level_explicit && options.level == 0 {
            return Err("level must be greater than zero".into());
        }
        if !options.level_explicit {
            options.level = next_level_number(&options.levels_dir)?;
        }
        if !options.output_explicit {
            options.output = options
                .levels_dir
                .join(format!("level_{}.json", options.level));
        }
        Ok(options)
    }
}

fn next_level_number(levels_dir: &Path) -> Result<u32, Box<dyn std::error::Error>> {
    let mut highest = 0;
    if !levels_dir.exists() {
        return Ok(1);
    }
    for entry in fs::read_dir(levels_dir)? {
        let path = entry?.path();
        if path
            .extension()
            .is_some_and(|extension| extension == "json")
        {
            highest = highest.max(level_number_from_path(&path)?);
        }
    }
    highest
        .checked_add(1)
        .ok_or_else(|| "next level number overflowed".into())
}

fn update_manifest(
    levels_dir: &Path,
    manifest_path: &Path,
    base_url: &str,
    catalog_version: u32,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut level_paths = fs::read_dir(levels_dir)?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .map(|path| {
            let order = level_number_from_path(&path)?;
            Ok((order, path))
        })
        .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
    level_paths.sort_by_key(|(order, _)| *order);

    let mut entries = Vec::with_capacity(level_paths.len());
    for (order, path) in level_paths {
        let bytes = fs::read(&path)?;
        let level = parse_level(&bytes, &path)?;
        validate_level(&level, order, &path)?;

        let digest = Sha256::digest(&bytes);
        let sha256 = digest
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        entries.push(ManifestLevel {
            id: format!("level_{order}"),
            order,
            url: format!("{base_url}/level_{order}.json"),
            sha256,
            size: bytes.len(),
            enabled: true,
        });
    }

    if let Some(parent) = manifest_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    let manifest = Manifest {
        schema_version: 1,
        catalog_version,
        levels: entries,
    };
    fs::write(manifest_path, serde_json::to_vec_pretty(&manifest)?)?;
    println!("Manifest contains {} level(s)", manifest.levels.len());
    Ok(())
}

fn parse_level(bytes: &[u8], path: &Path) -> Result<Level, Box<dyn std::error::Error>> {
    let json_bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    serde_json::from_slice(json_bytes)
        .map_err(|error| format!("Invalid level {}: {error}", path.display()).into())
}

fn level_number_from_path(path: &Path) -> Result<u32, Box<dyn std::error::Error>> {
    let file_stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .ok_or_else(|| format!("Invalid level filename: {}", path.display()))?;
    let number = file_stem
        .strip_prefix("level_")
        .ok_or_else(|| {
            format!(
                "Expected filename level_<number>.json, got {}",
                path.display()
            )
        })?
        .parse::<u32>()
        .map_err(|_| {
            format!(
                "Expected filename level_<number>.json, got {}",
                path.display()
            )
        })?;
    if number == 0 {
        return Err(format!("Level number must be greater than zero: {}", path.display()).into());
    }
    Ok(number)
}

fn validate_level(
    level: &Level,
    expected_number: u32,
    path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if level.level != expected_number {
        return Err(format!(
            "{} has level {}, expected {}",
            path.display(),
            level.level,
            expected_number
        )
        .into());
    }
    if level.max_moves == 0 || level.colors.is_empty() {
        return Err(format!("{} has invalid maxMoves or empty colors", path.display()).into());
    }

    let color_count = level.colors.len();
    for index in 1..=color_count {
        let key = index.to_string();
        let color = level.colors.get(&key).ok_or_else(|| {
            format!(
                "{} colors must use continuous keys starting at 1",
                path.display()
            )
        })?;
        if !valid_hex_color(color) {
            return Err(format!("{} has invalid color {color}", path.display()).into());
        }
    }
    if !(1..=color_count as u8).contains(&level.target_color) {
        return Err(format!("{} has invalid targetColor", path.display()).into());
    }
    let width = level.grid.first().map_or(0, Vec::len);
    if width == 0 || level.grid.iter().any(|row| row.len() != width) {
        return Err(format!("{} grid must be non-empty and rectangular", path.display()).into());
    }
    if level
        .grid
        .iter()
        .flatten()
        .any(|color| *color == 0 || *color > color_count as u8)
    {
        return Err(format!("{} grid contains an unknown color", path.display()).into());
    }
    Ok(())
}

fn valid_hex_color(value: &str) -> bool {
    let value = value.strip_prefix('#').unwrap_or(value);
    (value.len() == 6 || value.len() == 8)
        && value.chars().all(|character| character.is_ascii_hexdigit())
}

fn load_templates(directory: &PathBuf) -> Result<Vec<Template>, Box<dyn std::error::Error>> {
    let mut paths = fs::read_dir(directory)?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .collect::<Vec<_>>();
    paths.sort();

    if paths.is_empty() {
        return Err(format!("No .json templates found in {}", directory.display()).into());
    }

    paths
        .into_iter()
        .map(|path| {
            let matrix: Vec<Vec<u8>> = serde_json::from_reader(fs::File::open(&path)?)?;
            let height = matrix.len();
            let width = matrix.first().map_or(0, Vec::len);
            if width == 0 || height == 0 || matrix.iter().any(|row| row.len() != width) {
                return Err(format!(
                    "Template {} must be a non-empty rectangular matrix",
                    path.display()
                )
                .into());
            }

            let cells = matrix
                .iter()
                .enumerate()
                .flat_map(|(y, row)| {
                    row.iter()
                        .enumerate()
                        .filter_map(move |(x, value)| (*value != 0).then_some(Cell { x, y }))
                })
                .collect::<Vec<_>>();
            if cells.is_empty() {
                return Err(format!(
                    "Template {} must contain at least one non-zero cell",
                    path.display()
                )
                .into());
            }

            Ok(Template {
                cells,
                width,
                height,
                path,
            })
        })
        .collect()
}

fn paint_template(
    grid: &mut [Vec<u8>],
    template: &[Cell],
    color: u8,
    rng: &mut StdRng,
    width: usize,
    height: usize,
    template_width: usize,
    template_height: usize,
) {
    let start_x = rng.random_range(0..=width.saturating_sub(template_width));
    let start_y = rng.random_range(0..=height.saturating_sub(template_height));
    for cell in template {
        grid[start_y + cell.y][start_x + cell.x] = color;
    }
}

fn color_palette() -> BTreeMap<String, String> {
    BTreeMap::from([
        ("1".into(), "#F44336".into()),
        ("2".into(), "#4CAF50".into()),
        ("3".into(), "#2196F3".into()),
        ("4".into(), "#FFC107".into()),
    ])
}

fn random_seed() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as u64)
        .unwrap_or(0)
}

fn print_help() {
    println!("Color Flood level generator");
    println!("Options: --output PATH --level N --width N --height N");
    println!("         --repeats N --target-color 1..4 --seed N");
    println!("         --templates-dir PATH --levels-dir PATH");
    println!("         --manifest PATH --base-url URL --catalog-version N");
    println!("         --manifest-only");
}
