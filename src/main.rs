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
const COLOR_COUNT: u8 = 4;

// Sau 2 vòng non-target thì vòng tiếp theo được phép dùng target.
const NON_TARGET_LOOPS_BEFORE_TARGET: usize = 2;

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

    let base_seed = options.seed.unwrap_or_else(random_seed);

    for offset in 0..options.count {
        let offset_u32 = u32::try_from(offset).map_err(|_| "count exceeds u32 range")?;

        let level_number = options
            .level
            .checked_add(offset_u32)
            .ok_or("level number overflowed")?;

        let seed = base_seed
            .checked_add(offset as u64)
            .ok_or("seed overflowed while generating levels")?;

        let level = generate_level(&templates, &options, level_number, seed);

        let output = if options.count == 1 && options.output_explicit {
            options.output.clone()
        } else {
            options
                .levels_dir
                .join(format!("level_{level_number}.json"))
        };

        if let Some(parent) = output.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }

        fs::write(&output, serde_json::to_vec_pretty(&level)?)?;

        println!(
            "Generated {} (level {}, seed {}, targetColor {}, maxMoves {})",
            output.display(),
            level_number,
            seed,
            level.target_color,
            level.max_moves
        );
    }

    update_manifest(
        &options.levels_dir,
        &options.manifest,
        &options.base_url,
        options.catalog_version,
    )?;

    println!("Updated manifest {}", options.manifest.display());

    Ok(())
}

// ============================================================
// LEVEL GENERATION
// ============================================================

fn generate_level(templates: &[Template], options: &Options, level: u32, seed: u64) -> Level {
    let mut rng = StdRng::seed_from_u64(seed);

    // --------------------------------------------------------
    // BƯỚC 1:
    // Random target color.
    //
    // Nếu muốn ép target từ CLI thì vẫn có thể dùng
    // --target-color.
    //
    // Nhưng mặc định target sẽ random.
    // --------------------------------------------------------
    let target_color = if options.target_color_random {
        rng.random_range(1..=COLOR_COUNT)
    } else {
        options.target_color
    };

    // --------------------------------------------------------
    // BƯỚC 2:
    // Khởi tạo toàn bộ grid bằng target.
    // --------------------------------------------------------
    let mut grid: Vec<Vec<u8>> = vec![vec![target_color; options.width]; options.height];

    // --------------------------------------------------------
    // BƯỚC 3:
    // Mỗi iteration:
    //
    //   0 -> non-target
    //   1 -> non-target
    //   2 -> target
    //   3 -> non-target
    //   4 -> non-target
    //   5 -> target
    //
    // ...
    // --------------------------------------------------------
    for iteration in 0..options.repeats {
        let color = color_for_iteration(iteration, target_color, COLOR_COUNT);

        // ----------------------------------------------------
        // Tìm template + vị trí hợp lệ.
        //
        // Vị trí hợp lệ nghĩa là template đổi ít nhất một ô.
        //
        // Template vẫn có thể chồng lên các layer cũ.
        // ----------------------------------------------------
        let candidates =
            find_placement_candidates(&grid, templates, color, options.width, options.height);

        if candidates.is_empty() {
            // Không còn vị trí nào để đặt template.
            //
            // Không panic để tránh làm crash generator.
            // Có thể dừng sớm.
            eprintln!(
                "Warning: no valid placement at iteration {} \
                 for level {}. Stopping generation early.",
                iteration, level
            );

            break;
        }

        // Random một candidate.
        let candidate_index = rng.random_range(0..candidates.len());

        let candidate = candidates[candidate_index];

        // ----------------------------------------------------
        // Tô template bằng màu đã chọn.
        // ----------------------------------------------------
        paint_template_at(
            &mut grid,
            &templates[candidate.template_index].cells,
            color,
            candidate.start_x,
            candidate.start_y,
        );
    }

    Level {
        level,
        max_moves: options.repeats + 1,
        target_color,
        colors: color_palette(),
        grid,
    }
}

// ============================================================
// COLOR LOGIC
// ============================================================

/// Chọn màu theo target.
///
/// Logic:
///
/// Target = 4:
///     3, 2, 4, 1, 3, 4, 2, 1, 4...
///
/// Target = 3:
///     2, 1, 3, 4, 2, 3, 1, 4, 3...
///
/// Quan trọng:
/// - luôn có 2 màu non-target trước target
/// - màu non-target được lấy theo thứ tự "liền kề"
/// - target xuất hiện ở iteration thứ 3 của mỗi chu kỳ.
fn color_for_iteration(iteration: usize, target_color: u8, color_count: u8) -> u8 {
    let available_colors = adjacent_colors(target_color, color_count);

    let cycle_length = NON_TARGET_LOOPS_BEFORE_TARGET + 1;

    let cycle_position = iteration % cycle_length;

    // Sau 2 vòng non-target -> target.
    if cycle_position == NON_TARGET_LOOPS_BEFORE_TARGET {
        return target_color;
    }

    // Tính index của non-target color.
    let non_target_iteration =
        (iteration / cycle_length) * NON_TARGET_LOOPS_BEFORE_TARGET + cycle_position;

    available_colors[non_target_iteration % available_colors.len()]
}

/// Tạo danh sách màu quanh target.
///
/// Ví dụ target = 4:
///
///     [3, 2, 1]
///
/// target = 3:
///
///     [2, 1, 4]
///
/// target = 2:
///
///     [1, 4, 3]
///
/// target = 1:
///
///     [4, 3, 2]
///
/// Tức là bắt đầu từ màu ngay trước target,
/// sau đó đi vòng xuống dưới và wrap lên trên.
fn adjacent_colors(target_color: u8, color_count: u8) -> Vec<u8> {
    let mut colors = Vec::new();

    // Màu liền trước target.
    if target_color > 1 {
        for color in (1..target_color).rev() {
            colors.push(color);
        }
    }

    // Wrap sang màu lớn nhất.
    if target_color < color_count {
        for color in (target_color + 1..=color_count).rev() {
            colors.push(color);
        }
    }

    // Fallback an toàn.
    if colors.is_empty() {
        colors.push(target_color);
    }

    colors
}

// ============================================================
// PLACEMENT LOGIC
// ============================================================

#[derive(Clone, Copy)]
struct PlacementCandidate {
    template_index: usize,
    start_x: usize,
    start_y: usize,
}

/// Tìm tất cả vị trí có thể đặt template.
///
/// Template được phép chồng lên các layer cũ. Điều kiện duy nhất ngoài việc
/// nằm trong grid là ít nhất một cell phải thực sự đổi màu; nhờ vậy mỗi layer
/// đều tạo ra thay đổi và level có thể có các hình dạng đan xen phức tạp hơn.
fn find_placement_candidates(
    grid: &[Vec<u8>],
    templates: &[Template],
    paint_color: u8,
    width: usize,
    height: usize,
) -> Vec<PlacementCandidate> {
    let mut candidates = Vec::new();

    for (template_index, template) in templates.iter().enumerate() {
        if template.width > width || template.height > height {
            continue;
        }

        let max_x = width - template.width;

        let max_y = height - template.height;

        for start_y in 0..=max_y {
            for start_x in 0..=max_x {
                if can_place_template(grid, &template.cells, paint_color, start_x, start_y) {
                    candidates.push(PlacementCandidate {
                        template_index,
                        start_x,
                        start_y,
                    });
                }
            }
        }
    }

    candidates
}

/// Kiểm tra template có ít nhất một cell sẽ đổi màu.
fn can_place_template(
    grid: &[Vec<u8>],
    template: &[Cell],
    paint_color: u8,
    start_x: usize,
    start_y: usize,
) -> bool {
    template
        .iter()
        .any(|cell| grid[start_y + cell.y][start_x + cell.x] != paint_color)
}

/// Paint template tại vị trí đã xác định.
fn paint_template_at(
    grid: &mut [Vec<u8>],
    template: &[Cell],
    color: u8,
    start_x: usize,
    start_y: usize,
) {
    for cell in template {
        grid[start_y + cell.y][start_x + cell.x] = color;
    }
}

// ============================================================
// OPTIONS
// ============================================================

struct Options {
    output: PathBuf,
    level: u32,
    width: usize,
    height: usize,
    repeats: usize,
    count: usize,

    // Màu target mặc định.
    target_color: u8,

    // true = random target.
    target_color_random: bool,

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
            count: 1,

            // Giá trị mặc định vẫn là 4.
            target_color: DEFAULT_TARGET_COLOR,

            // MẶC ĐỊNH: random target.
            target_color_random: true,

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

                "--width" => {
                    options.width = value()?.parse()?;
                }

                "--height" => {
                    options.height = value()?.parse()?;
                }

                "--repeats" => {
                    options.repeats = value()?.parse()?;
                }

                "--count" => {
                    options.count = value()?.parse()?;
                }

                "--target-color" => {
                    options.target_color = value()?.parse()?;

                    // Nếu user truyền target-color
                    // thì không random.
                    options.target_color_random = false;
                }

                "--random-target" => {
                    options.target_color_random = true;
                }

                "--seed" => {
                    options.seed = Some(value()?.parse()?);
                }

                "--templates-dir" => {
                    options.templates_dir = value()?.into();
                }

                "--levels-dir" => {
                    options.levels_dir = value()?.into();
                }

                "--manifest" => {
                    options.manifest = value()?.into();
                }

                "--base-url" => {
                    options.base_url = value()?.trim_end_matches('/').to_owned();
                }

                "--catalog-version" => {
                    options.catalog_version = value()?.parse()?;
                }

                "--manifest-only" => {
                    options.manifest_only = true;
                }

                "--help" | "-h" => {
                    print_help();
                    std::process::exit(0);
                }

                unknown => {
                    return Err(format!("Unknown argument: {unknown}").into());
                }
            }
        }

        if options.width == 0 || options.height == 0 {
            return Err("width and height must be greater than zero".into());
        }

        if options.count == 0 {
            return Err("count must be greater than zero".into());
        }

        if options.count > 1 && options.output_explicit {
            return Err("--output cannot be used with --count greater than 1".into());
        }

        if !(1..=COLOR_COUNT).contains(&options.target_color) {
            return Err("target-color must be between 1 and 4".into());
        }

        if options.level_explicit && options.level == 0 {
            return Err("level must be greater than zero".into());
        }

        if !options.level_explicit {
            options.level = next_level_number(&options.levels_dir)?;
        }

        let last_offset =
            u32::try_from(options.count - 1).map_err(|_| "count exceeds u32 range")?;

        options
            .level
            .checked_add(last_offset)
            .ok_or("level range overflowed")?;

        Ok(options)
    }
}

// ============================================================
// LEVEL NUMBER
// ============================================================

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

// ============================================================
// MANIFEST
// ============================================================

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

// ============================================================
// PARSE / VALIDATE LEVEL
// ============================================================

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

// ============================================================
// LOAD TEMPLATES
// ============================================================

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

// ============================================================
// COLOR PALETTE
// ============================================================

fn color_palette() -> BTreeMap<String, String> {
    BTreeMap::from([
        ("1".into(), "#F44336".into()),
        ("2".into(), "#4CAF50".into()),
        ("3".into(), "#2196F3".into()),
        ("4".into(), "#FFC107".into()),
    ])
}

// ============================================================
// RANDOM SEED
// ============================================================

fn random_seed() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as u64)
        .unwrap_or(0)
}

// ============================================================
// HELP
// ============================================================

fn print_help() {
    println!("Color Flood level generator");

    println!("Options: --output PATH --level N --count N");

    println!("         --width N --height N --repeats N");

    println!("         --target-color 1..4");

    println!("         --random-target");

    println!("         --seed N");

    println!("         --templates-dir PATH --levels-dir PATH");

    println!("         --manifest PATH --base-url URL --catalog-version N");

    println!("         --manifest-only");

    println!();
    println!("Default behavior: target color is random.");

    println!("Use --target-color N to force a specific target.");
}

// ============================================================
// TESTS
// ============================================================

#[cfg(test)]
mod tests {
    use super::{Cell, adjacent_colors, can_place_template, color_for_iteration};

    #[test]
    fn target_four_sequence() {
        let colors: Vec<u8> = (0..9)
            .map(|iteration| color_for_iteration(iteration, 4, 4))
            .collect();

        assert_eq!(colors, vec![3, 2, 4, 1, 3, 4, 2, 1, 4]);
    }

    #[test]
    fn target_three_sequence() {
        let colors: Vec<u8> = (0..9)
            .map(|iteration| color_for_iteration(iteration, 3, 4))
            .collect();

        assert_eq!(colors, vec![2, 1, 3, 4, 2, 3, 1, 4, 3]);
    }

    #[test]
    fn target_two_sequence() {
        let colors: Vec<u8> = (0..9)
            .map(|iteration| color_for_iteration(iteration, 2, 4))
            .collect();

        assert_eq!(colors, vec![1, 4, 2, 3, 1, 2, 4, 3, 2]);
    }

    #[test]
    fn target_one_sequence() {
        let colors: Vec<u8> = (0..9)
            .map(|iteration| color_for_iteration(iteration, 1, 4))
            .collect();

        assert_eq!(colors, vec![4, 3, 1, 2, 4, 1, 3, 2, 1]);
    }

    #[test]
    fn adjacent_colors_for_target_four() {
        assert_eq!(adjacent_colors(4, 4), vec![3, 2, 1]);
    }

    #[test]
    fn adjacent_colors_for_target_three() {
        assert_eq!(adjacent_colors(3, 4), vec![2, 1, 4]);
    }

    #[test]
    fn adjacent_colors_for_target_two() {
        assert_eq!(adjacent_colors(2, 4), vec![1, 4, 3]);
    }

    #[test]
    fn adjacent_colors_for_target_one() {
        assert_eq!(adjacent_colors(1, 4), vec![4, 3, 2]);
    }

    #[test]
    fn placement_allows_overlap_when_a_cell_changes() {
        let grid = vec![vec![1, 2]];
        let template = vec![Cell { x: 0, y: 0 }, Cell { x: 1, y: 0 }];

        assert!(can_place_template(&grid, &template, 1, 0, 0));
    }

    #[test]
    fn placement_rejects_a_noop_layer() {
        let grid = vec![vec![1, 1]];
        let template = vec![Cell { x: 0, y: 0 }, Cell { x: 1, y: 0 }];

        assert!(!can_place_template(&grid, &template, 1, 0, 0));
    }
}
