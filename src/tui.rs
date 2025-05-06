use anyhow::{anyhow, Result};
use crossterm::event::{self, Event};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Style, Stylize},
    text::Text,
    widgets::{Block, Row, Table, TableState},
    Frame,
};
use time::format_description;

use crate::Album;

pub(crate) struct InitTui {
    pub(crate) old_albums: Vec<Album>,
    pub(crate) new_albums: Vec<Album>,
    pub(crate) new_other: Vec<Album>,
}

struct AppState {
    albums: Vec<Album>,
    other: Vec<Album>,
    table_state: TableState,
}

pub(crate) fn run(init: InitTui) -> Result<Vec<Album>> {
    let mut init = init;
    let mut terminal = ratatui::init();
    let mut table_state = TableState::default();
    init.old_albums.append(&mut init.new_albums);
    let mut app_state = AppState {
        albums: init.old_albums,
        other: init.new_other,
        table_state,
    };
    loop {
        terminal
            .draw(|frame| draw(frame, &mut app_state))
            .expect("failed to draw frame");
        if matches!(event::read().expect("failed to read event"), Event::Key(_)) {
            break;
        }
    }
    ratatui::restore();
    Err(anyhow!("NYI"))
}

fn album_to_row(album: &Album) -> Row {
    let format = format_description::parse("[year]-[month]-[day]").expect("Could not do format");
    let date: String = album
        .date
        .and_then(|d| d.format(&format).ok())
        .unwrap_or_else(|| "NONE".to_string());
    Row::new(vec![album.title.clone(), album.artist.clone(), date])
}

fn construct_table<'a>(albums: &'a [Album], header: &'a str) -> Table<'a> {
    let rows = albums.iter().map(album_to_row);

    // Columns widths are constrained in the same way as Layout...
    let widths = [
        Constraint::Percentage(25),
        Constraint::Percentage(25),
        Constraint::Percentage(25),
    ];
    Table::new(rows, widths)
        // ...and they can be separated by a fixed spacing.
        .column_spacing(1)
        // You can set the style of the entire Table.
        .style(Style::new().blue())
        // It has an optional header, which is simply a Row always visible at the top.
        .header(
            Row::new(vec!["Title", "Artist", "Date"])
                .style(Style::new().bold())
                // To add space between the header and the rest of the rows, specify the margin
                .bottom_margin(1),
        )
        // As any other widget, a Table can be wrapped in a Block.
        .block(Block::new().title(header))
        // The selected row, column, cell and its content can also be styled.
        .row_highlight_style(Style::new().reversed())
        .column_highlight_style(Style::new().red())
        .cell_highlight_style(Style::new().blue())
        // ...and potentially show a symbol in front of the selection.
        .highlight_symbol(">>")
}

fn draw(frame: &mut Frame, app_state: &mut AppState) {
    let new_albums_table = construct_table(&app_state.albums, "New Albums");

    let others_table = construct_table(&app_state.other, "Others");

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(frame.area());

    frame.render_stateful_widget(new_albums_table, layout[0], &mut app_state.table_state);
    frame.render_widget(others_table, layout[1]);
}
