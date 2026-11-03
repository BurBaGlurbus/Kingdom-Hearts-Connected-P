//! Reads the HUD's movies, their textures and fonts from the player's own paks.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use kh3_data::Paks;
use kh3_data::package::Package;
use kh3_data::props::{self, Value};
use kh3_data::texture::Texture as Cooked;

use crate::def::{Font, MovieDef, Pack, Texture};
use crate::gfx::{self, texture_key};
use crate::render::{Fonts, IconSet, pad_glyphs};

const SCALEFORM: &str = "/Game/UI/Scaleform/";
const COMMAND: &str = "01_hud/hud_command/HudCommand";
const TARGET: &str = "01_hud/hud_target/HudTarget";
const STATUS: &str = "01_hud/hud_status/HudStatus";
const FONT: &str = "91_font/TresFont";
const ICON_TEXT: &str = "/Game/UI/IconText";

/// What the deck and the shotlock's display draw.
pub struct HudAssets {
    pub command: Arc<MovieDef>,
    pub target: Arc<MovieDef>,
    pub status: Arc<MovieDef>,
    pub fonts: Fonts,
    /// Movie, textures found and textures named: what the log reports.
    pub report: Vec<(String, usize, usize)>,
}

/// A movie package's raw bytes and the assets it names.
fn raw_movie(p: &Package) -> Result<(Vec<u8>, Vec<String>), String> {
    let i = (0..p.exports.len())
        .find(|&i| p.class_of(i) == "SwfMovie")
        .ok_or("no SwfMovie export")?;
    let (list, _) = props::export_props(p, i).map_err(|e| e.to_string())?;
    let raw = match list.get("RawData") {
        Some(Value::Raw(b)) => b.clone(),
        Some(Value::Array(v)) => v
            .iter()
            .filter_map(|x| match x {
                Value::Byte(b) => Some(*b),
                Value::Int(b) => Some(*b as u8),
                _ => None,
            })
            .collect(),
        _ => return Err("no RawData".into()),
    };
    let refs = list
        .array("ReferencedAssetNames")
        .unwrap_or_default()
        .iter()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect();
    Ok((raw, refs))
}

fn cooked_texture(p: &Package, name: &str) -> Option<Texture> {
    let is_tex = |i: usize| p.class_of(i) == "Texture2D";
    let i = (0..p.exports.len())
        .find(|&i| is_tex(i) && p.exports[i].name.eq_ignore_ascii_case(name))
        .or_else(|| (0..p.exports.len()).find(|&i| is_tex(i)))?;
    let t = Cooked::read(p, i).ok()?;
    let mip = t.mips.iter().find(|m| !m.bytes.is_empty())?;
    let rgba = gfx::decode_texture(&t.format, mip.width, mip.height, &mip.bytes)?;
    Some(Texture {
        file: name.to_string(),
        width: mip.width,
        height: mip.height,
        rgba,
    })
}

/// A texture a movie names: through its referenced assets, then beside the movie, and in the
/// language folder of either.
fn find_texture(paks: &Paks, rel: &str, refs: &[String], name: &str) -> Option<Texture> {
    let dir = rel.rsplit_once('/').map_or("", |d| d.0);
    let mut cands: Vec<String> = Vec::new();
    for r in refs {
        let path = r.split('.').next().unwrap_or(r);
        if path
            .rsplit('/')
            .next()
            .is_some_and(|b| b.eq_ignore_ascii_case(name))
        {
            cands.push(path.to_string());
            if let Some((d, b)) = path.rsplit_once('/') {
                cands.push(format!("{d}/en/{b}"));
            }
        }
    }
    cands.push(format!("{SCALEFORM}{dir}/{name}"));
    cands.push(format!("{SCALEFORM}{dir}/en/{name}"));
    for c in cands {
        if let Ok(Some(p)) = paks.game_package(&c)
            && let Some(t) = cooked_texture(&p, name)
        {
            return Some(t);
        }
    }
    None
}

fn add_movie(
    paks: &Paks,
    pack: &mut Pack,
    report: &mut Vec<(String, usize, usize)>,
    name: &str,
    rel: &str,
) -> Result<Vec<u8>, String> {
    let path = format!("{SCALEFORM}{rel}");
    let p = paks
        .game_package(&path)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("{path}: not in the paks"))?;
    let (raw, refs) = raw_movie(&p).map_err(|e| format!("{path}: {e}"))?;
    let movie = gfx::read_movie(name, &raw).map_err(|e| format!("{path}: {e}"))?;
    let keys: HashSet<String> = movie.images.iter().map(texture_key).collect();
    let mut found = 0;
    for key in &keys {
        if pack.textures.contains_key(key) {
            found += 1;
        } else if let Some(t) = find_texture(paks, rel, &refs, key) {
            pack.textures.insert(key.clone(), Arc::new(t));
            found += 1;
        }
    }
    report.push((name.to_string(), found, keys.len()));
    pack.movie_files.insert(name.to_string(), movie.json);
    Ok(raw)
}

/// The rows of a data table package.
fn table_rows(paks: &Paks, path: &str) -> Option<Vec<(String, props::Props)>> {
    let p = paks.game_package(path).ok()??;
    let i = (0..p.exports.len()).find(|&i| p.class_of(i) == "DataTable")?;
    props::data_table_rows(&p, i).ok()
}

/// The icons of text: `UI/IconText`'s category icons and the chosen pad set's buttons.
fn icon_set(paks: &Paks) -> Option<IconSet> {
    let folder = pad_glyphs();
    let mut set = IconSet::default();
    for (image, table) in [
        (
            format!("{ICON_TEXT}/IconText"),
            format!("{ICON_TEXT}/IconTextData"),
        ),
        (
            format!("{ICON_TEXT}/{folder}/IconText_pf"),
            format!("{ICON_TEXT}/{folder}/IconTextData_pf"),
        ),
    ] {
        let package = paks.game_package(&image).ok()??;
        let name = image.rsplit('/').next()?;
        let texture = cooked_texture(&package, name)?;
        let index = set.textures.len();
        set.textures.push(Arc::new(texture));
        let number = |row: &props::Props, key: &str| {
            row.f32(key)
                .or_else(|| row.int(key).map(|v| v as f32))
                .unwrap_or(0.)
        };
        for (name, row) in table_rows(paks, &table)? {
            set.icons.insert(
                name.to_lowercase(),
                (
                    index,
                    number(&row, "X"),
                    number(&row, "Y"),
                    number(&row, "Width"),
                    number(&row, "Height"),
                ),
            );
        }
    }
    for (alias, to) in [("decide", "cross"), ("cancel", "circle")] {
        if let Some(icon) = set.icons.get(to).copied() {
            set.icons.insert(alias.to_owned(), icon);
        }
    }
    Some(set)
}

/// Reads the deck, the shotlock's display, the gauges and the fonts.
pub fn load(paks: &Paks) -> Result<HudAssets, String> {
    let mut pack = Pack::default();
    let mut report = Vec::new();
    add_movie(paks, &mut pack, &mut report, "HudCommand", COMMAND)?;
    add_movie(paks, &mut pack, &mut report, "HudTarget", TARGET)?;
    add_movie(paks, &mut pack, &mut report, "HudStatus", STATUS)?;
    // The fonts: compacted, bound to their classes.
    let path = format!("{SCALEFORM}{FONT}");
    let p = paks
        .game_package(&path)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("{path}: not in the paks"))?;
    let (raw, _) = raw_movie(&p).map_err(|e| format!("{path}: {e}"))?;
    let tres = gfx::read_movie("TresFont", &raw).map_err(|e| format!("{path}: {e}"))?;
    let mut fonts = Fonts::default();
    for (class, pos, len) in &tres.fonts {
        let f: Font =
            gfx::read_font(class, &raw, *pos, *len).map_err(|e| format!("{path}: {e}"))?;
        let f = Arc::new(f);
        pack.fonts.insert(class.clone(), f.clone());
        fonts.map.insert(class.clone(), f);
    }
    fonts.default = fonts.map.get("$KH2Menu").cloned();
    fonts.icons = icon_set(paks).map(Arc::new);
    let command = pack
        .movie("HudCommand")
        .map_err(|e| format!("HudCommand: {e}"))?;
    let target = pack
        .movie("HudTarget")
        .map_err(|e| format!("HudTarget: {e}"))?;
    let status = pack
        .movie("HudStatus")
        .map_err(|e| format!("HudStatus: {e}"))?;
    Ok(HudAssets {
        command,
        target,
        status,
        fonts,
        report,
    })
}
