//! Icon theme lookup and loading, against fixture themes.

// Test helpers (outside `#[test]` fns) unwrap: a failed fixture should fail the test.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};

use neue_desktop::{IconFormat, IconLoader, IconLookup};

fn write(root: &Path, relative: &str, bytes: impl AsRef<[u8]>) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

/// A `size`×`size` PNG filled with one RGBA colour.
fn png(size: u32, rgba: [u8; 4]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, size, size);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header().unwrap();
    let data: Vec<u8> = (0..size * size).flat_map(|_| rgba).collect();
    writer.write_image_data(&data).unwrap();
    writer.finish().unwrap();
    out
}

const SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="48" height="48" viewBox="0 0 48 48">
  <rect width="48" height="24" fill="#ff0000"/><rect y="24" width="48" height="24" fill="#0000ff" opacity="0.5"/>
</svg>"##;

fn index(inherits: &str, dirs: &[(&str, &str)]) -> String {
    let names: Vec<_> = dirs.iter().map(|(d, _)| *d).collect();
    let mut text = format!(
        "[Icon Theme]\nName=T\nInherits={inherits}\nDirectories={}\n",
        names.join(",")
    );
    for (d, body) in dirs {
        text.push_str(&format!("\n[{d}]\n{body}\n"));
    }
    text
}

struct Fixture {
    _dir: tempfile::TempDir,
    roots: Vec<PathBuf>,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let r = dir.path().join("icons");
    write(
        &r,
        "Main/index.theme",
        index(
            "Parent",
            &[
                ("16x16/apps", "Size=16\nType=Fixed"),
                ("32x32/apps", "Size=32\nType=Fixed"),
                ("32x32@2x/apps", "Size=32\nScale=2\nType=Fixed"),
                (
                    "scalable/apps",
                    "Size=48\nMinSize=16\nMaxSize=512\nType=Scalable",
                ),
                ("24/places", "Size=24\nType=Threshold\nThreshold=4"),
            ],
        ),
    );
    write(&r, "Main/16x16/apps/small.png", png(16, [255, 0, 0, 255]));
    write(&r, "Main/32x32/apps/small.png", png(32, [0, 255, 0, 255]));
    write(
        &r,
        "Main/32x32@2x/apps/small.png",
        png(64, [0, 0, 255, 255]),
    );
    write(&r, "Main/scalable/apps/vector.svg", SVG);
    write(&r, "Main/16x16/apps/both.png", png(16, [1, 2, 3, 255]));
    write(&r, "Main/scalable/apps/both.svg", SVG);
    write(&r, "Main/24/places/folder.png", png(24, [9, 9, 9, 255]));
    // A parent theme, and the fallback, and hicolor.
    write(
        &r,
        "Parent/index.theme",
        index("", &[("48x48/apps", "Size=48\nType=Fixed")]),
    );
    write(
        &r,
        "Parent/48x48/apps/inherited.png",
        png(48, [5, 5, 5, 255]),
    );
    write(
        &r,
        "Papirus/index.theme",
        index("", &[("64x64/apps", "Size=64\nType=Fixed")]),
    );
    write(
        &r,
        "Papirus/64x64/apps/papirus-only.png",
        png(64, [7, 7, 7, 255]),
    );
    write(
        &r,
        "Papirus/64x64/apps/inherited.png",
        png(64, [6, 6, 6, 255]),
    );
    write(
        &r,
        "hicolor/index.theme",
        index("", &[("128x128/apps", "Size=128\nType=Fixed")]),
    );
    write(&r, "hicolor/128x128/apps/hi.png", png(128, [8, 8, 8, 255]));
    write(&r, "pixmaps/loose.png", png(20, [4, 4, 4, 255]));
    write(&r, "Broken/index.theme", "garbage"); // parses to nothing
    write(&r, "Main/16x16/apps/corrupt.png", b"not a png");
    Fixture {
        roots: vec![r],
        _dir: dir,
    }
}

fn lookup(f: &Fixture) -> IconLookup {
    IconLookup::new(
        &f.roots,
        "Main",
        Some("Papirus"),
        &[f.roots[0].join("pixmaps")],
    )
}

fn rel(f: &Fixture, found: &neue_desktop::IconFile) -> String {
    found
        .path
        .strip_prefix(&f.roots[0])
        .unwrap()
        .to_string_lossy()
        .into_owned()
}

#[test]
fn the_search_order_is_theme_parents_fallback_hicolor() {
    let f = fixture();
    assert_eq!(
        lookup(&f).theme_names(),
        ["Main", "Parent", "Papirus", "hicolor"]
    );
}

#[test]
fn an_exact_directory_wins_and_scale_selects_the_2x_directory() {
    let f = fixture();
    let l = lookup(&f);
    let at = |size, scale| rel(&f, &l.find("small", size, scale).unwrap());
    assert_eq!(at(16, 1), "Main/16x16/apps/small.png");
    assert_eq!(at(32, 1), "Main/32x32/apps/small.png");
    assert_eq!(at(32, 2), "Main/32x32@2x/apps/small.png");
}

#[test]
fn without_an_exact_directory_the_closest_one_is_used() {
    let f = fixture();
    let l = lookup(&f);
    // 20 is nearer 16 (4) than 32 (12); 28 nearer 32.
    assert_eq!(
        rel(&f, &l.find("small", 20, 1).unwrap()),
        "Main/16x16/apps/small.png"
    );
    assert_eq!(
        rel(&f, &l.find("small", 28, 1).unwrap()),
        "Main/32x32/apps/small.png"
    );
    // A Threshold directory matches within its threshold.
    assert_eq!(
        rel(&f, &l.find("folder", 27, 1).unwrap()),
        "Main/24/places/folder.png"
    );
}

#[test]
fn scalable_directories_match_a_range_and_prefer_png_then_svg_per_directory() {
    let f = fixture();
    let l = lookup(&f);
    let v = l.find("vector", 256, 1).unwrap();
    assert_eq!(
        (rel(&f, &v).as_str(), v.format),
        ("Main/scalable/apps/vector.svg", IconFormat::Svg)
    );
    // Listed order: the 16x16 PNG matches 16 before the scalable SVG does.
    let both = l.find("both", 16, 1).unwrap();
    assert_eq!(both.format, IconFormat::Png);
    assert_eq!(l.find("both", 64, 1).unwrap().format, IconFormat::Svg);
}

#[test]
fn parents_come_before_the_fallback_and_hicolor_and_pixmaps_are_last() {
    let f = fixture();
    let l = lookup(&f);
    assert_eq!(
        rel(&f, &l.find("inherited", 64, 1).unwrap()),
        "Parent/48x48/apps/inherited.png"
    );
    assert_eq!(
        rel(&f, &l.find("papirus-only", 16, 1).unwrap()),
        "Papirus/64x64/apps/papirus-only.png"
    );
    assert_eq!(
        rel(&f, &l.find("hi", 16, 1).unwrap()),
        "hicolor/128x128/apps/hi.png"
    );
    assert_eq!(
        rel(&f, &l.find("loose", 16, 1).unwrap()),
        "pixmaps/loose.png"
    );
    assert!(l.find("nope", 16, 1).is_none());
    assert!(l.find("", 16, 1).is_none());
}

#[test]
fn names_with_extensions_and_absolute_paths_work() {
    let f = fixture();
    let l = lookup(&f);
    assert!(l.find("small.png", 16, 1).is_some());
    let abs = f.roots[0].join("pixmaps/loose.png");
    assert_eq!(l.find(abs.to_str().unwrap(), 16, 1).unwrap().path, abs);
    assert!(l.find("/nonexistent/x.png", 16, 1).is_none());
    assert!(
        l.find("/etc/hosts", 16, 1).is_none(),
        "not an image extension"
    );
}

#[test]
fn a_user_directory_in_front_overrides_the_system_theme() {
    let f = fixture();
    let user = f._dir.path().join("user-icons");
    write(
        &user,
        "Main/16x16/apps/small.png",
        png(16, [200, 200, 200, 255]),
    );
    let roots = vec![user.clone(), f.roots[0].clone()];
    let l = IconLookup::new(&roots, "Main", None, &[]);
    // The theme's directories come from the highest-priority index.theme; a theme without one
    // there still contributes its files through the system one.
    assert_eq!(
        l.find("small", 16, 1).unwrap().path,
        user.join("Main/16x16/apps/small.png")
    );
    assert_eq!(l.theme_names(), ["Main", "Parent", "hicolor"]);
}

#[test]
fn png_icons_load_with_their_pixels() {
    let f = fixture();
    let mut loader = IconLoader::new(lookup(&f));
    let icon = loader.load("small", 16, 1.0).unwrap();
    let pic = icon.picture().unwrap();
    assert_eq!((pic.width(), pic.height()), (16, 16));
    // Cached: the same icon (identity) comes back.
    assert_eq!(loader.load("small", 16, 1.0).unwrap(), icon);
    // Another scale is another lookup (the 2x directory) and another icon.
    let big = loader.load("small", 32, 2.0).unwrap();
    assert_eq!(big.picture().unwrap().width(), 64);
    assert_ne!(big, icon);
}

#[test]
fn svg_icons_are_rasterised_at_the_requested_size() {
    let f = fixture();
    let mut loader = IconLoader::new(lookup(&f));
    let at1 = loader.load("vector", 24, 1.0).unwrap();
    assert_eq!(at1.picture().unwrap().width(), 24);
    let at2 = loader.load("vector", 24, 2.0).unwrap();
    assert_eq!(at2.picture().unwrap().width(), 48);
    assert!(at1.has_own_colors());
}

#[test]
fn svg_colours_and_transparency_survive_rasterising() {
    let f = fixture();
    let file = lookup(&f).find("vector", 48, 1).unwrap();
    let icon = neue_desktop::icon_load::load_file(&file, 4).unwrap();
    let pic = icon.picture().unwrap();
    assert_eq!((pic.width(), pic.height()), (4, 4));
    // Rendered through neue-paint, the top is opaque red and the bottom half-transparent blue.
    let mut scene = neue_paint::Scene::new(4.0, 4.0);
    neue_paint::icon::icon(
        &mut scene,
        neue_paint::kurbo::Rect::new(0.0, 0.0, 4.0, 4.0),
        &icon,
        neue_paint::peniko::Color::BLACK,
        neue_paint::Variant::default(),
    );
    let out = neue_paint::cpu::CpuRenderer::default()
        .render(&scene, 1.0)
        .unwrap();
    let data = out.data_as_u8_slice();
    assert_eq!(&data[..4], [255, 0, 0, 255]);
    let bottom = &data[(3 * 4) * 4..(3 * 4) * 4 + 4];
    assert!(
        bottom[2] > 100 && (120..136).contains(&bottom[3]),
        "{bottom:?}"
    );
}

#[test]
fn unusable_files_load_as_none() {
    let f = fixture();
    let mut loader = IconLoader::new(lookup(&f));
    assert!(loader.load("corrupt", 16, 1.0).is_none());
    assert!(loader.load("missing", 16, 1.0).is_none());
    assert!(
        loader.load("small", 16, f64::NAN).is_some(),
        "a bad scale is 1"
    );
}
