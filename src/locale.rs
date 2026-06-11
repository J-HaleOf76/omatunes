use std::sync::OnceLock;

static LOCALE: OnceLock<&'static Strings> = OnceLock::new();

pub struct Strings {
    pub tab_library:       &'static str,
    pub tab_playlists:     &'static str,
    pub sidebar_folders:   &'static str,
    pub sidebar_playlists: &'static str,
    pub new_playlist:      &'static str,
    pub no_track:          &'static str,
    pub no_tracks_found:   &'static str,
    pub select_folder:     &'static str,
    pub select_playlist:   &'static str,
    pub unknown:           &'static str,
    track_singular:        &'static str,
    track_plural:          &'static str,
}

impl Strings {
    pub fn track_count(&self, n: usize) -> String {
        if n == 1 {
            format!("1 {}", self.track_singular)
        } else {
            format!("{} {}", n, self.track_plural)
        }
    }
}

// ── Traduções ─────────────────────────────────────────────────────────────────

static EN: Strings = Strings {
    tab_library:       "Library",
    tab_playlists:     "Playlists",
    sidebar_folders:   "Folders",
    sidebar_playlists: "Playlists",
    new_playlist:      " New playlist",
    no_track:          "No track",
    no_tracks_found:   "No tracks found",
    select_folder:     "Select a folder",
    select_playlist:   "Select a playlist",
    unknown:           "Unknown",
    track_singular:    "track",
    track_plural:      "tracks",
};

static PT_BR: Strings = Strings {
    tab_library:       "Biblioteca",
    tab_playlists:     "Playlists",
    sidebar_folders:   "Pastas",
    sidebar_playlists: "Playlists",
    new_playlist:      " Nova playlist",
    no_track:          "Nenhuma faixa",
    no_tracks_found:   "Nenhuma faixa encontrada",
    select_folder:     "Selecione uma pasta",
    select_playlist:   "Selecione uma playlist",
    unknown:           "Desconhecido",
    track_singular:    "faixa",
    track_plural:      "faixas",
};

static ES: Strings = Strings {
    tab_library:       "Biblioteca",
    tab_playlists:     "Listas",
    sidebar_folders:   "Carpetas",
    sidebar_playlists: "Listas",
    new_playlist:      " Nueva lista",
    no_track:          "Sin pista",
    no_tracks_found:   "Sin pistas",
    select_folder:     "Selecciona una carpeta",
    select_playlist:   "Selecciona una lista",
    unknown:           "Desconocido",
    track_singular:    "pista",
    track_plural:      "pistas",
};

// ── Inicialização ─────────────────────────────────────────────────────────────

pub fn load() {
    LOCALE.get_or_init(detect);
}

pub fn get() -> &'static Strings {
    LOCALE.get_or_init(detect)
}

fn detect() -> &'static Strings {
    let lang = std::env::var("LANG")
        .or_else(|_| std::env::var("LANGUAGE"))
        .or_else(|_| std::env::var("LC_ALL"))
        .unwrap_or_default();

    // Pega só a parte antes do '.' (ex: "pt_BR.UTF-8" → "pt_BR")
    let lang = lang.split('.').next().unwrap_or("").to_lowercase();

    if lang.starts_with("pt") { &PT_BR }
    else if lang.starts_with("es") { &ES }
    else { &EN }
}
