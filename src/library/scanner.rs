use std::collections::HashSet;
use std::path::Path;
use std::time::UNIX_EPOCH;

use anyhow::Result;
use lofty::prelude::*;
use lofty::probe::Probe;
use walkdir::WalkDir;

use super::store::Store;

const AUDIO_EXTENSIONS: &[&str] = &["mp3", "flac", "ogg", "opus", "wav", "aac", "m4a", "wma", "aiff"];

/// Indexa um único arquivo de áudio. Persiste library.json ao terminar.
pub fn scan_file(store: &mut Store, path: &Path) -> Result<bool> {
    let ext = path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase());
    let Some(ext) = ext else { return Ok(false) };
    if !AUDIO_EXTENSIONS.contains(&ext.as_str()) {
        return Ok(false);
    }

    let path_str = path.to_string_lossy().to_string();

    let mtime = path.metadata().ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);

    if store.track_mtime(&path_str) == Some(mtime) {
        return Ok(true);
    }

    match read_tags(path) {
        Ok(info) => {
            store.upsert_track(
                &path_str,
                &info.title,
                &info.artist,
                &info.album,
                &info.album_artist,
                info.track_number,
                info.duration_ms,
                mtime,
                info.cover.as_deref(),
            )?;
            store.save_library()?;
            Ok(true)
        }
        Err(e) => {
            eprintln!("Erro ao ler tags de {path_str}: {e}");
            Ok(false)
        }
    }
}

pub fn scan_directory(store: &mut Store, root: &Path) -> Result<usize> {
    let mut count = 0;
    let mut seen: HashSet<String> = HashSet::new();

    for entry in WalkDir::new(root)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let path = entry.path();

        let ext = path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase());
        let Some(ext) = ext else { continue };
        if !AUDIO_EXTENSIONS.contains(&ext.as_str()) {
            continue;
        }

        let path_str = path.to_string_lossy().to_string();
        seen.insert(path_str.clone());

        let mtime = entry.metadata().ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);

        if store.track_mtime(&path_str) == Some(mtime) {
            continue;
        }

        match read_tags(path) {
            Ok(info) => {
                store.upsert_track(
                    &path_str,
                    &info.title,
                    &info.artist,
                    &info.album,
                    &info.album_artist,
                    info.track_number,
                    info.duration_ms,
                    mtime,
                    info.cover.as_deref(),
                )?;
                count += 1;
            }
            Err(e) => eprintln!("Erro ao ler tags de {path_str}: {e}"),
        }
    }

    let removed = store.remove_missing_tracks(root, &seen)?;
    if removed > 0 {
        eprintln!("Scanner: {removed} faixa(s) removida(s) (arquivos não encontrados)");
    }

    store.save_library()?;
    Ok(count)
}

// ── Leitura de tags ────────────────────────────────────────────────────────────

struct TrackInfo {
    title: String,
    artist: String,
    album_artist: String,
    album: String,
    track_number: Option<u32>,
    duration_ms: u64,
    cover: Option<Vec<u8>>,
}

fn read_tags(path: &Path) -> Result<TrackInfo> {
    let tagged = Probe::open(path)?.read()?;
    let duration_ms = tagged.properties().duration().as_millis() as u64;
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

    let folder_artist = path.parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or(crate::locale::get().unknown)
        .to_string();

    let folder_album = path.parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or(crate::locale::get().unknown)
        .to_string();

    let artist = tags
        .and_then(|t| t.artist())
        .map(|s| s.to_string())
        .unwrap_or_else(|| folder_artist.clone());

    let album_artist = tags
        .and_then(|t| t.get_string(&lofty::tag::ItemKey::AlbumArtist))
        .map(|s| s.to_string())
        .unwrap_or_else(|| folder_artist);

    let album = tags
        .and_then(|t| t.album())
        .map(|s| s.to_string())
        .unwrap_or(folder_album);

    let track_number = tags.and_then(|t| t.track());

    let cover = tags
        .and_then(|t| {
            t.pictures().iter().find(|p| {
                matches!(
                    p.pic_type(),
                    lofty::picture::PictureType::CoverFront | lofty::picture::PictureType::Other
                )
            })
            .map(|p| p.data().to_vec())
        })
        .or_else(|| cover_from_folder(path));

    Ok(TrackInfo { title, artist, album_artist, album, track_number, duration_ms, cover })
}

const COVER_FILENAMES: &[&str] = &["cover.jpg", "cover.png", "cover.webp", "folder.jpg", "folder.png"];

fn cover_from_folder(path: &Path) -> Option<Vec<u8>> {
    let dir = path.parent()?;
    for name in COVER_FILENAMES {
        if let Ok(data) = std::fs::read(dir.join(name)) {
            return Some(data);
        }
    }
    None
}
