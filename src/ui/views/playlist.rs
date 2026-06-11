use iced::widget::{button, column, container, row, scrollable, text, Space};
use iced::{Alignment, Element, Length};

use crate::app::{AppState, Message};
use crate::ui::{icons, theme};

pub fn view(state: &AppState) -> Element<'_, Message> {
    let sidebar = playlist_sidebar(state);
    let track_list = playlist_tracks(state);

    row![sidebar, track_list]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn playlist_sidebar(state: &AppState) -> Element<'_, Message> {
    let new_btn = button(
        row![
            text(icons::ICON_PLUS)
                .font(icons::NERD_FONT_MONO)
                .color(theme::accent())
                .size(13),
            text(state.strings.new_playlist)
                .color(theme::accent())
                .size(13),
        ]
        .align_y(Alignment::Center)
        .spacing(4),
    )
    .on_press(Message::CreatePlaylist)
    .style(iced::widget::button::text)
    .padding([6, 12]);

    let items: Element<Message> = column(
        state.playlists.iter().map(|pl| {
            let is_selected = state.selected_playlist.as_ref() == Some(&pl.id);

            let row_content = row![
                text(&pl.name)
                    .color(if is_selected { theme::accent() } else { theme::text() })
                    .size(14)
                    .width(Length::Fill),
                text(format!("{}", pl.track_count))
                    .color(theme::overlay0())
                    .size(12),
            ]
            .align_y(Alignment::Center)
            .padding([6, 12]);

            let styled = if is_selected {
                container(row_content).style(theme::selected_row).width(Length::Fill)
            } else {
                container(row_content).width(Length::Fill)
            };

            button(styled)
                .on_press(Message::SelectPlaylist(pl.id))
                .style(iced::widget::button::text)
                .width(Length::Fill)
                .padding(0)
                .into()
        })
        .collect::<Vec<_>>(),
    )
    .spacing(2)
    .into();

    container(
        column![
            text(state.strings.sidebar_playlists)
                .color(theme::subtext())
                .size(11)
                .font(crate::ui::icons::UI_FONT_BOLD),
            Space::with_height(4),
            new_btn,
            Space::with_height(4),
            scrollable(items).height(Length::Fill),
        ]
        .padding(8),
    )
    .style(theme::sidebar)
    .width(200)
    .height(Length::Fill)
    .into()
}

fn playlist_tracks(state: &AppState) -> Element<'_, Message> {
    let current_id = state.current_track.as_ref().map(|t| t.id);

    let tracks = &state.playlist_tracks;

    if tracks.is_empty() {
        return container(
            text(state.strings.select_playlist)
                .color(theme::overlay0())
                .size(15),
        )
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .width(Length::Fill)
        .height(Length::Fill)
        .into();
    }

    let rows: Element<Message> = column(
        tracks.iter().enumerate().map(|(i, track)| {
            let is_current = current_id == Some(track.id);

            let row_content = row![
                text(format!("{}", i + 1))
                    .color(theme::overlay0())
                    .size(13)
                    .width(30),
                text(&track.title)
                    .color(if is_current { theme::accent() } else { theme::text() })
                    .size(14)
                    .width(Length::FillPortion(3)),
                text(&track.artist)
                    .color(theme::subtext())
                    .size(13)
                    .width(Length::FillPortion(2)),
                text(track.duration_str())
                    .color(theme::subtext())
                    .size(13)
                    .width(60),
            ]
            .spacing(12)
            .align_y(Alignment::Center)
            .padding([5, 12]);

            let styled = if is_current {
                container(row_content).style(theme::selected_row).width(Length::Fill)
            } else {
                container(row_content).width(Length::Fill)
            };

            button(styled)
                .on_press(Message::PlayTrack(track.clone()))
                .style(iced::widget::button::text)
                .width(Length::Fill)
                .padding(0)
                .into()
        })
        .collect::<Vec<_>>(),
    )
    .spacing(2)
    .into();

    container(scrollable(rows))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
