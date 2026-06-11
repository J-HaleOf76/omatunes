use std::path::PathBuf;
use std::time::Duration;

use iced::widget::{button, column, container, row, text, Space};
use iced::{Alignment, Element, Length, Subscription, Task, Theme};
use mpris_server::{LoopStatus, PlaybackStatus};

use crate::audio::{AudioCommand, AudioEvent, AudioPlayer, MprisCommand, MprisUpdate, PlaybackState};
use crate::audio::mpris;
use crate::library::models::{Playlist, Track};
use crate::library::{scan_directory, Database};
use crate::ui::{theme, views};

// ── Mensagens ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum Message {
    // Navegação
    SwitchTab(Tab),
    SelectFolder(PathBuf),
    SelectPlaylist(i64),

    // Controles de playback
    PlayTrack(Track),
    PlayPause,
    NextTrack,
    PreviousTrack,
    Seek(Duration),
    VolumeChanged(f32),
    ToggleShuffle,
    ToggleRepeat,

    // Poll periódico do canal de eventos de áudio e MPRIS
    PollAudio,
    // Verificação periódica de mudança de tema
    CheckTheme,

    // Biblioteca
    LibraryScanned(usize),

    // Playlists
    CreatePlaylist,
    DeletePlaylist(i64),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Tab {
    Library,
    Playlists,
}

// ── Estado global ─────────────────────────────────────────────────────────────

pub struct AppState {
    pub tab: Tab,
    pub playback_state: PlaybackState,
    pub current_track: Option<Track>,
    pub queue: Vec<Track>,
    pub queue_index: usize,
    pub position: Duration,
    pub duration: Duration,
    pub volume: f32,
    pub shuffle: bool,
    pub repeat: bool,

    pub folders: Vec<PathBuf>,
    pub selected_folder: Option<PathBuf>,
    pub tracks: Vec<Track>,

    pub playlists: Vec<Playlist>,
    pub selected_playlist: Option<i64>,
    pub playlist_tracks: Vec<Track>,

    // Tema do iced reconstruído quando o Omarchy muda de tema
    pub iced_theme: iced::Theme,
    loaded_theme_name: String,

    audio: AudioPlayer,
    db: Database,

    mpris_cmd_rx: tokio::sync::mpsc::UnboundedReceiver<MprisCommand>,
    mpris_update_tx: tokio::sync::mpsc::UnboundedSender<MprisUpdate>,
}

impl AppState {
    fn new() -> (Self, Task<Message>) {
        let audio = AudioPlayer::spawn();

        let data_dir = dirs_next();
        std::fs::create_dir_all(&data_dir).ok();
        let db_path = data_dir.join("lavanda.db");
        let db = Database::open(&db_path).expect("Não foi possível abrir o banco de dados");

        let folders = music_subfolders(&home_music_dir());
        let playlists = db.all_playlists().unwrap_or_default();

        let (mpris_cmd_tx, mpris_cmd_rx) = tokio::sync::mpsc::unbounded_channel();
        let (mpris_update_tx, mpris_update_rx) = tokio::sync::mpsc::unbounded_channel();
        mpris::launch(mpris_cmd_tx, mpris_update_rx);

        let loaded_theme_name = theme::read_current_theme_name();
        let iced_theme = build_iced_theme();

        let state = AppState {
            tab: Tab::Library,
            playback_state: PlaybackState::Stopped,
            current_track: None,
            queue: Vec::new(),
            queue_index: 0,
            position: Duration::ZERO,
            duration: Duration::ZERO,
            volume: 0.8,
            shuffle: false,
            repeat: false,
            folders,
            selected_folder: None,
            tracks: Vec::new(),
            playlists,
            selected_playlist: None,
            playlist_tracks: Vec::new(),
            iced_theme,
            loaded_theme_name,
            audio,
            db,
            mpris_cmd_rx,
            mpris_update_tx,
        };

        let music_dir = home_music_dir();
        let task = Task::perform(
            async move {
                let db_path = dirs_next().join("lavanda.db");
                if let Ok(db) = Database::open(&db_path) {
                    scan_directory(&db, &music_dir).unwrap_or(0)
                } else {
                    0
                }
            },
            Message::LibraryScanned,
        );

        (state, task)
    }

    fn send_mpris(&self, update: MprisUpdate) {
        let _ = self.mpris_update_tx.send(update);
    }

    fn notify_mpris_track(&self, status: PlaybackStatus) {
        if let Some(track) = &self.current_track {
            self.send_mpris(MprisUpdate::Metadata {
                title: track.title.clone(),
                artist: track.artist.clone(),
                album: track.album.clone(),
                duration_us: track.duration.as_micros() as i64,
            });
        }
        self.send_mpris(MprisUpdate::Status(status));
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::SwitchTab(tab) => {
                self.tab = tab;
                Task::none()
            }

            Message::SelectFolder(path) => {
                self.selected_folder = Some(path.clone());
                self.tracks = self.db
                    .tracks_in_folder(&path.to_string_lossy())
                    .unwrap_or_default();
                Task::none()
            }

            Message::SelectPlaylist(id) => {
                self.selected_playlist = Some(id);
                self.playlist_tracks = self.db.playlist_tracks(id).unwrap_or_default();
                Task::none()
            }

            Message::PlayTrack(track) => {
                self.audio.send(AudioCommand::Play(track.path.clone()));
                self.audio.send(AudioCommand::SetVolume(self.volume));
                self.current_track = Some(track);
                self.playback_state = PlaybackState::Playing;
                self.position = Duration::ZERO;
                self.notify_mpris_track(PlaybackStatus::Playing);
                Task::none()
            }

            Message::PlayPause => {
                match self.playback_state {
                    PlaybackState::Playing => {
                        self.audio.send(AudioCommand::Pause);
                        self.playback_state = PlaybackState::Paused;
                        self.send_mpris(MprisUpdate::Status(PlaybackStatus::Paused));
                    }
                    PlaybackState::Paused => {
                        self.audio.send(AudioCommand::Resume);
                        self.playback_state = PlaybackState::Playing;
                        self.send_mpris(MprisUpdate::Status(PlaybackStatus::Playing));
                    }
                    PlaybackState::Stopped => {
                        if let Some(first) = self.tracks.first().cloned() {
                            self.audio.send(AudioCommand::Play(first.path.clone()));
                            self.current_track = Some(first);
                            self.playback_state = PlaybackState::Playing;
                            self.position = Duration::ZERO;
                            self.notify_mpris_track(PlaybackStatus::Playing);
                        }
                    }
                }
                Task::none()
            }

            Message::NextTrack => {
                self.advance_track(1);
                Task::none()
            }

            Message::PreviousTrack => {
                self.advance_track(-1);
                Task::none()
            }

            Message::Seek(dur) => {
                self.audio.send(AudioCommand::Seek(dur));
                self.position = dur;
                Task::none()
            }

            Message::VolumeChanged(v) => {
                self.volume = v;
                self.audio.send(AudioCommand::SetVolume(self.volume));
                self.send_mpris(MprisUpdate::Volume(v as f64));
                Task::none()
            }

            Message::ToggleShuffle => {
                self.shuffle = !self.shuffle;
                self.send_mpris(MprisUpdate::Shuffle(self.shuffle));
                Task::none()
            }

            Message::ToggleRepeat => {
                self.repeat = !self.repeat;
                let loop_status = if self.repeat { LoopStatus::Track } else { LoopStatus::None };
                self.send_mpris(MprisUpdate::Loop(loop_status));
                Task::none()
            }

            Message::PollAudio => {
                // Drena eventos de áudio
                while let Ok(event) = self.audio.event_rx.try_recv() {
                    match event {
                        AudioEvent::Progress { position, duration } => {
                            self.position = position;
                            self.duration = duration;
                        }
                        AudioEvent::Paused => {
                            self.playback_state = PlaybackState::Paused;
                        }
                        AudioEvent::Stopped => {
                            self.playback_state = PlaybackState::Stopped;
                            self.position = Duration::ZERO;
                            self.send_mpris(MprisUpdate::Status(PlaybackStatus::Stopped));
                        }
                        AudioEvent::TrackEnded => {
                            if self.repeat {
                                if let Some(t) = self.current_track.clone() {
                                    self.audio.send(AudioCommand::Play(t.path));
                                    self.notify_mpris_track(PlaybackStatus::Playing);
                                }
                            } else {
                                self.advance_track(1);
                            }
                        }
                        AudioEvent::Error(e) => eprintln!("Erro de áudio: {e}"),
                        AudioEvent::Playing { .. } => {
                            self.playback_state = PlaybackState::Playing;
                        }
                    }
                }

                // Drena comandos MPRIS vindos do D-Bus (Waybar, etc.)
                while let Ok(cmd) = self.mpris_cmd_rx.try_recv() {
                    match cmd {
                        MprisCommand::Play => {
                            if !matches!(self.playback_state, PlaybackState::Playing) {
                                self.audio.send(AudioCommand::Resume);
                                self.playback_state = PlaybackState::Playing;
                                self.send_mpris(MprisUpdate::Status(PlaybackStatus::Playing));
                            }
                        }
                        MprisCommand::Pause => {
                            if matches!(self.playback_state, PlaybackState::Playing) {
                                self.audio.send(AudioCommand::Pause);
                                self.playback_state = PlaybackState::Paused;
                                self.send_mpris(MprisUpdate::Status(PlaybackStatus::Paused));
                            }
                        }
                        MprisCommand::PlayPause => {
                            match self.playback_state {
                                PlaybackState::Playing => {
                                    self.audio.send(AudioCommand::Pause);
                                    self.playback_state = PlaybackState::Paused;
                                    self.send_mpris(MprisUpdate::Status(PlaybackStatus::Paused));
                                }
                                _ => {
                                    self.audio.send(AudioCommand::Resume);
                                    self.playback_state = PlaybackState::Playing;
                                    self.send_mpris(MprisUpdate::Status(PlaybackStatus::Playing));
                                }
                            }
                        }
                        MprisCommand::Next => { self.advance_track(1); }
                        MprisCommand::Previous => { self.advance_track(-1); }
                        MprisCommand::Stop => {
                            self.audio.send(AudioCommand::Stop);
                            self.playback_state = PlaybackState::Stopped;
                            self.position = Duration::ZERO;
                            self.send_mpris(MprisUpdate::Status(PlaybackStatus::Stopped));
                        }
                    }
                }

                Task::none()
            }

            Message::CheckTheme => {
                let current = theme::read_current_theme_name();
                if !current.is_empty() && current != self.loaded_theme_name {
                    theme::reload_system_theme();
                    self.iced_theme = build_iced_theme();
                    self.loaded_theme_name = current;
                    eprintln!("lavanda: tema atualizado para \"{}\"", self.loaded_theme_name);
                }
                Task::none()
            }

            Message::LibraryScanned(count) => {
                eprintln!("Biblioteca: {count} faixas indexadas");
                self.folders = music_subfolders(&home_music_dir());
                self.playlists = self.db.all_playlists().unwrap_or_default();
                Task::none()
            }

            Message::CreatePlaylist => {
                let name = format!("Playlist {}", self.playlists.len() + 1);
                self.db.create_playlist(&name).ok();
                self.playlists = self.db.all_playlists().unwrap_or_default();
                Task::none()
            }

            Message::DeletePlaylist(id) => {
                self.db.delete_playlist(id).ok();
                self.playlists = self.db.all_playlists().unwrap_or_default();
                if self.selected_playlist == Some(id) {
                    self.selected_playlist = None;
                    self.playlist_tracks.clear();
                }
                Task::none()
            }
        }
    }

    fn view(&self) -> Element<'_, Message> {
        let header = self.header_view();
        let player_panel = views::player::view(self);

        let content = match self.tab {
            Tab::Library => views::library::view(self),
            Tab::Playlists => views::playlist::view(self),
        };

        let main = column![header, player_panel, content]
            .spacing(0)
            .width(Length::Fill)
            .height(Length::Fill);

        container(main)
            .style(|_: &Theme| iced::widget::container::Style {
                background: Some(iced::Background::Color(theme::base())),
                ..Default::default()
            })
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            iced::time::every(Duration::from_millis(100)).map(|_| Message::PollAudio),
            iced::time::every(Duration::from_secs(3)).map(|_| Message::CheckTheme),
        ])
    }

    fn header_view(&self) -> Element<'_, Message> {
        let tab_btn = |label: &'static str, tab: Tab| {
            let is_active = self.tab == tab;
            let color = if is_active { theme::accent() } else { theme::subtext() };
            button(
                text(label).color(color).size(14).font(
                    if is_active { crate::ui::icons::UI_FONT_BOLD } else { crate::ui::icons::UI_FONT }
                ),
            )
            .on_press(Message::SwitchTab(tab))
            .style(iced::widget::button::text)
            .padding([8, 16])
        };

        let nav = row![
            text(crate::ui::icons::ICON_MUSIC)
                .font(crate::ui::icons::NERD_FONT_MONO)
                .color(theme::accent())
                .size(16),
            text(" lavanda")
                .color(theme::accent())
                .size(16)
                .font(crate::ui::icons::UI_FONT_BOLD),
            Space::with_width(24),
            tab_btn("Biblioteca", Tab::Library),
            tab_btn("Playlists", Tab::Playlists),
        ]
        .align_y(Alignment::Center)
        .spacing(0);

        container(nav)
            .style(theme::header)
            .width(Length::Fill)
            .padding([0, 12])
            .into()
    }

    fn advance_track(&mut self, delta: i32) {
        if self.tracks.is_empty() {
            return;
        }

        let next_idx = if self.shuffle {
            use rand::Rng;
            let current_idx = self.current_track.as_ref()
                .and_then(|ct| self.tracks.iter().position(|t| t.id == ct.id));
            let len = self.tracks.len();
            if len == 1 {
                0
            } else {
                let mut rng = rand::thread_rng();
                let mut idx = rng.gen_range(0..len);
                if let Some(cur) = current_idx {
                    while idx == cur { idx = rng.gen_range(0..len); }
                }
                idx
            }
        } else {
            let current_idx = self.current_track.as_ref()
                .and_then(|ct| self.tracks.iter().position(|t| t.id == ct.id));
            match current_idx {
                Some(i) => {
                    let new = i as i32 + delta;
                    if new < 0 { self.tracks.len() - 1 } else { new as usize % self.tracks.len() }
                }
                None => 0,
            }
        };

        if let Some(track) = self.tracks.get(next_idx).cloned() {
            self.audio.send(AudioCommand::Play(track.path.clone()));
            self.current_track = Some(track);
            self.playback_state = PlaybackState::Playing;
            self.position = Duration::ZERO;
            self.notify_mpris_track(PlaybackStatus::Playing);
        }
    }
}

fn music_subfolders(music_dir: &PathBuf) -> Vec<PathBuf> {
    let mut folders: Vec<PathBuf> = std::fs::read_dir(music_dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .map(|e| e.path())
        .collect();
    folders.sort();
    folders
}

fn dirs_next() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(home).join(".local/share/lavanda")
}

fn home_music_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(home).join("Music")
}

fn build_iced_theme() -> Theme {
    Theme::custom(
        "Omarchy".into(),
        iced::theme::Palette {
            background: theme::base(),
            text:       theme::text(),
            primary:    theme::accent(),
            success:    theme::green(),
            danger:     theme::red(),
        },
    )
}

// ── Ponto de entrada iced ─────────────────────────────────────────────────────

pub fn run() -> iced::Result {
    iced::application("lavanda", AppState::update, AppState::view)
        .subscription(AppState::subscription)
        .default_font(iced::Font {
            family: iced::font::Family::Name("JetBrainsMono Nerd Font Mono"),
            weight: iced::font::Weight::Normal,
            stretch: iced::font::Stretch::Normal,
            style: iced::font::Style::Normal,
        })
        .theme(|state: &AppState| state.iced_theme.clone())
        .window(iced::window::Settings {
            size: iced::Size::new(960.0, 640.0),
            min_size: Some(iced::Size::new(700.0, 480.0)),
            ..Default::default()
        })
        .run_with(AppState::new)
}
