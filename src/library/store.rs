use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::models::{Playlist, Track};

// ── Formatos JSON persistidos ──────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Default)]
struct LibraryJson {
    version: u32,
    tracks: Vec<TrackJson>,
}

#[derive(Serialize, Deserialize, Clone)]
struct TrackJson {
    path: String,
    title: String,
    artist: String,
    album_artist: String,
    album: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    track_number: Option<u32>,
    duration_ms: u64,
    mtime: u64,
    // nome do arquivo de capa em covers_dir (ex: "a1b2c3d4e5f60708.jpg")
    #[serde(skip_serializing_if = "Option::is_none")]
    cover: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct PlaylistJson {
    name: String,
    tracks: Vec<String>, // paths absolutos, em ordem
}

// ── Tipos internos ─────────────────────────────────────────────────────────────

#[derive(Clone)]
struct TrackRecord {
    id: i64,
    json: TrackJson,
}

struct PlaylistRecord {
    id: i64,
    name: String,
    tracks: Vec<String>,
}

// ── Store ──────────────────────────────────────────────────────────────────────

pub struct Store {
    library_path: PathBuf,
    playlists_dir: PathBuf,
    covers_dir: PathBuf,

    tracks: Vec<TrackRecord>,
    by_path: HashMap<String, usize>,

    playlists: Vec<PlaylistRecord>,
    next_track_id: i64,
    next_playlist_id: i64,
}

impl Store {
    /// Abre (ou cria) o store.
    /// - `config_dir` → ~/.config/lavanda/   (playlists/, editável pelo usuário)
    /// - `cache_dir`  → ~/.cache/lavanda/    (library.json, covers/)
    pub fn open(config_dir: &Path, cache_dir: &Path) -> Result<Self> {
        let library_path = cache_dir.join("library.json");
        let playlists_dir = config_dir.join("playlists");
        let covers_dir = cache_dir.join("covers");

        std::fs::create_dir_all(cache_dir)?;
        std::fs::create_dir_all(&playlists_dir)?;
        std::fs::create_dir_all(&covers_dir)?;

        // ── Carrega biblioteca ─────────────────────────────────────────────────
        let raw_tracks = if library_path.exists() {
            let data = std::fs::read_to_string(&library_path)?;
            serde_json::from_str::<LibraryJson>(&data).unwrap_or_default().tracks
        } else {
            Vec::new()
        };

        let mut tracks: Vec<TrackRecord> = Vec::with_capacity(raw_tracks.len());
        let mut by_path: HashMap<String, usize> = HashMap::with_capacity(raw_tracks.len());
        for (i, json) in raw_tracks.into_iter().enumerate() {
            by_path.insert(json.path.clone(), i);
            tracks.push(TrackRecord { id: (i as i64) + 1, json });
        }
        let next_track_id = tracks.len() as i64 + 1;

        // ── Carrega playlists ──────────────────────────────────────────────────
        let mut files: Vec<_> = std::fs::read_dir(&playlists_dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.path().extension().map_or(false, |x| x == "json"))
            .collect();
        files.sort_by_key(|e| e.file_name());

        let mut playlists: Vec<PlaylistRecord> = Vec::new();
        for (i, entry) in files.iter().enumerate() {
            if let Ok(data) = std::fs::read_to_string(entry.path()) {
                if let Ok(pl) = serde_json::from_str::<PlaylistJson>(&data) {
                    playlists.push(PlaylistRecord {
                        id: (i as i64) + 1,
                        name: pl.name,
                        tracks: pl.tracks,
                    });
                }
            }
        }
        let next_playlist_id = playlists.len() as i64 + 1;

        Ok(Store {
            library_path,
            playlists_dir,
            covers_dir,
            tracks,
            by_path,
            playlists,
            next_track_id,
            next_playlist_id,
        })
    }

    /// Persiste library.json. Chamado explicitamente pelo scanner ao término do scan.
    pub fn save_library(&self) -> Result<()> {
        let lib = LibraryJson {
            version: 1,
            tracks: self.tracks.iter().map(|r| r.json.clone()).collect(),
        };
        let json = serde_json::to_string_pretty(&lib)?;
        std::fs::write(&self.library_path, json)?;
        Ok(())
    }

    /// Relê library.json do disco (após scan assíncrono).
    pub fn reload_library(&mut self) -> Result<()> {
        let raw_tracks = if self.library_path.exists() {
            let data = std::fs::read_to_string(&self.library_path)?;
            serde_json::from_str::<LibraryJson>(&data).unwrap_or_default().tracks
        } else {
            Vec::new()
        };
        self.tracks.clear();
        self.by_path.clear();
        for (i, json) in raw_tracks.into_iter().enumerate() {
            self.by_path.insert(json.path.clone(), i);
            self.tracks.push(TrackRecord { id: (i as i64) + 1, json });
        }
        self.next_track_id = self.tracks.len() as i64 + 1;
        Ok(())
    }

    // ── Capas ──────────────────────────────────────────────────────────────────

    fn cover_filename(parent_dir: &Path) -> String {
        let hash = fnv1a(parent_dir.to_string_lossy().as_bytes());
        format!("{:016x}.jpg", hash)
    }

    fn write_cover(&self, parent_dir: &Path, data: &[u8]) -> Option<String> {
        let filename = Self::cover_filename(parent_dir);
        std::fs::write(self.covers_dir.join(&filename), data).ok()?;
        Some(filename)
    }

    /// Carrega os bytes da capa para o path de faixa dado.
    pub fn load_cover_for_path(&self, path: &str) -> Option<Vec<u8>> {
        let idx = *self.by_path.get(path)?;
        let cover_file = self.tracks[idx].json.cover.as_ref()?;
        std::fs::read(self.covers_dir.join(cover_file)).ok()
    }

    // ── API de faixas ──────────────────────────────────────────────────────────

    pub fn track_mtime(&self, path: &str) -> Option<u64> {
        Some(self.tracks[*self.by_path.get(path)?].json.mtime)
    }

    pub fn track_id_by_path(&self, path: &str) -> Option<i64> {
        Some(self.tracks[*self.by_path.get(path)?].id)
    }

    /// Insere ou atualiza uma faixa em memória. Chame `save_library()` ao terminar o scan.
    pub fn upsert_track(
        &mut self,
        path: &str,
        title: &str,
        artist: &str,
        album: &str,
        album_artist: &str,
        track_number: Option<u32>,
        duration_ms: u64,
        mtime: u64,
        cover: Option<&[u8]>,
    ) -> Result<()> {
        let parent = Path::new(path).parent();
        let cover_file = cover
            .zip(parent)
            .and_then(|(data, dir)| self.write_cover(dir, data))
            .or_else(|| {
                // mantém capa existente se não vier nova
                self.by_path.get(path).and_then(|&i| self.tracks[i].json.cover.clone())
            });

        let json = TrackJson {
            path: path.to_string(),
            title: title.to_string(),
            artist: artist.to_string(),
            album_artist: album_artist.to_string(),
            album: album.to_string(),
            track_number,
            duration_ms,
            mtime,
            cover: cover_file,
        };

        if let Some(&idx) = self.by_path.get(path) {
            self.tracks[idx].json = json;
        } else {
            let id = self.next_track_id;
            self.next_track_id += 1;
            self.by_path.insert(path.to_string(), self.tracks.len());
            self.tracks.push(TrackRecord { id, json });
        }

        Ok(())
    }

    pub fn remove_missing_tracks(&mut self, root: &Path, seen: &HashSet<String>) -> Result<usize> {
        let prefix = format!("{}/", root.to_string_lossy().trim_end_matches('/'));
        let to_remove: Vec<String> = self.tracks.iter()
            .filter(|r| r.json.path.starts_with(&prefix) && !seen.contains(&r.json.path))
            .map(|r| r.json.path.clone())
            .collect();

        let count = to_remove.len();
        for path in &to_remove {
            if let Some(idx) = self.by_path.remove(path) {
                self.tracks.remove(idx);
                // reindexar posições após remoção
                for i in self.by_path.values_mut() {
                    if *i > idx {
                        *i -= 1;
                    }
                }
            }
        }
        Ok(count)
    }

    fn build_track(&self, record: &TrackRecord) -> Track {
        let j = &record.json;
        Track {
            id: record.id,
            path: PathBuf::from(&j.path),
            title: j.title.clone(),
            artist: j.artist.clone(),
            album: j.album.clone(),
            album_id: 0,
            track_number: j.track_number,
            duration: Duration::from_millis(j.duration_ms),
            cover_data: None, // carregado sob demanda em PlayTrack
        }
    }

    pub fn tracks_in_folder(&self, folder_path: &str) -> Result<Vec<Track>> {
        let prefix = format!("{}/", folder_path.trim_end_matches('/'));
        let mut records: Vec<&TrackRecord> = self.tracks.iter()
            .filter(|r| r.json.path.starts_with(&prefix))
            .collect();
        records.sort_by(|a, b| {
            a.json.album.cmp(&b.json.album)
                .then(a.json.track_number.cmp(&b.json.track_number))
                .then(a.json.title.cmp(&b.json.title))
        });
        Ok(records.iter().map(|r| self.build_track(r)).collect())
    }

    // ── API de playlists ───────────────────────────────────────────────────────

    pub fn all_playlists(&self) -> Result<Vec<Playlist>> {
        Ok(self.playlists.iter().map(|pl| Playlist {
            id: pl.id,
            name: pl.name.clone(),
            track_count: pl.tracks.len(),
        }).collect())
    }

    pub fn create_playlist(&mut self, name: &str) -> Result<i64> {
        let id = self.next_playlist_id;
        self.next_playlist_id += 1;
        let pl = PlaylistRecord { id, name: name.to_string(), tracks: Vec::new() };
        self.save_playlist(&pl)?;
        self.playlists.push(pl);
        Ok(id)
    }

    pub fn delete_playlist(&mut self, playlist_id: i64) -> Result<()> {
        if let Some(pos) = self.playlists.iter().position(|p| p.id == playlist_id) {
            let pl = self.playlists.remove(pos);
            let _ = std::fs::remove_file(self.playlist_path(&pl.name));
            // reassigna IDs para manter sequência
            for (i, p) in self.playlists.iter_mut().enumerate() {
                p.id = (i as i64) + 1;
            }
            self.next_playlist_id = self.playlists.len() as i64 + 1;
        }
        Ok(())
    }

    pub fn playlist_tracks(&self, playlist_id: i64) -> Result<Vec<Track>> {
        let Some(pl) = self.playlists.iter().find(|p| p.id == playlist_id) else {
            return Ok(Vec::new());
        };
        Ok(pl.tracks.iter()
            .filter_map(|path| {
                let idx = *self.by_path.get(path.as_str())?;
                Some(self.build_track(&self.tracks[idx]))
            })
            .collect())
    }

    pub fn add_track_to_playlist(&mut self, playlist_id: i64, track_id: i64) -> Result<()> {
        let path = match self.tracks.iter().find(|r| r.id == track_id) {
            Some(r) => r.json.path.clone(),
            None => return Ok(()),
        };
        let Some(pos) = self.playlists.iter().position(|p| p.id == playlist_id) else {
            return Ok(());
        };
        if self.playlists[pos].tracks.contains(&path) {
            return Ok(());
        }
        self.playlists[pos].tracks.push(path);
        // borrow posicional: sem iter_mut ativo, save_playlist pode emprestar &self
        let pl = &self.playlists[pos];
        self.save_playlist(pl)?;
        Ok(())
    }

    fn playlist_path(&self, name: &str) -> PathBuf {
        self.playlists_dir.join(format!("{}.json", sanitize_filename(name)))
    }

    fn save_playlist(&self, pl: &PlaylistRecord) -> Result<()> {
        let pj = PlaylistJson { name: pl.name.clone(), tracks: pl.tracks.clone() };
        let json = serde_json::to_string_pretty(&pj)?;
        std::fs::write(self.playlist_path(&pl.name), json)?;
        Ok(())
    }
}

// ── Helpers ────────────────────────────────────────────────────────────────────

fn fnv1a(data: &[u8]) -> u64 {
    const OFFSET: u64 = 0xcbf29ce484222325;
    const PRIME: u64 = 0x100000001b3;
    data.iter().fold(OFFSET, |h, &b| (h ^ b as u64).wrapping_mul(PRIME))
}

fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\0' => '_',
            _ => c,
        })
        .collect()
}
