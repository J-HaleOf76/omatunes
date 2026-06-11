use std::collections::HashSet;
use std::path::Path;
use std::time::UNIX_EPOCH;

use anyhow::Result;
use lofty::prelude::*;
use lofty::probe::Probe;
use walkdir::WalkDir;

use super::db::Database;

const AUDIO_EXTENSIONS: &[&str] = &["mp3", "flac", "ogg", "opus", "wav", "aac", "m4a", "wma", "aiff"];

pub fn scan_directory(db: &Database, root: &Path) -> Result<usize> {
    let mut count = 0;
    let mut seen: HashSet<String> = HashSet::new();

    for entry in WalkDir::new(root)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let path = entry.path();

        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase());

        let Some(ext) = ext else { continue };
        if !AUDIO_EXTENSIONS.contains(&ext.as_str()) {
            continue;
        }

        let path_str = path.to_string_lossy().to_string();
        seen.insert(path_str.clone());

        let mtime = entry
            .metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);

        if db.track_mtime(&path_str) == Some(mtime) {
            continue;
        }

        match read_tags(path) {
            Ok(info) => {
                let artist_id = db.upsert_artist(&info.artist)?;
                let album_artist_id = db.upsert_artist(
                    if info.album_artist.is_empty() { &info.artist } else { &info.album_artist }
                )?;
                let album_id = db.upsert_album(
                    &info.album,
                    album_artist_id,
                    info.year,
                    info.cover.as_deref(),
                )?;

                db.upsert_track(
                    &path_str,
                    &info.title,
                    artist_id,
                    album_id,
                    &info.album_artist,
                    info.track_number,
                    info.duration_ms,
                    mtime,
                    info.cover.as_deref(),
                )?;

                count += 1;
            }
            Err(e) => {
                eprintln!("Erro ao ler tags de {path_str}: {e}");
            }
        }
    }

    // Remove do banco qualquer faixa sob `root` que não existe mais no disco.
    let removed = db.remove_missing_tracks(root, &seen)?;
    if removed > 0 {
        eprintln!("Scanner: {removed} faixa(s) removida(s) do banco (arquivos não encontrados)");
    }

    Ok(count)
}

struct TrackInfo {
    title: String,
    artist: String,
    album_artist: String,
    album: String,
    year: Option<u32>,
    track_number: Option<u32>,
    duration_ms: u64,
    cover: Option<Vec<u8>>,
}

fn read_tags(path: &Path) -> Result<TrackInfo> {
    let tagged = Probe::open(path)?.read()?;

    let properties = tagged.properties();
    let duration_ms = properties.duration().as_millis() as u64;

    let tags = tagged.primary_tag();

    let title = tags
        .and_then(|t| t.title())
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(crate::locale::get().unknown)
                .to_string()
        });

    let folder_album = path.parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or(crate::locale::get().unknown)
        .to_string();
    let folder_artist = path.parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or(crate::locale::get().unknown)
        .to_string();

    let artist = tags
        .and_then(|t| t.artist())
        .map(|s| s.to_string())
        .unwrap_or(folder_artist.clone());

    let album_artist = tags
        .and_then(|t| t.get_string(&lofty::tag::ItemKey::AlbumArtist))
        .map(|s| s.to_string())
        .unwrap_or(folder_artist);

    let album = tags
        .and_then(|t| t.album())
        .map(|s| s.to_string())
        .unwrap_or(folder_album);

    let year = tags.and_then(|t| t.year());
    let track_number = tags.and_then(|t| t.track());

    let cover = tags.and_then(|t| {
        t.pictures().iter().find(|p| {
            matches!(p.pic_type(), lofty::picture::PictureType::CoverFront | lofty::picture::PictureType::Other)
        }).map(|p| p.data().to_vec())
    });

    Ok(TrackInfo { title, artist, album_artist, album, year, track_number, duration_ms, cover })
}
